//! Contract-address prices through DefiLlama (API_PROVIDERS.md §3.3).
//!
//! Endpoint change recorded on 2026-10-04: the specification lists
//! `https://api.llama.fi` as the free base, but the price routes
//! `/prices/current/{coins}` and `/prices/historical/{ts}/{coins}` return 404
//! there and are served by the official free coins host
//! `https://coins.llama.fi`. No key is used; the Pro API is not involved.
//! A token missing from the response is unpriced, never zero.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use portfolio_core::address::ton_to_friendly;
use portfolio_core::decimal::Dec;
use portfolio_core::network::NetworkId;
use reqwest::header::HeaderMap;
use serde::Deserialize;
use url::Url;

use crate::error::ProviderError;
use crate::esplora::parse_base;
use crate::http::{Budget, HttpClient, HttpConfig};
use crate::util::de_dec;

pub const PROVIDER: &str = "defillama";
pub const DEFAULT_BASE: &str = "https://coins.llama.fi/";
/// Coins per request, keeping URLs comfortably short.
const BATCH: usize = 40;
/// Quotes below this confidence are stored as `low_confidence`.
pub const MIN_CONFIDENCE: &str = "0.9";
/// Data points the chart route returns at most per request (coins x span).
pub const MAX_CHART_POINTS: i64 = 500;

/// DefiLlama chain prefix for a network.
pub fn chain_prefix(network: NetworkId) -> Option<&'static str> {
    Some(match network {
        NetworkId::Ethereum => "ethereum",
        NetworkId::Base => "base",
        NetworkId::Arbitrum => "arbitrum",
        NetworkId::Optimism => "optimism",
        NetworkId::Polygon => "polygon",
        NetworkId::Bsc => "bsc",
        NetworkId::Solana => "solana",
        NetworkId::Tron => "tron",
        NetworkId::Ton => "ton",
        _ => return None,
    })
}

/// `chain:contract` identity used by the coins API. Solana mints and TRON
/// contracts are case-sensitive and passed exactly; TON Jetton masters are
/// stored raw and sent in the bounceable user-friendly form the service keys
/// its Jetton prices by.
pub fn coin_id(network: NetworkId, contract: &str) -> Option<String> {
    let identity = match network {
        NetworkId::Ton => ton_to_friendly(contract, true).ok()?,
        _ => contract.to_owned(),
    };
    Some(format!("{}:{identity}", chain_prefix(network)?))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlamaQuote {
    pub price_usd: Dec,
    pub timestamp: i64,
    pub confidence: Option<Dec>,
    pub symbol: Option<String>,
    pub decimals: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct Coins {
    coins: BTreeMap<String, CoinItem>,
}

#[derive(Debug, Deserialize)]
struct CoinItem {
    #[serde(default, deserialize_with = "de_dec")]
    price: Option<Dec>,
    timestamp: i64,
    #[serde(default, deserialize_with = "de_dec")]
    confidence: Option<Dec>,
    symbol: Option<String>,
    decimals: Option<u32>,
}

/// A daily (or coarser) historical series for one coin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlamaSeries {
    pub confidence: Option<Dec>,
    /// Ascending `(unix seconds, USD price)`.
    pub points: Vec<(i64, Dec)>,
}

#[derive(Debug, Deserialize)]
struct ChartBody {
    coins: BTreeMap<String, ChartCoin>,
}

#[derive(Debug, Deserialize)]
struct ChartCoin {
    #[serde(default, deserialize_with = "de_dec")]
    confidence: Option<Dec>,
    #[serde(default)]
    prices: Vec<ChartPoint>,
}

#[derive(Debug, Deserialize)]
struct ChartPoint {
    timestamp: i64,
    #[serde(default, deserialize_with = "de_dec")]
    price: Option<Dec>,
}

pub struct DefiLlama {
    http: HttpClient,
    base: Url,
}

impl DefiLlama {
    pub fn new(base: &str, budget: Arc<Budget>) -> Result<Self, ProviderError> {
        Self::with_config(base, budget, Self::default_config())
    }

    pub fn default_config() -> HttpConfig {
        HttpConfig {
            min_interval: Duration::from_millis(1000),
            ..HttpConfig::default()
        }
    }

    pub fn with_config(
        base: &str,
        budget: Arc<Budget>,
        config: HttpConfig,
    ) -> Result<Self, ProviderError> {
        Ok(DefiLlama {
            http: HttpClient::new(PROVIDER, config, budget, HeaderMap::new())?,
            base: parse_base(base)?,
        })
    }

    pub fn http(&self) -> &HttpClient {
        &self.http
    }

    async fn fetch(
        &self,
        endpoint: &'static str,
        path_prefix: &str,
        ids: &[String],
    ) -> Result<BTreeMap<String, LlamaQuote>, ProviderError> {
        let mut out = BTreeMap::new();
        for chunk in ids.chunks(BATCH) {
            let path = format!("{path_prefix}{}", chunk.join(","));
            let url = self
                .base
                .join(&path)
                .map_err(|_| ProviderError::InvalidResponse {
                    provider: PROVIDER,
                    endpoint,
                    detail: "invalid coin id".into(),
                })?;
            let body = self.http.get(endpoint, url).await?;
            let coins: Coins = body.json(PROVIDER, endpoint)?;
            for (id, item) in coins.coins {
                // Keyed by the identity exactly as requested; the service may
                // echo EVM addresses in another letter case.
                let Some(requested) = chunk
                    .iter()
                    .find(|c| **c == id)
                    .or_else(|| chunk.iter().find(|c| equivalent_coin(c, &id)))
                else {
                    continue;
                };
                let Some(price) = item.price else { continue };
                out.insert(
                    requested.clone(),
                    LlamaQuote {
                        price_usd: price,
                        timestamp: item.timestamp,
                        confidence: item.confidence,
                        symbol: item.symbol,
                        decimals: item.decimals,
                    },
                );
            }
        }
        Ok(out)
    }

    /// Current prices keyed by the requested coin id.
    pub async fn current(
        &self,
        ids: &[String],
    ) -> Result<BTreeMap<String, LlamaQuote>, ProviderError> {
        self.fetch("prices/current", "prices/current/", ids).await
    }

    /// Daily series for one coin starting at `start` with `span` points
    /// (`/chart/{coin}?start=&span=&period=1d`). `None` when the service has
    /// no series for the coin. Points outside the requested window are rejected.
    pub async fn daily_chart(
        &self,
        coin: &str,
        start: i64,
        span: i64,
    ) -> Result<Option<LlamaSeries>, ProviderError> {
        const ENDPOINT: &str = "chart";
        let span = span.clamp(1, MAX_CHART_POINTS);
        let mut url = self.base.join(&format!("chart/{coin}")).map_err(|_| {
            ProviderError::InvalidResponse {
                provider: PROVIDER,
                endpoint: ENDPOINT,
                detail: "invalid coin id".into(),
            }
        })?;
        url.query_pairs_mut()
            .append_pair("start", &start.to_string())
            .append_pair("span", &span.to_string())
            .append_pair("period", "1d");
        let body = self.http.get(ENDPOINT, url).await?;
        let chart: ChartBody = body.json(PROVIDER, ENDPOINT)?;
        let Some((_, coin_data)) = chart
            .coins
            .into_iter()
            .find(|(id, _)| equivalent_coin(id, coin))
        else {
            return Ok(None);
        };
        let end = start + span * 86_400;
        let mut points = Vec::with_capacity(coin_data.prices.len());
        for p in coin_data.prices {
            if p.timestamp < start - 86_400 || p.timestamp > end + 86_400 {
                return Err(ProviderError::InvalidResponse {
                    provider: PROVIDER,
                    endpoint: ENDPOINT,
                    detail: "timestamp outside the requested range".into(),
                });
            }
            if let Some(price) = p.price.filter(|p| !bigdecimal::Signed::is_negative(p)) {
                points.push((p.timestamp, price));
            }
        }
        points.sort_by_key(|(t, _)| *t);
        points.dedup_by_key(|(t, _)| *t);
        Ok(Some(LlamaSeries {
            confidence: coin_data.confidence,
            points,
        }))
    }

    /// Prices nearest to `at` (Unix seconds) keyed by the requested coin id.
    pub async fn historical(
        &self,
        at: i64,
        ids: &[String],
    ) -> Result<BTreeMap<String, LlamaQuote>, ProviderError> {
        self.fetch(
            "prices/historical",
            &format!("prices/historical/{at}/"),
            ids,
        )
        .await
    }
}

/// Case folding is only valid for hexadecimal EVM contract identities.
fn equivalent_coin(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let Some((chain, _)) = a.split_once(':') else {
        return false;
    };
    matches!(
        chain,
        "ethereum" | "base" | "arbitrum" | "optimism" | "polygon" | "bsc"
    ) && a.eq_ignore_ascii_case(b)
}

#[cfg(test)]
mod identity_tests {
    use super::*;
    #[test]
    fn price_identity_keeps_base58_case() {
        assert!(!equivalent_coin("solana:AbC", "solana:abc"));
        assert!(!equivalent_coin("tron:TAbC", "tron:Tabc"));
        assert!(!equivalent_coin("ton:EQAbC", "ton:EQabc"));
        assert!(equivalent_coin("ethereum:0xAbC", "ethereum:0xabc"));
    }
}
