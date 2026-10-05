//! TON account data through TonAPI (API_PROVIDERS.md §3.6).
//!
//! Routes: `GET /v2/accounts/{id}` (TON balance), `GET /v2/accounts/{id}/jettons`
//! (Jetton balances keyed by master), `GET /v2/accounts/{id}/events` (account
//! events newest first, continued with `before_lt`/`next_from`) and
//! `GET /v2/accounts/{id}/events/{event_id}` (re-check of an unfinished
//! trace). A free key is sent as `Authorization: Bearer`; without one the
//! documented anonymous rate is much lower and is paced accordingly.
//!
//! Canonical economic representation: one account event per trace. TonAPI's
//! event already folds the underlying transactions, messages, bounces and
//! refunds of the trace into actions plus `extra`, the account's TON change
//! not explained by the actions (fees net of refunds). Transactions are kept
//! only as evidence references, so nothing is counted twice.

use std::sync::Arc;
use std::time::Duration;

use num_bigint::BigInt;
use portfolio_core::address::normalize_address;
use portfolio_core::decimal::parse_raw_amount;
use portfolio_core::network::NetworkId;
use portfolio_store::ingest::{
    AssetSpec, Decoding, Direction, FeeAttribution, FeeSpec, LegSpec, TxSpec, TxStatus,
    Verification,
};
use reqwest::StatusCode;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use serde::Deserialize;
use serde_json::Value;
use url::Url;

use crate::error::ProviderError;
use crate::esplora::parse_base;
use crate::http::{Budget, HttpClient, HttpConfig};

pub const PROVIDER: &str = "tonapi";
pub const DEFAULT_BASE: &str = "https://tonapi.io/v2/";
/// Events per page (the documented maximum is 100).
pub const PAGE_SIZE: u32 = 50;
const QUERY_VERSION: &str = "v1;events";

#[derive(Debug, Clone, Deserialize)]
struct AccountInfo {
    balance: Value,
    #[serde(default)]
    status: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct JettonBalances {
    #[serde(default)]
    balances: Vec<JettonBalance>,
}

#[derive(Debug, Clone, Deserialize)]
struct JettonBalance {
    balance: String,
    jetton: JettonInfo,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JettonInfo {
    pub address: String,
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub decimals: u32,
    #[serde(default)]
    pub verification: Option<String>,
}

impl JettonInfo {
    /// Jettons are identified by their master contract (raw form), never by
    /// the holder's Jetton wallet address.
    pub fn asset(&self) -> Option<AssetSpec> {
        let master = canonical(&self.address)?;
        Some(AssetSpec {
            network: NetworkId::Ton,
            contract: Some(master),
            decimals: self.decimals,
            symbol: self.symbol.clone(),
            name: self.name.clone(),
            verification: match self.verification.as_deref() {
                Some("whitelist") => Verification::Verified,
                Some("blacklist") => Verification::Spam,
                _ => Verification::Unverified,
            },
            provider: PROVIDER,
        })
    }
}

/// Current holdings of one TON account.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TonHoldings {
    pub nanoton: BigInt,
    pub status: Option<String>,
    pub jettons: Vec<(AssetSpec, BigInt)>,
}

/// One account event as returned by the events routes.
#[derive(Debug, Clone, Deserialize)]
pub struct Event {
    pub event_id: String,
    pub timestamp: i64,
    #[serde(default)]
    pub actions: Vec<Value>,
    #[serde(default)]
    pub is_scam: bool,
    pub lt: i64,
    #[serde(default)]
    pub in_progress: bool,
    #[serde(default)]
    pub extra: i64,
}

#[derive(Debug, Clone, Deserialize)]
struct EventsDoc {
    #[serde(default)]
    events: Vec<Event>,
    #[serde(default)]
    next_from: i64,
}

/// A page of events and the logical time to continue below.
#[derive(Debug, Clone)]
pub struct EventPage {
    pub events: Vec<Event>,
    pub next: Option<String>,
}

pub struct TonApi {
    http: HttpClient,
    base: Url,
    keyed: bool,
}

impl TonApi {
    /// `api_key` may be empty: anonymous access works at a stricter rate.
    pub fn new(base: &str, api_key: &str, budget: Arc<Budget>) -> Result<Self, ProviderError> {
        let config = Self::default_config(!api_key.trim().is_empty());
        Self::with_config(base, api_key, budget, config)
    }

    /// One request per second with a free key; about one per four seconds
    /// anonymously (API_PROVIDERS.md §3.6).
    pub fn default_config(keyed: bool) -> HttpConfig {
        HttpConfig {
            min_interval: Duration::from_millis(if keyed { 1_100 } else { 4_000 }),
            ..HttpConfig::default()
        }
    }

    pub fn with_config(
        base: &str,
        api_key: &str,
        budget: Arc<Budget>,
        config: HttpConfig,
    ) -> Result<Self, ProviderError> {
        let mut headers = HeaderMap::new();
        let keyed = !api_key.trim().is_empty();
        if keyed {
            let mut value = HeaderValue::from_str(&format!("Bearer {}", api_key.trim()))
                .map_err(|_| ProviderError::MissingKey { provider: PROVIDER })?;
            value.set_sensitive(true);
            headers.insert(AUTHORIZATION, value);
        }
        Ok(TonApi {
            http: HttpClient::new(PROVIDER, config, budget, headers)?,
            base: parse_base(base)?,
            keyed,
        })
    }

    pub fn http(&self) -> &HttpClient {
        &self.http
    }

    pub fn keyed(&self) -> bool {
        self.keyed
    }

    fn url(&self, path: &str) -> Result<Url, ProviderError> {
        self.base
            .join(path)
            .map_err(|_| invalid("url", "invalid address"))
    }

    /// TON balance and Jetton balances. A never-used address holds nothing.
    pub async fn holdings(&self, account: &str) -> Result<TonHoldings, ProviderError> {
        let url = self.url(&format!("accounts/{account}"))?;
        let info: AccountInfo = match self.http.get("account", url).await {
            Ok(body) => body.json(PROVIDER, "account")?,
            Err(ProviderError::NotFound { .. }) => return Ok(TonHoldings::default()),
            Err(e) => return Err(e),
        };
        let nanoton = match &info.balance {
            Value::Number(n) => n
                .as_u64()
                .map(BigInt::from)
                .ok_or_else(|| invalid("account", "invalid balance"))?,
            Value::String(s) => {
                parse_raw_amount(s).map_err(|_| invalid("account", "invalid balance"))?
            }
            _ => return Err(invalid("account", "missing balance")),
        };
        let url = self.url(&format!("accounts/{account}/jettons"))?;
        let doc: JettonBalances = self
            .http
            .get("jettons", url)
            .await?
            .json(PROVIDER, "jettons")?;
        let mut jettons: Vec<(AssetSpec, BigInt)> = Vec::new();
        for b in doc.balances {
            let asset = b
                .jetton
                .asset()
                .ok_or_else(|| invalid("jettons", "invalid Jetton master address"))?;
            let raw =
                parse_raw_amount(&b.balance).map_err(|_| invalid("jettons", "invalid balance"))?;
            match jettons.iter_mut().find(|(a, _)| a.id() == asset.id()) {
                Some((_, existing)) => *existing += raw,
                None => jettons.push((asset, raw)),
            }
        }
        Ok(TonHoldings {
            nanoton,
            status: info.status,
            jettons,
        })
    }

    pub fn fingerprint(page_size: u32) -> String {
        format!("{QUERY_VERSION};size={page_size}")
    }

    /// One page of events, newest first, strictly below `before_lt`.
    pub async fn events(
        &self,
        account: &str,
        before_lt: Option<&str>,
        page_size: u32,
    ) -> Result<EventPage, ProviderError> {
        let mut url = self.url(&format!("accounts/{account}/events"))?;
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("limit", &page_size.clamp(1, 100).to_string());
            if let Some(lt) = before_lt {
                if lt.parse::<i64>().is_err() {
                    return Err(invalid("events", "invalid continuation"));
                }
                q.append_pair("before_lt", lt);
            }
        }
        let doc: EventsDoc = self
            .http
            .get("events", url)
            .await?
            .json(PROVIDER, "events")?;
        let next = (doc.next_from > 0 && !doc.events.is_empty()).then(|| doc.next_from.to_string());
        if next.is_some() && next.as_deref() == before_lt {
            return Err(ProviderError::RepeatedCursor {
                provider: PROVIDER,
                endpoint: "events",
            });
        }
        Ok(EventPage {
            events: doc.events,
            next,
        })
    }

    /// One event of the account; `None` when it no longer exists.
    pub async fn event(
        &self,
        account: &str,
        event_id: &str,
    ) -> Result<Option<Event>, ProviderError> {
        if !event_id.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err(invalid("event", "invalid event id"));
        }
        let url = self.url(&format!("accounts/{account}/events/{event_id}"))?;
        match self.http.get("event", url).await {
            Ok(body) if body.status == StatusCode::OK => Ok(Some(body.json(PROVIDER, "event")?)),
            Ok(_) => Ok(None),
            Err(ProviderError::NotFound { .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

fn invalid(endpoint: &'static str, detail: &str) -> ProviderError {
    ProviderError::InvalidResponse {
        provider: PROVIDER,
        endpoint,
        detail: detail.to_owned(),
    }
}

/// Canonical raw form of a TON address in any accepted representation.
fn canonical(address: &str) -> Option<String> {
    normalize_address(NetworkId::Ton, address)
        .ok()
        .map(|a| a.canonical)
}

fn party(action: &Value, key: &str) -> Option<String> {
    action
        .get(key)
        .and_then(|p| p.get("address"))
        .and_then(Value::as_str)
        .and_then(canonical)
}

fn amount(v: Option<&Value>) -> Option<BigInt> {
    match v? {
        Value::Number(n) => n.as_u64().map(BigInt::from),
        Value::String(s) => parse_raw_amount(s).ok(),
        _ => None,
    }
}

fn leg(asset: AssetSpec, signed: BigInt, leg_type: &str) -> LegSpec {
    let direction = if signed > BigInt::from(0) {
        Direction::In
    } else if signed < BigInt::from(0) {
        Direction::Out
    } else {
        Direction::SelfTransfer
    };
    LegSpec {
        unresolved: direction == Direction::In,
        asset,
        signed_raw: signed,
        direction,
        leg_type: leg_type.to_owned(),
        decoding: Decoding::Interpreted,
    }
}

fn ton() -> AssetSpec {
    AssetSpec::native(NetworkId::Ton, PROVIDER)
}

/// Signed movement for a sender/recipient pair from the account's side.
fn signed_for(
    account: &str,
    sender: Option<&str>,
    recipient: Option<&str>,
    raw: BigInt,
) -> Option<BigInt> {
    match (sender == Some(account), recipient == Some(account)) {
        (true, true) => Some(BigInt::from(0)),
        (true, false) => Some(-raw),
        (false, true) => Some(raw),
        (false, false) => None,
    }
}

/// Normalizes one event for `account` (canonical raw address).
pub fn event_for_account(event: &Event, account: &str) -> TxSpec {
    let mut legs = Vec::new();
    let mut decoding = Decoding::Interpreted;
    let mut kinds: Vec<String> = Vec::new();
    let mut failed_actions = 0usize;
    let mut base_transactions: Vec<String> = Vec::new();

    for action in &event.actions {
        let kind = action
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("Unknown");
        kinds.push(kind.to_owned());
        if let Some(list) = action.get("base_transactions").and_then(Value::as_array) {
            base_transactions.extend(list.iter().filter_map(Value::as_str).map(str::to_owned));
        }
        // A failed action (for example a bounced transfer) moved nothing; the
        // fees it cost and any refund are already in `extra`.
        if action.get("status").and_then(Value::as_str) != Some("ok") {
            failed_actions += 1;
            continue;
        }
        let body = action.get(kind).unwrap_or(&Value::Null);
        match kind {
            "TonTransfer" => {
                let s = party(body, "sender");
                let r = party(body, "recipient");
                match amount(body.get("amount"))
                    .and_then(|raw| signed_for(account, s.as_deref(), r.as_deref(), raw))
                {
                    Some(signed) => legs.push(leg(ton(), signed, "transfer")),
                    None => decoding = Decoding::Partial,
                }
            }
            "JettonTransfer" | "JettonMint" | "JettonBurn" => {
                let s = party(body, "sender");
                let r = party(body, "recipient");
                let asset = body
                    .get("jetton")
                    .and_then(|j| serde_json::from_value::<JettonInfo>(j.clone()).ok())
                    .and_then(|j| j.asset());
                let raw = amount(body.get("amount"));
                let leg_type = match kind {
                    "JettonMint" => "mint",
                    "JettonBurn" => "burn",
                    _ => "transfer",
                };
                match (asset, raw) {
                    (Some(asset), Some(raw)) => {
                        match signed_for(account, s.as_deref(), r.as_deref(), raw) {
                            Some(signed) => legs.push(leg(asset, signed, leg_type)),
                            None => decoding = Decoding::Partial,
                        }
                    }
                    _ => decoding = Decoding::Partial,
                }
            }
            "JettonSwap" => {
                if party(body, "user_wallet").as_deref() != Some(account) {
                    decoding = Decoding::Partial;
                    continue;
                }
                let side = |master: &str,
                            jetton_amount: &str,
                            ton_amount: &str|
                 -> Option<(AssetSpec, BigInt)> {
                    match body.get(master).filter(|m| !m.is_null()) {
                        Some(m) => {
                            let info: JettonInfo = serde_json::from_value(m.clone()).ok()?;
                            Some((info.asset()?, amount(body.get(jetton_amount))?))
                        }
                        None => Some((ton(), amount(body.get(ton_amount))?)),
                    }
                };
                match (
                    side("jetton_master_in", "amount_in", "ton_in"),
                    side("jetton_master_out", "amount_out", "ton_out"),
                ) {
                    (Some((ain, rin)), Some((aout, rout))) => {
                        legs.push(leg(ain, -rin, "trade"));
                        legs.push(leg(aout, rout, "trade"));
                    }
                    _ => decoding = Decoding::Partial,
                }
            }
            "SmartContractExec" => {
                // TON attached to a contract call leaves the account; what the
                // call achieved is not interpreted.
                if party(body, "executor").as_deref() == Some(account)
                    && let Some(raw) = amount(body.get("ton_attached"))
                    && raw > BigInt::from(0)
                {
                    legs.push(leg(ton(), -raw, "contract_call"));
                }
                decoding = Decoding::Partial;
            }
            "DepositStake" | "WithdrawStake" => {
                if party(body, "staker").as_deref() == Some(account)
                    && let Some(raw) = amount(body.get("amount"))
                {
                    let (signed, t) = if kind == "DepositStake" {
                        (-raw, "stake")
                    } else {
                        (raw, "unstake")
                    };
                    legs.push(leg(ton(), signed, t));
                } else {
                    decoding = Decoding::Partial;
                }
            }
            // No fungible movement of the account.
            "ContractDeploy" | "Subscribe" | "UnSubscribe" | "DomainRenew" => {}
            // NFTs are deferred (SPECIFICATION.md §3.3); other action kinds are
            // kept visible but not interpreted.
            _ => decoding = Decoding::Partial,
        }
    }

    // TON change not explained by the actions: fees, net of refunds.
    let fee = (event.extra < 0).then(|| FeeSpec {
        asset: ton(),
        raw: BigInt::from(-event.extra),
        attribution: FeeAttribution::Exact,
    });
    if event.extra > 0 {
        legs.push(leg(ton(), BigInt::from(event.extra), "refund"));
    }

    let operation = operation_for(&kinds, &legs);
    TxSpec {
        network: NetworkId::Ton,
        part: None,
        hash: event.event_id.to_ascii_lowercase(),
        block_height: None,
        position: Some(event.lt.to_string()),
        occurred_at: event.timestamp,
        status: if event.in_progress {
            TxStatus::Pending
        } else {
            TxStatus::Confirmed
        },
        provider: PROVIDER,
        operation,
        legs,
        fee,
        decoding: if event.in_progress {
            Decoding::Partial
        } else {
            decoding
        },
        evidence: serde_json::json!({
            "provider": PROVIDER,
            "event_id": event.event_id,
            "lt": event.lt,
            "actions": kinds,
            "failed_actions": failed_actions,
            "extra_nanoton": event.extra,
            "is_scam": event.is_scam,
            "in_progress": event.in_progress,
            "base_transactions": base_transactions,
        }),
    }
}

fn operation_for(kinds: &[String], legs: &[LegSpec]) -> String {
    if kinds.iter().any(|k| k == "JettonSwap") {
        return "trade".into();
    }
    let principal: Vec<&LegSpec> = legs.iter().filter(|l| l.leg_type != "refund").collect();
    if kinds.len() == 1 && principal.len() <= 1 {
        let k = kinds[0].as_str();
        if matches!(k, "TonTransfer" | "JettonTransfer") {
            return match principal.first().map(|l| l.direction) {
                Some(Direction::In) => "receive",
                Some(Direction::Out) => "send",
                Some(Direction::SelfTransfer) => "self",
                None => "transfer",
            }
            .into();
        }
        return match k {
            "JettonMint" => "mint",
            "JettonBurn" => "burn",
            "DepositStake" => "stake",
            "WithdrawStake" => "unstake",
            "SmartContractExec" => "execute",
            "ContractDeploy" => "deploy",
            "NftItemTransfer" | "NftPurchase" => "nft",
            _ => "other",
        }
        .into();
    }
    if kinds.is_empty() {
        "other".into()
    } else {
        "execute".into()
    }
}
