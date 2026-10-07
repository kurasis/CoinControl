//! Authenticated Ankr Advanced API token balances. No USD values or history
//! from this endpoint are used as accounting evidence.
use crate::{
    ProviderError,
    http::{Budget, HttpClient, HttpConfig},
    rpc,
};
use num_bigint::BigInt;
use portfolio_core::{address::normalize_address, network::NetworkId};
use portfolio_store::ingest::{AssetSpec, Verification};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};
use url::Url;

pub const PROVIDER: &str = "ankr";
// Slightly conservative spacing also prevents a boundary burst in a rolling window.
pub const NODE_INTERVAL: Duration = Duration::from_millis(34);
pub const ADVANCED_INTERVAL: Duration = Duration::from_millis(2010);
const METHOD: &str = "ankr_getAccountBalance";
pub const PAGE_SIZE: usize = 200;

pub fn chain(network: NetworkId) -> Option<&'static str> {
    Some(match network {
        NetworkId::Ethereum => "eth",
        NetworkId::Base => "base",
        NetworkId::Arbitrum => "arbitrum",
        NetworkId::Optimism => "optimism",
        NetworkId::Polygon => "polygon",
        NetworkId::Bsc => "bsc",
        NetworkId::Solana => "solana",
        _ => return None,
    })
}

pub struct BalancePage {
    pub tokens: Vec<(AssetSpec, BigInt)>,
    pub next: Option<String>,
}

pub struct Ankr {
    http: HttpClient,
    url: Url,
}

impl Ankr {
    pub fn new(key: &str, budget: Arc<Budget>) -> Result<Self, ProviderError> {
        Self::with_config(
            endpoint("multichain", key)?,
            budget,
            HttpConfig {
                min_interval: ADVANCED_INTERVAL,
                max_retries: 1,
                ..HttpConfig::default()
            },
        )
    }

    pub fn with_config(
        url: Url,
        budget: Arc<Budget>,
        config: HttpConfig,
    ) -> Result<Self, ProviderError> {
        let http = HttpClient::new(PROVIDER, config, budget, Default::default())?;
        configure_pacing(&http, &url, true);
        Ok(Self { http, url })
    }

    pub fn http(&self) -> &HttpClient {
        &self.http
    }

    pub async fn balances_page(
        &self,
        network: NetworkId,
        address: &str,
        cursor: Option<&str>,
    ) -> Result<BalancePage, ProviderError> {
        if network.evm_chain_id().is_none() {
            return Err(rpc::invalid(
                PROVIDER,
                METHOD,
                "Advanced balances require an EVM mainnet",
            ));
        }
        let address = normalize_address(network, address)
            .map_err(|_| rpc::invalid(PROVIDER, METHOD, "invalid wallet"))?
            .canonical;
        let mut params = json!({"blockchain":chain(network).unwrap(),"walletAddress":address,
            "nativeFirst":true,"onlyWhitelisted":false,"pageSize":PAGE_SIZE});
        if let Some(cursor) = cursor {
            params["pageToken"] = json!(cursor);
        }
        let value = rpc::call(&self.http, self.url.clone(), METHOD, params, 700).await?;
        parse_page(network, &address, &value)
    }
}

pub(crate) fn endpoint(chain: &str, key: &str) -> Result<Url, ProviderError> {
    if key.trim().is_empty() {
        return Err(ProviderError::MissingKey { provider: PROVIDER });
    }
    let mut url = Url::parse(&format!("https://rpc.ankr.com/{chain}/")).expect("static URL");
    url.path_segments_mut()
        .expect("HTTP path")
        .pop_if_empty()
        .push(key.trim());
    Ok(url)
}

fn parse_page(
    network: NetworkId,
    address: &str,
    value: &Value,
) -> Result<BalancePage, ProviderError> {
    let invalid = || rpc::invalid(PROVIDER, METHOD, "invalid token balance evidence");
    let assets = value["assets"].as_array().ok_or_else(invalid)?;
    if assets.len() > PAGE_SIZE {
        return Err(invalid());
    }
    let mut tokens = Vec::new();
    let mut seen = BTreeSet::new();
    for asset in assets {
        if asset["blockchain"].as_str() != chain(network)
            || asset["holderAddress"]
                .as_str()
                .map(str::to_ascii_lowercase)
                .as_deref()
                != Some(address)
        {
            return Err(invalid());
        }
        match asset["tokenType"].as_str() {
            Some("NATIVE") => continue, // Native balance is independently obtained from Node RPC.
            Some("ERC20") => {}
            _ => return Err(invalid()),
        }
        let contract = normalize_address(
            network,
            asset["contractAddress"].as_str().ok_or_else(invalid)?,
        )
        .map_err(|_| invalid())?
        .canonical;
        if network == NetworkId::Polygon && contract == "0x0000000000000000000000000000000000001010"
        {
            continue;
        }
        if !seen.insert(contract.clone()) {
            return Err(invalid());
        }
        let decimals = asset["tokenDecimals"]
            .as_u64()
            .filter(|d| *d <= 255)
            .ok_or_else(invalid)? as u32;
        let raw = asset["balanceRawInteger"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 78 && s.bytes().all(|b| b.is_ascii_digit()))
            .ok_or_else(invalid)?;
        let raw = raw.parse::<BigInt>().map_err(|_| invalid())?;
        if raw.bits() > 256 {
            return Err(invalid());
        }
        tokens.push((
            AssetSpec {
                network,
                contract: Some(contract),
                decimals,
                symbol: asset["tokenSymbol"].as_str().map(str::to_owned),
                name: asset["tokenName"].as_str().map(str::to_owned),
                verification: Verification::Unverified,
                provider: PROVIDER,
            },
            raw,
        ));
    }
    let next = match value.get("nextPageToken") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) if s.is_empty() => None,
        Some(Value::String(s)) if s.len() <= 4096 => Some(s.clone()),
        _ => return Err(invalid()),
    };
    Ok(BalancePage { tokens, next })
}

// Preserve spacing when a synchronization or connection probe rebuilds its
// budget. Only a digest of the authenticated path segment identifies the key.
// API classes have distinct gates; chains and clients within each class share it.
#[derive(Default)]
struct Pacing {
    node: crate::http::PacingGate,
    advanced: crate::http::PacingGate,
}
pub(crate) fn configure_pacing(http: &HttpClient, url: &Url, advanced: bool) {
    static PACING: OnceLock<Mutex<BTreeMap<[u8; 32], Pacing>>> = OnceLock::new();
    let credential = url
        .path_segments()
        .and_then(|mut p| p.next_back())
        .unwrap_or("");
    let digest: [u8; 32] = Sha256::digest(credential.as_bytes()).into();
    let mut pacing = PACING
        .get_or_init(Default::default)
        .lock()
        .expect("Ankr pacing");
    let gates = pacing.entry(digest).or_default();
    http.set_pacing_gate(if advanced {
        gates.advanced.clone()
    } else {
        gates.node.clone()
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    const WALLET: &str = "0x1db3439a222c519ab44bb1144fc28167b4fa6ee6";
    fn asset() -> Value {
        json!({"blockchain":"bsc","holderAddress":WALLET,"tokenType":"ERC20",
            "contractAddress":"0x55d398326f99059ff775485246999027b3197955",
            "tokenDecimals":18,"balanceRawInteger":"900719925474099312345678901234567890",
            "balance":"1.234","balanceUsd":"999999","tokenSymbol":"USDT"})
    }
    #[test]
    fn indexed_raw_amounts_are_exact_and_do_not_trust_prices_or_whitelisting() {
        let page = parse_page(
            NetworkId::Bsc,
            WALLET,
            &json!({"assets":[asset()],"nextPageToken":"next"}),
        )
        .unwrap();
        assert_eq!(
            page.tokens[0].1.to_string(),
            "900719925474099312345678901234567890"
        );
        assert_eq!(page.tokens[0].0.verification, Verification::Unverified);
        assert_eq!(page.next.as_deref(), Some("next"));
    }
    #[test]
    fn wrong_chain_wallet_duplicates_and_malformed_amounts_are_rejected() {
        for (field, bad) in [
            ("blockchain", json!("eth")),
            (
                "holderAddress",
                json!("0x0000000000000000000000000000000000000000"),
            ),
            ("tokenDecimals", json!(256)),
            ("tokenDecimals", json!("18")),
            ("balanceRawInteger", json!(9007199254740993u64)),
            ("balanceRawInteger", json!("-1")),
            ("balanceRawInteger", json!("1e20")),
            ("balanceRawInteger", json!("9".repeat(78))),
            ("contractAddress", json!("invalid")),
        ] {
            let mut a = asset();
            a[field] = bad;
            assert!(
                parse_page(NetworkId::Bsc, WALLET, &json!({"assets":[a]})).is_err(),
                "{field}"
            );
        }
        assert!(parse_page(NetworkId::Bsc, WALLET, &json!({"assets":[asset(),asset()]})).is_err());
        assert!(
            parse_page(
                NetworkId::Bsc,
                WALLET,
                &json!({"assets":[],"nextPageToken":42})
            )
            .is_err()
        );
    }
    #[test]
    fn native_and_polygon_system_token_do_not_duplicate_rpc_native_balance() {
        let mut a = asset();
        a["tokenType"] = json!("NATIVE");
        assert!(
            parse_page(NetworkId::Bsc, WALLET, &json!({"assets":[a]}))
                .unwrap()
                .tokens
                .is_empty()
        );
        let mut a = asset();
        a["blockchain"] = json!("polygon");
        a["contractAddress"] = json!("0x0000000000000000000000000000000000001010");
        assert!(
            parse_page(NetworkId::Polygon, WALLET, &json!({"assets":[a]}))
                .unwrap()
                .tokens
                .is_empty()
        );
    }
    #[test]
    fn shipped_intervals_bound_rolling_windows_without_bursts() {
        assert!(NODE_INTERVAL.as_nanos() * 30 > 1_000_000_000);
        assert!(ADVANCED_INTERVAL.as_nanos() * 30 > 60_000_000_000);
    }
    #[tokio::test]
    async fn pacing_survives_budget_rebuilds_and_api_classes_are_independent() {
        use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .mount(&server)
            .await;
        let url = Url::parse(&format!("{}/pacing-distinct-key", server.uri())).unwrap();
        let config = HttpConfig {
            min_interval: Duration::from_millis(400),
            ..HttpConfig::default()
        };
        let a = Ankr::with_config(url.clone(), Budget::limited(5), config.clone()).unwrap();
        let first_start = std::time::Instant::now();
        a.http
            .post_json("probe", url.clone(), &json!({}))
            .await
            .unwrap();
        let b = Ankr::with_config(url.clone(), Budget::limited(5), config).unwrap();
        let started = std::time::Instant::now();
        let body = json!({});
        let pending = b.http.post_json("probe", url.clone(), &body);
        let node = HttpClient::new(
            PROVIDER,
            HttpConfig {
                min_interval: NODE_INTERVAL,
                ..HttpConfig::default()
            },
            a.http.budget().clone(),
            Default::default(),
        )
        .unwrap();
        configure_pacing(&node, &url, false);
        let node_read = async {
            node.post_json("probe", url.clone(), &json!({}))
                .await
                .unwrap();
            assert!(
                started.elapsed() < Duration::from_millis(350),
                "Node inherited Advanced pacing"
            );
        };
        let (advanced_result, ()) = tokio::join!(pending, node_read);
        advanced_result.unwrap();
        assert!(first_start.elapsed() >= Duration::from_millis(400));
        assert_eq!(a.http.budget().used(), 2);
        assert_eq!(b.http.budget().used(), 1);
    }
}
