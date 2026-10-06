//! Bitcoin through the public Blockstream Esplora API (API_PROVIDERS.md §3.4).
//!
//! Routes: `/address/{a}`, `/address/{a}/txs/chain[/{last_seen_txid}]` (25
//! confirmed transactions per page, newest first), `/address/{a}/txs/mempool`
//! (capped at 50), `/tx/{txid}`, `/blocks/tip/height`. Amounts are satoshis.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use num_bigint::BigInt;
use portfolio_core::network::NetworkId;
use portfolio_store::ingest::{
    AssetSpec, Decoding, Direction, FeeAttribution, FeeSpec, LegSpec, TxSpec, TxStatus,
};
use reqwest::header::HeaderMap;
use serde::Deserialize;
use url::Url;

use crate::error::ProviderError;
use crate::http::{Budget, HttpClient, HttpConfig};

pub const PROVIDER: &str = "esplora";
pub const DEFAULT_BASE: &str = "https://blockstream.info/api/";
/// Confirmed transactions per `/txs/chain` page (documented).
pub const CHAIN_PAGE_SIZE: usize = 25;
/// Documented cap of the mempool listing; a full list may hide records.
pub const MEMPOOL_CAP: usize = 50;

#[derive(Debug, Clone, Deserialize)]
pub struct AddressInfo {
    pub address: String,
    pub chain_stats: Stats,
    pub mempool_stats: Stats,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Stats {
    pub funded_txo_sum: u64,
    pub spent_txo_sum: u64,
    pub tx_count: u64,
}

impl AddressInfo {
    /// Confirmed balance in satoshis.
    pub fn confirmed_balance(&self) -> Result<u64, ProviderError> {
        self.chain_stats
            .funded_txo_sum
            .checked_sub(self.chain_stats.spent_txo_sum)
            .ok_or(ProviderError::InvalidResponse {
                provider: PROVIDER,
                endpoint: "address",
                detail: "spent exceeds funded".into(),
            })
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Tx {
    pub txid: String,
    pub vin: Vec<Vin>,
    pub vout: Vec<Vout>,
    #[serde(default)]
    pub fee: u64,
    pub status: TxState,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Vin {
    pub txid: String,
    pub vout: u32,
    pub prevout: Option<Vout>,
    #[serde(default)]
    pub is_coinbase: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Vout {
    pub scriptpubkey_address: Option<String>,
    pub value: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TxState {
    pub confirmed: bool,
    pub block_height: Option<i64>,
    pub block_hash: Option<String>,
    pub block_time: Option<i64>,
}

pub struct Esplora {
    http: HttpClient,
    base: Url,
}

impl Esplora {
    pub fn new(base: &str, budget: Arc<Budget>) -> Result<Self, ProviderError> {
        Self::with_config(base, budget, Self::default_config())
    }

    /// Public service without a guaranteed quota: one request per second.
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
        Self::with_provider(PROVIDER, base, budget, config)
    }

    pub fn with_provider(
        provider: &'static str,
        base: &str,
        budget: Arc<Budget>,
        config: HttpConfig,
    ) -> Result<Self, ProviderError> {
        Ok(Esplora {
            http: HttpClient::new(provider, config, budget, HeaderMap::new())?,
            base: parse_base(base)?,
        })
    }

    pub fn http(&self) -> &HttpClient {
        &self.http
    }

    fn url(&self, path: &str) -> Result<Url, ProviderError> {
        self.base
            .join(path)
            .map_err(|_| ProviderError::InvalidResponse {
                provider: PROVIDER,
                endpoint: "url",
                detail: "invalid path".into(),
            })
    }

    pub async fn address(&self, address: &str) -> Result<AddressInfo, ProviderError> {
        let body = self
            .http
            .get("address", self.url(&format!("address/{address}"))?)
            .await?;
        body.json(PROVIDER, "address")
    }

    /// One page of confirmed history, newest first, continuing after `last_seen`.
    pub async fn chain_txs(
        &self,
        address: &str,
        last_seen: Option<&str>,
    ) -> Result<Vec<Tx>, ProviderError> {
        let path = match last_seen {
            Some(txid) => format!("address/{address}/txs/chain/{txid}"),
            None => format!("address/{address}/txs/chain"),
        };
        let body = self.http.get("txs/chain", self.url(&path)?).await?;
        let txs: Vec<Tx> = body.json(PROVIDER, "txs/chain")?;
        if txs.len() > CHAIN_PAGE_SIZE {
            return Err(ProviderError::InvalidResponse {
                provider: PROVIDER,
                endpoint: "txs/chain",
                detail: format!("page larger than {CHAIN_PAGE_SIZE}"),
            });
        }
        Ok(txs)
    }

    pub async fn mempool_txs(&self, address: &str) -> Result<Vec<Tx>, ProviderError> {
        let body = self
            .http
            .get(
                "txs/mempool",
                self.url(&format!("address/{address}/txs/mempool"))?,
            )
            .await?;
        body.json(PROVIDER, "txs/mempool")
    }

    /// A transaction by ID; `None` when the service no longer knows it
    /// (dropped or replaced while unconfirmed).
    pub async fn tx(&self, txid: &str) -> Result<Option<Tx>, ProviderError> {
        match self.http.get("tx", self.url(&format!("tx/{txid}"))?).await {
            Ok(body) => body.json(PROVIDER, "tx").map(Some),
            Err(ProviderError::NotFound { .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub async fn tip_height(&self) -> Result<i64, ProviderError> {
        let body = self
            .http
            .get("blocks/tip/height", self.url("blocks/tip/height")?)
            .await?;
        std::str::from_utf8(&body.bytes)
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .ok_or(ProviderError::InvalidResponse {
                provider: PROVIDER,
                endpoint: "blocks/tip/height",
                detail: "not an integer".into(),
            })
    }
}

pub(crate) fn parse_base(base: &str) -> Result<Url, ProviderError> {
    let mut url = Url::parse(base).map_err(|_| ProviderError::InvalidResponse {
        provider: "config",
        endpoint: "base",
        detail: "invalid base URL".into(),
    })?;
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }
    Ok(url)
}

/// Per-address input/output sums of one transaction.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct Flow {
    spent: u64,
    received: u64,
}

/// Normalizes a Bitcoin transaction from the point of view of `account`.
///
/// `owned` is the union of every tracked Bitcoin address. The account's leg is
/// its net movement excluding the fee; the fee is attributed by who funded the
/// inputs:
/// - all inputs from this account: `exact`, the whole fee;
/// - all inputs from owned addresses, several of them: `shared`, pro rata by
///   input value, the integer remainder to the largest contributor;
/// - some inputs from addresses not tracked: `unknown`, pro rata by input
///   value (the true payer cannot be determined).
///
/// Returns `None` when the transaction does not touch the account.
pub fn tx_for_account(
    tx: &Tx,
    account: &str,
    owned: &BTreeSet<String>,
    observed_at: i64,
) -> Option<TxSpec> {
    let mut flows: BTreeMap<&str, Flow> = BTreeMap::new();
    let mut total_in: u128 = 0;
    let mut missing_prevout = false;
    let coinbase = tx.vin.iter().any(|v| v.is_coinbase);
    for vin in &tx.vin {
        if vin.is_coinbase {
            continue;
        }
        let Some(prev) = &vin.prevout else {
            missing_prevout = true;
            continue;
        };
        total_in += u128::from(prev.value);
        if let Some(addr) = prev.scriptpubkey_address.as_deref() {
            flows.entry(addr).or_default().spent += prev.value;
        }
    }
    for out in &tx.vout {
        if let Some(addr) = out.scriptpubkey_address.as_deref() {
            flows.entry(addr).or_default().received += out.value;
        }
    }
    let mine = flows.get(account)?.clone();

    let owned_inputs: Vec<(&str, u64)> = flows
        .iter()
        .filter(|(a, f)| f.spent > 0 && owned.contains(**a))
        .map(|(a, f)| (*a, f.spent))
        .collect();
    let owned_in: u128 = owned_inputs.iter().map(|(_, v)| u128::from(*v)).sum();
    let fee = u128::from(tx.fee);

    let fee_share = if mine.spent == 0 || fee == 0 || total_in == 0 {
        None
    } else if u128::from(mine.spent) == total_in && !missing_prevout {
        Some((fee, FeeAttribution::Exact))
    } else {
        let base = fee * u128::from(mine.spent) / total_in;
        if owned_in == total_in && !missing_prevout {
            // Deterministic remainder: largest owned contributor, then address order.
            let allocated: u128 = owned_inputs
                .iter()
                .map(|(_, v)| fee * u128::from(*v) / total_in)
                .sum();
            let largest = owned_inputs
                .iter()
                .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(a.0)))
                .map(|(a, _)| *a);
            let share = if largest == Some(account) {
                base + (fee - allocated)
            } else {
                base
            };
            Some((share, FeeAttribution::Shared))
        } else {
            Some((base, FeeAttribution::Unknown))
        }
    };

    let fee_raw = fee_share.map(|(v, _)| v).unwrap_or(0);
    // Net change of the account = received - spent = principal - fee share.
    let principal: BigInt =
        BigInt::from(mine.received) - BigInt::from(mine.spent) + BigInt::from(fee_raw);
    let zero = BigInt::from(0);
    let (direction, leg_type, operation) = if coinbase {
        (Direction::In, "coinbase", "receive")
    } else if principal > zero {
        (Direction::In, "receive", "receive")
    } else if principal < zero {
        (Direction::Out, "send", "send")
    } else {
        (Direction::SelfTransfer, "self", "self")
    };
    let decoding = if missing_prevout {
        Decoding::Partial
    } else {
        Decoding::Interpreted
    };
    let native = AssetSpec::native(NetworkId::Bitcoin, PROVIDER);
    let senders: BTreeSet<&str> = tx
        .vin
        .iter()
        .filter_map(|v| v.prevout.as_ref()?.scriptpubkey_address.as_deref())
        .collect();
    let recipients: BTreeSet<&str> = tx
        .vout
        .iter()
        .filter_map(|v| v.scriptpubkey_address.as_deref())
        .filter(|a| !senders.contains(a))
        .collect();
    let counterparty = if !missing_prevout && senders.len() == 1 {
        if direction == Direction::In {
            senders.first().map(|a| (*a).to_owned())
        } else if recipients.len() == 1 {
            recipients.first().map(|a| (*a).to_owned())
        } else {
            None
        }
    } else {
        None
    };
    let legs = vec![LegSpec {
        counterparty,
        asset: native.clone(),
        signed_raw: principal,
        direction,
        leg_type: leg_type.to_owned(),
        decoding,
        // Basis of a receipt from outside is unknown until classified (Stage C).
        unresolved: direction == Direction::In || missing_prevout,
    }];
    let fee = fee_share
        .filter(|(v, _)| *v > 0)
        .map(|(v, attribution)| FeeSpec {
            asset: native,
            raw: BigInt::from(v),
            attribution,
        });
    Some(TxSpec {
        network: NetworkId::Bitcoin,
        part: None,
        hash: tx.txid.to_ascii_lowercase(),
        block_height: tx.status.block_height.filter(|_| tx.status.confirmed),
        position: None,
        occurred_at: tx
            .status
            .block_time
            .filter(|_| tx.status.confirmed)
            .unwrap_or(observed_at),
        status: if tx.status.confirmed {
            TxStatus::Confirmed
        } else {
            TxStatus::Pending
        },
        provider: PROVIDER,
        operation: operation.to_owned(),
        legs,
        fee,
        decoding,
        evidence: serde_json::json!({
            "provider": PROVIDER,
            "txid": tx.txid,
            "block_hash": tx.status.block_hash,
        }),
    })
}
