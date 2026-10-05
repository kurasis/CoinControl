//! Market prices through Live Coin Watch (API_PROVIDERS.md §3.1).
//!
//! Read endpoints are POST with JSON and the `x-api-key` header:
//! `/coins/map` (custom set, at most 100 codes per call), `/coins/single/history`
//! (millisecond timestamps), `/credits`. `delta.day` is a ratio, normalized to a
//! percentage as `(delta.day - 1) * 100`.

use std::sync::Arc;
use std::time::Duration;

use bigdecimal::One;
use portfolio_core::decimal::Dec;
use portfolio_core::network::NetworkId;
use reqwest::header::{HeaderMap, HeaderValue};
use serde::Deserialize;
use url::Url;

use crate::error::ProviderError;
use crate::esplora::parse_base;
use crate::http::{Budget, HttpClient, HttpConfig};
use crate::util::de_dec;

pub const PROVIDER: &str = "livecoinwatch";
pub const DEFAULT_BASE: &str = "https://api.livecoinwatch.com/";
/// Documented result limit of `/coins/map`.
pub const MAP_LIMIT: usize = 100;

/// Curated, manually verified LCW codes for native assets. Tokens are never
/// mapped by ticker; they are priced by contract identity (DefiLlama).
///
/// Verified against `/coins/map` on 2026-10-05: LCW's `TON` is an unrelated
/// "TONToken"; Toncoin is `TONCOIN`. Polygon's native asset is `POL` (the
/// legacy `MATIC` code is a separate listing).
pub fn native_code(network: NetworkId) -> Option<&'static str> {
    Some(match network {
        NetworkId::Bitcoin => "BTC",
        NetworkId::Ethereum | NetworkId::Base | NetworkId::Arbitrum | NetworkId::Optimism => "ETH",
        NetworkId::Polygon => "POL",
        NetworkId::Bsc => "BNB",
        NetworkId::Solana => "SOL",
        NetworkId::Tron => "TRX",
        NetworkId::Ton => "TONCOIN",
    })
}

/// `(ratio - 1) * 100`, exact.
pub fn ratio_to_percent(ratio: &Dec) -> Dec {
    (ratio - Dec::one()) * Dec::from(100)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quote {
    pub code: String,
    pub rate_usd: Dec,
    pub change_24h_percent: Option<Dec>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryPoint {
    /// Unix seconds.
    pub at: i64,
    pub rate_usd: Dec,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Credits {
    pub remaining: u64,
    pub limit: u64,
}

#[derive(Debug, Deserialize)]
struct MapItem {
    code: String,
    #[serde(default, deserialize_with = "de_dec")]
    rate: Option<Dec>,
    #[serde(default)]
    delta: Option<Delta>,
}

#[derive(Debug, Deserialize)]
struct Delta {
    #[serde(default, deserialize_with = "de_dec")]
    day: Option<Dec>,
}

#[derive(Debug, Deserialize)]
struct History {
    history: Vec<HistoryItem>,
}

#[derive(Debug, Deserialize)]
struct HistoryItem {
    date: i64,
    #[serde(default, deserialize_with = "de_dec")]
    rate: Option<Dec>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreditsBody {
    daily_credits_remaining: u64,
    daily_credits_limit: u64,
}

pub struct LiveCoinWatch {
    http: HttpClient,
    base: Url,
}

impl LiveCoinWatch {
    pub fn new(base: &str, api_key: &str, budget: Arc<Budget>) -> Result<Self, ProviderError> {
        Self::with_config(base, api_key, budget, Self::default_config())
    }

    pub fn default_config() -> HttpConfig {
        HttpConfig {
            min_interval: Duration::from_millis(250),
            ..HttpConfig::default()
        }
    }

    pub fn with_config(
        base: &str,
        api_key: &str,
        budget: Arc<Budget>,
        config: HttpConfig,
    ) -> Result<Self, ProviderError> {
        if api_key.trim().is_empty() {
            return Err(ProviderError::MissingKey { provider: PROVIDER });
        }
        let mut headers = HeaderMap::new();
        let mut key = HeaderValue::from_str(api_key.trim())
            .map_err(|_| ProviderError::MissingKey { provider: PROVIDER })?;
        key.set_sensitive(true);
        headers.insert("x-api-key", key);
        Ok(LiveCoinWatch {
            http: HttpClient::new(PROVIDER, config, budget, headers)?,
            base: parse_base(base)?,
        })
    }

    pub fn http(&self) -> &HttpClient {
        &self.http
    }

    fn url(&self, path: &str) -> Url {
        self.base.join(path).expect("static path")
    }

    /// USD quotes for the given codes. Codes LCW does not know are simply
    /// absent from the result; the caller reports them as unpriced.
    pub async fn quotes(&self, codes: &[&str]) -> Result<Vec<Quote>, ProviderError> {
        let mut out = Vec::new();
        for chunk in codes.chunks(MAP_LIMIT) {
            let body = serde_json::json!({
                "codes": chunk,
                "currency": "USD",
                "sort": "rank",
                "order": "ascending",
                "offset": 0,
                "limit": 0,
                "meta": false,
            });
            let resp = self
                .http
                .post_json("coins/map", self.url("coins/map"), &body)
                .await?;
            let items: Vec<MapItem> = resp.json(PROVIDER, "coins/map")?;
            for item in items {
                if !chunk.contains(&item.code.as_str()) {
                    continue; // never accept a code that was not asked for
                }
                let Some(rate) = item.rate else { continue };
                out.push(Quote {
                    code: item.code,
                    rate_usd: rate,
                    change_24h_percent: item
                        .delta
                        .and_then(|d| d.day)
                        .map(|r| ratio_to_percent(&r)),
                });
            }
        }
        Ok(out)
    }

    /// Historical USD rates between two Unix-second instants, ascending.
    pub async fn history(
        &self,
        code: &str,
        start: i64,
        end: i64,
    ) -> Result<Vec<HistoryPoint>, ProviderError> {
        let body = serde_json::json!({
            "currency": "USD",
            "code": code,
            "start": start.saturating_mul(1000),
            "end": end.saturating_mul(1000),
            "meta": false,
        });
        let resp = self
            .http
            .post_json(
                "coins/single/history",
                self.url("coins/single/history"),
                &body,
            )
            .await?;
        let history: History = resp.json(PROVIDER, "coins/single/history")?;
        let mut points = Vec::with_capacity(history.history.len());
        for item in history.history {
            // Millisecond timestamps within the requested range.
            if item.date < start.saturating_mul(1000) - 86_400_000
                || item.date > end.saturating_mul(1000) + 86_400_000
            {
                return Err(ProviderError::InvalidResponse {
                    provider: PROVIDER,
                    endpoint: "coins/single/history",
                    detail: "timestamp outside the requested range".into(),
                });
            }
            if let Some(rate) = item.rate {
                points.push(HistoryPoint {
                    at: item.date.div_euclid(1000),
                    rate_usd: rate,
                });
            }
        }
        points.sort_by_key(|p| p.at);
        points.dedup_by_key(|p| p.at);
        Ok(points)
    }

    pub async fn credits(&self) -> Result<Credits, ProviderError> {
        let resp = self
            .http
            .post_json("credits", self.url("credits"), &serde_json::json!({}))
            .await?;
        let c: CreditsBody = resp.json(PROVIDER, "credits")?;
        Ok(Credits {
            remaining: c.daily_credits_remaining,
            limit: c.daily_credits_limit,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use portfolio_core::decimal::{parse_dec, to_canonical};

    #[test]
    fn day_ratio_becomes_percent() {
        let p = |s: &str| to_canonical(&ratio_to_percent(&parse_dec(s).unwrap()));
        assert_eq!(p("1.05"), "5");
        assert_eq!(p("0.8"), "-20");
        assert_eq!(p("1"), "0");
        assert_eq!(p("1.0051"), "0.51");
    }
}
