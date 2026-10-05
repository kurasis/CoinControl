//! TRON account data through TronGrid (API_PROVIDERS.md §3.5).
//!
//! Routes (v1 API): `GET /v1/accounts/{address}` for TRX, staked TRX and
//! TRC-20 balances; `GET /v1/accounts/{address}/transactions` for native
//! activity; `GET /v1/accounts/{address}/transactions/trc20` for TRC-20
//! transfer events. Both history routes are newest first and continue with
//! `fingerprint`; only confirmed records are requested. Authentication is the
//! `TRON-PRO-API-KEY` header. Amounts are integers in SUN or token units.
//!
//! The two history categories describe the same transactions from different
//! angles: a TRC-20 send is a `TriggerSmartContract` (with the fee) in the
//! native list and a `Transfer` event (with the token amount) in the TRC-20
//! list. Native records are ingested as the main view; every TRC-20 event is a
//! separate component (`TxSpec::part`), so the two add up instead of
//! replacing each other.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use num_bigint::BigInt;
use portfolio_core::address::{normalize_address, tron_to_hex};
use portfolio_core::decimal::parse_raw_amount;
use portfolio_core::network::NetworkId;
use portfolio_store::ingest::{
    AssetSpec, Decoding, Direction, FeeAttribution, FeeSpec, LegSpec, TxSpec, TxStatus,
    Verification,
};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::Deserialize;
use serde_json::Value;
use url::Url;

use crate::error::ProviderError;
use crate::esplora::parse_base;
use crate::http::{Budget, HttpClient, HttpConfig};

pub const PROVIDER: &str = "trongrid";
pub const DEFAULT_BASE: &str = "https://api.trongrid.io/";
/// Records per history page (the documented maximum is 200).
pub const PAGE_SIZE: u32 = 50;
/// Version of the history queries; a different fingerprint resets stored cursors.
const QUERY_VERSION: &str = "v1;confirmed";
/// Function selector of TRC-20 `transfer(address,uint256)`.
const TRANSFER_SELECTOR: &str = "a9059cbb";

#[derive(Debug, Deserialize)]
struct Envelope<T> {
    #[serde(default = "Vec::new")]
    data: Vec<T>,
    #[serde(default)]
    success: Option<bool>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    meta: Meta,
}

#[derive(Debug, Default, Deserialize)]
struct Meta {
    fingerprint: Option<String>,
    #[serde(default)]
    links: Option<MetaLinks>,
}

#[derive(Debug, Default, Deserialize)]
struct MetaLinks {
    next: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct AccountItem {
    #[serde(default)]
    balance: Option<i64>,
    #[serde(default, rename = "frozenV2")]
    frozen_v2: Vec<FrozenV2>,
    #[serde(default, rename = "unfrozenV2")]
    unfrozen_v2: Vec<UnfrozenV2>,
    #[serde(default, rename = "delegated_frozenV2_balance_for_bandwidth")]
    delegated_bandwidth: Option<i64>,
    #[serde(default)]
    account_resource: Option<AccountResource>,
    #[serde(default)]
    trc20: Vec<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct FrozenV2 {
    #[serde(default)]
    amount: Option<i64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct UnfrozenV2 {
    #[serde(default)]
    unfreeze_amount: Option<i64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct AccountResource {
    #[serde(default, rename = "delegated_frozenV2_balance_for_energy")]
    delegated_energy: Option<i64>,
}

/// Current holdings of one TRON account.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TronAccount {
    /// The account exists on chain (has been activated).
    pub exists: bool,
    /// Spendable TRX in SUN.
    pub liquid_sun: BigInt,
    /// TRX frozen for resources (Stake 2.0), delegated to others, or waiting
    /// out the unfreezing period. Still owned, so part of the holding.
    pub staked_sun: BigInt,
    /// TRC-20 balances by Base58 contract address, in token units.
    pub trc20: Vec<(String, BigInt)>,
}

impl TronAccount {
    pub fn total_trx(&self) -> BigInt {
        &self.liquid_sun + &self.staked_sun
    }
}

/// A native history record (only the fields the normalizer reads).
#[derive(Debug, Clone, Deserialize)]
pub struct NativeTx {
    #[serde(rename = "txID")]
    pub tx_id: String,
    #[serde(default, rename = "blockNumber")]
    pub block_number: Option<i64>,
    pub block_timestamp: i64,
    #[serde(default)]
    pub ret: Vec<TxRet>,
    #[serde(default)]
    pub net_fee: Option<i64>,
    #[serde(default)]
    pub energy_fee: Option<i64>,
    #[serde(default)]
    pub withdraw_amount: Option<i64>,
    pub raw_data: RawData,
    #[serde(default)]
    pub internal_transactions: Vec<InternalTx>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TxRet {
    #[serde(default, rename = "contractRet")]
    pub contract_ret: Option<String>,
    #[serde(default)]
    pub fee: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawData {
    #[serde(default)]
    pub contract: Vec<Contract>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Contract {
    #[serde(rename = "type")]
    pub kind: String,
    pub parameter: Parameter,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Parameter {
    #[serde(default)]
    pub value: Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InternalTx {
    #[serde(default)]
    pub from_address: Option<String>,
    #[serde(default)]
    pub to_address: Option<String>,
    #[serde(default)]
    pub data: Value,
}

/// One TRC-20 `Transfer` event touching the account.
#[derive(Debug, Clone, Deserialize)]
pub struct Trc20Transfer {
    pub transaction_id: String,
    pub token_info: TokenInfo,
    pub block_timestamp: i64,
    pub from: String,
    pub to: String,
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    pub value: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TokenInfo {
    pub symbol: Option<String>,
    pub address: String,
    pub decimals: u32,
    pub name: Option<String>,
}

impl TokenInfo {
    pub fn asset(&self) -> AssetSpec {
        AssetSpec {
            network: NetworkId::Tron,
            contract: Some(self.address.clone()),
            decimals: self.decimals,
            symbol: self.symbol.clone(),
            name: self.name.clone(),
            // TronGrid publishes no verification verdict; fake tokens that
            // imitate well-known symbols are common, so nothing is trusted by
            // name. Prices come only from contract identity.
            verification: Verification::Unverified,
            provider: PROVIDER,
        }
    }
}

/// A raw page and the continuation for the next (older) page.
#[derive(Debug, Clone)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next: Option<String>,
}

pub struct TronGrid {
    http: HttpClient,
    base: Url,
}

impl TronGrid {
    pub fn new(base: &str, api_key: &str, budget: Arc<Budget>) -> Result<Self, ProviderError> {
        Self::with_config(base, api_key, budget, Self::default_config())
    }

    /// Free-plan limits depend on the account; two requests per second stays
    /// well inside the published per-key rate while history imports run.
    pub fn default_config() -> HttpConfig {
        HttpConfig {
            min_interval: Duration::from_millis(500),
            ..HttpConfig::default()
        }
    }

    pub fn with_config(
        base: &str,
        api_key: &str,
        budget: Arc<Budget>,
        config: HttpConfig,
    ) -> Result<Self, ProviderError> {
        // Anonymous TronGrid access is not a production design (API_PROVIDERS.md §2).
        if api_key.trim().is_empty() {
            return Err(ProviderError::MissingKey { provider: PROVIDER });
        }
        let mut headers = HeaderMap::new();
        let mut value = HeaderValue::from_str(api_key.trim())
            .map_err(|_| ProviderError::MissingKey { provider: PROVIDER })?;
        value.set_sensitive(true);
        headers.insert(HeaderName::from_static("tron-pro-api-key"), value);
        Ok(TronGrid {
            http: HttpClient::new(PROVIDER, config, budget, headers)?,
            base: parse_base(base)?,
        })
    }

    pub fn http(&self) -> &HttpClient {
        &self.http
    }

    fn url(&self, path: &str) -> Result<Url, ProviderError> {
        self.base
            .join(path)
            .map_err(|_| invalid("url", "invalid address"))
    }

    async fn envelope<T: serde::de::DeserializeOwned>(
        &self,
        endpoint: &'static str,
        url: Url,
    ) -> Result<Envelope<T>, ProviderError> {
        let body = self.http.get(endpoint, url).await?;
        let doc: Envelope<T> = body.json(PROVIDER, endpoint)?;
        // TronGrid reports some failures inside HTTP 200.
        if doc.success == Some(false) {
            return Err(invalid(
                endpoint,
                doc.error.as_deref().unwrap_or("request not successful"),
            ));
        }
        Ok(doc)
    }

    /// Balances of an account. A never-activated address has no data and
    /// holds nothing.
    pub async fn account(&self, address: &str) -> Result<TronAccount, ProviderError> {
        let url = self.url(&format!("v1/accounts/{address}"))?;
        let doc: Envelope<AccountItem> = self.envelope("account", url).await?;
        let Some(item) = doc.data.into_iter().next() else {
            return Ok(TronAccount::default());
        };
        account_from(item)
    }

    /// Query fingerprint stored next to a cursor.
    pub fn fingerprint(category: &str, page_size: u32) -> String {
        format!("{QUERY_VERSION};{category};size={page_size}")
    }

    /// One page of confirmed native activity, newest first.
    pub async fn transactions(
        &self,
        address: &str,
        cursor: Option<&str>,
        page_size: u32,
    ) -> Result<Page<NativeTx>, ProviderError> {
        let mut url = self.url(&format!("v1/accounts/{address}/transactions"))?;
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("only_confirmed", "true")
                .append_pair("limit", &page_size.clamp(1, 200).to_string());
            if let Some(c) = cursor {
                q.append_pair("fingerprint", c);
            }
        }
        let doc: Envelope<Value> = self.envelope("transactions", url).await?;
        let next = continuation(&doc.meta, cursor, "transactions")?;
        // Unknown record shapes are a schema error for the whole page rather
        // than silently missing activity.
        let items = doc
            .data
            .into_iter()
            .map(|v| {
                serde_json::from_value::<NativeTx>(v)
                    .map_err(|e| invalid("transactions", &format!("unexpected record: {e}")))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Page { items, next })
    }

    /// One page of confirmed TRC-20 transfer events, newest first. With
    /// `contract`, only that token's events (used to learn token metadata).
    pub async fn trc20_transfers(
        &self,
        address: &str,
        cursor: Option<&str>,
        page_size: u32,
        contract: Option<&str>,
    ) -> Result<Page<Trc20Transfer>, ProviderError> {
        let mut url = self.url(&format!("v1/accounts/{address}/transactions/trc20"))?;
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("only_confirmed", "true")
                .append_pair("limit", &page_size.clamp(1, 200).to_string());
            if let Some(c) = contract {
                q.append_pair("contract_address", c);
            }
            if let Some(c) = cursor {
                q.append_pair("fingerprint", c);
            }
        }
        let doc: Envelope<Trc20Transfer> = self.envelope("transactions/trc20", url).await?;
        let next = continuation(&doc.meta, cursor, "transactions/trc20")?;
        Ok(Page {
            items: doc.data,
            next,
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

/// The next page exists only when `meta.links.next` is present; only the
/// opaque fingerprint is kept, never the provider's URL.
fn continuation(
    meta: &Meta,
    cursor: Option<&str>,
    endpoint: &'static str,
) -> Result<Option<String>, ProviderError> {
    let has_next = meta.links.as_ref().and_then(|l| l.next.as_ref()).is_some();
    let next = meta.fingerprint.clone().filter(|_| has_next);
    if next.is_some() && next.as_deref() == cursor {
        return Err(ProviderError::RepeatedCursor {
            provider: PROVIDER,
            endpoint,
        });
    }
    Ok(next)
}

fn account_from(item: AccountItem) -> Result<TronAccount, ProviderError> {
    let nonneg = |v: Option<i64>| BigInt::from(v.unwrap_or(0).max(0));
    let mut staked = BigInt::from(0);
    for f in &item.frozen_v2 {
        staked += nonneg(f.amount);
    }
    for u in &item.unfrozen_v2 {
        staked += nonneg(u.unfreeze_amount);
    }
    staked += nonneg(item.delegated_bandwidth);
    staked += nonneg(
        item.account_resource
            .as_ref()
            .and_then(|r| r.delegated_energy),
    );
    let mut trc20 = Vec::new();
    for entry in item.trc20 {
        for (contract, raw) in entry {
            let raw =
                parse_raw_amount(&raw).map_err(|_| invalid("account", "invalid TRC-20 balance"))?;
            trc20.push((contract, raw));
        }
    }
    Ok(TronAccount {
        exists: true,
        liquid_sun: nonneg(item.balance),
        staked_sun: staked,
        trc20,
    })
}

/// Base58 form of a hex (`41...`) address, if valid.
fn base58(hex_address: &str) -> Option<String> {
    normalize_address(NetworkId::Tron, hex_address)
        .ok()
        .map(|a| a.canonical)
}

fn native_leg(signed: BigInt, leg_type: &str) -> LegSpec {
    let direction = if signed > BigInt::from(0) {
        Direction::In
    } else if signed < BigInt::from(0) {
        Direction::Out
    } else {
        Direction::SelfTransfer
    };
    LegSpec {
        counterparty: None,
        unresolved: direction == Direction::In,
        asset: AssetSpec::native(NetworkId::Tron, PROVIDER),
        signed_raw: signed,
        direction,
        leg_type: leg_type.to_owned(),
        decoding: Decoding::Interpreted,
    }
}

fn int_field(value: &Value, key: &str) -> i64 {
    value.get(key).and_then(Value::as_i64).unwrap_or(0)
}

fn str_field<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

/// Normalizes one native record for `account` (Base58 canonical address).
pub fn native_tx_for_account(tx: &NativeTx, account: &str) -> Result<TxSpec, ProviderError> {
    let account_hex = tron_to_hex(account)
        .map_err(|_| invalid("transactions", "invalid account address"))?
        .to_ascii_lowercase();
    let is_account = |hex: Option<&str>| hex.is_some_and(|h| h.eq_ignore_ascii_case(&account_hex));
    let ret = tx.ret.first();
    let success = ret
        .and_then(|r| r.contract_ret.as_deref())
        .is_none_or(|r| r == "SUCCESS");
    let status = if success {
        TxStatus::Confirmed
    } else {
        TxStatus::Failed
    };
    let contract = tx.raw_data.contract.first();
    let kind = contract.map_or("Unknown", |c| c.kind.as_str());
    let value = contract.map_or(&Value::Null, |c| &c.parameter.value);
    let owner = str_field(value, "owner_address");
    let owned_by_account = is_account(owner);

    let mut legs = Vec::new();
    let mut decoding = Decoding::Interpreted;
    let operation: String;
    match kind {
        "TransferContract" => {
            let amount = BigInt::from(int_field(value, "amount").max(0));
            let to_account = is_account(str_field(value, "to_address"));
            let signed = match (owned_by_account, to_account) {
                (true, true) => BigInt::from(0),
                (true, false) => -amount,
                (false, true) => amount,
                (false, false) => BigInt::from(0),
            };
            operation = match (owned_by_account, to_account) {
                (true, true) => "self",
                (true, false) => "send",
                _ => "receive",
            }
            .to_owned();
            if status != TxStatus::Failed && (owned_by_account || to_account) {
                legs.push(
                    native_leg(signed, &operation).with_counterparty(if owned_by_account {
                        str_field(value, "to_address").and_then(base58)
                    } else {
                        owner.and_then(base58)
                    }),
                );
            }
        }
        "TriggerSmartContract" => {
            let data = str_field(value, "data").unwrap_or_default();
            operation = if data.len() >= 8 && data[..8].eq_ignore_ascii_case(TRANSFER_SELECTOR) {
                "send"
            } else {
                "execute"
            }
            .to_owned();
            let call_value = int_field(value, "call_value").max(0);
            if owned_by_account && call_value > 0 && status != TxStatus::Failed {
                legs.push(native_leg(-BigInt::from(call_value), "contract_call"));
            }
        }
        "WithdrawBalanceContract" => {
            operation = "claim_rewards".to_owned();
            match tx.withdraw_amount {
                Some(amount) if amount > 0 && owned_by_account && status != TxStatus::Failed => {
                    legs.push(native_leg(BigInt::from(amount), "staking_reward"));
                }
                Some(_) => {}
                None => decoding = Decoding::Partial,
            }
        }
        // Stake 2.0 moves TRX between liquid, frozen, delegated and unfreezing
        // states of the same owner. All of them are part of the TRX holding,
        // so these change nothing but the fee.
        "FreezeBalanceV2Contract"
        | "UnfreezeBalanceV2Contract"
        | "WithdrawExpireUnfreezeContract"
        | "CancelAllUnfreezeV2Contract"
        | "DelegateResourceContract"
        | "UnDelegateResourceContract"
        | "VoteWitnessContract" => {
            operation = match kind {
                "FreezeBalanceV2Contract" => "stake",
                "VoteWitnessContract" => "vote",
                "DelegateResourceContract" | "UnDelegateResourceContract" => "delegate",
                _ => "unstake",
            }
            .to_owned();
        }
        // TRC-10 tokens and legacy contract types are outside the release
        // scope; the record stays visible as raw activity.
        "TransferAssetContract" => {
            operation = "trc10_transfer".to_owned();
            decoding = Decoding::Partial;
        }
        other => {
            operation = other.trim_end_matches("Contract").to_ascii_lowercase();
            decoding = Decoding::RawOnly;
        }
    }

    // TRX moved to the account by contract execution (for example a DEX
    // paying out TRX). TRC-20 effects arrive through the TRC-20 category.
    if status != TxStatus::Failed {
        for internal in &tx.internal_transactions {
            if internal.data.get("rejected").and_then(Value::as_bool) == Some(true) {
                continue;
            }
            let trx = internal
                .data
                .get("call_value")
                .and_then(|c| c.get("_"))
                .and_then(Value::as_i64)
                .unwrap_or(0);
            if trx <= 0 {
                continue;
            }
            let to = is_account(internal.to_address.as_deref());
            let from = is_account(internal.from_address.as_deref());
            if to && !from {
                legs.push(native_leg(BigInt::from(trx), "internal_transfer"));
            }
        }
    }

    // TRX burned for bandwidth/energy plus any account-creation or memo fee,
    // charged to the signer. `ret.fee` is the authoritative total.
    let fee_sun = ret
        .and_then(|r| r.fee)
        .unwrap_or_else(|| tx.net_fee.unwrap_or(0) + tx.energy_fee.unwrap_or(0));
    let fee = (owned_by_account && fee_sun > 0).then(|| FeeSpec {
        asset: AssetSpec::native(NetworkId::Tron, PROVIDER),
        raw: BigInt::from(fee_sun),
        attribution: FeeAttribution::Exact,
    });

    Ok(TxSpec {
        network: NetworkId::Tron,
        part: None,
        hash: tx.tx_id.to_ascii_lowercase(),
        block_height: tx.block_number,
        position: None,
        occurred_at: tx.block_timestamp.div_euclid(1000),
        status,
        provider: PROVIDER,
        operation,
        legs,
        fee,
        decoding,
        evidence: serde_json::json!({
            "provider": PROVIDER,
            "category": "native",
            "tx_id": tx.tx_id,
            "contract_type": kind,
            "result": ret.and_then(|r| r.contract_ret.clone()),
            "fee_sun": fee_sun,
            "energy_fee_sun": tx.energy_fee,
            "net_fee_sun": tx.net_fee,
            "owner": owner.and_then(base58),
            "internal_transactions": tx.internal_transactions.len(),
        }),
    })
}

/// Normalizes a page of TRC-20 events for `account`. Each event is its own
/// component, identified by its content plus an occurrence index for
/// identical events in one transaction. Zero-value transfers (a common
/// address-poisoning pattern) and non-transfer events move nothing and are
/// skipped.
pub fn trc20_specs_for_account(events: &[Trc20Transfer], account: &str) -> Vec<TxSpec> {
    let mut seen: BTreeMap<String, u32> = BTreeMap::new();
    let mut out = Vec::new();
    for e in events {
        if e.kind.as_deref().is_some_and(|k| k != "Transfer") {
            continue;
        }
        let Ok(raw) = parse_raw_amount(&e.value) else {
            continue;
        };
        if raw == BigInt::from(0) {
            continue;
        }
        let from = e.from == account;
        let to = e.to == account;
        if !from && !to {
            continue;
        }
        let base = format!(
            "trc20:{}:{}:{}:{}",
            e.token_info.address, e.from, e.to, e.value
        );
        let n = seen
            .entry(format!("{}|{base}", e.transaction_id))
            .or_insert(0);
        let part = format!("{base}:{n}");
        *n += 1;
        let (signed, direction, operation) = match (from, to) {
            (true, true) => (BigInt::from(0), Direction::SelfTransfer, "self"),
            (true, false) => (-raw, Direction::Out, "send"),
            _ => (raw, Direction::In, "receive"),
        };
        out.push(TxSpec {
            network: NetworkId::Tron,
            part: Some(part),
            hash: e.transaction_id.to_ascii_lowercase(),
            block_height: None,
            position: None,
            occurred_at: e.block_timestamp.div_euclid(1000),
            status: TxStatus::Confirmed,
            provider: PROVIDER,
            operation: operation.to_owned(),
            legs: vec![LegSpec {
                counterparty: Some(if operation == "send" {
                    e.to.clone()
                } else {
                    e.from.clone()
                }),
                unresolved: direction == Direction::In,
                asset: e.token_info.asset(),
                signed_raw: signed,
                direction,
                leg_type: operation.to_owned(),
                decoding: Decoding::Interpreted,
            }],
            fee: None,
            decoding: Decoding::Interpreted,
            evidence: serde_json::json!({
                "provider": PROVIDER,
                "category": "trc20",
                "tx_id": e.transaction_id,
                "contract": e.token_info.address,
                "from": e.from,
                "to": e.to,
            }),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continuation_requires_a_next_link() {
        let meta = Meta {
            fingerprint: Some("abc".into()),
            links: None,
        };
        assert_eq!(continuation(&meta, None, "x").unwrap(), None);
        let meta = Meta {
            fingerprint: Some("abc".into()),
            links: Some(MetaLinks {
                next: Some("https://api.trongrid.io/...".into()),
            }),
        };
        assert_eq!(
            continuation(&meta, None, "x").unwrap().as_deref(),
            Some("abc")
        );
        assert!(continuation(&meta, Some("abc"), "x").is_err());
    }
}
