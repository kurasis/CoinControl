//! Independent balance reserves. RPC does not imply indexed wallet history.
//! Every reserve leaves historical evidence untouched and returns partial coverage.
use crate::{
    ProviderError,
    helius::Helius,
    http::{Budget, HttpClient, HttpConfig},
    rpc,
};
use num_bigint::BigInt;
use num_traits::ToPrimitive;
use portfolio_core::{
    address::{normalize_address, tron_to_hex},
    network::NetworkId,
};
use portfolio_store::ingest::{AssetSpec, Verification};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use url::Url;

pub const EVM: [NetworkId; 6] = [
    NetworkId::Ethereum,
    NetworkId::Base,
    NetworkId::Arbitrum,
    NetworkId::Optimism,
    NetworkId::Polygon,
    NetworkId::Bsc,
];
pub const BLOCKSCOUT_FREE_NETWORKS: [NetworkId; 3] = [
    NetworkId::Ethereum,
    NetworkId::Arbitrum,
    NetworkId::Optimism,
];
pub const ETHERSCAN_NETWORKS: [NetworkId; 3] =
    [NetworkId::Ethereum, NetworkId::Arbitrum, NetworkId::Polygon];
const SOL_MAINNET: &str = "https://solana-mainnet.core.chainstack.com/";
#[derive(Clone, Copy)]
pub enum Kind {
    Rpc,
    Blockscout,
    Etherscan,
    TonCenter,
}
pub struct Reserve {
    http: HttpClient,
    endpoints: BTreeMap<NetworkId, Url>,
    kind: Kind,
    solana: Option<Helius>,
}
pub struct Snapshot {
    pub warnings: Vec<String>,
    pub assets: Vec<(AssetSpec, BigInt)>,
    pub height: Option<i64>,
}

impl Reserve {
    pub fn new(
        provider: &'static str,
        key: &str,
        budget: Arc<Budget>,
    ) -> Result<Self, ProviderError> {
        let keyed = matches!(provider, "blockscout" | "etherscan" | "drpc" | "chainstack");
        if keyed && key.trim().is_empty() {
            return Err(ProviderError::MissingKey { provider });
        }
        let mut endpoints = Vec::new();
        let mut headers = HeaderMap::new();
        let kind = match provider {
            "blockscout" => {
                headers.insert(
                    AUTHORIZATION,
                    HeaderValue::from_str(&format!("Bearer {}", key.trim()))
                        .map_err(|_| rpc::invalid(provider, "config", "invalid credential"))?,
                );
                for n in BLOCKSCOUT_FREE_NETWORKS {
                    endpoints.push((n, "https://api.blockscout.com/".to_owned()));
                }
                Kind::Blockscout
            }
            "etherscan" => {
                for n in ETHERSCAN_NETWORKS {
                    let mut u = Url::parse("https://api.etherscan.io/v2/api").expect("static URL");
                    u.query_pairs_mut()
                        .append_pair("chainid", &n.evm_chain_id().unwrap().to_string())
                        .append_pair("apikey", key.trim());
                    endpoints.push((n, u.to_string()));
                }
                Kind::Etherscan
            }
            "toncenter" => {
                if !key.trim().is_empty() {
                    headers.insert(
                        "x-api-key",
                        HeaderValue::from_str(key.trim())
                            .map_err(|_| rpc::invalid(provider, "config", "invalid credential"))?,
                    );
                }
                endpoints.push((NetworkId::Ton, "https://toncenter.com/api/v3/".into()));
                Kind::TonCenter
            }
            "drpc" => {
                for (n, name) in EVM
                    .into_iter()
                    .zip(["ethereum", "base", "arbitrum", "optimism", "polygon", "bsc"])
                {
                    let mut u =
                        Url::parse(&format!("https://lb.drpc.live/{name}/")).expect("static URL");
                    u.path_segments_mut()
                        .expect("HTTP path")
                        .pop_if_empty()
                        .push(key.trim());
                    endpoints.push((n, u.to_string()));
                }
                Kind::Rpc
            }
            "publicnode" => {
                for (n, host) in EVM
                    .into_iter()
                    .zip([
                        "ethereum-rpc",
                        "base-rpc",
                        "arbitrum-one-rpc",
                        "optimism-rpc",
                        "polygon-bor-rpc",
                        "bsc-rpc",
                    ])
                    .chain([
                        (NetworkId::Solana, "solana-rpc"),
                        (NetworkId::Tron, "tron-solidity-rpc"),
                    ])
                {
                    endpoints.push((n, format!("https://{host}.publicnode.com/")));
                }
                Kind::Rpc
            }
            "chainstack" => {
                let value = key.trim();
                let u = if value.starts_with("https://") {
                    let u = Url::parse(value)
                        .map_err(|_| rpc::invalid(provider, "config", "invalid RPC endpoint"))?;
                    let host = u.host_str().unwrap_or("");
                    if !(host.ends_with(".chainstack.com") || host.ends_with(".p2pify.com"))
                        || !u.username().is_empty()
                        || u.password().is_some()
                        || u.fragment().is_some()
                    {
                        return Err(rpc::invalid(
                            provider,
                            "config",
                            "use the Solana HTTPS endpoint from Access and credentials",
                        ));
                    }
                    u
                } else {
                    // A platform management key cannot authenticate blockchain RPC.
                    if value.starts_with("cp_") || value.contains(char::is_whitespace) {
                        return Err(rpc::invalid(
                            provider,
                            "config",
                            "use a Solana node auth token or its full HTTPS RPC endpoint, not a platform management key",
                        ));
                    }
                    let mut u = Url::parse(SOL_MAINNET).expect("static URL");
                    u.path_segments_mut()
                        .expect("HTTP path")
                        .pop_if_empty()
                        .push(value);
                    u
                };
                endpoints.push((NetworkId::Solana, u.to_string()));
                Kind::Rpc
            }
            _ => return Err(rpc::invalid(provider, "config", "unknown reserve")),
        };
        Self::with_config(
            provider,
            kind,
            &endpoints,
            budget,
            headers,
            HttpConfig {
                min_interval: Duration::from_secs(1),
                max_retries: 1,
                timeout: Duration::from_secs(12),
                ..HttpConfig::default()
            },
        )
    }
    pub fn with_config(
        provider: &'static str,
        kind: Kind,
        endpoints: &[(NetworkId, String)],
        budget: Arc<Budget>,
        headers: HeaderMap,
        config: HttpConfig,
    ) -> Result<Self, ProviderError> {
        let endpoints: BTreeMap<_, _> = endpoints
            .iter()
            .map(|(n, u)| {
                Url::parse(u)
                    .map(|u| (*n, u))
                    .map_err(|_| rpc::invalid(provider, "config", "invalid endpoint"))
            })
            .collect::<Result<_, _>>()?;
        let solana = endpoints
            .get(&NetworkId::Solana)
            .map(|u| Helius::with_endpoint(provider, u.clone(), budget.clone(), config.clone()))
            .transpose()?;
        Ok(Self {
            http: HttpClient::new(provider, config, budget, headers)?,
            endpoints,
            kind,
            solana,
        })
    }
    pub fn provider(&self) -> &'static str {
        self.http.provider()
    }
    pub fn supports(&self, n: NetworkId) -> bool {
        self.endpoints.contains_key(&n)
    }
    pub fn clients(&self) -> Vec<&HttpClient> {
        let mut c = vec![&self.http];
        if let Some(s) = &self.solana {
            c.push(s.http())
        };
        c
    }
    pub fn http(&self) -> &HttpClient {
        &self.http
    }
    async fn get(&self, u: Url, operation: &'static str) -> Result<Value, ProviderError> {
        let body = self
            .http
            .get_cost(
                operation,
                u,
                if matches!(self.kind, Kind::Blockscout) {
                    20
                } else {
                    0
                },
            )
            .await?;
        body.json(self.provider(), operation)
    }
    pub async fn snapshot(
        &self,
        n: NetworkId,
        address: &str,
        known: &[AssetSpec],
    ) -> Result<Snapshot, ProviderError> {
        let p = self.provider();
        let base = self
            .endpoints
            .get(&n)
            .ok_or_else(|| rpc::invalid(p, "balances", "unsupported network"))?
            .clone();
        if let Some(s) = &self.solana
            && n == NetworkId::Solana
        {
            s.check_mainnet().await?;
            let mut h = s.reserve_holdings(address).await?;
            for (a, _) in &mut h.assets {
                a.provider = p;
            }
            return Ok(Snapshot {
                warnings: h.warnings,
                assets: h.assets,
                height: Some(h.slot),
            });
        }
        match self.kind {
            Kind::Blockscout => self.blockscout(base, n, address).await,
            Kind::Etherscan => self.etherscan(base, n, address, known).await,
            Kind::TonCenter => self.toncenter(base, address, known).await,
            Kind::Rpc if n == NetworkId::Tron => self.tron(base, address, known).await,
            Kind::Rpc => self.evm(base, n, address, known).await,
        }
    }
    async fn evm(
        &self,
        u: Url,
        n: NetworkId,
        address: &str,
        known: &[AssetSpec],
    ) -> Result<Snapshot, ProviderError> {
        let p = self.provider();
        let chain = rpc::call(&self.http, u.clone(), "eth_chainId", json!([]), 0).await?;
        if rpc::raw_hex(chain.as_str().unwrap_or(""), p, "eth_chainId")?.to_u64()
            != n.evm_chain_id()
        {
            return Err(rpc::invalid(p, "eth_chainId", "wrong mainnet"));
        }
        let height = rpc::call(&self.http, u.clone(), "eth_blockNumber", json!([]), 0).await?;
        let block = height
            .as_str()
            .ok_or_else(|| rpc::invalid(p, "eth_blockNumber", "missing height"))?;
        let h = rpc::raw_hex(block, p, "eth_blockNumber")?
            .to_i64()
            .ok_or_else(|| rpc::invalid(p, "eth_blockNumber", "height out of range"))?;
        let v = rpc::call(
            &self.http,
            u.clone(),
            "eth_getBalance",
            json!([address, block]),
            0,
        )
        .await?;
        let mut assets = vec![(
            AssetSpec::native(n, p),
            rpc::raw_hex(v.as_str().unwrap_or(""), p, "eth_getBalance")?,
        )];
        let addr = evm_address(address, p)?;
        for a in known.iter().filter(|a| a.contract.is_some()).take(20) {
            let contract = evm_address(a.contract.as_deref().unwrap(), p)?;
            if n == NetworkId::Polygon && contract == "0x0000000000000000000000000000000000001010" {
                continue;
            }
            let v = rpc::call(
                &self.http,
                u.clone(),
                "eth_call",
                json!([{"to":contract,"data":format!("0x70a08231{:0>64}",&addr[2..])},block]),
                0,
            )
            .await?;
            let mut a = a.clone();
            a.provider = p;
            assets.push((a, rpc::raw_hex(v.as_str().unwrap_or(""), p, "eth_call")?));
        }
        Ok(Snapshot {
            warnings: Vec::new(),
            assets,
            height: Some(h),
        })
    }
    async fn etherscan(
        &self,
        base: Url,
        n: NetworkId,
        address: &str,
        known: &[AssetSpec],
    ) -> Result<Snapshot, ProviderError> {
        let p = self.provider();
        evm_address(address, p)?;
        let mut assets = Vec::new();
        for a in std::iter::once(AssetSpec::native(n, p)).chain(
            known
                .iter()
                .filter(|a| a.contract.is_some())
                .take(20)
                .cloned(),
        ) {
            if n == NetworkId::Polygon
                && a.contract.as_deref() == Some("0x0000000000000000000000000000000000001010")
            {
                continue;
            }
            let mut u = base.clone();
            u.query_pairs_mut()
                .append_pair("module", "account")
                .append_pair(
                    "action",
                    if a.contract.is_some() {
                        "tokenbalance"
                    } else {
                        "balance"
                    },
                )
                .append_pair("address", address)
                .append_pair("tag", "latest");
            if let Some(c) = &a.contract {
                evm_address(c, p)?;
                u.query_pairs_mut().append_pair("contractaddress", c);
            }
            let v = self.get(u, "account balance").await?;
            if v["status"].as_str() != Some("1") {
                return Err(indexer_error(p, &v));
            }
            let raw = decimal(&v["result"], p, "account balance")?;
            let mut a = a;
            a.provider = p;
            assets.push((a, raw));
        }
        Ok(Snapshot {
            warnings: Vec::new(),
            assets,
            height: None,
        })
    }
    async fn blockscout(
        &self,
        base: Url,
        n: NetworkId,
        address: &str,
    ) -> Result<Snapshot, ProviderError> {
        let p = self.provider();
        evm_address(address, p)?;
        let path = format!("{}/api/v2/addresses/{address}", n.evm_chain_id().unwrap());
        let info = self
            .get(
                base.join(&path)
                    .map_err(|_| rpc::invalid(p, "address", "invalid path"))?,
                "address",
            )
            .await?;
        if info["hash"].as_str().map(str::to_ascii_lowercase) != Some(address.to_ascii_lowercase())
        {
            return Err(rpc::invalid(p, "address", "unexpected wallet"));
        }
        if info["coin_balance"].is_null() {
            return Err(ProviderError::CapabilityUnavailable {
                provider: p,
                endpoint: "address balance not indexed",
            });
        }
        let mut assets = vec![(
            AssetSpec::native(n, p),
            decimal(&info["coin_balance"], p, "address")?,
        )];
        let mut u = base
            .join(&format!("{path}/tokens"))
            .map_err(|_| rpc::invalid(p, "tokens", "invalid path"))?;
        u.query_pairs_mut().append_pair("type", "ERC-20");
        // Bounded discovery. Omitted tokens remain stale; they never become zero.
        for _ in 0..4 {
            let v = self.get(u.clone(), "tokens").await?;
            let rows = v["items"]
                .as_array()
                .ok_or_else(|| rpc::invalid(p, "tokens", "missing balances"))?;
            for row in rows {
                let t = &row["token"];
                if t["type"].as_str() != Some("ERC-20") {
                    continue;
                }
                let contract = evm_address(t["address_hash"].as_str().unwrap_or(""), p)?;
                if n == NetworkId::Polygon
                    && contract == "0x0000000000000000000000000000000000001010"
                {
                    continue;
                }
                let decimals = decimal(&t["decimals"], p, "tokens")?
                    .to_u32()
                    .filter(|d| *d <= 255)
                    .ok_or_else(|| rpc::invalid(p, "tokens", "invalid decimals"))?;
                let a = AssetSpec {
                    network: n,
                    contract: Some(contract),
                    decimals,
                    symbol: t["symbol"].as_str().map(str::to_owned),
                    name: t["name"].as_str().map(str::to_owned),
                    verification: Verification::Unverified,
                    provider: p,
                };
                assets.push((a, decimal(&row["value"], p, "tokens")?));
            }
            let Some(next) = v.get("next_page_params").filter(|v| !v.is_null()) else {
                break;
            };
            let params = next
                .as_object()
                .ok_or_else(|| rpc::invalid(p, "tokens", "invalid pagination"))?;
            let mut next_url = base
                .join(&format!("{path}/tokens"))
                .map_err(|_| rpc::invalid(p, "tokens", "invalid path"))?;
            next_url.query_pairs_mut().append_pair("type", "ERC-20");
            for (k, v) in params {
                if !matches!(k.as_str(), "fiat_value" | "id" | "value" | "items_count") {
                    return Err(rpc::invalid(p, "tokens", "unexpected pagination field"));
                }
                if !v.is_null() {
                    next_url.query_pairs_mut().append_pair(
                        k,
                        &v.as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| v.to_string()),
                    );
                }
            }
            if next_url == u {
                return Err(ProviderError::RepeatedCursor {
                    provider: p,
                    endpoint: "tokens",
                });
            }
            u = next_url;
        }
        Ok(Snapshot {
            warnings: Vec::new(),
            assets,
            height: info["block_number_balance_updated_at"].as_i64(),
        })
    }
    async fn toncenter(
        &self,
        base: Url,
        address: &str,
        known: &[AssetSpec],
    ) -> Result<Snapshot, ProviderError> {
        let p = self.provider();
        let canonical = normalize_address(NetworkId::Ton, address)
            .map_err(|_| rpc::invalid(p, "accountStates", "invalid account"))?
            .canonical;
        let mut u = base
            .join("accountStates")
            .map_err(|_| rpc::invalid(p, "accountStates", "invalid path"))?;
        u.query_pairs_mut().append_pair("address", &canonical);
        let v = self.get(u, "accountStates").await?;
        let accounts = v["accounts"]
            .as_array()
            .ok_or_else(|| rpc::invalid(p, "accountStates", "missing accounts"))?;
        let row = accounts
            .iter()
            .find(|r| {
                r["address"]
                    .as_str()
                    .and_then(|a| normalize_address(NetworkId::Ton, a).ok())
                    .is_some_and(|a| a.canonical == canonical)
            })
            .ok_or_else(|| rpc::invalid(p, "accountStates", "missing requested account"))?;
        let mut assets = vec![(
            AssetSpec::native(NetworkId::Ton, p),
            decimal(&row["balance"], p, "accountStates")?,
        )];
        // Known Jettons have exact cached decimals; unknown masters cannot be guessed.
        let mut u = base
            .join("jetton/wallets")
            .map_err(|_| rpc::invalid(p, "jetton/wallets", "invalid path"))?;
        u.query_pairs_mut()
            .append_pair("owner_address", &canonical)
            .append_pair("limit", "1000")
            .append_pair("offset", "0");
        let v = self.get(u, "jetton/wallets").await?;
        let wallets = v["jetton_wallets"]
            .as_array()
            .ok_or_else(|| rpc::invalid(p, "jetton/wallets", "missing wallets"))?;
        for row in wallets {
            let owner = row["owner"]
                .as_str()
                .and_then(|a| normalize_address(NetworkId::Ton, a).ok())
                .ok_or_else(|| rpc::invalid(p, "jetton/wallets", "missing owner"))?;
            if owner.canonical != canonical {
                return Err(rpc::invalid(p, "jetton/wallets", "unexpected owner"));
            }
            let master = row["jetton"]
                .as_str()
                .and_then(|a| normalize_address(NetworkId::Ton, a).ok())
                .ok_or_else(|| rpc::invalid(p, "jetton/wallets", "missing master"))?;
            let cached = known
                .iter()
                .find(|a| a.contract.as_deref() == Some(master.canonical.as_str()))
                .cloned();
            let info = v["metadata"][&master.canonical]["token_info"]
                .as_array()
                .and_then(|xs| {
                    xs.iter().find(|t| {
                        t["type"].as_str() == Some("jetton_masters")
                            && t["valid"].as_bool() == Some(true)
                    })
                });
            let asset = cached.or_else(|| {
                let info = info?;
                let d = decimal(&info["extra"]["decimals"], p, "jetton/wallets")
                    .ok()?
                    .to_u32()
                    .filter(|d| *d <= 255)?;
                Some(AssetSpec {
                    network: NetworkId::Ton,
                    contract: Some(master.canonical.clone()),
                    decimals: d,
                    symbol: info["symbol"].as_str().map(str::to_owned),
                    name: info["name"].as_str().map(str::to_owned),
                    verification: if info["is_scam"].as_bool() == Some(true) {
                        Verification::Spam
                    } else {
                        Verification::Unverified
                    },
                    provider: p,
                })
            });
            if let Some(mut a) = asset {
                a.provider = p;
                assets.push((a, decimal(&row["balance"], p, "jetton/wallets")?));
            }
        }
        Ok(Snapshot {
            warnings: Vec::new(),
            assets,
            height: None,
        })
    }
    async fn tron(
        &self,
        base: Url,
        address: &str,
        known: &[AssetSpec],
    ) -> Result<Snapshot, ProviderError> {
        let p = self.provider();
        let u = base
            .join("walletsolidity/getaccount")
            .map_err(|_| rpc::invalid(p, "getaccount", "invalid path"))?;
        let body = self
            .http
            .post_json("getaccount", u, &json!({"address":address,"visible":true}))
            .await?;
        let v: Value = body.json(p, "getaccount")?;
        if v["address"].as_str() != Some(address) {
            return Err(rpc::invalid(
                p,
                "getaccount",
                "missing requested account (unactivated accounts are not inferred as zero)",
            ));
        }
        let mut total = optional_decimal(&v["balance"], p)?;
        for field in ["frozenV2", "unfrozenV2"] {
            if let Some(rows) = v[field].as_array() {
                for r in rows {
                    total += optional_decimal(
                        &r[if field == "frozenV2" {
                            "amount"
                        } else {
                            "unfreeze_amount"
                        }],
                        p,
                    )?;
                }
            }
        }
        total += optional_decimal(&v["delegated_frozenV2_balance_for_bandwidth"], p)?;
        total += optional_decimal(
            &v["account_resource"]["delegated_frozenV2_balance_for_energy"],
            p,
        )?;
        let mut assets = vec![(AssetSpec::native(NetworkId::Tron, p), total)];
        let owner = tron_to_hex(address)
            .map_err(|_| rpc::invalid(p, "triggerconstantcontract", "invalid owner"))?;
        for a in known.iter().filter(|a| a.contract.is_some()).take(20) {
            let c = a.contract.as_deref().unwrap();
            let u = base
                .join("walletsolidity/triggerconstantcontract")
                .map_err(|_| rpc::invalid(p, "triggerconstantcontract", "invalid path"))?;
            let b=self.http.post_json("triggerconstantcontract",u,&json!({"owner_address":address,"contract_address":c,"function_selector":"balanceOf(address)","parameter":format!("{:0>64}",&owner[2..]),"visible":true})).await?;
            let v: Value = b.json(p, "triggerconstantcontract")?;
            if v["result"]["result"].as_bool() != Some(true) {
                return Err(rpc::invalid(
                    p,
                    "triggerconstantcontract",
                    "token call failed",
                ));
            }
            let raw = v["constant_result"][0]
                .as_str()
                .ok_or_else(|| rpc::invalid(p, "triggerconstantcontract", "missing result"))?;
            let raw = rpc::raw_hex(&format!("0x{raw}"), p, "triggerconstantcontract")?;
            let mut a = a.clone();
            a.provider = p;
            assets.push((a, raw));
        }
        Ok(Snapshot {
            warnings: Vec::new(),
            assets,
            height: None,
        })
    }
}
fn optional_decimal(v: &Value, p: &'static str) -> Result<BigInt, ProviderError> {
    if v.is_null() {
        Ok(BigInt::from(0))
    } else {
        decimal(v, p, "getaccount")
    }
}
fn decimal(v: &Value, p: &'static str, m: &'static str) -> Result<BigInt, ProviderError> {
    let s = v
        .as_str()
        .map(str::to_owned)
        .or_else(|| v.as_u64().map(|n| n.to_string()))
        .ok_or_else(|| rpc::invalid(p, m, "missing integer amount"))?;
    if s.is_empty() || s.len() > 100 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(rpc::invalid(p, m, "invalid integer amount"));
    }
    s.parse()
        .map_err(|_| rpc::invalid(p, m, "invalid integer amount"))
}
fn evm_address(s: &str, p: &'static str) -> Result<String, ProviderError> {
    if s.len() != 42 || !s.starts_with("0x") || !s[2..].bytes().all(|b| b.is_ascii_hexdigit()) {
        Err(rpc::invalid(p, "balances", "invalid EVM address"))
    } else {
        Ok(s.to_ascii_lowercase())
    }
}
fn indexer_error(p: &'static str, v: &Value) -> ProviderError {
    let msg = v["result"].as_str().unwrap_or("").to_ascii_lowercase();
    if msg.contains("rate limit") || msg.contains("limit reached") {
        ProviderError::RateLimited {
            provider: p,
            endpoint: "account balance",
            retry_after_secs: None,
        }
    } else if msg.contains("key") {
        ProviderError::Auth {
            provider: p,
            endpoint: "account balance",
            status: 401,
        }
    } else if msg.contains("free")
        || msg.contains("paid")
        || msg.contains("upgrade")
        || msg.contains("not supported")
    {
        ProviderError::CapabilityUnavailable {
            provider: p,
            endpoint: "account balance",
        }
    } else {
        rpc::invalid(
            p,
            "account balance",
            "indexer rejected request or free network is unavailable",
        )
    }
}
