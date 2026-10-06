//! Live read-only provider tests (TESTING.md Layer B).
//!
//! Opt-in only: set `RUN_LIVE_API_TESTS=1` and `LIVE_TEST_PROVIDERS` (comma
//! list). Run through `npm run test:live`, which also checks credentials and
//! writes the sanitized report. Without opt-in every test returns immediately
//! without network access, so `cargo test` never spends quota.
//!
//! Each provider gets a hard request budget (`LIVE_TEST_MAX_REQUESTS_PER_PROVIDER`,
//! default 50, retries included). Results go to `target/live-report/*.json`:
//! provider, endpoints, request count, checks and timings, never credentials.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use num_bigint::BigInt;
use portfolio_core::clock::{SystemClock, parse_rfc3339};
use portfolio_core::decimal::{parse_dec, to_canonical};
use portfolio_core::network::NetworkId;
use portfolio_providers::defillama::{self, DefiLlama};
use portfolio_providers::esplora::{self, Esplora};
use portfolio_providers::http::Budget;
use portfolio_providers::livecoinwatch::{self, LiveCoinWatch};
use portfolio_providers::tonapi::{self, TonApi};
use portfolio_providers::trongrid::{self, TronGrid};
use portfolio_providers::zerion::{self, Positions, Window, Zerion};
use portfolio_providers::{Providers, SyncEngine, SyncOptions};
use portfolio_providers::{alchemy::Alchemy, helius::Helius};
use portfolio_store::ingest::{FeeAttribution, TxStatus};
use portfolio_store::{ChartRange, Coverage, ProfileKind, Scope, Store};
use serde::Serialize;
use serde_json::Value;

fn selected(provider: &str) -> bool {
    std::env::var("RUN_LIVE_API_TESTS").as_deref() == Ok("1")
        && std::env::var("LIVE_TEST_PROVIDERS")
            .unwrap_or_default()
            .split(',')
            .any(|p| p.trim() == provider)
}

fn key(var: &str) -> String {
    std::env::var(var)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| panic!("{var} is not configured"))
}

static BUDGETS: std::sync::OnceLock<
    std::sync::Mutex<std::collections::BTreeMap<&'static str, Arc<Budget>>>,
> = std::sync::OnceLock::new();
fn budget(provider: &'static str) -> Arc<Budget> {
    let max = std::env::var("LIVE_TEST_MAX_REQUESTS_PER_PROVIDER")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50);
    BUDGETS
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .entry(provider)
        .or_insert_with(|| match provider {
            "helius" => Budget::limited_with_credits(max, 5_000),
            "alchemy" => Budget::limited_with_credits(max, 10_000),
            _ => Budget::limited(max),
        })
        .clone()
}
fn budget_usage() -> std::collections::BTreeMap<&'static str, u32> {
    BUDGETS
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .iter()
        .map(|(p, b)| (*p, b.used()))
        .collect()
}

fn credit_usage() -> std::collections::BTreeMap<&'static str, u32> {
    BUDGETS
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .iter()
        .map(|(p, b)| (*p, b.credits()))
        .collect()
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn targets() -> Value {
    let file = std::env::var("LIVE_TEST_TARGETS_FILE")
        .unwrap_or_else(|_| "tests/live/public-targets.json".into());
    let path = root().join(file);
    serde_json::from_str(&std::fs::read_to_string(&path).expect("targets manifest")).unwrap()
}

#[derive(Serialize)]
struct Check {
    name: String,
    result: &'static str,
    detail: String,
}

/// Sanitized per-provider evidence written for `scripts/test-live.mjs`.
#[derive(Serialize)]
struct Report {
    provider: String,
    started_at: i64,
    duration_ms: u128,
    requests: u32,
    endpoints: BTreeSet<&'static str>,
    checks: Vec<Check>,
    #[serde(skip)]
    start: Option<Instant>,
    #[serde(skip)]
    completed: bool,
    #[serde(skip)]
    usage_start: std::collections::BTreeMap<String, u32>,
}

impl Report {
    fn new(provider: &str) -> Self {
        use portfolio_core::clock::Clock;
        Report {
            provider: provider.into(),
            started_at: SystemClock.now(),
            duration_ms: 0,
            requests: 0,
            endpoints: BTreeSet::new(),
            checks: Vec::new(),
            start: Some(Instant::now()),
            completed: false,
            usage_start: budget_usage()
                .into_iter()
                .map(|(p, n)| (p.to_owned(), n))
                .collect(),
        }
    }

    fn endpoint(&mut self, e: &'static str) {
        self.endpoints.insert(e);
    }

    /// Records a check and panics on failure after the report is saved.
    fn check(&mut self, name: &str, ok: bool, detail: impl Into<String>) {
        let detail = detail.into();
        println!("{} {name}: {detail}", if ok { "PASS" } else { "FAIL" });
        self.checks.push(Check {
            name: name.into(),
            result: if ok { "PASS" } else { "FAIL" },
            detail,
        });
    }

    fn finish(mut self, requests: u32) {
        self.completed = true;
        self.requests = requests;
        self.duration_ms = self.start.take().map_or(0, |s| s.elapsed().as_millis());
        let dir = root().join("target/live-report");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("usage.json"),
            serde_json::to_string_pretty(&budget_usage()).unwrap(),
        )
        .unwrap();
        std::fs::write(
            dir.join("estimated-credits.json"),
            serde_json::to_string_pretty(&credit_usage()).unwrap(),
        )
        .unwrap();
        let file = dir.join(format!("{}.json", self.provider));
        std::fs::write(&file, serde_json::to_string_pretty(&self).unwrap()).unwrap();
        let failed: Vec<_> = self
            .checks
            .iter()
            .filter(|c| c.result != "PASS")
            .map(|c| c.name.clone())
            .collect();
        assert!(
            failed.is_empty(),
            "{} failed checks: {failed:?}",
            self.provider
        );
    }
}

impl Drop for Report {
    fn drop(&mut self) {
        if self.completed {
            return;
        }
        self.duration_ms = self.start.take().map_or(0, |s| s.elapsed().as_millis());
        let usage = budget_usage();
        self.requests = if let Some(n) = usage.get(self.provider.as_str()) {
            n.saturating_sub(*self.usage_start.get(&self.provider).unwrap_or(&0))
        } else {
            usage
                .iter()
                .map(|(p, n)| n.saturating_sub(*self.usage_start.get(*p).unwrap_or(&0)))
                .sum()
        };
        self.checks.push(Check {
            name: "Suite completion".into(),
            result: "FAIL",
            detail: "Provider request aborted; consult sanitized test output for status".into(),
        });
        let dir = root().join("target/live-report");
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(json) = serde_json::to_string_pretty(&self) {
            let _ = std::fs::write(dir.join(format!("{}.json", self.provider)), json);
        }
        if let Ok(json) = serde_json::to_string_pretty(&usage) {
            let _ = std::fs::write(dir.join("usage.json"), json);
        }
        if let Ok(json) = serde_json::to_string_pretty(&credit_usage()) {
            let _ = std::fs::write(dir.join("estimated-credits.json"), json);
        }
    }
}

fn s(v: &Value) -> &str {
    v.as_str().expect("string in manifest")
}

#[tokio::test]
async fn helius_live() {
    if !selected("helius") {
        return;
    }
    let t = targets();
    let b = budget("helius");
    let before = b.used();
    let api = Helius::new(&key("HELIUS_API_KEY"), b.clone()).unwrap();
    let mut r = Report::new("helius");
    let address = s(&t["solana"]["address"]);
    r.endpoint("getBalance/getTokenAccountsByOwner");
    let holdings = api.holdings(address).await.unwrap();
    r.check(
        "SOL and SPL holdings",
        holdings
            .assets
            .iter()
            .any(|(a, raw)| a.contract.is_none() && *raw > BigInt::from(0))
            && holdings.assets.iter().any(|(a, _)| a.contract.is_some()),
        format!(
            "{} distinct identities; finalized slot {}",
            holdings.assets.len(),
            holdings.slot
        ),
    );
    r.endpoint("getTransactionsForAddress");
    let first = api.transactions(address, None).await.unwrap();
    r.check(
        "full related-account history",
        !first.txs.is_empty(),
        format!("{} normalized transactions", first.txs.len()),
    );
    let cursor = first
        .next
        .as_deref()
        .expect("public target has multiple pages");
    let second = api.transactions(address, Some(cursor)).await.unwrap();
    let hashes: BTreeSet<_> = first.txs.iter().map(|tx| &tx.hash).collect();
    r.check(
        "keyset pagination",
        !second.txs.is_empty() && second.txs.iter().all(|tx| !hashes.contains(&tx.hash)),
        format!(
            "{} second-page transactions; no duplicate signatures",
            second.txs.len()
        ),
    );
    r.check(
        "bounded credits",
        b.credits() <= 5_000,
        format!("{} estimated credits; ceiling 5000", b.credits()),
    );
    r.finish(b.used() - before);
}

#[tokio::test]
async fn alchemy_live() {
    if !selected("alchemy") {
        return;
    }
    let t = targets();
    let b = budget("alchemy");
    let before = b.used();
    let api = Alchemy::new(&key("ALCHEMY_API_KEY"), b.clone()).unwrap();
    let mut r = Report::new("alchemy");
    let address = s(&t["ethereum"]["address"]);
    r.endpoint("eth_getTransactionReceipt/eth_getTransactionByHash");
    let known = &t["ethereum"]["known_transactions"][0];
    let tx = api
        .transaction(
            NetworkId::Ethereum,
            &address.to_ascii_lowercase(),
            s(&known["hash"]),
            parse_rfc3339(s(&known["mined_at"])).unwrap(),
        )
        .await
        .unwrap();
    r.check(
        "known native payment and exact fee",
        tx.legs.iter().any(|l| {
            l.asset.contract.is_none() && l.signed_raw.to_string() == "-79000000000000000000"
        }) && tx
            .fee
            .as_ref()
            .is_some_and(|f| f.raw.to_string() == "6151018965000"),
        "independent known 79 ETH principal; exact sender receipt fee",
    );
    r.endpoint("alchemy_getTokenBalances/alchemy_getTokenMetadata");
    let (usdc, raw) = api
        .token_balance(
            NetworkId::Ethereum,
            address,
            "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
        )
        .await
        .unwrap();
    r.check(
        "exact contract USDC balance",
        usdc.decimals == 6 && raw >= BigInt::from(0),
        "ERC-20 raw hexadecimal balance; official USDC contract and six decimals",
    );
    for n in portfolio_providers::alchemy::NETWORKS {
        r.endpoint("eth_chainId/eth_getBalance/alchemy_getAssetTransfers");
        if let Err(error) = api.check_chain(n).await {
            r.check(
                &format!("{} mainnet access", n.as_str()),
                false,
                error.to_string(),
            );
            continue;
        }
        let balance = api.native_balance(n, address).await.unwrap();
        let page = api.transfer_index(n, address, true, None).await.unwrap();
        let rows = page["transfers"].as_array().unwrap();
        r.check(
            &format!("{} mainnet balance/history", n.as_str()),
            !rows.is_empty() && balance >= BigInt::from(0),
            format!(
                "correct chain ID; {} bounded transfer-index rows",
                rows.len()
            ),
        );
        if n == NetworkId::Ethereum {
            let cursor = page["pageKey"]
                .as_str()
                .expect("public target has multiple pages");
            let next = api
                .transfer_index(n, address, true, Some(cursor))
                .await
                .unwrap();
            let ids: BTreeSet<_> = rows
                .iter()
                .map(|v| v["uniqueId"].as_str().unwrap())
                .collect();
            let rows2 = next["transfers"].as_array().unwrap();
            r.check(
                "Ethereum index pagination",
                !rows2.is_empty()
                    && rows2
                        .iter()
                        .all(|v| !ids.contains(v["uniqueId"].as_str().unwrap())),
                format!("{} second-page rows", rows2.len()),
            );
        }
    }
    r.check(
        "bounded CU",
        b.credits() <= 10_000,
        format!("{} conservative estimated CU; ceiling 10000", b.credits()),
    );
    r.finish(b.used() - before);
}

#[tokio::test]
async fn esplora_live() {
    if !selected("esplora") {
        println!("SKIP esplora: not selected");
        return;
    }
    let t = targets();
    let b = budget("esplora");
    let used_before = b.used();
    let api = Esplora::new(esplora::DEFAULT_BASE, b.clone()).unwrap();
    let mut r = Report::new("esplora");

    r.endpoint("blocks/tip/height");
    let tip = api.tip_height().await.unwrap();
    r.check("tip height", tip > 900_000, format!("height {tip}"));

    let paging = s(&t["bitcoin"]["paging_address"]["address"]);
    r.endpoint("address");
    let info = api.address(paging).await.unwrap();
    let balance = info.confirmed_balance().unwrap();
    r.check(
        "balance and statistics",
        info.address == paging && info.chain_stats.tx_count > 50,
        format!(
            "confirmed balance {balance} sat, {} confirmed txs",
            info.chain_stats.tx_count
        ),
    );

    r.endpoint("txs/chain");
    let page1 = api.chain_txs(paging, None).await.unwrap();
    let last = page1.last().unwrap().txid.clone();
    let page2 = api.chain_txs(paging, Some(&last)).await.unwrap();
    let ids1: BTreeSet<_> = page1.iter().map(|t| &t.txid).collect();
    let overlap = page2.iter().filter(|t| ids1.contains(&t.txid)).count();
    let ordered = page1
        .iter()
        .chain(page2.iter())
        .map(|t| t.status.block_height.unwrap_or(i64::MAX))
        .collect::<Vec<_>>()
        .windows(2)
        .all(|w| w[0] >= w[1]);
    r.check(
        "two confirmed history pages via last_seen_txid",
        page1.len() == esplora::CHAIN_PAGE_SIZE && !page2.is_empty() && overlap == 0 && ordered,
        format!(
            "page sizes {} and {}, overlap {overlap}, newest-first {ordered}",
            page1.len(),
            page2.len()
        ),
    );

    let known = &t["bitcoin"]["known_transaction"];
    r.endpoint("tx");
    let tx = api.tx(s(&known["txid"])).await.unwrap().expect("known tx");
    r.check(
        "known transaction status and fee",
        tx.status.confirmed
            && tx.status.block_height == known["block_height"].as_i64()
            && Some(tx.fee) == known["fee_sats"].as_u64(),
        format!("height {:?}, fee {} sat", tx.status.block_height, tx.fee),
    );
    let owned: BTreeSet<String> = known["effects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| s(&e["address"]).to_owned())
        .collect();
    for effect in known["effects"].as_array().unwrap() {
        let addr = s(&effect["address"]);
        let spec = esplora::tx_for_account(&tx, addr, &owned, 0).expect("touches address");
        let principal = BigInt::from(effect["principal_sats"].as_i64().unwrap());
        let fee = effect["fee_sats"].as_u64().unwrap();
        let got_fee = spec.fee.as_ref().map(|f| f.raw.clone()).unwrap_or_default();
        let exact = spec
            .fee
            .as_ref()
            .is_none_or(|f| f.attribution == FeeAttribution::Exact);
        r.check(
            &format!("input/output effect for {addr}"),
            spec.legs[0].signed_raw == principal && got_fee == BigInt::from(fee) && exact,
            format!(
                "principal {} sat, fee {got_fee} sat",
                spec.legs[0].signed_raw
            ),
        );
    }
    r.endpoint("txs/mempool");
    let mempool = api.mempool_txs(paging).await.unwrap();
    r.check(
        "mempool listing within documented cap",
        mempool.len() <= esplora::MEMPOOL_CAP,
        format!("{} pending", mempool.len()),
    );
    r.finish(b.used() - used_before);
}

#[tokio::test]
async fn zerion_live() {
    if !selected("zerion") {
        println!("SKIP zerion: not selected");
        return;
    }
    let t = targets();
    let b = budget("zerion");
    let used_before = b.used();
    let api = Zerion::new(zerion::DEFAULT_BASE, &key("ZERION_API_KEY"), b.clone()).unwrap();
    let mut r = Report::new("zerion");
    let address = s(&t["ethereum"]["address"]).to_ascii_lowercase();

    r.endpoint("positions");
    match api.positions(NetworkId::Ethereum, &address).await.unwrap() {
        Positions::Indexing => r.check("positions", false, "address still indexing"),
        Positions::Ready(p) => {
            let native = p.iter().find(|x| x.asset.contract.is_none());
            let all_eth = p.iter().all(|x| x.asset.network == NetworkId::Ethereum);
            let spam = p
                .iter()
                .filter(|x| x.asset.verification.as_str() == "spam")
                .count();
            r.check(
                "simple positions on Ethereum only",
                !p.is_empty() && all_eth && native.is_some(),
                format!(
                    "{} positions ({spam} flagged as trash), native balance {} wei",
                    p.len(),
                    native.map(|n| n.raw.to_string()).unwrap_or_default()
                ),
            );
        }
    }

    r.endpoint("transactions");
    let p1 = api
        .transactions(NetworkId::Ethereum, &address, None, 5, Window::default())
        .await
        .unwrap();
    let cursor = p1.next_cursor.clone().expect("second page");
    let p2 = api
        .transactions(
            NetworkId::Ethereum,
            &address,
            Some(&cursor),
            5,
            Window::default(),
        )
        .await
        .unwrap();
    let h1: BTreeSet<_> = p1.txs.iter().map(|t| t.hash.clone()).collect();
    let dup = p2.txs.iter().filter(|t| h1.contains(&t.hash)).count();
    let times: Vec<i64> = p1
        .txs
        .iter()
        .chain(&p2.txs)
        .map(|t| t.occurred_at)
        .collect();
    let ordered = times.windows(2).all(|w| w[0] >= w[1]);
    r.check(
        "two distinct history pages with advancing cursor",
        p1.txs.len() == 5
            && !p2.txs.is_empty()
            && dup == 0
            && ordered
            && p2.next_cursor.as_deref() != Some(cursor.as_str()),
        format!(
            "page sizes {} and {}, duplicates {dup}, newest-first {ordered}",
            p1.txs.len(),
            p2.txs.len()
        ),
    );

    for known in t["ethereum"]["known_transactions"].as_array().unwrap() {
        let hash = s(&known["hash"]);
        let mined = parse_rfc3339(s(&known["mined_at"])).unwrap() * 1000;
        let window = Window {
            min_mined_at_ms: Some(mined - 60_000),
            max_mined_at_ms: Some(mined + 60_000),
        };
        let page = api
            .transactions(NetworkId::Ethereum, &address, None, 20, window)
            .await
            .unwrap();
        let Some(tx) = page.txs.iter().find(|t| t.hash == hash) else {
            r.check(
                &format!("known activity {hash}"),
                false,
                "not found in window",
            );
            continue;
        };
        let mut ok = tx.operation == s(&known["operation"]);
        ok &= match s(&known["status"]) {
            "failed" => tx.status == TxStatus::Failed && tx.legs.is_empty(),
            _ => tx.status == TxStatus::Confirmed,
        };
        let fee = tx
            .fee
            .as_ref()
            .map(|f| to_canonical(&portfolio_core::decimal::raw_to_quantity(&f.raw, 18)));
        if let Some(expected) = known["fee"].as_str() {
            ok &= fee.as_deref() == Some(expected);
        }
        if let Some(leg) = known["native_leg"].as_str() {
            ok &= tx.legs.iter().any(|l| {
                l.asset.contract.is_none()
                    && to_canonical(&portfolio_core::decimal::raw_to_quantity(&l.signed_raw, 18))
                        == leg
            });
        }
        if let Some(n) = known["legs"].as_u64() {
            ok &= tx.legs.len() as u64 == n;
        }
        r.check(
            &format!("known activity {} {}", tx.operation, &hash[..10]),
            ok,
            format!(
                "status {:?}, legs {}, fee {}",
                tx.status,
                tx.legs.len(),
                fee.unwrap_or_else(|| "none".into())
            ),
        );
    }

    // Every other required EVM network: holdings and one history page,
    // partitioned to the requested chain.
    let evm = &t["evm_networks"];
    let evm_address = s(&evm["address"]).to_ascii_lowercase();
    // The full-network engine suite already exercises these chains. Avoid
    // spending the same credential's quota twice in a combined run.
    let combined = selected("trongrid") && selected("tonapi");
    for name in evm["networks"]
        .as_array()
        .unwrap()
        .iter()
        .map(s)
        .filter(|_| !combined)
    {
        let network = NetworkId::parse(name).unwrap();
        let positions = api.positions(network, &evm_address).await.unwrap();
        let page = api
            .transactions(network, &evm_address, None, 10, Window::default())
            .await
            .unwrap();
        let (count, native, on_network) = match &positions {
            Positions::Ready(p) => (
                p.len(),
                p.iter()
                    .find(|x| x.asset.contract.is_none())
                    .map(|x| x.raw.to_string()),
                p.iter().all(|x| x.asset.network == network),
            ),
            Positions::Indexing => (0, None, true),
        };
        let polygon_alias = network != NetworkId::Polygon
            || matches!(&positions, Positions::Ready(p) if p.iter().all(|x| x.asset.contract.as_deref() != Some("0x0000000000000000000000000000000000001010")));
        let fee_native = page
            .txs
            .iter()
            .filter_map(|t| t.fee.as_ref())
            .all(|f| f.asset.contract.is_none());
        r.check(
            &format!("{name}: holdings and history on this chain only"),
            count > 0
                && on_network
                && polygon_alias
                && !page.txs.is_empty()
                && page.txs.iter().all(|t| t.network == network)
                && fee_native,
            format!(
                "{count} positions, native {} raw, {} transactions on the first page, fees in native asset {fee_native}",
                native.unwrap_or_else(|| "none".into()),
                page.txs.len()
            ),
        );
    }

    // Solana through the same API: case-sensitive identities preserved.
    let sol = &t["solana"];
    let sol_address = s(&sol["address"]);
    match api.positions(NetworkId::Solana, sol_address).await.unwrap() {
        Positions::Indexing => r.check("solana: positions", false, "address still indexing"),
        Positions::Ready(p) => {
            let native = p.iter().find(|x| x.asset.contract.is_none());
            let mixed_case = p
                .iter()
                .filter_map(|x| x.asset.contract.as_deref())
                .any(|c| c.chars().any(|ch| ch.is_ascii_uppercase()));
            let unique: BTreeSet<_> = p.iter().map(|x| x.asset.id()).collect();
            r.check(
                "solana: SOL and SPL holdings with exact mint identities",
                native.is_some_and(|n| n.asset.decimals == 9)
                    && mixed_case
                    && unique.len() == p.len(),
                format!(
                    "{} positions, SOL {} lamports, mixed-case mints kept {mixed_case}",
                    p.len(),
                    native.map(|n| n.raw.to_string()).unwrap_or_default()
                ),
            );
        }
    }
    let known = &sol["known_transaction"];
    let hash = s(&known["hash"]);
    let mined = parse_rfc3339(s(&known["mined_at"])).unwrap() * 1000;
    let page = api
        .transactions(
            NetworkId::Solana,
            sol_address,
            None,
            20,
            Window {
                min_mined_at_ms: Some(mined - 60_000),
                max_mined_at_ms: Some(mined + 60_000),
            },
        )
        .await
        .unwrap();
    let tx = page.txs.iter().find(|t| t.hash == hash);
    r.check(
        "solana: known transfer found with its exact signature",
        tx.is_some_and(|tx| {
            tx.operation == s(&known["operation"])
                && tx.fee.is_none()
                && tx.legs.iter().any(|l| {
                    l.asset.contract.is_none()
                        && to_canonical(&portfolio_core::decimal::raw_to_quantity(&l.signed_raw, 9))
                            == s(&known["native_leg"])
                })
        }),
        format!(
            "{} transactions in window, legs {:?}",
            page.txs.len(),
            tx.map(|t| t.legs.len())
        ),
    );
    r.finish(b.used() - used_before);
}

#[tokio::test]
async fn trongrid_live() {
    if !selected("trongrid") {
        println!("SKIP trongrid: not selected");
        return;
    }
    let t = targets();
    let b = budget("trongrid");
    let used_before = b.used();
    let api = TronGrid::new(trongrid::DEFAULT_BASE, &key("TRONGRID_API_KEY"), b.clone()).unwrap();
    let mut r = Report::new("trongrid");
    let tron = &t["tron"];
    let address = s(&tron["address"]);

    r.endpoint("v1/accounts");
    let account = api.account(address).await.unwrap();
    r.check(
        "key access and balances",
        account.exists && account.total_trx() > BigInt::from(0) && !account.trc20.is_empty(),
        format!(
            "liquid {} SUN, staked {} SUN, {} TRC-20 balances",
            account.liquid_sun,
            account.staked_sun,
            account.trc20.len()
        ),
    );

    r.endpoint("v1/accounts/transactions");
    let p1 = api.transactions(address, None, 20).await.unwrap();
    let cursor = p1.next.clone().expect("second native page");
    let p2 = api.transactions(address, Some(&cursor), 20).await.unwrap();
    let ids: BTreeSet<_> = p1.items.iter().map(|x| x.tx_id.clone()).collect();
    let overlap = p2.items.iter().filter(|x| ids.contains(&x.tx_id)).count();
    let ordered = p1
        .items
        .iter()
        .chain(&p2.items)
        .map(|x| x.block_timestamp)
        .collect::<Vec<_>>()
        .windows(2)
        .all(|w| w[0] >= w[1]);
    let normalized = p1
        .items
        .iter()
        .chain(&p2.items)
        .map(|x| trongrid::native_tx_for_account(x, address))
        .collect::<Result<Vec<_>, _>>();
    r.check(
        "native history: two pages, fingerprint continuity, all records normalized",
        p1.items.len() == 20
            && !p2.items.is_empty()
            && overlap == 0
            && ordered
            && normalized.is_ok(),
        format!(
            "page sizes {} and {}, overlap {overlap}, newest-first {ordered}, normalize {:?}",
            p1.items.len(),
            p2.items.len(),
            normalized.as_ref().err()
        ),
    );

    r.endpoint("v1/accounts/transactions/trc20");
    let t1 = api.trc20_transfers(address, None, 20, None).await.unwrap();
    let tcur = t1.next.clone().expect("second TRC-20 page");
    let t2 = api
        .trc20_transfers(address, Some(&tcur), 20, None)
        .await
        .unwrap();
    let keys: BTreeSet<_> = trongrid::trc20_specs_for_account(&t1.items, address)
        .into_iter()
        .map(|x| (x.hash, x.part))
        .collect();
    let dup = trongrid::trc20_specs_for_account(&t2.items, address)
        .into_iter()
        .filter(|x| keys.contains(&(x.hash.clone(), x.part.clone())))
        .count();
    r.check(
        "TRC-20 history: two pages without duplicate events",
        t1.items.len() == 20 && !t2.items.is_empty() && dup == 0,
        format!(
            "page sizes {} and {}, duplicates {dup}",
            t1.items.len(),
            t2.items.len()
        ),
    );

    // Known TRC-20 send: token amount from the TRC-20 category, the energy
    // and bandwidth fee from the native record of the same transaction.
    let k = &tron["known_trc20_send"];
    let at = k["block_timestamp_ms"].as_i64().unwrap();
    let contract = s(&k["contract"]);
    let mut found = None;
    let mut cursor: Option<String> = None;
    for _ in 0..4 {
        let page = api
            .trc20_transfers(address, cursor.as_deref(), 50, Some(contract))
            .await
            .unwrap();
        if let Some(e) = page
            .items
            .iter()
            .find(|e| e.transaction_id == s(&k["tx_id"]))
        {
            found = Some(e.clone());
            break;
        }
        if page.items.last().is_none_or(|e| e.block_timestamp < at) {
            break;
        }
        match page.next {
            Some(n) => cursor = Some(n),
            None => break,
        }
    }
    let spec = found
        .as_ref()
        .map(|e| trongrid::trc20_specs_for_account(std::slice::from_ref(e), address));
    r.check(
        "known TRC-20 transfer normalized by contract identity",
        spec.as_ref().is_some_and(|v| {
            v.len() == 1
                && v[0].legs[0].signed_raw.to_string() == s(&k["value"])
                && v[0].legs[0].asset.contract.as_deref() == Some(contract)
                && v[0].legs[0].asset.decimals == 6
        }),
        format!(
            "found {}, leg {:?}",
            found.is_some(),
            spec.as_ref()
                .and_then(|v| v.first())
                .map(|x| x.legs[0].signed_raw.to_string())
        ),
    );

    // Known fee-bearing records, located through bounded pages of the
    // account's outgoing activity.
    let mut fee_checks = 0;
    let mut cursor: Option<String> = None;
    let wanted = [&tron["known_trc20_send"], &tron["known_trx_send"]];
    for _ in 0..6 {
        let page = api
            .transactions(address, cursor.as_deref(), 200)
            .await
            .unwrap();
        for w in wanted {
            if let Some(tx) = page.items.iter().find(|x| x.tx_id == s(&w["tx_id"])) {
                let spec = trongrid::native_tx_for_account(tx, address).unwrap();
                let fee = spec.fee.as_ref().map(|f| f.raw.to_string());
                let mut ok = fee.as_deref() == Some(&w["fee_sun"].as_i64().unwrap().to_string());
                if let Some(amount) = w["amount_sun"].as_str() {
                    ok &= spec.legs.len() == 1 && spec.legs[0].signed_raw.to_string() == amount;
                } else {
                    ok &= spec.legs.is_empty() && spec.operation == "send";
                    ok &= tx.energy_fee == w["energy_fee_sun"].as_i64()
                        && tx.net_fee == w["net_fee_sun"].as_i64();
                }
                r.check(
                    &format!("fee-bearing receipt {}", &s(&w["tx_id"])[..10]),
                    ok,
                    format!(
                        "operation {}, legs {}, fee {:?} SUN",
                        spec.operation,
                        spec.legs.len(),
                        fee
                    ),
                );
                fee_checks += 1;
            }
        }
        if fee_checks == wanted.len() {
            break;
        }
        let oldest = page.items.last().map(|x| x.block_timestamp).unwrap_or(0);
        if oldest < at - 86_400_000 {
            break;
        }
        match page.next {
            Some(n) => cursor = Some(n),
            None => break,
        }
    }
    r.check(
        "known fee-bearing records located",
        fee_checks == wanted.len(),
        format!("{fee_checks} of {} found", wanted.len()),
    );
    r.finish(b.used() - used_before);
}

#[tokio::test]
async fn tonapi_live() {
    if !selected("tonapi") {
        println!("SKIP tonapi: not selected");
        return;
    }
    let t = targets();
    let b = budget("tonapi");
    let used_before = b.used();
    let api = TonApi::new(tonapi::DEFAULT_BASE, &key("TONAPI_API_KEY"), b.clone()).unwrap();
    let mut r = Report::new("tonapi");
    let ton = &t["ton"];
    let raw = s(&ton["raw"]);
    let friendly =
        portfolio_core::address::normalize_address(NetworkId::Ton, s(&ton["address"])).unwrap();
    r.check(
        "friendly and raw address forms are one account",
        friendly.canonical == raw,
        friendly.canonical.clone(),
    );

    r.endpoint("accounts");
    r.endpoint("accounts/jettons");
    let holdings = api.holdings(raw).await.unwrap();
    let masters_raw = holdings.jettons.iter().all(|(a, _)| {
        a.contract
            .as_deref()
            .is_some_and(|c| c.starts_with("0:") || c.starts_with("-1:"))
    });
    r.check(
        "TON and Jetton holdings keyed by Jetton master",
        holdings.nanoton > BigInt::from(0) && !holdings.jettons.is_empty() && masters_raw,
        format!(
            "{} nanoton, {} Jettons ({} whitelisted, {} blacklisted)",
            holdings.nanoton,
            holdings.jettons.len(),
            holdings
                .jettons
                .iter()
                .filter(|(a, _)| a.verification.as_str() == "verified")
                .count(),
            holdings
                .jettons
                .iter()
                .filter(|(a, _)| a.verification.as_str() == "spam")
                .count()
        ),
    );

    r.endpoint("accounts/events");
    let p1 = api.events(raw, None, 10).await.unwrap();
    let next = p1.next.clone().expect("second page");
    let p2 = api.events(raw, Some(&next), 10).await.unwrap();
    let ids: BTreeSet<_> = p1.events.iter().map(|e| e.event_id.clone()).collect();
    let overlap = p2
        .events
        .iter()
        .filter(|e| ids.contains(&e.event_id))
        .count();
    let lts: Vec<i64> = p1.events.iter().chain(&p2.events).map(|e| e.lt).collect();
    let ordered = lts.windows(2).all(|w| w[0] > w[1]);
    r.check(
        "events: two pages continued by logical time",
        p1.events.len() == 10 && !p2.events.is_empty() && overlap == 0 && ordered,
        format!(
            "page sizes {} and {}, overlap {overlap}, strictly descending lt {ordered}",
            p1.events.len(),
            p2.events.len()
        ),
    );

    // One event per trace: its base transactions are evidence, not extra records.
    let specs: Vec<_> = p1
        .events
        .iter()
        .chain(&p2.events)
        .map(|e| tonapi::event_for_account(e, raw))
        .collect();
    let linked = specs
        .iter()
        .filter(|x| {
            x.evidence["base_transactions"]
                .as_array()
                .is_some_and(|a| !a.is_empty())
        })
        .count();
    let unique: BTreeSet<_> = specs.iter().map(|x| x.hash.clone()).collect();
    r.check(
        "events link to transactions without duplicate records",
        unique.len() == specs.len() && linked == specs.len(),
        format!(
            "{} events, {linked} with transaction references",
            specs.len()
        ),
    );

    r.endpoint("accounts/events/{id}");
    let k = &ton["known_event"];
    let event = api.event(raw, s(&k["event_id"])).await.unwrap();
    let spec = event.as_ref().map(|e| tonapi::event_for_account(e, raw));
    r.check(
        "known Jetton receipt with master identity and fee",
        spec.as_ref().is_some_and(|x| {
            x.status == TxStatus::Confirmed
                && x.occurred_at == k["timestamp"].as_i64().unwrap()
                && x.legs.len() == 1
                && x.legs[0].asset.contract.as_deref() == Some(s(&k["jetton_master"]))
                && x.legs[0].signed_raw.to_string() == s(&k["amount"])
                && x.fee.as_ref().map(|f| f.raw.to_string())
                    == Some(k["fee_nanoton"].as_i64().unwrap().to_string())
        }),
        format!(
            "found {}, legs {:?}, fee {:?}",
            event.is_some(),
            spec.as_ref().map(|x| x.legs.len()),
            spec.as_ref()
                .and_then(|x| x.fee.as_ref().map(|f| f.raw.to_string()))
        ),
    );
    r.finish(b.used() - used_before);
}

/// Stage D: every required network synchronized through the app's engine:
/// holdings, a bounded history import, prices and accounting replay.
#[tokio::test]
async fn networks_live() {
    if !(selected("zerion") && selected("trongrid") && selected("tonapi")) {
        println!("SKIP networks: zerion, trongrid and tonapi must all be selected");
        return;
    }
    let t = targets();
    let store = Store::open_in_memory(ProfileKind::Test, Arc::new(SystemClock))
        .await
        .unwrap();
    let wallet = store.create_wallet("Live networks").await.unwrap();
    let mut accounts = Vec::new();
    let evm = s(&t["evm_networks"]["address"]);
    for name in t["evm_networks"]["networks"]
        .as_array()
        .unwrap()
        .iter()
        .map(s)
    {
        let n = NetworkId::parse(name).unwrap();
        accounts.push(store.add_account(&wallet.id, n, evm, None).await.unwrap());
    }
    for (n, addr) in [
        (NetworkId::Solana, s(&t["solana"]["address"])),
        (NetworkId::Tron, s(&t["tron"]["address"])),
        (NetworkId::Ton, s(&t["ton"]["address"])),
    ] {
        accounts.push(store.add_account(&wallet.id, n, addr, None).await.unwrap());
    }
    let budgets = [
        budget("zerion"),
        budget("trongrid"),
        budget("tonapi"),
        budget("livecoinwatch"),
        budget("defillama"),
    ];
    let used_before: u32 = budgets.iter().map(|b| b.used()).sum();
    let providers = Providers {
        zerion: Some(
            Zerion::new(
                zerion::DEFAULT_BASE,
                &key("ZERION_API_KEY"),
                budgets[0].clone(),
            )
            .unwrap(),
        ),
        trongrid: Some(
            TronGrid::new(
                trongrid::DEFAULT_BASE,
                &key("TRONGRID_API_KEY"),
                budgets[1].clone(),
            )
            .unwrap(),
        ),
        tonapi: Some(
            TonApi::new(
                tonapi::DEFAULT_BASE,
                &key("TONAPI_API_KEY"),
                budgets[2].clone(),
            )
            .unwrap(),
        ),
        livecoinwatch: selected("livecoinwatch").then(|| {
            LiveCoinWatch::new(
                livecoinwatch::DEFAULT_BASE,
                &key("LIVECOINWATCH_API_KEY"),
                budgets[3].clone(),
            )
            .unwrap()
        }),
        defillama: selected("defillama")
            .then(|| DefiLlama::new(defillama::DEFAULT_BASE, budgets[4].clone()).unwrap()),
        ..Providers::default()
    };
    let engine = SyncEngine::new(
        store.clone(),
        providers,
        SyncOptions {
            max_history_pages: 1,
            zerion_page_size: 20,
            max_pending_checks: 2,
            max_price_history_requests: 0,
        },
    );
    let mut r = Report::new("networks");
    for account in &accounts {
        let rep = engine.sync_account(account).await;
        r.check(
            &format!(
                "{}: holdings and first history page persisted",
                account.network.as_str()
            ),
            rep.error.is_none() && rep.balance_refreshed && rep.new_transactions > 0,
            format!(
                "provider {:?}, {} transactions in {} pages, coverage {:?}, error {:?}",
                rep.provider, rep.new_transactions, rep.pages_fetched, rep.coverage, rep.error
            ),
        );
        let again = engine.sync_account(account).await;
        r.check(
            &format!(
                "{}: immediate re-sync adds nothing",
                account.network.as_str()
            ),
            again.error.is_none() && again.new_transactions == 0,
            format!(
                "{} new, {} pages",
                again.new_transactions, again.pages_fetched
            ),
        );
    }
    if selected("livecoinwatch") || selected("defillama") {
        let prices = engine.refresh_prices().await;
        let holdings = store.list_holdings(&Scope::All).await.unwrap();
        let mut missing = Vec::new();
        for n in [
            NetworkId::Base,
            NetworkId::Polygon,
            NetworkId::Bsc,
            NetworkId::Solana,
            NetworkId::Tron,
            NetworkId::Ton,
        ] {
            let id = format!("{}:native", n.as_str());
            if !holdings
                .iter()
                .any(|h| h.asset_id == id && h.value_usd.is_some())
            {
                missing.push(id);
            }
        }
        let usdt_tron = holdings.iter().any(|h| {
            h.asset_id == "tron:token:TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t" && h.price_usd.is_some()
        });
        r.check(
            "native assets of every network valued; case-sensitive token identity priced",
            missing.is_empty() && usdt_tron,
            format!(
                "{} priced, {} unpriced, missing {missing:?}, TRON USDT priced {usdt_tron}, errors {:?}",
                prices.priced,
                prices.unpriced.len(),
                prices.errors
            ),
        );
    }
    let replay = store.replay_accounting().await;
    r.check(
        "accounting replay over all networks",
        replay.is_ok(),
        format!("{replay:?}"),
    );
    let status = store.sync_status().await.unwrap();
    r.check(
        "coverage reported for every account",
        status
            .iter()
            .all(|x| x.coverage.is_some() && x.last_error.is_none()),
        status
            .iter()
            .map(|x| format!("{:?}", x.coverage))
            .collect::<Vec<_>>()
            .join(", "),
    );
    r.finish(budgets.iter().map(|b| b.used()).sum::<u32>() - used_before);
}

#[tokio::test]
async fn livecoinwatch_live() {
    if !selected("livecoinwatch") {
        println!("SKIP livecoinwatch: not selected");
        return;
    }
    let t = targets();
    let b = budget("livecoinwatch");
    let used_before = b.used();
    let api = LiveCoinWatch::new(
        livecoinwatch::DEFAULT_BASE,
        &key("LIVECOINWATCH_API_KEY"),
        b.clone(),
    )
    .unwrap();
    let mut r = Report::new("livecoinwatch");
    let lcw = &t["livecoinwatch"];

    r.endpoint("credits");
    let credits = api.credits().await.unwrap();
    r.check(
        "authenticated credits",
        credits.limit > 0,
        format!(
            "provider-reported {} of {} daily credits remaining",
            credits.remaining, credits.limit
        ),
    );

    r.endpoint("coins/map");
    let mut codes: Vec<&str> = lcw["codes"].as_array().unwrap().iter().map(s).collect();
    codes.push(s(&lcw["missing_code"]));
    let quotes = api.quotes(&codes).await.unwrap();
    let got: BTreeSet<_> = quotes.iter().map(|q| q.code.as_str()).collect();
    let all_positive = quotes.iter().all(|q| q.rate_usd > parse_dec("0").unwrap());
    let sane_change = quotes.iter().all(|q| {
        q.change_24h_percent
            .as_ref()
            .is_some_and(|c| *c > parse_dec("-100").unwrap() && *c < parse_dec("1000").unwrap())
    });
    r.check(
        "USD quote batch for mapped assets",
        lcw["codes"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| got.contains(s(c)))
            && all_positive,
        quotes
            .iter()
            .map(|q| format!("{}={}", q.code, to_canonical(&q.rate_usd)))
            .collect::<Vec<_>>()
            .join(", "),
    );
    r.check(
        "24h ratio normalized to percent",
        sane_change,
        quotes
            .iter()
            .map(|q| {
                format!(
                    "{} {}%",
                    q.code,
                    q.change_24h_percent
                        .as_ref()
                        .map(to_canonical)
                        .unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join(", "),
    );
    r.check(
        "missing code is absent, not zero",
        !got.contains(s(&lcw["missing_code"])),
        format!("{} returned codes", got.len()),
    );

    r.endpoint("coins/single/history");
    let h = &lcw["history"];
    let start = parse_rfc3339(s(&h["start"])).unwrap();
    let end = parse_rfc3339(s(&h["end"])).unwrap();
    let points = api.history(s(&h["code"]), start, end).await.unwrap();
    let ascending = points.windows(2).all(|w| w[0].at < w[1].at);
    let in_range = points
        .iter()
        .all(|p| p.at >= start - 86_400 && p.at <= end + 86_400);
    r.check(
        "historical series with ascending timestamps",
        points.len() >= 3 && ascending && in_range,
        format!(
            "{} points, first {:?}, last {:?}",
            points.len(),
            points.first().map(|p| p.at),
            points.last().map(|p| p.at)
        ),
    );
    r.finish(b.used() - used_before);
}

#[tokio::test]
async fn defillama_live() {
    if !selected("defillama") {
        println!("SKIP defillama: not selected");
        return;
    }
    let t = targets();
    let b = budget("defillama");
    let used_before = b.used();
    let api = DefiLlama::new(defillama::DEFAULT_BASE, b.clone()).unwrap();
    let mut r = Report::new("defillama");
    let d = &t["defillama"];
    let token = s(&d["token"]).to_owned();
    let missing = s(&d["missing_token"]).to_owned();
    use portfolio_core::clock::Clock;
    let now = SystemClock.now();

    r.endpoint("prices/current");
    let current = api
        .current(&[token.clone(), missing.clone()])
        .await
        .unwrap();
    let q = current.get(&token);
    r.check(
        "current price by contract identity",
        q.is_some_and(|q| {
            q.price_usd > parse_dec("0.9").unwrap()
                && q.price_usd < parse_dec("1.1").unwrap()
                && (now - q.timestamp).abs() < 86_400
                && q.decimals == Some(6)
        }),
        format!(
            "price {:?}, timestamp {:?}, confidence {:?}",
            q.map(|q| to_canonical(&q.price_usd)),
            q.map(|q| q.timestamp),
            q.and_then(|q| q.confidence.as_ref().map(to_canonical))
        ),
    );
    r.check(
        "missing token is absent, not zero",
        !current.contains_key(&missing),
        "not returned",
    );

    r.endpoint("prices/historical");
    let at = d["historical_at"].as_i64().unwrap();
    let hist = api
        .historical(at, std::slice::from_ref(&token))
        .await
        .unwrap();
    let hq = hist.get(&token);
    r.check(
        "historical price near the requested time",
        hq.is_some_and(|q| {
            (q.timestamp - at).abs() <= 3_600
                && q.price_usd > parse_dec("0.9").unwrap()
                && q.price_usd < parse_dec("1.1").unwrap()
        }),
        format!(
            "price {:?} at {:?}",
            hq.map(|q| to_canonical(&q.price_usd)),
            hq.map(|q| q.timestamp)
        ),
    );

    r.endpoint("chart");
    let start = at - at.rem_euclid(86_400);
    let series = api.daily_chart(&token, start, 30).await.unwrap();
    let pts = series.as_ref().map_or(&[][..], |s| &s.points[..]);
    r.check(
        "daily chart returns one dollar-pegged point per day in range",
        pts.len() >= 25
            && pts.iter().all(|(t, p)| {
                *t >= start - 86_400
                    && *t <= start + 31 * 86_400
                    && *p > parse_dec("0.9").unwrap()
                    && *p < parse_dec("1.1").unwrap()
            })
            && pts.windows(2).all(|w| w[1].0 - w[0].0 >= 82_800),
        format!(
            "{} points, first {:?}, last {:?}",
            pts.len(),
            pts.first().map(|(t, p)| (*t, to_canonical(p))),
            pts.last().map(|(t, p)| (*t, to_canonical(p)))
        ),
    );
    let native = api
        .daily_chart("coingecko:ethereum", start, 7)
        .await
        .unwrap();
    r.check(
        "native asset daily chart by CoinGecko identity",
        native.as_ref().is_some_and(|s| s.points.len() >= 6),
        format!("{:?} points", native.map(|s| s.points.len())),
    );
    r.finish(b.used() - used_before);
}

/// The stage B vertical slice against real services: add addresses, read
/// holdings, paginate and persist history, then value the holdings.
#[tokio::test]
async fn vertical_slice_live() {
    if !selected("esplora") {
        println!("SKIP vertical slice: esplora not selected");
        return;
    }
    let t = targets();
    let store = Store::open_in_memory(ProfileKind::Test, Arc::new(SystemClock))
        .await
        .unwrap();
    let wallet = store.create_wallet("Live test").await.unwrap();
    let btc_addr = s(&t["bitcoin"]["sync_address"]["address"]);
    let btc = store
        .add_account(&wallet.id, NetworkId::Bitcoin, btc_addr, None)
        .await
        .unwrap();
    let eth = if selected("zerion") {
        Some(
            store
                .add_account(
                    &wallet.id,
                    NetworkId::Ethereum,
                    s(&t["ethereum"]["address"]),
                    None,
                )
                .await
                .unwrap(),
        )
    } else {
        None
    };
    let budgets = [
        budget("esplora"),
        budget("zerion"),
        budget("livecoinwatch"),
        budget("defillama"),
    ];
    let used_before: u32 = budgets.iter().map(|b| b.used()).sum();
    let providers = Providers {
        esplora: Some(Esplora::new(esplora::DEFAULT_BASE, budgets[0].clone()).unwrap()),
        zerion: selected("zerion").then(|| {
            Zerion::new(
                zerion::DEFAULT_BASE,
                &key("ZERION_API_KEY"),
                budgets[1].clone(),
            )
            .unwrap()
        }),
        livecoinwatch: selected("livecoinwatch").then(|| {
            LiveCoinWatch::new(
                livecoinwatch::DEFAULT_BASE,
                &key("LIVECOINWATCH_API_KEY"),
                budgets[2].clone(),
            )
            .unwrap()
        }),
        defillama: selected("defillama")
            .then(|| DefiLlama::new(defillama::DEFAULT_BASE, budgets[3].clone()).unwrap()),
        ..Providers::default()
    };
    // Small pages and a two-page cap keep the large Ethereum history bounded.
    let engine = SyncEngine::new(
        store.clone(),
        providers,
        SyncOptions {
            max_history_pages: 2,
            zerion_page_size: 25,
            max_pending_checks: 2,
            max_price_history_requests: 2,
        },
    );
    let mut r = Report::new("vertical-slice");

    let rb = engine.sync_account(&btc).await;
    let min_tx = t["bitcoin"]["sync_address"]["min_transactions"]
        .as_u64()
        .unwrap();
    r.check(
        "bitcoin: holdings and complete history persisted",
        rb.error.is_none()
            && rb.balance_refreshed
            && rb.coverage == Coverage::Complete
            && u64::from(rb.new_transactions) >= min_tx,
        format!(
            "{} transactions in {} pages, coverage {:?}, error {:?}",
            rb.new_transactions, rb.pages_fetched, rb.coverage, rb.error
        ),
    );
    let rb2 = engine.sync_account(&btc).await;
    r.check(
        "bitcoin: re-sync adds nothing",
        rb2.error.is_none() && rb2.new_transactions == 0 && rb2.pages_fetched == 1,
        format!("{} new, {} pages", rb2.new_transactions, rb2.pages_fetched),
    );

    if let Some(eth) = &eth {
        let re = engine.sync_account(eth).await;
        r.check(
            "ethereum: holdings and first history pages persisted",
            re.error.is_none() && re.balance_refreshed && re.new_transactions > 0,
            format!(
                "{} transactions in {} pages, coverage {:?} (backfill continues next sweep), error {:?}",
                re.new_transactions, re.pages_fetched, re.coverage, re.error
            ),
        );
    }

    if selected("livecoinwatch") || selected("defillama") {
        let prices = engine.refresh_prices().await;
        let holdings = store.list_holdings(&Scope::All).await.unwrap();
        let btc_row = holdings.iter().find(|h| h.asset_id == "bitcoin:native");
        r.check(
            "prices: bitcoin holding valued",
            btc_row.is_some_and(|h| h.value_usd.is_some()),
            format!(
                "{} priced, {} unpriced, errors {:?}; BTC {} x {:?} = {:?}",
                prices.priced,
                prices.unpriced.len(),
                prices.errors,
                btc_row.map(|h| h.quantity.clone()).unwrap_or_default(),
                btc_row.and_then(|h| h.price_usd.clone()),
                btc_row.and_then(|h| h.value_usd.clone())
            ),
        );
        let summary = store.portfolio_summary(&Scope::All).await.unwrap();
        r.check(
            "portfolio total available",
            summary.total_value_usd.is_some(),
            format!(
                "{} holdings, {} unpriced, {} spam excluded",
                summary.holding_count, summary.unpriced_count, summary.excluded_spam_count
            ),
        );
    }
    let activity = store
        .list_activity(&Scope::All, &Default::default(), None, 200)
        .await
        .unwrap();
    r.check(
        "activity readable through the app's query",
        !activity.rows.is_empty(),
        format!("{} rows on the first page", activity.rows.len()),
    );

    if selected("defillama") {
        let history = engine.refresh_price_history().await;
        r.check(
            "price history: daily series downloaded within the request cap",
            history.errors.is_empty()
                && history.points > 0
                && history.requests <= SyncOptions::default().max_price_history_requests,
            format!(
                "{} requests, {} points, {} assets pending, {} unavailable, errors {:?}",
                history.requests,
                history.points,
                history.pending_assets,
                history.unavailable.len(),
                history.errors
            ),
        );
        let replay = store.replay_accounting().await;
        r.check(
            "accounting replay over real history",
            replay.is_ok(),
            format!("{replay:?}"),
        );
        let chart = store
            .asset_chart(&Scope::All, "bitcoin:native", ChartRange::Year)
            .await
            .unwrap();
        let priced = chart.price.iter().filter(|p| p.price_usd.is_some()).count();
        let valued = chart
            .holdings
            .points
            .iter()
            .filter(|p| p.value_usd.is_some())
            .count();
        r.check(
            "bitcoin 1Y price and holdings charts use historical prices",
            priced > 300 && valued > 300,
            format!(
                "{} of {} price points, {} valued holdings points",
                priced,
                chart.price.len(),
                valued
            ),
        );
    }
    r.finish(budgets.iter().map(|b| b.used()).sum::<u32>() - used_before);
}
