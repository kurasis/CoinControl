//! End-to-end synchronization against a local mock server: add address, read
//! holdings, paginate history, persist, resume, and price (TESTING.md Layer A).

mod common;

use common::{NOW, SENTINEL_KEY, fast, fixture, store};
use portfolio_core::network::NetworkId;
use portfolio_providers::defillama::DefiLlama;
use portfolio_providers::esplora::Esplora;
use portfolio_providers::http::Budget;
use portfolio_providers::livecoinwatch::LiveCoinWatch;
use portfolio_providers::zerion::Zerion;
use portfolio_providers::{Providers, SyncEngine, SyncOptions};
use portfolio_store::{BalanceStatus, Coverage, Scope, Store};
use serde_json::json;
use wiremock::matchers::{method, path, path_regex, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const BTC: &str = "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa";
const SENDER: &str = "1XPTgDRhN8RFnzniWCddobD9iKZatrvH4";
const ETH: &str = "0x1db3439a222c519ab44bb1144fc28167b4fa6ee6";

fn btc_tx(n: u32) -> serde_json::Value {
    json!({
        "txid": format!("{n:064x}"),
        "vin": [{"txid": "00".repeat(32), "vout": 0, "is_coinbase": false,
                 "prevout": {"scriptpubkey_address": SENDER, "value": 20_000}}],
        "vout": [{"scriptpubkey_address": BTC, "value": 10_000},
                 {"scriptpubkey_address": SENDER, "value": 9_000}],
        "fee": 1_000,
        "status": {"confirmed": true, "block_height": 800_000 + n, "block_hash": "ff",
                   "block_time": 1_700_000_000 + i64::from(n) * 600}
    })
}

/// Newest first: transaction numbers `top`, `top-1`, ... down to 1.
fn chain(top: u32) -> Vec<serde_json::Value> {
    (1..=top).rev().map(btc_tx).collect()
}

async fn mount_btc(server: &MockServer, txs: &[serde_json::Value]) {
    server.reset().await;
    Mock::given(path("/blocks/tip/height"))
        .respond_with(ResponseTemplate::new(200).set_body_string("900000"))
        .mount(server)
        .await;
    let funded = 10_000 * txs.len() as u64;
    Mock::given(path(format!("/address/{BTC}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "address": BTC,
            "chain_stats": {"funded_txo_count": txs.len(), "funded_txo_sum": funded,
                            "spent_txo_count": 0, "spent_txo_sum": 0, "tx_count": txs.len()},
            "mempool_stats": {"funded_txo_count": 0, "funded_txo_sum": 0,
                              "spent_txo_count": 0, "spent_txo_sum": 0, "tx_count": 0}
        })))
        .mount(server)
        .await;
    Mock::given(path(format!("/address/{BTC}/txs/mempool")))
        .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
        .mount(server)
        .await;
    // Pages of 25 keyed by the last txid seen, exactly like Esplora.
    let pages: Vec<&[serde_json::Value]> = txs.chunks(25).collect();
    if pages.is_empty() {
        Mock::given(path(format!("/address/{BTC}/txs/chain")))
            .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
            .mount(server)
            .await;
    }
    for (i, page) in pages.iter().enumerate() {
        let route = if i == 0 {
            format!("/address/{BTC}/txs/chain")
        } else {
            let last = pages[i - 1].last().unwrap()["txid"]
                .as_str()
                .unwrap()
                .to_owned();
            format!("/address/{BTC}/txs/chain/{last}")
        };
        Mock::given(path(route))
            .respond_with(ResponseTemplate::new(200).set_body_json(page))
            .mount(server)
            .await;
    }
    if txs.len().is_multiple_of(25) && !txs.is_empty() {
        let last = txs.last().unwrap()["txid"].as_str().unwrap().to_owned();
        Mock::given(path(format!("/address/{BTC}/txs/chain/{last}")))
            .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
            .mount(server)
            .await;
    }
}

fn btc_engine(store: &Store, server: &MockServer, pages: u32) -> SyncEngine {
    SyncEngine::new(
        store.clone(),
        Providers {
            esplora: Some(
                Esplora::with_config(&server.uri(), Budget::unlimited(), fast()).unwrap(),
            ),
            ..Providers::default()
        },
        SyncOptions {
            max_history_pages: pages,
            ..SyncOptions::default()
        },
    )
}

async fn activity_count(store: &Store) -> usize {
    let mut n = 0;
    let mut cursor = None;
    loop {
        let page = store
            .list_activity(&Scope::All, &Default::default(), cursor.as_deref(), 200)
            .await
            .unwrap();
        n += page.rows.len();
        match page.next_cursor {
            Some(c) => cursor = Some(c),
            None => return n,
        }
    }
}

#[tokio::test]
async fn bitcoin_history_backfills_resumes_and_never_duplicates() {
    let server = MockServer::start().await;
    let store = store().await;
    let w = store.create_wallet("BTC").await.unwrap();
    let account = store
        .add_account(&w.id, NetworkId::Bitcoin, BTC, None)
        .await
        .unwrap();
    mount_btc(&server, &chain(53)).await;

    // Run 1, two pages allowed: 50 of 53 transactions, backfill pending.
    let r1 = btc_engine(&store, &server, 2).sync_account(&account).await;
    assert_eq!(r1.error, None);
    assert!(r1.balance_refreshed);
    assert_eq!((r1.pages_fetched, r1.new_transactions), (2, 50));
    assert_eq!(r1.coverage, Coverage::Loading);
    let holdings = store.list_holdings(&Scope::All).await.unwrap();
    assert_eq!(
        holdings[0].quantity, "0.0053",
        "balance comes from address stats"
    );
    assert_eq!(holdings[0].balance_status, BalanceStatus::Fresh);

    // Run 2 (interrupted import resumes): forward pass stops at known history,
    // backfill fetches the last page and completes.
    let r2 = btc_engine(&store, &server, 2).sync_account(&account).await;
    assert_eq!(r2.error, None);
    assert_eq!((r2.pages_fetched, r2.new_transactions), (2, 3));
    assert_eq!(r2.coverage, Coverage::Complete);
    assert_eq!(activity_count(&store).await, 53);

    // Run 3: nothing new; one page, no duplicates.
    let r3 = btc_engine(&store, &server, 8).sync_account(&account).await;
    assert_eq!((r3.pages_fetched, r3.new_transactions), (1, 0));
    assert_eq!(activity_count(&store).await, 53);

    // New activity after completion is picked up by the forward pass.
    mount_btc(&server, &chain(54)).await;
    let r4 = btc_engine(&store, &server, 8).sync_account(&account).await;
    assert_eq!((r4.pages_fetched, r4.new_transactions), (1, 1));
    assert_eq!(r4.coverage, Coverage::Complete);
    assert_eq!(activity_count(&store).await, 54);

    let status = store.sync_status().await.unwrap();
    assert_eq!(status[0].transaction_count, 54);
    assert_eq!(status[0].coverage, Some(Coverage::Complete));
    assert_eq!(status[0].earliest_covered_at, Some(1_700_000_600));
    let usage = store
        .provider_usage(&portfolio_core::clock::utc_day(NOW))
        .await
        .unwrap();
    assert!(
        usage
            .iter()
            .any(|u| u.provider == "esplora" && u.requests > 0)
    );
}

#[tokio::test]
async fn pending_bitcoin_transaction_confirms_or_is_dropped() {
    let server = MockServer::start().await;
    let store = store().await;
    let w = store.create_wallet("BTC").await.unwrap();
    let account = store
        .add_account(&w.id, NetworkId::Bitcoin, BTC, None)
        .await
        .unwrap();
    mount_btc(&server, &chain(2)).await;
    let mut pending = btc_tx(99);
    pending["status"] = json!({"confirmed": false});
    Mock::given(path(format!("/address/{BTC}/txs/mempool")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([pending])))
        .with_priority(1)
        .up_to_n_times(1)
        .mount(&server)
        .await;
    btc_engine(&store, &server, 4).sync_account(&account).await;
    let page = store
        .list_activity(&Scope::All, &Default::default(), None, 50)
        .await
        .unwrap();
    assert_eq!(page.rows[0].status, "pending");
    assert_eq!(page.rows[0].occurred_at, NOW);

    // Next run: it left the mempool and the service no longer knows it.
    Mock::given(path(format!("/tx/{:064x}", 99)))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    btc_engine(&store, &server, 4).sync_account(&account).await;
    let page = store
        .list_activity(&Scope::All, &Default::default(), None, 50)
        .await
        .unwrap();
    let dropped = page
        .rows
        .iter()
        .find(|r| r.transaction_id.ends_with(&format!("{:064x}", 99)))
        .unwrap();
    assert_eq!(dropped.status, "reorged");
}

async fn mount_zerion(server: &MockServer, positions: &str) {
    server.reset().await;
    Mock::given(method("GET"))
        .and(path(format!("/wallets/{ETH}/positions/")))
        .and(query_param("filter[chain_ids]", "ethereum"))
        .respond_with(ResponseTemplate::new(200).set_body_string(positions))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/wallets/{ETH}/transactions/")))
        .and(query_param("page[after]", "CURSOR2"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(fixture("zerion/transactions_page2.json")),
        )
        .with_priority(1)
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/wallets/{ETH}/transactions/")))
        .and(query_param("filter[chain_ids]", "ethereum"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(fixture("zerion/transactions_page1.json")),
        )
        .with_priority(2)
        .mount(server)
        .await;
}

fn zerion_engine(store: &Store, server: &MockServer) -> SyncEngine {
    SyncEngine::new(
        store.clone(),
        Providers {
            zerion: Some(
                Zerion::with_config(&server.uri(), SENTINEL_KEY, Budget::unlimited(), fast())
                    .unwrap(),
            ),
            ..Providers::default()
        },
        SyncOptions::default(),
    )
}

#[tokio::test]
async fn ethereum_positions_history_fees_and_partitioning() {
    let server = MockServer::start().await;
    let store = store().await;
    let w = store.create_wallet("EVM").await.unwrap();
    let account = store
        .add_account(&w.id, NetworkId::Ethereum, ETH, None)
        .await
        .unwrap();
    mount_zerion(&server, &fixture("zerion/positions.json")).await;

    let report = zerion_engine(&store, &server).sync_account(&account).await;
    assert_eq!(report.error, None);
    assert_eq!(report.pages_fetched, 2);
    assert_eq!(report.new_transactions, 7);
    assert_eq!(report.coverage, Coverage::Complete);

    // Holdings: ETH and WETH on Ethereum. The Base position in the response is
    // ignored, and the trash token is excluded from totals as spam.
    let holdings = store.list_holdings(&Scope::All).await.unwrap();
    let ids: Vec<_> = holdings.iter().map(|h| h.asset_id.as_str()).collect();
    assert_eq!(ids.len(), 2, "{ids:?}");
    assert!(ids.contains(&"ethereum:native"));
    assert!(ids.contains(&"ethereum:token:0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"));
    assert!(holdings.iter().all(|h| h.network == NetworkId::Ethereum));
    let summary = store.portfolio_summary(&Scope::All).await.unwrap();
    assert_eq!(summary.excluded_spam_count, 1);

    let page = store
        .list_activity(&Scope::All, &Default::default(), None, 50)
        .await
        .unwrap();
    let by_hash = |h: &str| {
        page.rows
            .iter()
            .find(|r| r.transaction_id.ends_with(h))
            .unwrap_or_else(|| panic!("{h} missing"))
    };
    // Own 79 ETH send with its exact fee.
    let send = by_hash("0x6d688bc397c67c08198373cc0a868af4e9069ed6943342385017c56a3ee3906c");
    assert_eq!(send.legs[0].signed_quantity, "-79");
    assert_eq!(send.fee_quantity.as_deref(), Some("0.000006151018965"));
    // Failed call: no asset movement, fee still paid.
    let failed = by_hash("0x15cc37b3693b232e608e6c13f1ff8d5f039d9c05dfa7c4cd6579f674624750c7");
    assert_eq!(failed.status, "failed");
    assert!(failed.legs.is_empty());
    assert_eq!(failed.fee_quantity.as_deref(), Some("0.000117495682771194"));
    // Swap with two legs.
    let trade = by_hash("0x8215ba177dbab4da04ce852f76571e56b02bd01025c11aefe4a31ed7284d5f23");
    assert_eq!(trade.legs.len(), 2);
    assert_eq!(trade.operation, "trade");
    // Address-poisoning "send" signed by someone else: no fee for this account.
    let poison = by_hash("0xba4abf04cd9e56582bf3e6461c5f92d938eda3f5a36702c333d3e6615c18735e");
    assert_eq!(poison.fee_quantity, None);
    // NFT-only movement: listed, flagged as partially decoded, no fungible leg.
    let nft = by_hash("0xe3916baadb393e8e2827625d4d6191d04c5e8998afcc9be796bb8b508f579898");
    assert!(nft.legs.is_empty() && nft.unresolved);
    // Approvals only cost a fee and still appear in activity.
    let approvals: Vec<_> = page
        .rows
        .iter()
        .filter(|r| r.operation == "approve")
        .collect();
    assert_eq!(approvals.len(), 2);
    assert!(
        approvals
            .iter()
            .all(|r| r.legs.is_empty() && r.fee_quantity.is_some())
    );

    // Re-sync with WETH gone from positions: it becomes zero, not stale data.
    let mut positions: serde_json::Value =
        serde_json::from_str(&fixture("zerion/positions.json")).unwrap();
    positions["data"]
        .as_array_mut()
        .unwrap()
        .retain(|p| p["attributes"]["fungible_info"]["symbol"] != "WETH");
    mount_zerion(&server, &positions.to_string()).await;
    let again = zerion_engine(&store, &server).sync_account(&account).await;
    assert_eq!(again.error, None);
    assert_eq!(again.new_transactions, 0);
    assert_eq!(again.pages_fetched, 1, "stops at known history");
    let holdings = store.list_holdings(&Scope::All).await.unwrap();
    assert_eq!(holdings.len(), 1);
    assert_eq!(holdings[0].asset_id, "ethereum:native");
}

#[tokio::test]
async fn auth_failure_keeps_last_balance_as_stale_and_reports() {
    let server = MockServer::start().await;
    let store = store().await;
    let w = store.create_wallet("EVM").await.unwrap();
    let account = store
        .add_account(&w.id, NetworkId::Ethereum, ETH, None)
        .await
        .unwrap();
    mount_zerion(&server, &fixture("zerion/positions.json")).await;
    zerion_engine(&store, &server).sync_account(&account).await;

    server.reset().await;
    Mock::given(path_regex("^/wallets/.*"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    let engine = zerion_engine(&store, &server);
    let report = engine.sync_account(&account).await;
    let error = report.error.unwrap();
    assert!(error.contains("credential rejected"), "{error}");
    assert!(!error.contains(SENTINEL_KEY));
    let holdings = store.list_holdings(&Scope::All).await.unwrap();
    assert_eq!(
        holdings.len(),
        2,
        "failed refresh never replaces balances with zero"
    );
    assert!(
        holdings
            .iter()
            .all(|h| h.balance_status == BalanceStatus::Stale)
    );
    // The provider is stopped for the rest of the run: no further requests.
    let before = server.received_requests().await.unwrap().len();
    let stopped = engine.sync_account(&account).await;
    assert!(stopped.error.unwrap().contains("credential rejected"));
    assert_eq!(server.received_requests().await.unwrap().len(), before);
    let status = store.sync_status().await.unwrap();
    assert!(status[0].last_error.is_some());
}

#[tokio::test]
async fn missing_keys_are_explicit_on_every_keyed_network() {
    let store = store().await;
    let w = store.create_wallet("Other").await.unwrap();
    let tron = store
        .add_account(
            &w.id,
            NetworkId::Tron,
            "TLa2f6VPqDgRE67v1736s7bJ8Ray5wYjU7",
            None,
        )
        .await
        .unwrap();
    let eth = store
        .add_account(&w.id, NetworkId::Ethereum, ETH, None)
        .await
        .unwrap();
    let engine = SyncEngine::new(store.clone(), Providers::default(), SyncOptions::default());
    let r = engine.sync_account(&tron).await;
    assert_ne!(r.coverage, Coverage::Complete);
    assert!(r.error.unwrap().contains("no API key"));
    let r = engine.sync_account(&eth).await;
    assert!(r.error.unwrap().contains("no API key"));
    assert!(store.list_holdings(&Scope::All).await.unwrap().is_empty());
}

#[tokio::test]
async fn prices_value_holdings_exactly_and_never_as_zero() {
    let server = MockServer::start().await;
    let store = store().await;
    let w = store.create_wallet("Mixed").await.unwrap();
    let btc = store
        .add_account(&w.id, NetworkId::Bitcoin, BTC, None)
        .await
        .unwrap();
    let eth = store
        .add_account(&w.id, NetworkId::Ethereum, ETH, None)
        .await
        .unwrap();
    mount_btc(&server, &chain(3)).await;
    btc_engine(&store, &server, 2).sync_account(&btc).await;
    mount_zerion(&server, &fixture("zerion/positions.json")).await;
    zerion_engine(&store, &server).sync_account(&eth).await;

    server.reset().await;
    Mock::given(method("POST"))
        .and(path("/coins/map"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"[{"code":"BTC","rate":85042.5048971402,"delta":{"day":1.0051}},
                {"code":"ETH","rate":2695.48,"delta":{"day":0.98}},
                {"code":"DOGE","rate":0.1,"delta":{"day":1}}]"#,
        ))
        .mount(&server)
        .await;
    // WETH has a confident contract quote.
    Mock::given(method("GET"))
        .and(path_regex("^/prices/current/.*"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"coins": {
            "ethereum:0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2":
                {"price": 2694.1, "timestamp": NOW - 60, "confidence": 0.99, "decimals": 18, "symbol": "WETH"}
        }})))
        .mount(&server)
        .await;
    let engine = SyncEngine::new(
        store.clone(),
        Providers {
            livecoinwatch: Some(
                LiveCoinWatch::with_config(
                    &server.uri(),
                    SENTINEL_KEY,
                    Budget::unlimited(),
                    fast(),
                )
                .unwrap(),
            ),
            defillama: Some(
                DefiLlama::with_config(&server.uri(), Budget::unlimited(), fast()).unwrap(),
            ),
            ..Providers::default()
        },
        SyncOptions::default(),
    );
    let report = engine.refresh_prices().await;
    assert_eq!(report.errors, Vec::<String>::new());
    assert_eq!(report.priced, 3, "{report:?}");
    assert!(report.unpriced.is_empty());

    let holdings = store.list_holdings(&Scope::All).await.unwrap();
    let btc_row = holdings
        .iter()
        .find(|h| h.asset_id == "bitcoin:native")
        .unwrap();
    // 0.0003 BTC * 85042.5048971402 = 25.51275146914206
    assert_eq!(btc_row.value_usd.as_deref(), Some("25.51275146914206"));
    assert_eq!(btc_row.change_24h_percent.as_deref(), Some("0.51"));
    let eth_row = holdings
        .iter()
        .find(|h| h.asset_id == "ethereum:native")
        .unwrap();
    assert_eq!(eth_row.change_24h_percent.as_deref(), Some("-2"));
    assert!(holdings.iter().all(|h| h.value_usd.is_some()));
}

#[tokio::test]
async fn token_without_quote_stays_unpriced() {
    let server = MockServer::start().await;
    let store = store().await;
    let w = store.create_wallet("EVM").await.unwrap();
    let eth = store
        .add_account(&w.id, NetworkId::Ethereum, ETH, None)
        .await
        .unwrap();
    mount_zerion(&server, &fixture("zerion/positions.json")).await;
    zerion_engine(&store, &server).sync_account(&eth).await;
    server.reset().await;
    // Native ETH falls back to DefiLlama's CoinGecko identity without an LCW
    // key; WETH is missing from the response; a low-confidence quote is unusable.
    Mock::given(path_regex("^/prices/current/.*"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"coins": {
            "coingecko:ethereum": {"price": 2695.0, "timestamp": NOW, "confidence": 0.99}
        }})))
        .mount(&server)
        .await;
    let engine = SyncEngine::new(
        store.clone(),
        Providers {
            defillama: Some(
                DefiLlama::with_config(&server.uri(), Budget::unlimited(), fast()).unwrap(),
            ),
            ..Providers::default()
        },
        SyncOptions::default(),
    );
    let report = engine.refresh_prices().await;
    assert_eq!(report.priced, 1);
    assert_eq!(
        report.unpriced,
        vec!["ethereum:token:0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2".to_owned()]
    );
    let summary = store.portfolio_summary(&Scope::All).await.unwrap();
    assert_eq!(summary.unpriced_count, 1);
    let weth = store
        .list_holdings(&Scope::All)
        .await
        .unwrap()
        .into_iter()
        .find(|h| h.symbol.as_deref() == Some("WETH"))
        .unwrap();
    assert_eq!(weth.value_usd, None, "unpriced is not $0");
}

/// Answers `/chart/{coin}?start&span` with one daily point per requested day.
struct DailyChart;

impl wiremock::Respond for DailyChart {
    fn respond(&self, req: &wiremock::Request) -> ResponseTemplate {
        let q: std::collections::HashMap<_, _> = req.url.query_pairs().into_owned().collect();
        let start: i64 = q["start"].parse().unwrap();
        let span: i64 = q["span"].parse().unwrap();
        assert!(
            span <= 500,
            "DefiLlama allows at most 500 points per request"
        );
        assert_eq!(q["period"], "1d");
        let coin = req.url.path().trim_start_matches("/chart/").to_owned();
        let prices: Vec<_> = (0..span)
            .map(|i| json!({"timestamp": start + i * 86_400, "price": 2000.5}))
            .collect();
        ResponseTemplate::new(200).set_body_json(json!({"coins": {
            coin: {"symbol": "ETH", "confidence": 0.99, "prices": prices}
        }}))
    }
}

#[tokio::test]
async fn price_history_is_downloaded_once_and_remembered() {
    let server = MockServer::start().await;
    let store = store().await;
    let w = store.create_wallet("EVM").await.unwrap();
    let eth = store
        .add_account(&w.id, NetworkId::Ethereum, ETH, None)
        .await
        .unwrap();
    mount_zerion(&server, &fixture("zerion/positions.json")).await;
    zerion_engine(&store, &server).sync_account(&eth).await;
    server.reset().await;
    // Only quoted assets get history: price ETH and WETH, not the other tokens.
    Mock::given(path_regex("^/prices/current/.*"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"coins": {
            "coingecko:ethereum": {"price": 2695.0, "timestamp": NOW, "confidence": 0.99},
            "ethereum:0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2":
                {"price": 2694.1, "timestamp": NOW - 60, "confidence": 0.99, "decimals": 18}
        }})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex("^/chart/coingecko:ethereum$"))
        .respond_with(DailyChart)
        .mount(&server)
        .await;
    // The WETH contract has no series: remembered as unavailable, not zero.
    Mock::given(method("GET"))
        .and(path_regex("^/chart/ethereum:.*"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"coins": {}})))
        .mount(&server)
        .await;
    let engine = || {
        SyncEngine::new(
            store.clone(),
            Providers {
                defillama: Some(
                    DefiLlama::with_config(&server.uri(), Budget::unlimited(), fast()).unwrap(),
                ),
                ..Providers::default()
            },
            SyncOptions::default(),
        )
    };
    engine().refresh_prices().await;
    let first = engine().refresh_price_history().await;
    assert!(first.errors.is_empty(), "{first:?}");
    assert!(first.points > 0, "{first:?}");
    assert_eq!(
        first.unavailable,
        vec!["ethereum:token:0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2".to_owned()]
    );
    assert_eq!(first.pending_assets, 0, "{first:?}");
    assert!(store.accounting_dirty().await.unwrap());

    // Coverage is remembered: nothing is requested again the same day.
    let requests = server.received_requests().await.unwrap().len();
    let second = engine().refresh_price_history().await;
    assert_eq!(second.requests, 0, "{second:?}");
    assert_eq!(server.received_requests().await.unwrap().len(), requests);
}

#[tokio::test]
async fn cancellation_saves_the_first_page_and_resumes_without_duplicates() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let server = MockServer::start().await;
    let store = store().await;
    let wallet = store.create_wallet("Cancel test").await.unwrap();
    let account = store
        .add_account(&wallet.id, NetworkId::Bitcoin, BTC, None)
        .await
        .unwrap();
    mount_btc(&server, &chain(53)).await;
    let cancelled = Arc::new(AtomicBool::new(false));
    let flag = cancelled.clone();
    let first = btc_engine(&store, &server, 8)
        .with_cancellation(cancelled)
        .with_progress(Arc::new(move |_, pages, _| {
            if pages > 0 {
                flag.store(true, Ordering::Relaxed);
            }
        }));
    let report = first.sync_account(&account).await;
    assert_eq!(report.error, None);
    assert_eq!(report.coverage, Coverage::Paused);
    assert_eq!(activity_count(&store).await, 25);
    assert!(
        store
            .checkpoint(&account.id, "esplora", "history")
            .await
            .unwrap()
            .backfill_cursor
            .is_some()
    );
    let resumed = btc_engine(&store, &server, 8).sync_account(&account).await;
    assert_eq!(resumed.error, None);
    assert_eq!(resumed.coverage, Coverage::Complete);
    assert_eq!(activity_count(&store).await, 53);
}

#[tokio::test]
async fn a_confirmed_bitcoin_transaction_disappearing_from_the_chain_rolls_back() {
    let server = MockServer::start().await;
    let store = store().await;
    let wallet = store.create_wallet("Reorg test").await.unwrap();
    let account = store
        .add_account(&wallet.id, NetworkId::Bitcoin, BTC, None)
        .await
        .unwrap();
    let mut transaction = btc_tx(1);
    transaction["status"]["block_height"] = json!(900000);
    let hash = transaction["txid"].as_str().unwrap().to_owned();
    mount_btc(&server, &[transaction.clone()]).await;
    Mock::given(path(format!("/tx/{hash}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(transaction))
        .mount(&server)
        .await;
    assert_eq!(
        btc_engine(&store, &server, 2)
            .sync_account(&account)
            .await
            .error,
        None
    );
    store.replay_accounting().await.unwrap();
    mount_btc(&server, &[]).await;
    Mock::given(path(format!("/tx/{hash}")))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    assert_eq!(
        btc_engine(&store, &server, 2)
            .sync_account(&account)
            .await
            .error,
        None
    );
    store.replay_accounting().await.unwrap();
    let rows = store
        .list_activity(&Scope::All, &Default::default(), None, 200)
        .await
        .unwrap()
        .rows;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, "reorged");
    assert!(store.list_holdings(&Scope::All).await.unwrap().is_empty());
}
