//! Solana RPC holdings and paginated full transactions including owned token accounts.
//! Token/rent/program effects remain partial and require review; no wallet secrets.
use crate::{
    ProviderError,
    http::{Budget, HttpClient, HttpConfig},
    rpc,
};
use num_bigint::BigInt;
use portfolio_core::network::NetworkId;
use portfolio_store::ingest::{
    AssetSpec, Decoding, Direction, FeeAttribution, FeeSpec, LegSpec, TxSpec, TxStatus,
    Verification,
};
use reqwest::header::HeaderMap;
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use url::Url;

pub const PROVIDER: &str = "helius";
pub const DEFAULT_BASE: &str = "https://mainnet.helius-rpc.com/";
pub const TOKEN_PROGRAM: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
pub const TOKEN_2022: &str = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";
pub const PAGE_SIZE: u32 = 20;

pub struct Helius {
    http: HttpClient,
    url: Url,
}
pub struct Holdings {
    pub assets: Vec<(AssetSpec, BigInt)>,
    pub slot: i64,
}
pub struct Page {
    pub txs: Vec<TxSpec>,
    pub next: Option<String>,
}

impl Helius {
    pub fn new(key: &str, budget: Arc<Budget>) -> Result<Self, ProviderError> {
        Self::with_config(
            DEFAULT_BASE,
            key,
            budget,
            HttpConfig {
                min_interval: Duration::from_millis(200),
                ..HttpConfig::default()
            },
        )
    }
    pub fn with_config(
        base: &str,
        key: &str,
        budget: Arc<Budget>,
        config: HttpConfig,
    ) -> Result<Self, ProviderError> {
        if key.trim().is_empty() {
            return Err(ProviderError::MissingKey { provider: PROVIDER });
        }
        let mut url =
            Url::parse(base).map_err(|_| rpc::invalid(PROVIDER, "client", "invalid base"))?;
        url.query_pairs_mut().append_pair("api-key", key.trim());
        Ok(Self {
            http: HttpClient::new(PROVIDER, config, budget, HeaderMap::new())?,
            url,
        })
    }
    pub fn http(&self) -> &HttpClient {
        &self.http
    }
    pub async fn slot(&self) -> Result<i64, ProviderError> {
        rpc::call(
            &self.http,
            self.url.clone(),
            "getSlot",
            json!([{"commitment":"finalized"}]),
            1,
        )
        .await?
        .as_i64()
        .ok_or_else(|| rpc::invalid(PROVIDER, "getSlot", "missing slot"))
    }
    pub async fn holdings(&self, address: &str) -> Result<Holdings, ProviderError> {
        const M: &str = "getBalance";
        let native = rpc::call(
            &self.http,
            self.url.clone(),
            M,
            json!([address,{"commitment":"finalized"}]),
            1,
        )
        .await?;
        let slot = native["context"]["slot"]
            .as_i64()
            .ok_or_else(|| rpc::invalid(PROVIDER, M, "missing slot"))?;
        let amount = native["value"]
            .as_u64()
            .ok_or_else(|| rpc::invalid(PROVIDER, M, "missing lamports"))?;
        let mut assets = BTreeMap::<String, (AssetSpec, BigInt)>::new();
        for program in [TOKEN_PROGRAM, TOKEN_2022] {
            const M: &str = "getTokenAccountsByOwner";
            let response = rpc::call(&self.http,self.url.clone(),M,json!([address,{"programId":program},{"encoding":"jsonParsed","commitment":"finalized","minContextSlot":slot}]),1).await?;
            let accounts = response["value"]
                .as_array()
                .ok_or_else(|| rpc::invalid(PROVIDER, M, "missing token accounts"))?;
            for account in accounts {
                let info = &account["account"]["data"]["parsed"]["info"];
                if info["owner"].as_str() != Some(address) {
                    return Err(rpc::invalid(PROVIDER, M, "unexpected token owner"));
                }
                let mint = rpc::text(info, "mint", PROVIDER, M)?;
                let decimals = info["tokenAmount"]["decimals"]
                    .as_u64()
                    .filter(|d| *d <= 255)
                    .ok_or_else(|| rpc::invalid(PROVIDER, M, "missing decimals"))?
                    as u32;
                let raw = decimal_raw(&info["tokenAmount"]["amount"], M)?;
                let asset = token(mint, decimals);
                let entry = assets
                    .entry(mint.into())
                    .or_insert_with(|| (asset, BigInt::from(0)));
                if entry.0.decimals != decimals {
                    return Err(rpc::invalid(PROVIDER, M, "conflicting token decimals"));
                }
                entry.1 += raw;
            }
        }
        let mut assets: Vec<_> = assets.into_values().collect();
        assets.push((
            AssetSpec::native(NetworkId::Solana, PROVIDER),
            BigInt::from(amount),
        ));
        Ok(Holdings { assets, slot })
    }
    pub async fn transactions(
        &self,
        address: &str,
        cursor: Option<&str>,
    ) -> Result<Page, ProviderError> {
        const M: &str = "getTransactionsForAddress";
        let mut config = json!({"transactionDetails":"full","encoding":"jsonParsed","maxSupportedTransactionVersion":0,"sortOrder":"desc","commitment":"finalized","limit":PAGE_SIZE,"filters":{"status":"any","tokenAccounts":"all"}});
        if let Some(c) = cursor {
            config["paginationToken"] = json!(c);
        }
        let result = rpc::call(
            &self.http,
            self.url.clone(),
            M,
            json!([address, config]),
            10,
        )
        .await?;
        let data = result["data"]
            .as_array()
            .ok_or_else(|| rpc::invalid(PROVIDER, M, "missing history data"))?;
        let mut txs = Vec::new();
        for tx in data {
            txs.push(normalize(tx, address)?);
        }
        let next = result
            .get("paginationToken")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        if next.as_deref().is_some_and(|n| Some(n) == cursor) {
            return Err(ProviderError::RepeatedCursor {
                provider: PROVIDER,
                endpoint: M,
            });
        }
        Ok(Page { txs, next })
    }
}

fn token(mint: &str, decimals: u32) -> AssetSpec {
    AssetSpec {
        network: NetworkId::Solana,
        contract: Some(mint.into()),
        decimals,
        symbol: None,
        name: None,
        verification: Verification::Unverified,
        provider: PROVIDER,
    }
}
fn decimal_raw(v: &Value, m: &'static str) -> Result<BigInt, ProviderError> {
    v.as_str()
        .filter(|s| !s.is_empty() && s.len() <= 78 && s.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|s| BigInt::parse_bytes(s.as_bytes(), 10))
        .ok_or_else(|| rpc::invalid(PROVIDER, m, "invalid raw token quantity"))
}
fn leg(asset: AssetSpec, raw: BigInt, counterparty: Option<String>, kind: &str) -> LegSpec {
    let direction = if raw < BigInt::from(0) {
        Direction::Out
    } else {
        Direction::In
    };
    LegSpec {
        asset,
        signed_raw: raw,
        direction,
        counterparty,
        leg_type: kind.into(),
        decoding: Decoding::Partial,
        unresolved: true,
    }
}

/// Account deltas exclude the fee; other program/rent effects remain explicitly unresolved.
pub fn normalize(v: &Value, address: &str) -> Result<TxSpec, ProviderError> {
    const M: &str = "getTransactionsForAddress";
    let transaction = &v["transaction"];
    let hash = transaction["signatures"][0]
        .as_str()
        .ok_or_else(|| rpc::invalid(PROVIDER, M, "missing signature"))?
        .to_owned();
    let meta = &v["meta"];
    if meta.is_null() {
        return Err(rpc::invalid(PROVIDER, M, "missing transaction metadata"));
    }
    let keys = transaction["message"]["accountKeys"]
        .as_array()
        .ok_or_else(|| rpc::invalid(PROVIDER, M, "missing parsed account keys"))?;
    fn key(v: &Value) -> Option<&str> {
        v.as_str().or_else(|| v["pubkey"].as_str())
    }
    let payer = keys.first().and_then(key);
    let fee_raw = BigInt::from(
        meta["fee"]
            .as_u64()
            .ok_or_else(|| rpc::invalid(PROVIDER, M, "missing fee"))?,
    );
    let failed = !meta["err"].is_null();
    let native = AssetSpec::native(NetworkId::Solana, PROVIDER);
    let fee = (payer == Some(address)).then(|| FeeSpec {
        asset: native.clone(),
        raw: fee_raw.clone(),
        attribution: FeeAttribution::Exact,
    });
    let mut legs = Vec::new();
    if !failed {
        if let Some(index) = keys.iter().position(|v| key(v) == Some(address)) {
            let pre = meta["preBalances"][index]
                .as_u64()
                .ok_or_else(|| rpc::invalid(PROVIDER, M, "missing pre balance"))?;
            let post = meta["postBalances"][index]
                .as_u64()
                .ok_or_else(|| rpc::invalid(PROVIDER, M, "missing post balance"))?;
            let delta = BigInt::from(post) - BigInt::from(pre)
                + if fee.is_some() {
                    fee_raw
                } else {
                    BigInt::from(0)
                };
            // Infer counterparties only for a plain, single System Program transfer.
            let instructions = transaction["message"]["instructions"].as_array();
            let cp = instructions.filter(|i| i.len() == 1).and_then(|i| {
                let p = &i[0]["parsed"];
                if i[0]["program"].as_str() != Some("system")
                    || p["type"].as_str() != Some("transfer")
                {
                    return None;
                }
                let info = &p["info"];
                let from = info["source"].as_str()?;
                let to = info["destination"].as_str()?;
                let amount = BigInt::from(info["lamports"].as_u64()?);
                if from == address && to != address && delta == -amount.clone() {
                    Some(to.to_owned())
                } else if to == address && from != address && delta == amount {
                    Some(from.to_owned())
                } else {
                    None
                }
            });
            if delta != BigInt::from(0) {
                let mut l = leg(native.clone(), delta, cp, "unknown");
                if l.counterparty.is_some() {
                    l.leg_type = "transfer".into();
                    l.unresolved = false;
                    l.decoding = Decoding::Interpreted;
                }
                legs.push(l);
            }
        }
        let mut tokens = BTreeMap::<String, (u32, BigInt)>::new();
        for (name, sign) in [("preTokenBalances", -1), ("postTokenBalances", 1)] {
            let rows = meta[name]
                .as_array()
                .ok_or_else(|| rpc::invalid(PROVIDER, M, "missing token balances"))?;
            for row in rows {
                if row["owner"].as_str() != Some(address) {
                    continue;
                }
                let mint = rpc::text(row, "mint", PROVIDER, M)?;
                let decimals = row["uiTokenAmount"]["decimals"]
                    .as_u64()
                    .filter(|d| *d <= 255)
                    .ok_or_else(|| rpc::invalid(PROVIDER, M, "missing token decimals"))?
                    as u32;
                let raw = decimal_raw(&row["uiTokenAmount"]["amount"], M)?;
                let entry = tokens
                    .entry(mint.into())
                    .or_insert((decimals, BigInt::from(0)));
                if entry.0 != decimals {
                    return Err(rpc::invalid(PROVIDER, M, "conflicting decimals"));
                }
                entry.1 += raw * sign;
            }
        }
        for (mint, (decimals, delta)) in tokens {
            if delta != BigInt::from(0) {
                legs.push(leg(token(&mint, decimals), delta, None, "unknown"));
            }
        }
    }
    Ok(TxSpec {
        network: NetworkId::Solana,
        hash,
        part: None,
        block_height: Some(
            v["slot"]
                .as_i64()
                .ok_or_else(|| rpc::invalid(PROVIDER, M, "missing slot"))?,
        ),
        position: v
            .get("transactionIndex")
            .and_then(Value::as_u64)
            .map(|n| format!("{n:010}")),
        occurred_at: v["blockTime"]
            .as_i64()
            .ok_or_else(|| rpc::invalid(PROVIDER, M, "missing block time"))?,
        status: if failed {
            TxStatus::Failed
        } else {
            TxStatus::Confirmed
        },
        provider: PROVIDER,
        operation: if failed { "failed" } else { "execute" }.into(),
        legs,
        fee,
        decoding: Decoding::Partial,
        evidence: json!({"source":"getTransactionsForAddress","token_accounts":"all","native_effects":"rent/program effects require review"}),
    })
}
