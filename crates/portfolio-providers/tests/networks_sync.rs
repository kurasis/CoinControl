//! Stage D synchronization against local mock servers (TESTING.md Layer A):
//! TRON's two history categories add up into one transaction, staked TRX is
//! part of the holding, token metadata is learned without fabricating
//! balances, TON traces still in progress are re-read until final, and
//! repeated runs never duplicate anything.

mod common;

use common::{fast, store};
use portfolio_core::network::NetworkId;
use portfolio_providers::http::Budget;
use portfolio_providers::tonapi::TonApi;
use portfolio_providers::trongrid::TronGrid;
use portfolio_providers::{Providers, SyncEngine, SyncOptions};
use portfolio_store::{ActivityRow, Coverage, Scope, Store};
use serde_json::{Value, json};
use wiremock::matchers::{method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TRON: &str = "TT2T17KZhoDu47i2E4FWxfG79zdkEWkU9N";
const TRON_HEX: &str = "41bb1712d44d09feee51d071219b4c5d9792b76b29";
const OTHER: &str = "TN8rTPYz5AvRYGGtzRLdA96nSfoqCy7gtq";
const OTHER_HEX: &str = "41857484a0018c1b223f674f1e72f55633f4479b36";
const USDT: &str = "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t";
const NO_METADATA: &str = "TXDk8mbtRbXeYuMNS83CfKPaYYT8XWv9Hz";
const SEND_TX: &str = "fc476d111024e15f8a9e899253fb34bf5fae713c67e50f57cbed34babe9cb584";

fn usdt_event(tx: &str, from: &str, to: &str, value: &str, at_ms: i64) -> Value {
    json!({"transaction_id": tx,
           "token_info": {"symbol": "USDT", "address": USDT, "decimals": 6, "name": "Tether USD"},
           "block_timestamp": at_ms, "from": from, "to": to, "type": "Transfer", "value": value})
}

fn trx_tx(id: &str, owner: &str, to: &str, amount: i64, fee: i64, at_ms: i64) -> Value {
    json!({"ret": [{"contractRet": "SUCCESS", "fee": fee}], "txID": id, "blockNumber": 1,
           "block_timestamp": at_ms, "net_fee": fee, "energy_fee": 0,
           "raw_data": {"contract": [{"type": "TransferContract", "parameter": {"value":
               {"amount": amount, "owner_address": owner, "to_address": to}}}]},
           "internal_transactions": []})
}

fn page(data: Vec<Value>, next: Option<&str>) -> Value {
    match next {
        Some(f) => json!({"data": data, "success": true,
                          "meta": {"at": 1, "fingerprint": f, "page_size": data.len(),
                                   "links": {"next": "https://api.trongrid.io/next"}}}),
        None => json!({"data": data, "success": true, "meta": {"at": 1, "page_size": data.len()}}),
    }
}

async fn mount_tron(server: &MockServer) {
    let accounts = format!("/v1/accounts/{TRON}");
    Mock::given(method("GET"))
        .and(path(accounts.clone()))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"success": true, "meta": {},
            "data": [{"address": TRON_HEX, "balance": 10_000_000,
                      "frozenV2": [{}, {"type": "ENERGY", "amount": 5_000_000}],
                      "unfrozenV2": [{"unfreeze_amount": 1_000_000, "unfreeze_expire_time": 1}],
                      "trc20": [{USDT: "4000000"}, {NO_METADATA: "77"}]}]})),
        )
        .mount(server)
        .await;

    let trc20 = format!("{accounts}/transactions/trc20");
    // Metadata lookups by contract.
    Mock::given(method("GET"))
        .and(path(trc20.clone()))
        .and(query_param("contract_address", USDT))
        .respond_with(ResponseTemplate::new(200).set_body_json(page(
            vec![usdt_event(
                SEND_TX,
                TRON,
                OTHER,
                "3123513000000",
                1_790_761_893_000,
            )],
            None,
        )))
        .with_priority(1)
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path(trc20.clone()))
        .and(query_param("contract_address", NO_METADATA))
        .respond_with(ResponseTemplate::new(200).set_body_json(page(vec![], None)))
        .with_priority(1)
        .mount(server)
        .await;
    // TRC-20 history: the send (same transaction as a native record) and a receipt.
    Mock::given(method("GET"))
        .and(path(trc20))
        .and(query_param_is_missing("contract_address"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page(
            vec![
                usdt_event(SEND_TX, TRON, OTHER, "3123513000000", 1_790_761_893_000),
                usdt_event(&"cd".repeat(32), OTHER, TRON, "5000000", 1_790_000_000_000),
            ],
            None,
        )))
        .with_priority(2)
        .mount(server)
        .await;

    // Native history in two pages.
    let native = format!("{accounts}/transactions");
    let token_send = json!({"ret": [{"contractRet": "SUCCESS", "fee": 13_373_400}], "txID": SEND_TX,
        "blockNumber": 86_696_464, "block_timestamp": 1_790_761_893_000_i64, "net_fee": 345_000,
        "energy_fee": 13_028_400,
        "raw_data": {"contract": [{"type": "TriggerSmartContract", "parameter": {"value": {
            "data": "a9059cbb00", "owner_address": TRON_HEX,
            "contract_address": "41a614f803b6fd780986a42c78ec9c7f77e6ded13c"}}}]},
        "internal_transactions": []});
    Mock::given(method("GET"))
        .and(path(native.clone()))
        .and(query_param("fingerprint", "F2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page(
            vec![trx_tx(
                &"ab".repeat(32),
                TRON_HEX,
                OTHER_HEX,
                2_000_000,
                1_100_000,
                1_780_000_000_000,
            )],
            None,
        )))
        .with_priority(1)
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path(native))
        .respond_with(ResponseTemplate::new(200).set_body_json(page(
            vec![
                trx_tx(
                    &"ef".repeat(32),
                    OTHER_HEX,
                    TRON_HEX,
                    8,
                    0,
                    1_791_000_000_000,
                ),
                token_send,
            ],
            Some("F2"),
        )))
        .with_priority(2)
        .mount(server)
        .await;
}

fn tron_engine(store: &Store, server: &MockServer) -> SyncEngine {
    SyncEngine::new(
        store.clone(),
        Providers {
            trongrid: Some(
                TronGrid::with_config(&server.uri(), "test-key", Budget::unlimited(), fast())
                    .unwrap(),
            ),
            ..Providers::default()
        },
        SyncOptions::default(),
    )
}

async fn rows(store: &Store) -> Vec<ActivityRow> {
    store
        .list_activity(&Scope::All, &Default::default(), None, 200)
        .await
        .unwrap()
        .rows
}

#[tokio::test]
async fn tron_native_and_trc20_categories_add_up_without_duplicates() {
    let server = MockServer::start().await;
    mount_tron(&server).await;
    let store = store().await;
    let w = store.create_wallet("TRON").await.unwrap();
    let account = store
        .add_account(&w.id, NetworkId::Tron, TRON, None)
        .await
        .unwrap();

    let r1 = tron_engine(&store, &server).sync_account(&account).await;
    assert_eq!(r1.error, None);
    assert!(r1.balance_refreshed);
    assert_eq!(r1.coverage, Coverage::Complete, "both categories exhausted");
    assert_eq!(r1.provider.as_deref(), Some("trongrid"));

    let holdings = store.list_holdings(&Scope::All).await.unwrap();
    let qty = |id: &str| {
        holdings
            .iter()
            .find(|h| h.asset_id == id)
            .map(|h| h.quantity.clone())
    };
    assert_eq!(
        qty("tron:native").as_deref(),
        Some("16"),
        "liquid + frozen + unfreezing TRX"
    );
    assert_eq!(qty(&format!("tron:token:{USDT}")).as_deref(), Some("4"));
    assert_eq!(
        qty(&format!("tron:token:{NO_METADATA}")),
        None,
        "a balance without decimals is missing, never guessed"
    );

    let all = rows(&store).await;
    assert_eq!(all.len(), 4, "{all:#?}");
    let send = all
        .iter()
        .find(|r| r.transaction_id.ends_with(SEND_TX))
        .unwrap();
    assert_eq!(send.operation, "send");
    assert_eq!(send.legs.len(), 1, "token leg from the TRC-20 category");
    assert_eq!(send.legs[0].signed_quantity, "-3123513");
    assert_eq!(
        send.fee_quantity.as_deref(),
        Some("13.3734"),
        "fee from the native record"
    );

    // A second run re-reads the newest pages: the native record is ingested
    // again and must keep the TRC-20 component; nothing is duplicated.
    let r2 = tron_engine(&store, &server).sync_account(&account).await;
    assert_eq!(r2.error, None);
    assert_eq!(r2.new_transactions, 0);
    let again = rows(&store).await;
    assert_eq!(again.len(), 4);
    let send = again
        .iter()
        .find(|r| r.transaction_id.ends_with(SEND_TX))
        .unwrap();
    assert_eq!(send.legs.len(), 1);
    assert_eq!(send.fee_quantity.as_deref(), Some("13.3734"));

    let status = store.sync_status().await.unwrap();
    assert_eq!(status[0].coverage, Some(Coverage::Complete));
    assert_eq!(status[0].transaction_count, 4);
}

#[tokio::test]
async fn tron_without_key_reports_the_missing_credential() {
    let store = store().await;
    let w = store.create_wallet("TRON").await.unwrap();
    let account = store
        .add_account(&w.id, NetworkId::Tron, TRON, None)
        .await
        .unwrap();
    let engine = SyncEngine::new(store.clone(), Providers::default(), SyncOptions::default());
    let r = engine.sync_account(&account).await;
    assert!(r.error.unwrap().contains("trongrid: no API key"));
    assert!(TronGrid::new("https://api.trongrid.io/", " ", Budget::unlimited()).is_err());
}

// ------------------------------------------------------------------ TON

const TON_RAW: &str = "0:83dfd552e63729b472fcbcc8c45ebcc6691702558b68ec7527e1ba403a0f31a8";
const TON_FRIENDLY: &str = "EQCD39VS5jcptHL8vMjEXrzGaRcCVYto7HUn4bpAOg8xqB2N";
const PEER: &str = "0:53bff336e1032a6e2c848678cd09f614e742f645f987f49094893c35c6992709";
const MASTER: &str = "0:b113a994b5024a16719f69139328eb759596c38a25f59028b146fecdc3621dfe";

fn ton_event(id: &str, lt: i64, amount: i64, in_progress: bool) -> Value {
    json!({"event_id": id, "timestamp": 1_790_000_000 + lt, "lt": lt, "in_progress": in_progress,
           "extra": -100, "is_scam": false,
           "actions": [{"type": "TonTransfer", "status": "ok", "TonTransfer": {
               "sender": {"address": PEER}, "recipient": {"address": TON_RAW}, "amount": amount},
               "base_transactions": [format!("{lt:064x}")]}]})
}

async fn mount_ton(server: &MockServer, newest_in_progress: bool) {
    server.reset().await;
    Mock::given(method("GET"))
        .and(path(format!("/accounts/{TON_RAW}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "address": TON_RAW, "balance": 3_000_000_000_i64, "status": "active"})))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/accounts/{TON_RAW}/jettons")))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"balances": [
            {"balance": "2500000", "wallet_address": {"address": PEER},
             "jetton": {"address": MASTER, "name": "Tether USD", "symbol": "USD₮", "decimals": 6,
                        "verification": "whitelist"}}]})),
        )
        .mount(server)
        .await;
    let events = format!("/accounts/{TON_RAW}/events");
    Mock::given(method("GET"))
        .and(path(events.clone()))
        .and(query_param("before_lt", "200"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "events": [ton_event("0a", 100, 1_000_000_000, false)], "next_from": 0})))
        .with_priority(1)
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path(events.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "events": [ton_event("0c", 300, 7, newest_in_progress), ton_event("0b", 200, 2_000_000_000, false)],
            "next_from": 200})))
        .with_priority(2)
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("{events}/0c")))
        .respond_with(ResponseTemplate::new(200).set_body_json(ton_event(
            "0c",
            300,
            7,
            newest_in_progress,
        )))
        .mount(server)
        .await;
}

fn ton_engine(store: &Store, server: &MockServer, pages: u32) -> SyncEngine {
    SyncEngine::new(
        store.clone(),
        Providers {
            tonapi: Some(
                TonApi::with_config(&server.uri(), "", Budget::unlimited(), fast()).unwrap(),
            ),
            ..Providers::default()
        },
        SyncOptions {
            max_history_pages: pages,
            ..SyncOptions::default()
        },
    )
}

#[tokio::test]
async fn ton_trace_in_progress_is_reread_until_final() {
    let server = MockServer::start().await;
    mount_ton(&server, true).await;
    let store = store().await;
    let w = store.create_wallet("TON").await.unwrap();
    // The user-friendly form is stored as the canonical raw account.
    let account = store
        .add_account(&w.id, NetworkId::Ton, TON_FRIENDLY, None)
        .await
        .unwrap();
    assert_eq!(account.canonical_address, TON_RAW);

    // One page per run: the second page arrives through the backfill cursor.
    let r1 = ton_engine(&store, &server, 1).sync_account(&account).await;
    assert_eq!(r1.error, None);
    assert_eq!((r1.pages_fetched, r1.new_transactions), (1, 2));
    assert_eq!(r1.coverage, Coverage::Loading);
    let holdings = store.list_holdings(&Scope::All).await.unwrap();
    assert!(
        holdings
            .iter()
            .any(|h| h.asset_id == "ton:native" && h.quantity == "3")
    );
    assert!(
        holdings
            .iter()
            .any(|h| h.asset_id == format!("ton:token:{MASTER}") && h.quantity == "2.5")
    );
    let pending = rows(&store).await;
    assert_eq!(pending.iter().filter(|r| r.status == "pending").count(), 1);

    // The trace completes; the next run finalizes it and finishes the backfill.
    mount_ton(&server, false).await;
    let r2 = ton_engine(&store, &server, 2).sync_account(&account).await;
    assert_eq!(r2.error, None);
    assert_eq!(r2.new_transactions, 1);
    assert_eq!(r2.coverage, Coverage::Complete);
    let all = rows(&store).await;
    assert_eq!(all.len(), 3, "event and its transactions are one record");
    assert!(all.iter().all(|r| r.status == "confirmed"), "{all:#?}");
    assert!(
        all.iter()
            .all(|r| r.fee_quantity.as_deref() == Some("0.0000001"))
    );

    let r3 = ton_engine(&store, &server, 8).sync_account(&account).await;
    assert_eq!((r3.new_transactions, r3.pages_fetched), (0, 1));
    assert_eq!(rows(&store).await.len(), 3);
}

#[tokio::test]
async fn tokens_without_a_quote_are_not_requested_again_for_a_day() {
    use portfolio_providers::defillama::DefiLlama;
    use wiremock::matchers::path_regex;

    let server = MockServer::start().await;
    mount_tron(&server).await;
    Mock::given(path_regex("^/prices/current/.*"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"coins": {
            "coingecko:tron": {"price": 0.33, "symbol": "TRX", "timestamp": common::NOW, "confidence": 0.99}
        }})))
        .mount(&server)
        .await;
    let store = store().await;
    let w = store.create_wallet("TRON").await.unwrap();
    let account = store
        .add_account(&w.id, NetworkId::Tron, TRON, None)
        .await
        .unwrap();
    let engine = SyncEngine::new(
        store.clone(),
        Providers {
            trongrid: Some(
                TronGrid::with_config(&server.uri(), "test-key", Budget::unlimited(), fast())
                    .unwrap(),
            ),
            defillama: Some(
                DefiLlama::with_config(&server.uri(), Budget::unlimited(), fast()).unwrap(),
            ),
            ..Providers::default()
        },
        SyncOptions::default(),
    );
    engine.sync_account(&account).await;
    let price_paths = |reqs: &[wiremock::Request]| -> Vec<String> {
        reqs.iter()
            .map(|r| r.url.path().to_owned())
            .filter(|p| p.starts_with("/prices/current/"))
            .collect()
    };

    let first = engine.refresh_prices().await;
    assert_eq!(first.priced, 1);
    let reqs = server.received_requests().await.unwrap();
    let paths = price_paths(&reqs);
    assert_eq!(paths.len(), 1);
    // Case-sensitive TRON identity sent exactly as stored.
    assert!(paths[0].contains(&format!("tron:{USDT}")), "{paths:?}");

    let second = engine.refresh_prices().await;
    assert_eq!(second.priced, 1);
    assert!(second.unpriced.contains(&format!("tron:token:{USDT}")));
    let reqs = server.received_requests().await.unwrap();
    let paths = price_paths(&reqs);
    assert_eq!(paths.len(), 2);
    assert!(
        !paths[1].contains(USDT),
        "a known miss is not asked again: {paths:?}"
    );
    assert!(
        paths[1].contains("coingecko:tron"),
        "natives are always refreshed"
    );
}
