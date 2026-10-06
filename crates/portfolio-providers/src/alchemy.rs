//! Selected EVM mainnets: RPC balances/token discovery and transfer-indexed history.
//! Receipt logs provide whole-transaction ERC-20 effects, not page fragments.
//! Transfers do not discover failed/approval-only calls or all internal transfers.
use crate::{
    ProviderError,
    http::{Budget, HttpClient, HttpConfig},
    rpc,
};
use num_bigint::BigInt;
use num_traits::ToPrimitive;
use portfolio_core::{clock::parse_rfc3339, network::NetworkId};
use portfolio_store::ingest::{
    AssetSpec, Decoding, Direction, FeeAttribution, FeeSpec, LegSpec, TxSpec, TxStatus,
    Verification,
};
use reqwest::header::HeaderMap;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};
use url::Url;

pub const PROVIDER: &str = "alchemy";
/// Conservative upper-bound reservation per shipped method, not provider-reported usage.
pub const ESTIMATED_CU: u32 = 500;
pub fn estimated_cost(method: &str) -> u32 {
    match method {
        "alchemy_getAssetTransfers" => 120,
        "alchemy_getTokenBalances" => 20,
        "alchemy_getTokenMetadata" => 10,
        _ => ESTIMATED_CU,
    }
}
pub const PAGE_SIZE: u32 = 5;
const TRANSFER_TOPIC: &str = "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";
pub const NETWORKS: [NetworkId; 5] = [
    NetworkId::Ethereum,
    NetworkId::Base,
    NetworkId::Arbitrum,
    NetworkId::Optimism,
    NetworkId::Polygon,
];

pub struct Alchemy {
    http: HttpClient,
    endpoints: BTreeMap<NetworkId, Url>,
    metadata: std::sync::Mutex<BTreeMap<(NetworkId, String), AssetSpec>>,
    store: Option<portfolio_store::Store>,
}
pub struct Holdings {
    pub assets: Vec<(AssetSpec, BigInt)>,
    pub complete: bool,
}
pub struct Page {
    pub txs: Vec<TxSpec>,
    pub next: Option<String>,
}

#[derive(Deserialize, Serialize)]
struct Cursor {
    block: String,
    marker: String,
    hash: String,
}

impl Alchemy {
    pub fn new(key: &str, budget: Arc<Budget>) -> Result<Self, ProviderError> {
        let hosts = [
            "eth-mainnet",
            "base-mainnet",
            "arb-mainnet",
            "opt-mainnet",
            "polygon-mainnet",
        ];
        let bases: Vec<_> = NETWORKS
            .into_iter()
            .zip(hosts)
            .map(|(n, h)| (n, format!("https://{h}.g.alchemy.com/v2/")))
            .collect();
        Self::with_config(
            &bases,
            key,
            budget,
            HttpConfig {
                min_interval: Duration::from_secs(1),
                ..HttpConfig::default()
            },
        )
    }
    pub fn with_config(
        bases: &[(NetworkId, String)],
        key: &str,
        budget: Arc<Budget>,
        config: HttpConfig,
    ) -> Result<Self, ProviderError> {
        if key.trim().is_empty() {
            return Err(ProviderError::MissingKey { provider: PROVIDER });
        }
        let mut endpoints = BTreeMap::new();
        for (n, base) in bases {
            if !NETWORKS.contains(n) {
                return Err(rpc::invalid(PROVIDER, "client", "unsupported network"));
            }
            let mut url = Url::parse(base)
                .map_err(|_| rpc::invalid(PROVIDER, "client", "invalid endpoint"))?;
            url.path_segments_mut()
                .map_err(|_| rpc::invalid(PROVIDER, "client", "invalid endpoint"))?
                .pop_if_empty()
                .push(key.trim());
            endpoints.insert(*n, url);
        }
        Ok(Self {
            http: HttpClient::new(PROVIDER, config, budget, HeaderMap::new())?,
            endpoints,
            metadata: std::sync::Mutex::new(BTreeMap::new()),
            store: None,
        })
    }
    pub fn http(&self) -> &HttpClient {
        &self.http
    }
    pub fn with_metadata_store(mut self, store: portfolio_store::Store) -> Self {
        self.store = Some(store);
        self
    }
    async fn call(&self, n: NetworkId, m: &'static str, p: Value) -> Result<Value, ProviderError> {
        let url = self
            .endpoints
            .get(&n)
            .ok_or_else(|| rpc::invalid(PROVIDER, m, "unsupported network"))?
            .clone();
        rpc::call(&self.http, url, m, p, estimated_cost(m)).await
    }
    pub async fn check_chain(&self, n: NetworkId) -> Result<(), ProviderError> {
        let v = self.call(n, "eth_chainId", json!([])).await?;
        let id = rpc::raw_hex(v.as_str().unwrap_or(""), PROVIDER, "eth_chainId")?.to_u64();
        if id != n.evm_chain_id() {
            return Err(rpc::invalid(PROVIDER, "eth_chainId", "wrong mainnet"));
        }
        Ok(())
    }
    pub async fn native_balance(
        &self,
        n: NetworkId,
        address: &str,
    ) -> Result<BigInt, ProviderError> {
        let v = self
            .call(n, "eth_getBalance", json!([address, "latest"]))
            .await?;
        rpc::raw_hex(v.as_str().unwrap_or(""), PROVIDER, "eth_getBalance")
    }

    pub async fn token_balance(
        &self,
        n: NetworkId,
        address: &str,
        contract: &str,
    ) -> Result<(AssetSpec, BigInt), ProviderError> {
        const M: &str = "alchemy_getTokenBalances";
        let address = evm_address(address, M)?;
        let contract = evm_address(contract, M)?;
        let response = self.call(n, M, json!([address, [contract]])).await?;
        if response["address"]
            .as_str()
            .map(str::to_ascii_lowercase)
            .as_deref()
            != Some(address.as_str())
        {
            return Err(rpc::invalid(PROVIDER, M, "unexpected wallet"));
        }
        let rows = response["tokenBalances"]
            .as_array()
            .ok_or_else(|| rpc::invalid(PROVIDER, M, "missing token balances"))?;
        let row = rows
            .iter()
            .find(|r| {
                r["contractAddress"]
                    .as_str()
                    .map(str::to_ascii_lowercase)
                    .as_deref()
                    == Some(contract.as_str())
            })
            .ok_or_else(|| rpc::invalid(PROVIDER, M, "missing requested token"))?;
        if !row.get("error").is_none_or(Value::is_null) {
            return Err(rpc::invalid(PROVIDER, M, "token balance lookup failed"));
        }
        let raw = rpc::raw_hex(rpc::text(row, "tokenBalance", PROVIDER, M)?, PROVIDER, M)?;
        Ok((self.token(n, &contract).await?, raw))
    }
    async fn token(&self, n: NetworkId, contract: &str) -> Result<AssetSpec, ProviderError> {
        const M: &str = "alchemy_getTokenMetadata";
        let contract = evm_address(contract, M)?;
        let key = (n, contract.clone());
        if let Some(asset) = self
            .metadata
            .lock()
            .expect("metadata cache")
            .get(&key)
            .cloned()
        {
            return Ok(asset);
        }
        if let Some(store) = &self.store {
            let id = portfolio_store::demo::asset_id(n, Some(&contract));
            if let Some(asset) = store
                .asset_spec(&id, PROVIDER)
                .await
                .map_err(|_| rpc::invalid(PROVIDER, M, "cached metadata unavailable"))?
            {
                self.metadata
                    .lock()
                    .expect("metadata cache")
                    .insert(key, asset.clone());
                return Ok(asset);
            }
        }
        let v = self.call(n, M, json!([contract])).await?;
        let decimals = v["decimals"]
            .as_u64()
            .filter(|d| *d <= 255)
            .ok_or_else(|| rpc::invalid(PROVIDER, M, "missing token decimals"))?
            as u32;
        let asset = AssetSpec {
            network: n,
            contract: Some(contract),
            decimals,
            symbol: v.get("symbol").and_then(Value::as_str).map(str::to_owned),
            name: v.get("name").and_then(Value::as_str).map(str::to_owned),
            verification: Verification::Unverified,
            provider: PROVIDER,
        };
        self.metadata
            .lock()
            .expect("metadata cache")
            .insert(key, asset.clone());
        Ok(asset)
    }
    pub async fn holdings(&self, n: NetworkId, address: &str) -> Result<Holdings, ProviderError> {
        self.check_chain(n).await?;
        let address = evm_address(address, "balances")?;
        let native = self.native_balance(n, &address).await?;
        let mut assets = vec![(AssetSpec::native(n, PROVIDER), native)];
        let mut cursor: Option<String> = None;
        let mut seen = BTreeSet::new();
        // Bounded discovery; never zero omitted holdings after a capped scan.
        for _ in 0..8 {
            const M: &str = "alchemy_getTokenBalances";
            let mut options = json!({"maxCount":20});
            if let Some(c) = &cursor {
                options["pageKey"] = json!(c);
            }
            let v = self.call(n, M, json!([address, "erc20", options])).await?;
            if v["address"]
                .as_str()
                .map(str::to_ascii_lowercase)
                .as_deref()
                != Some(address.as_str())
            {
                return Err(rpc::invalid(PROVIDER, M, "unexpected wallet"));
            }
            let tokens = v["tokenBalances"]
                .as_array()
                .ok_or_else(|| rpc::invalid(PROVIDER, M, "missing token balances"))?;
            for t in tokens {
                if !t.get("error").is_none_or(Value::is_null) {
                    return Err(rpc::invalid(PROVIDER, M, "token balance lookup failed"));
                }
                let contract = evm_address(rpc::text(t, "contractAddress", PROVIDER, M)?, M)?;
                if !seen.insert(contract.clone()) {
                    continue;
                }
                let raw = rpc::raw_hex(rpc::text(t, "tokenBalance", PROVIDER, M)?, PROVIDER, M)?;
                // Polygon's native pseudo-contract must never be counted as an ERC-20.
                if n == NetworkId::Polygon
                    && contract == "0x0000000000000000000000000000000000001010"
                {
                    continue;
                }
                if raw == BigInt::from(0) {
                    continue;
                }
                if assets.len() >= 51 {
                    return Ok(Holdings {
                        assets,
                        complete: false,
                    });
                }
                assets.push((self.token(n, &contract).await?, raw));
            }
            let next = v
                .get("pageKey")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_owned);
            if next.is_none() {
                return Ok(Holdings {
                    assets,
                    complete: true,
                });
            }
            if next == cursor {
                return Err(ProviderError::RepeatedCursor {
                    provider: PROVIDER,
                    endpoint: M,
                });
            }
            cursor = next;
        }
        Ok(Holdings {
            assets,
            complete: false,
        })
    }
    /// One direction has its own persisted cursor so overlap in the other cannot skip history.
    pub async fn transactions(
        &self,
        n: NetworkId,
        address: &str,
        incoming: bool,
        cursor: Option<&str>,
    ) -> Result<Page, ProviderError> {
        // Alchemy page keys expire; persist a chain block/event boundary instead.
        let v = self.portable_index(n, address, incoming, cursor).await?;
        self.normalize_index(n, address, cursor, &v).await
    }

    async fn portable_index(
        &self,
        n: NetworkId,
        address: &str,
        incoming: bool,
        cursor: Option<&str>,
    ) -> Result<Value, ProviderError> {
        const M: &str = "alchemy_getAssetTransfers";
        let saved = cursor
            .map(|c| {
                serde_json::from_str::<Cursor>(c)
                    .map_err(|_| rpc::invalid(PROVIDER, M, "invalid saved chain boundary"))
            })
            .transpose()?;
        if let Some(c) = &saved {
            rpc::raw_hex(&c.block, PROVIDER, M)?;
        }
        let mut options = json!({"fromBlock":"0x0","toBlock":saved.as_ref().map_or("latest",|c|c.block.as_str()),"category":["external","erc20"],"excludeZeroValue":true,"withMetadata":true,"order":"desc","maxCount":"0x3e8"});
        options[if incoming { "toAddress" } else { "fromAddress" }] =
            json!(evm_address(address, M)?);
        let mut found = saved.is_none();
        let mut collected = Vec::new();
        let mut seen_pages = BTreeSet::new();
        for _ in 0..8 {
            let response = self.call(n, M, json!([options])).await?;
            let rows = response["transfers"]
                .as_array()
                .ok_or_else(|| rpc::invalid(PROVIDER, M, "missing transfers"))?;
            let mut more = false;
            for row in rows {
                if !found {
                    if row["uniqueId"].as_str() == saved.as_ref().map(|c| c.marker.as_str()) {
                        found = true;
                    }
                    continue;
                }
                if collected.len() >= PAGE_SIZE as usize {
                    more = true;
                    break;
                }
                collected.push(row.clone());
            }
            let page_key = response
                .get("pageKey")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty());
            if more || (collected.len() >= PAGE_SIZE as usize && page_key.is_some()) {
                let row = collected
                    .last()
                    .ok_or_else(|| rpc::invalid(PROVIDER, M, "missing chain boundary"))?;
                let block = rpc::text(row, "blockNum", PROVIDER, M)?.to_owned();
                rpc::raw_hex(&block, PROVIDER, M)?;
                let next = Cursor {
                    block,
                    marker: rpc::text(row, "uniqueId", PROVIDER, M)?.into(),
                    hash: rpc::text(row, "hash", PROVIDER, M)?.to_ascii_lowercase(),
                };
                return Ok(
                    json!({"transfers":collected,"pageKey":serde_json::to_string(&next).map_err(|_|rpc::invalid(PROVIDER,M,"invalid boundary"))?}),
                );
            }
            match page_key {
                None if !found => {
                    return Err(rpc::invalid(
                        PROVIDER,
                        M,
                        "saved event boundary no longer available; history remains partial",
                    ));
                }
                None => return Ok(json!({"transfers":collected})),
                Some(next) => {
                    if !seen_pages.insert(next.to_owned()) {
                        return Err(ProviderError::RepeatedCursor {
                            provider: PROVIDER,
                            endpoint: M,
                        });
                    }
                    options["pageKey"] = json!(next);
                }
            }
        }
        Err(rpc::invalid(
            PROVIDER,
            M,
            "chain boundary scan exceeded safe page budget",
        ))
    }

    /// Bounded index contract probe; avoids receipt enrichment during per-chain smoke checks.
    pub async fn transfer_index(
        &self,
        n: NetworkId,
        address: &str,
        incoming: bool,
        cursor: Option<&str>,
    ) -> Result<Value, ProviderError> {
        const M: &str = "alchemy_getAssetTransfers";
        let address = evm_address(address, M)?;
        let mut options = json!({"fromBlock":"0x0","toBlock":"latest","category":["external","erc20"],"excludeZeroValue":true,"withMetadata":true,"order":"desc","maxCount":format!("0x{PAGE_SIZE:x}")});
        options[if incoming { "toAddress" } else { "fromAddress" }] = json!(address);
        if let Some(c) = cursor {
            options["pageKey"] = json!(c);
        }
        let v = self.call(n, M, json!([options])).await?;
        if !v["transfers"].is_array() {
            return Err(rpc::invalid(PROVIDER, M, "missing transfers"));
        }
        Ok(v)
    }

    async fn normalize_index(
        &self,
        n: NetworkId,
        address: &str,
        cursor: Option<&str>,
        v: &Value,
    ) -> Result<Page, ProviderError> {
        const M: &str = "alchemy_getAssetTransfers";
        let rows = v["transfers"]
            .as_array()
            .ok_or_else(|| rpc::invalid(PROVIDER, M, "missing transfers"))?;
        let mut hashes = BTreeSet::new();
        if let Some(c) = cursor.and_then(|c| serde_json::from_str::<Cursor>(c).ok()) {
            hashes.insert(c.hash);
        }
        let mut txs = Vec::new();
        for row in rows {
            let hash = rpc::text(row, "hash", PROVIDER, M)?;
            if !hashes.insert(hash.to_ascii_lowercase()) {
                continue;
            }
            let timestamp = row["metadata"]["blockTimestamp"]
                .as_str()
                .and_then(parse_rfc3339)
                .ok_or_else(|| rpc::invalid(PROVIDER, M, "missing block timestamp"))?;
            txs.push(self.transaction(n, address, hash, timestamp).await?);
        }
        let next = v
            .get("pageKey")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        if next.as_deref().is_some_and(|next| Some(next) == cursor) {
            return Err(ProviderError::RepeatedCursor {
                provider: PROVIDER,
                endpoint: M,
            });
        }
        Ok(Page { txs, next })
    }
    pub async fn transaction(
        &self,
        n: NetworkId,
        address: &str,
        hash: &str,
        timestamp: i64,
    ) -> Result<TxSpec, ProviderError> {
        const M: &str = "eth_getTransactionReceipt";
        let hash = hash.to_ascii_lowercase();
        let receipt = self.call(n, M, json!([hash])).await?;
        let tx = self
            .call(n, "eth_getTransactionByHash", json!([hash]))
            .await?;
        if receipt["transactionHash"]
            .as_str()
            .map(str::to_ascii_lowercase)
            .as_deref()
            != Some(hash.as_str())
            || tx["hash"].as_str().map(str::to_ascii_lowercase).as_deref() != Some(hash.as_str())
        {
            return Err(rpc::invalid(
                PROVIDER,
                M,
                "missing or mismatched transaction",
            ));
        }
        let from = evm_address(rpc::text(&tx, "from", PROVIDER, M)?, M)?;
        let to = tx
            .get("to")
            .and_then(Value::as_str)
            .map(|s| evm_address(s, M))
            .transpose()?;
        let failed = rpc::raw_hex(rpc::text(&receipt, "status", PROVIDER, M)?, PROVIDER, M)?
            == BigInt::from(0);
        let native = AssetSpec::native(n, PROVIDER);
        let fee = if from == address {
            let gas = rpc::raw_hex(rpc::text(&receipt, "gasUsed", PROVIDER, M)?, PROVIDER, M)?;
            let price = rpc::raw_hex(
                rpc::text(&receipt, "effectiveGasPrice", PROVIDER, M)?,
                PROVIDER,
                M,
            )?;
            let l1 = if matches!(n, NetworkId::Base | NetworkId::Optimism) {
                rpc::raw_hex(rpc::text(&receipt, "l1Fee", PROVIDER, M)?, PROVIDER, M)?
            } else {
                BigInt::from(0)
            };
            Some(FeeSpec {
                asset: native.clone(),
                raw: gas * price + l1,
                attribution: FeeAttribution::Exact,
            })
        } else {
            None
        };
        let mut legs = Vec::new();
        if !failed {
            let raw = rpc::raw_hex(rpc::text(&tx, "value", PROVIDER, M)?, PROVIDER, M)?;
            if raw != BigInt::from(0) {
                if from == address && to.as_deref() != Some(address) {
                    legs.push(leg(native.clone(), -raw, to.clone()));
                } else if to.as_deref() == Some(address) && from != address {
                    legs.push(leg(native.clone(), raw, Some(from.clone())));
                }
            }
            let logs = receipt["logs"]
                .as_array()
                .ok_or_else(|| rpc::invalid(PROVIDER, M, "missing logs"))?;
            let mut token_assets = BTreeMap::new();
            for log in logs {
                let Some(topics) = log["topics"].as_array() else {
                    return Err(rpc::invalid(PROVIDER, M, "missing topics"));
                };
                // ERC-721 has four topics, unlike standard ERC-20 Transfer.
                if topics.len() != 3 || topics[0].as_str() != Some(TRANSFER_TOPIC) {
                    continue;
                }
                let source = topic_address(&topics[1], M)?;
                let dest = topic_address(&topics[2], M)?;
                if source != address && dest != address {
                    continue;
                }
                if source == address && dest == address {
                    continue;
                }
                let contract = evm_address(rpc::text(log, "address", PROVIDER, M)?, M)?;
                if n == NetworkId::Polygon
                    && contract == "0x0000000000000000000000000000000000001010"
                {
                    continue;
                }
                let raw = rpc::raw_hex(rpc::text(log, "data", PROVIDER, M)?, PROVIDER, M)?;
                if raw == BigInt::from(0) {
                    continue;
                }
                if !token_assets.contains_key(&contract) {
                    token_assets.insert(contract.clone(), self.token(n, &contract).await?);
                }
                let asset = token_assets[&contract].clone();
                legs.push(if source == address {
                    leg(asset, -raw, Some(dest))
                } else {
                    leg(asset, raw, Some(source))
                });
            }
        }
        Ok(TxSpec {
            network: n,
            hash,
            part: None,
            block_height: rpc::raw_hex(
                rpc::text(&receipt, "blockNumber", PROVIDER, M)?,
                PROVIDER,
                M,
            )?
            .to_i64(),
            position: rpc::raw_hex(
                rpc::text(&receipt, "transactionIndex", PROVIDER, M)?,
                PROVIDER,
                M,
            )?
            .to_u64()
            .map(|n| format!("{n:010}")),
            occurred_at: timestamp,
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
            evidence: json!({"source":"receipt_logs_and_transaction","discovery":"external/erc20 transfer index","internal_transfers":"not imported"}),
        })
    }
}
fn evm_address(s: &str, m: &'static str) -> Result<String, ProviderError> {
    if s.len() != 42 || !s.starts_with("0x") || !s[2..].bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(rpc::invalid(PROVIDER, m, "invalid EVM address"));
    }
    Ok(s.to_ascii_lowercase())
}
fn topic_address(v: &Value, m: &'static str) -> Result<String, ProviderError> {
    let s = v
        .as_str()
        .filter(|s| {
            s.len() == 66
                && s.starts_with("0x")
                && s[2..].bytes().all(|b| b.is_ascii_hexdigit())
                && s[2..26] == *"000000000000000000000000"
        })
        .ok_or_else(|| rpc::invalid(PROVIDER, m, "invalid address topic"))?;
    evm_address(&format!("0x{}", &s[26..]), m)
}
fn leg(asset: AssetSpec, raw: BigInt, counterparty: Option<String>) -> LegSpec {
    LegSpec {
        asset,
        direction: if raw < BigInt::from(0) {
            Direction::Out
        } else {
            Direction::In
        },
        signed_raw: raw,
        counterparty,
        leg_type: "transfer".into(),
        decoding: Decoding::Partial,
        unresolved: true,
    }
}
