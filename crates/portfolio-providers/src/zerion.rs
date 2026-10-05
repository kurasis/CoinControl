//! EVM account data through the Zerion API (API_PROVIDERS.md §3.2).
//!
//! Routes: `GET /wallets/{address}/positions/` with `filter[positions]=only_simple`
//! and `GET /wallets/{address}/transactions/` with `page[after]` continuation.
//! Both are scoped by `filter[chain_ids]` so results from other networks never
//! leak into an account; every item's chain is verified again on receipt.
//! Authentication: HTTP Basic with the key as username and an empty password.

use std::sync::Arc;
use std::time::Duration;

use num_bigint::BigInt;
use portfolio_core::clock::parse_rfc3339;
use portfolio_core::decimal::parse_raw_amount;
use portfolio_core::network::{NetworkFamily, NetworkId};
use portfolio_store::ingest::{
    AssetSpec, Decoding, Direction, FeeAttribution, FeeSpec, LegSpec, TxSpec, TxStatus,
    Verification,
};
use reqwest::StatusCode;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use serde::Deserialize;
use url::Url;

use crate::error::ProviderError;
use crate::esplora::parse_base;
use crate::http::{Budget, HttpClient, HttpConfig};

pub const PROVIDER: &str = "zerion";
pub const DEFAULT_BASE: &str = "https://api.zerion.io/v1/";
/// Largest documented transactions page.
pub const MAX_PAGE_SIZE: u32 = 100;
/// Version of the transaction query; a different fingerprint resets a stored cursor.
const QUERY_VERSION: &str = "v1;trash=no_filter";

/// Contract under which Zerion reports Polygon's native POL (the chain's
/// native-token system contract). It is the native asset, not a token.
const POLYGON_NATIVE_CONTRACT: &str = "0x0000000000000000000000000000000000001010";

/// Zerion's chain identifier for a supported network.
pub fn chain_id(network: NetworkId) -> Option<&'static str> {
    Some(match network {
        NetworkId::Ethereum => "ethereum",
        NetworkId::Base => "base",
        NetworkId::Arbitrum => "arbitrum",
        NetworkId::Optimism => "optimism",
        NetworkId::Polygon => "polygon",
        NetworkId::Bsc => "binance-smart-chain",
        NetworkId::Solana => "solana",
        _ => return None,
    })
}

#[derive(Debug, Deserialize)]
struct Document<T> {
    data: T,
    #[serde(default)]
    links: Links,
}

#[derive(Debug, Default, Deserialize)]
struct Links {
    next: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Quantity {
    pub int: String,
    pub decimals: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FungibleInfo {
    pub name: Option<String>,
    pub symbol: Option<String>,
    #[serde(default)]
    pub flags: FungibleFlags,
    #[serde(default)]
    pub implementations: Vec<Implementation>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct FungibleFlags {
    #[serde(default)]
    pub verified: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Implementation {
    pub chain_id: String,
    pub address: Option<String>,
    pub decimals: u32,
}

#[derive(Debug, Clone, Deserialize)]
struct Relationships {
    chain: Option<Related>,
}

#[derive(Debug, Clone, Deserialize)]
struct Related {
    data: Option<IdRef>,
}

#[derive(Debug, Clone, Deserialize)]
struct IdRef {
    id: String,
}

impl Relationships {
    fn chain(&self) -> Option<&str> {
        self.chain.as_ref()?.data.as_ref().map(|d| d.id.as_str())
    }
}

#[derive(Debug, Clone, Deserialize)]
struct PositionItem {
    attributes: PositionAttributes,
    relationships: Relationships,
}

#[derive(Debug, Clone, Deserialize)]
struct PositionAttributes {
    position_type: String,
    quantity: Quantity,
    fungible_info: Option<FungibleInfo>,
    #[serde(default)]
    flags: TrashFlags,
    updated_at_block: Option<i64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct TrashFlags {
    #[serde(default)]
    is_trash: bool,
}

/// One wallet holding on the requested network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Position {
    pub asset: AssetSpec,
    pub raw: BigInt,
    pub block: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Positions {
    Ready(Vec<Position>),
    /// The address is being indexed; this is "loading", not "empty".
    Indexing,
}

#[derive(Debug, Clone, Deserialize)]
struct TxItem {
    attributes: TxAttributes,
    relationships: Relationships,
}

#[derive(Debug, Clone, Deserialize)]
struct TxAttributes {
    operation_type: String,
    hash: String,
    mined_at_block: Option<i64>,
    mined_at: String,
    sent_from: Option<String>,
    status: String,
    fee: Option<TxFee>,
    #[serde(default)]
    transfers: Vec<Transfer>,
    #[serde(default)]
    approvals: Vec<serde_json::Value>,
    #[serde(default)]
    flags: TrashFlags,
}

#[derive(Debug, Clone, Deserialize)]
struct TxFee {
    fungible_info: Option<FungibleInfo>,
    quantity: Quantity,
}

#[derive(Debug, Clone, Deserialize)]
struct Transfer {
    sender: Option<String>,
    recipient: Option<String>,
    fungible_info: Option<FungibleInfo>,
    nft_info: Option<serde_json::Value>,
    direction: String,
    quantity: Quantity,
}

/// One page of normalized transactions.
#[derive(Debug, Clone)]
pub struct TxPage {
    pub txs: Vec<TxSpec>,
    pub next_cursor: Option<String>,
    pub indexing: bool,
}

/// Optional `mined_at` window in milliseconds (inclusive), for bounded queries.
#[derive(Debug, Clone, Copy, Default)]
pub struct Window {
    pub min_mined_at_ms: Option<i64>,
    pub max_mined_at_ms: Option<i64>,
}

pub struct Zerion {
    http: HttpClient,
    base: Url,
}

impl Zerion {
    pub fn new(base: &str, api_key: &str, budget: Arc<Budget>) -> Result<Self, ProviderError> {
        Self::with_config(base, api_key, budget, Self::default_config())
    }

    /// Documented free limit is 3 requests/second, but throttling was observed
    /// at about 2/s; one request per second leaves headroom for other tools.
    pub fn default_config() -> HttpConfig {
        HttpConfig {
            min_interval: Duration::from_millis(1000),
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
        let token = basic_auth(api_key.trim());
        let mut headers = HeaderMap::new();
        let mut value = HeaderValue::from_str(&token)
            .map_err(|_| ProviderError::MissingKey { provider: PROVIDER })?;
        value.set_sensitive(true);
        headers.insert(AUTHORIZATION, value);
        Ok(Zerion {
            http: HttpClient::new(PROVIDER, config, budget, headers)?,
            base: parse_base(base)?,
        })
    }

    pub fn http(&self) -> &HttpClient {
        &self.http
    }

    fn wallet_url(&self, address: &str, route: &str) -> Result<Url, ProviderError> {
        self.base
            .join(&format!("wallets/{address}/{route}/"))
            .map_err(|_| ProviderError::InvalidResponse {
                provider: PROVIDER,
                endpoint: "url",
                detail: "invalid address".into(),
            })
    }

    /// Simple wallet positions on one network.
    pub async fn positions(
        &self,
        network: NetworkId,
        address: &str,
    ) -> Result<Positions, ProviderError> {
        let chain = chain_id(network).ok_or(ProviderError::InvalidResponse {
            provider: PROVIDER,
            endpoint: "positions",
            detail: format!("network {} not supported", network.as_str()),
        })?;
        let mut url = self.wallet_url(address, "positions")?;
        url.query_pairs_mut()
            .append_pair("filter[positions]", "only_simple")
            .append_pair("currency", "usd")
            .append_pair("filter[chain_ids]", chain)
            .append_pair("filter[trash]", "no_filter")
            .append_pair("sort", "value");
        let body = self.http.get("positions", url).await?;
        if body.status == StatusCode::ACCEPTED {
            return Ok(Positions::Indexing);
        }
        let doc: Document<Vec<PositionItem>> = body.json(PROVIDER, "positions")?;
        let mut out = Vec::new();
        for item in doc.data {
            if item.relationships.chain() != Some(chain) {
                // Defensive partitioning: never count another network's holding.
                continue;
            }
            let a = item.attributes;
            if a.position_type != "wallet" {
                continue;
            }
            let Some(info) = a.fungible_info else {
                continue;
            };
            let Some(mut asset) = asset_from(network, chain, &info, a.quantity.decimals) else {
                return Err(invalid("positions", "position without an implementation"));
            };
            if a.flags.is_trash {
                asset.verification = Verification::Spam;
            }
            let raw = parse_raw_amount(&a.quantity.int)
                .map_err(|_| invalid("positions", "invalid quantity"))?;
            // Several positions of one asset (for example two Solana token
            // accounts of the same mint) are one holding.
            match out
                .iter_mut()
                .find(|p: &&mut Position| p.asset.id() == asset.id())
            {
                Some(existing) => {
                    existing.raw += raw;
                    existing.block = existing.block.max(a.updated_at_block);
                }
                None => out.push(Position {
                    asset,
                    raw,
                    block: a.updated_at_block,
                }),
            }
        }
        Ok(Positions::Ready(out))
    }

    /// Query fingerprint stored next to a cursor.
    pub fn fingerprint(network: NetworkId, page_size: u32) -> String {
        format!(
            "{QUERY_VERSION};chain={};size={page_size}",
            chain_id(network).unwrap_or("?")
        )
    }

    /// One page of transactions for `account` (canonical address), newest first.
    pub async fn transactions(
        &self,
        network: NetworkId,
        account: &str,
        cursor: Option<&str>,
        page_size: u32,
        window: Window,
    ) -> Result<TxPage, ProviderError> {
        let chain = chain_id(network).ok_or(invalid("transactions", "network not supported"))?;
        let mut url = self.wallet_url(account, "transactions")?;
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("currency", "usd")
                .append_pair("filter[chain_ids]", chain)
                .append_pair("filter[trash]", "no_filter")
                .append_pair("page[size]", &page_size.clamp(1, MAX_PAGE_SIZE).to_string());
            if let Some(min) = window.min_mined_at_ms {
                q.append_pair("filter[min_mined_at]", &min.to_string());
            }
            if let Some(max) = window.max_mined_at_ms {
                q.append_pair("filter[max_mined_at]", &max.to_string());
            }
            if let Some(c) = cursor {
                q.append_pair("page[after]", c);
            }
        }
        let body = self.http.get("transactions", url).await?;
        if body.status == StatusCode::ACCEPTED {
            return Ok(TxPage {
                txs: Vec::new(),
                next_cursor: None,
                indexing: true,
            });
        }
        let doc: Document<Vec<TxItem>> = body.json(PROVIDER, "transactions")?;
        let next_cursor = doc.links.next.as_deref().map(after_param).transpose()?;
        if next_cursor.is_some() && next_cursor.as_deref() == cursor {
            return Err(ProviderError::RepeatedCursor {
                provider: PROVIDER,
                endpoint: "transactions",
            });
        }
        let mut txs = Vec::with_capacity(doc.data.len());
        for item in doc.data {
            if item.relationships.chain() != Some(chain) {
                continue;
            }
            txs.push(normalize_tx(network, chain, account, item.attributes)?);
        }
        Ok(TxPage {
            txs,
            next_cursor,
            indexing: false,
        })
    }
}

fn invalid(endpoint: &'static str, detail: &str) -> ProviderError {
    ProviderError::InvalidResponse {
        provider: PROVIDER,
        endpoint,
        detail: detail.to_owned(),
    }
}

/// Extracts `page[after]` from a `links.next` URL. Only the opaque token is
/// kept, so a stored cursor is replayed with this build's own query parameters.
fn after_param(next: &str) -> Result<String, ProviderError> {
    let url = Url::parse(next).map_err(|_| invalid("transactions", "invalid next link"))?;
    url.query_pairs()
        .find(|(k, _)| k == "page[after]")
        .map(|(_, v)| v.into_owned())
        .ok_or_else(|| invalid("transactions", "next link without page[after]"))
}

fn basic_auth(key: &str) -> String {
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(format!("{key}:"));
    format!("Basic {encoded}")
}

/// The chain-specific asset identity of a Zerion fungible on `chain`.
fn asset_from(
    network: NetworkId,
    chain: &str,
    info: &FungibleInfo,
    quantity_decimals: u32,
) -> Option<AssetSpec> {
    let imp = info.implementations.iter().find(|i| i.chain_id == chain)?;
    let contract = imp
        .address
        .as_deref()
        .filter(|a| !a.is_empty())
        .map(|a| canonical_identifier(network, a))
        .filter(|a| !(network == NetworkId::Polygon && a == POLYGON_NATIVE_CONTRACT));
    let decimals = if contract.is_none() {
        network.native_decimals()
    } else {
        imp.decimals
    };
    if decimals != quantity_decimals {
        return None;
    }
    Some(AssetSpec {
        network,
        decimals,
        symbol: info.symbol.clone(),
        name: info.name.clone(),
        verification: if contract.is_none() || info.flags.verified {
            Verification::Verified
        } else {
            Verification::Unverified
        },
        contract,
        provider: PROVIDER,
    })
}

/// EVM identifiers are case-insensitive hex and stored lowercase; Solana
/// base58 identifiers are case-sensitive and kept exactly.
fn canonical_identifier(network: NetworkId, text: &str) -> String {
    match network.family() {
        NetworkFamily::Evm => text.to_ascii_lowercase(),
        _ => text.to_owned(),
    }
}

fn same_address(network: NetworkId, a: &str, b: &str) -> bool {
    match network.family() {
        NetworkFamily::Evm => a.eq_ignore_ascii_case(b),
        _ => a == b,
    }
}

fn normalize_tx(
    network: NetworkId,
    chain: &str,
    account: &str,
    a: TxAttributes,
) -> Result<TxSpec, ProviderError> {
    let occurred_at =
        parse_rfc3339(&a.mined_at).ok_or_else(|| invalid("transactions", "invalid mined_at"))?;
    let status = match a.status.as_str() {
        "confirmed" => TxStatus::Confirmed,
        "failed" => TxStatus::Failed,
        "pending" => TxStatus::Pending,
        _ => TxStatus::Pending,
    };
    let mut decoding = if status == TxStatus::Pending && a.status != "pending" {
        Decoding::Partial
    } else {
        Decoding::Interpreted
    };
    let mut legs = Vec::new();
    let mut skipped_nft = 0usize;
    // A reverted call moves no assets; only its fee is real.
    if status != TxStatus::Failed {
        for t in &a.transfers {
            let Some(info) = &t.fungible_info else {
                if t.nft_info.is_some() {
                    skipped_nft += 1;
                }
                decoding = Decoding::Partial;
                continue;
            };
            let Some(asset) = asset_from(network, chain, info, t.quantity.decimals) else {
                decoding = Decoding::Partial;
                continue;
            };
            let raw = parse_raw_amount(&t.quantity.int)
                .map_err(|_| invalid("transactions", "invalid transfer quantity"))?;
            let (signed, direction) = match t.direction.as_str() {
                "in" => (raw, Direction::In),
                "out" => (-raw, Direction::Out),
                "self" => (BigInt::from(0), Direction::SelfTransfer),
                _ => {
                    decoding = Decoding::Partial;
                    continue;
                }
            };
            legs.push(LegSpec {
                counterparty: (if direction == Direction::Out {
                    t.recipient.as_deref()
                } else {
                    t.sender.as_deref()
                })
                .and_then(|a| portfolio_core::address::normalize_address(network, a).ok())
                .map(|a| a.canonical),
                unresolved: direction == Direction::In,
                asset,
                signed_raw: signed,
                direction,
                leg_type: a.operation_type.clone(),
                decoding: Decoding::Interpreted,
            });
        }
    }
    // The fee belongs to this account only if it signed the transaction.
    let paid_by_account = a
        .sent_from
        .as_deref()
        .is_some_and(|s| same_address(network, s, account));
    let fee = match (&a.fee, paid_by_account) {
        (Some(fee), true) => {
            let asset = fee
                .fungible_info
                .as_ref()
                .and_then(|i| asset_from(network, chain, i, fee.quantity.decimals))
                .unwrap_or_else(|| AssetSpec::native(network, PROVIDER));
            let raw = parse_raw_amount(&fee.quantity.int)
                .map_err(|_| invalid("transactions", "invalid fee quantity"))?;
            (raw > BigInt::from(0)).then_some(FeeSpec {
                asset,
                raw,
                attribution: FeeAttribution::Exact,
            })
        }
        _ => None,
    };
    Ok(TxSpec {
        network,
        part: None,
        hash: canonical_identifier(network, &a.hash),
        block_height: a.mined_at_block,
        position: None,
        occurred_at,
        status,
        provider: PROVIDER,
        operation: a.operation_type,
        legs,
        fee,
        decoding,
        evidence: serde_json::json!({
            "provider": PROVIDER,
            "hash": a.hash,
            "status": a.status,
            "is_trash": a.flags.is_trash,
            "approvals": a.approvals.len(),
            "nft_transfers_skipped": skipped_nft,
            "sent_from": a.sent_from,
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_auth_encodes_key_with_empty_password() {
        assert_eq!(basic_auth("zk_dev_123"), "Basic emtfZGV2XzEyMzo=");
    }

    #[test]
    fn cursor_is_taken_from_next_link() {
        let next = "https://api.zerion.io/v1/wallets/0xabc/transactions/?currency=usd&page%5Bafter%5D=WyJ4Il0%3D&page%5Bsize%5D=5";
        assert_eq!(after_param(next).unwrap(), "WyJ4Il0=");
        assert!(after_param("https://x/?a=b").is_err());
    }
}
