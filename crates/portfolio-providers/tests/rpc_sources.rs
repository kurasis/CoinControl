//! Controlled RPC fixtures verify exact accounting effects, source selection and failures.
mod common;
use common::{SENTINEL_KEY, fast, store};
use num_bigint::BigInt;
use portfolio_core::network::NetworkId;
use portfolio_providers::{
    ProviderError, Providers, SyncEngine, SyncOptions,
    alchemy::Alchemy,
    helius::{self, Helius},
    http::Budget,
};
use portfolio_store::{Coverage, Scope};
use serde_json::{Value, json};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_partial_json, method},
};

const SOL: &str = "86xCnPeV69n6t3DnyGvkKobf9FdN2H9oiVDdaMpo2MMY";
const MINT: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
const ADDR: &str = "0x1db3439a222c519ab44bb1144fc28167b4fa6ee6";
const OTHER: &str = "0x2222222222222222222222222222222222222222";
const CONTRACT: &str = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";
const HASH: &str = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const TRANSFER: &str = "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";

async fn mount(server: &MockServer, m: &str, result: Value) {
    Mock::given(method("POST"))
        .and(body_partial_json(json!({"method":m})))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"jsonrpc":"2.0","id":1,"result":result})),
        )
        .mount(server)
        .await;
}
fn alchemy(server: &MockServer, b: std::sync::Arc<Budget>) -> Alchemy {
    Alchemy::with_config(
        &[
            (NetworkId::Ethereum, server.uri()),
            (NetworkId::Base, server.uri()),
        ],
        SENTINEL_KEY,
        b,
        fast(),
    )
    .unwrap()
}
fn helius(server: &MockServer, b: std::sync::Arc<Budget>) -> Helius {
    Helius::with_config(&server.uri(), SENTINEL_KEY, b, fast()).unwrap()
}
fn token_row(owner: &str, raw: &str, index: u32) -> Value {
    json!({"accountIndex":index,"mint":MINT,"owner":owner,"uiTokenAmount":{"amount":raw,"decimals":6}})
}
fn sol_tx(failed: bool) -> Value {
    json!({"slot":200,"transactionIndex":3,"blockTime":1700000000,
        "transaction":{"signatures":["caseSensitiveSignature"],"message":{"accountKeys":[{"pubkey":SOL},{"pubkey":"counterparty"}],"instructions":[{"program":"system","parsed":{"type":"transfer","info":{"source":SOL,"destination":"counterparty","lamports":1000000}}}]}},
        "meta":{"err":if failed {json!({"InstructionError":[0,"InvalidArgument"]})} else {Value::Null},"fee":5000,
            "preBalances":[10000000,0],"postBalances":[8995000,1000000],
            "preTokenBalances":[token_row(SOL,"9007199254740993",1)],
            "postTokenBalances":[token_row(SOL,"9007199254740992",1)]}})
}
fn topic(address: &str) -> String {
    format!("0x000000000000000000000000{}", &address[2..])
}
fn evm_receipt(failed: bool) -> Value {
    json!({"transactionHash":HASH,"blockNumber":"0xa","transactionIndex":"0x2","status":if failed {"0x0"} else {"0x1"},"gasUsed":"0x5208","effectiveGasPrice":"0x3b9aca00","l1Fee":"0x64",
        "logs":[{"address":CONTRACT,"topics":[TRANSFER,topic(ADDR),topic(OTHER)],"data":"0x20000000000001"},
                 {"address":CONTRACT,"topics":[TRANSFER,topic(OTHER),topic(ADDR)],"data":"0x01"},
                 {"address":CONTRACT,"topics":[TRANSFER,topic(ADDR),topic(ADDR)],"data":"0x10"},
                 {"address":CONTRACT,"topics":[TRANSFER,topic(ADDR),topic(OTHER),"0x01"],"data":"0x01"}]})
}
async fn evm_defaults(server: &MockServer, failed: bool) {
    mount(server, "eth_chainId", json!("0x1")).await;
    mount(server, "eth_getBalance", json!("0x1000000000000001")).await;
    mount(server,"alchemy_getTokenBalances",json!({"address":ADDR,"tokenBalances":[{"contractAddress":CONTRACT,"tokenBalance":"0x20000000000001"}]})).await;
    mount(
        server,
        "alchemy_getTokenMetadata",
        json!({"name":"USD Coin","symbol":"USDC","decimals":6}),
    )
    .await;
    mount(server, "eth_getTransactionReceipt", evm_receipt(failed)).await;
    mount(
        server,
        "eth_getTransactionByHash",
        json!({"hash":HASH,"from":ADDR,"to":OTHER,"value":"0xde0b6b3a7640000"}),
    )
    .await;
    mount(server,"alchemy_getAssetTransfers",json!({"transfers":[{"hash":HASH,"uniqueId":"a","metadata":{"blockTimestamp":"2023-11-14T22:13:20Z"}},{"hash":HASH,"uniqueId":"b","metadata":{"blockTimestamp":"2023-11-14T22:13:20Z"}}]})).await;
}
async fn sol_defaults(server: &MockServer) {
    mount(
        server,
        "getBalance",
        json!({"context":{"slot":100},"value":9007199254740993u64}),
    )
    .await;
    // Two token accounts of the same mint, including the second token program.
    mount(server,"getTokenAccountsByOwner",json!({"context":{"slot":101},"value":[{"account":{"data":{"parsed":{"info":{"owner":SOL,"mint":MINT,"tokenAmount":{"amount":"9007199254740993","decimals":6}}}}}}]})).await;
    mount(
        server,
        "getTransactionsForAddress",
        json!({"data":[sol_tx(false)],"paginationToken":null}),
    )
    .await;
}

#[tokio::test]
async fn solana_exact_fee_and_closed_token_account_delta() {
    let tx = helius::normalize(&sol_tx(false), SOL).unwrap();
    assert_eq!(tx.fee.unwrap().raw.to_string(), "5000");
    assert_eq!(tx.legs[0].signed_raw.to_string(), "-1000000");
    assert_eq!(tx.legs[0].counterparty.as_deref(), Some("counterparty"));
    assert!(!tx.legs[0].unresolved);
    assert_eq!(tx.legs[1].signed_raw.to_string(), "-1");
    assert_eq!(tx.legs[1].asset.contract.as_deref(), Some(MINT));
    let mut v = sol_tx(false);
    v["meta"]["postTokenBalances"] = json!([]);
    assert_eq!(
        helius::normalize(&v, SOL).unwrap().legs[1]
            .signed_raw
            .to_string(),
        "-9007199254740993"
    );
}
#[tokio::test]
async fn solana_failed_fee_only_and_sponsored_fee() {
    let tx = helius::normalize(&sol_tx(true), SOL).unwrap();
    assert!(tx.legs.is_empty());
    assert_eq!(tx.fee.unwrap().raw.to_string(), "5000");
    let mut v = sol_tx(false);
    v["transaction"]["message"]["accountKeys"][0]["pubkey"] = json!("sponsor");
    assert!(helius::normalize(&v, SOL).unwrap().fee.is_none());
}
#[tokio::test]
async fn solana_program_and_rent_effects_stay_unresolved() {
    let mut v = sol_tx(false);
    v["transaction"]["message"]["instructions"][0]["parsed"]["type"] = json!("createAccount");
    let tx = helius::normalize(&v, SOL).unwrap();
    assert!(tx.legs[0].unresolved);
    assert!(tx.legs[0].counterparty.is_none());
    v["meta"]["fee"] = Value::Null;
    assert!(helius::normalize(&v, SOL).is_err());
}
#[tokio::test]
async fn helius_actual_sync_aggregates_mints_and_remains_partial() {
    let server = MockServer::start().await;
    sol_defaults(&server).await;
    let store = store().await;
    let wallet = store.create_wallet("Solana").await.unwrap();
    let account = store
        .add_account(&wallet.id, NetworkId::Solana, SOL, None)
        .await
        .unwrap();
    let engine = SyncEngine::new(
        store.clone(),
        Providers {
            helius: Some(helius(&server, Budget::limited_with_credits(50, 100))),
            ..Providers::default()
        },
        SyncOptions::default(),
    );
    for _ in 0..2 {
        let r = engine.sync_account(&account).await;
        assert!(r.error.is_none(), "{:?}", r.error);
        assert_eq!(r.provider.as_deref(), Some("helius"));
        assert_eq!(r.coverage, Coverage::Partial);
    }
    let holdings = engine
        .providers()
        .helius
        .as_ref()
        .unwrap()
        .holdings(SOL)
        .await
        .unwrap();
    assert_eq!(
        holdings
            .assets
            .iter()
            .find(|(a, _)| a.contract.as_deref() == Some(MINT))
            .unwrap()
            .1
            .to_string(),
        "18014398509481986"
    );
    assert_eq!(
        store
            .list_activity(&Scope::All, &Default::default(), None, 50)
            .await
            .unwrap()
            .rows
            .len(),
        1
    );
    let usage = store
        .provider_usage(&portfolio_core::clock::utc_day(common::NOW))
        .await
        .unwrap();
    assert_eq!(usage[0].requests, 8);
    assert_eq!(usage[0].credits, 26);
}
#[tokio::test]
async fn alchemy_receipt_logs_exact_amounts_and_fee_once() {
    let server = MockServer::start().await;
    evm_defaults(&server, false).await;
    let api = alchemy(&server, Budget::unlimited());
    let tx = api
        .transaction(NetworkId::Ethereum, ADDR, HASH, 1700000000)
        .await
        .unwrap();
    assert_eq!(tx.legs.len(), 3);
    assert_eq!(tx.legs[0].signed_raw.to_string(), "-1000000000000000000");
    assert_eq!(tx.legs[1].signed_raw.to_string(), "-9007199254740993");
    assert_eq!(tx.legs[2].signed_raw.to_string(), "1");
    assert_eq!(tx.fee.unwrap().raw.to_string(), "21000000000000");
    let base = api
        .transaction(NetworkId::Base, ADDR, HASH, 1700000000)
        .await
        .unwrap();
    assert_eq!(base.fee.unwrap().raw.to_string(), "21000000000100");
    let received = api
        .transaction(NetworkId::Ethereum, OTHER, HASH, 1700000000)
        .await
        .unwrap();
    assert!(received.fee.is_none());
}
#[tokio::test]
async fn alchemy_failed_transaction_has_fee_without_principal() {
    let server = MockServer::start().await;
    evm_defaults(&server, true).await;
    let tx = alchemy(&server, Budget::unlimited())
        .transaction(NetworkId::Ethereum, ADDR, HASH, 1700000000)
        .await
        .unwrap();
    assert!(tx.legs.is_empty());
    assert_eq!(tx.fee.unwrap().raw.to_string(), "21000000000000");
}
#[tokio::test]
async fn alchemy_actual_sync_preserves_foreign_evidence_and_deduplicates_rows() {
    let server = MockServer::start().await;
    evm_defaults(&server, false).await;
    let store = store().await;
    let wallet = store.create_wallet("EVM").await.unwrap();
    let account = store
        .add_account(&wallet.id, NetworkId::Ethereum, ADDR, None)
        .await
        .unwrap();
    let api = alchemy(&server, Budget::unlimited());
    let mut richer = api
        .transaction(NetworkId::Ethereum, ADDR, HASH, 1700000000)
        .await
        .unwrap();
    richer.provider = "zerion";
    richer.operation = "trade".into();
    richer.fee.as_mut().unwrap().raw = BigInt::from(123);
    store
        .ingest_transaction(&account.id, &richer)
        .await
        .unwrap();
    let engine = SyncEngine::new(
        store.clone(),
        Providers {
            alchemy: Some(api),
            ..Providers::default()
        },
        SyncOptions::default(),
    );
    assert_eq!(engine.selected_provider(NetworkId::Bsc), Some("zerion"));
    for _ in 0..2 {
        let r = engine.sync_account(&account).await;
        assert!(r.error.is_none(), "{:?}", r.error);
        assert_eq!(r.provider.as_deref(), Some("alchemy"));
        assert_eq!(r.coverage, Coverage::Partial);
        assert_eq!(r.new_transactions, 0);
    }
    let activity = store
        .list_activity(&Scope::All, &Default::default(), None, 50)
        .await
        .unwrap();
    assert_eq!(activity.rows.len(), 1);
    assert_eq!(activity.rows[0].operation, "trade");
    // Exact prior fee and operation survive a partial-provider replacement.
    assert_eq!(
        activity.rows[0].fee_quantity.as_deref(),
        Some("0.000000000000000123")
    );
}
#[tokio::test]
async fn rpc_error_redacts_key_and_stops_shared_budget() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).respond_with(ResponseTemplate::new(200).set_body_json(json!({"jsonrpc":"2.0","id":1,"error":{"code":429,"message":format!("quota exhausted {SENTINEL_KEY}")}}))).expect(1).mount(&server).await;
    let b = Budget::limited_with_credits(50, 100);
    let api = helius(&server, b.clone());
    for _ in 0..2 {
        let e = api.slot().await.unwrap_err();
        assert!(matches!(e, ProviderError::RateLimited { .. }));
        assert!(!format!("{e:?} {e}").contains(SENTINEL_KEY));
    }
    assert_eq!(b.used(), 1);
    assert_eq!(b.credits(), 1);
}
#[tokio::test]
async fn credits_limit_counts_retries_and_prevents_next_request() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(503))
        .expect(2)
        .mount(&server)
        .await;
    let b = Budget::limited_with_credits(50, 1000);
    let api = alchemy(&server, b.clone());
    assert!(matches!(
        api.check_chain(NetworkId::Ethereum).await.unwrap_err(),
        ProviderError::BudgetExhausted { .. }
    ));
    assert_eq!(b.used(), 2);
    assert_eq!(b.credits(), 1000);
}
#[tokio::test]
async fn wrong_chain_and_missing_metadata_do_not_fabricate_balances() {
    let server = MockServer::start().await;
    mount(&server, "eth_chainId", json!("0x89")).await;
    assert!(
        alchemy(&server, Budget::unlimited())
            .holdings(NetworkId::Ethereum, ADDR)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn short_key_in_http_error_never_reaches_caller() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(400)
                .set_body_json(json!({"message":format!("invalid key {SENTINEL_KEY}")})),
        )
        .mount(&server)
        .await;
    let error = helius(&server, Budget::unlimited())
        .slot()
        .await
        .unwrap_err();
    assert!(!format!("{error} {error:?}").contains(SENTINEL_KEY));
}

#[tokio::test]
async fn cancellation_stops_before_next_rpc_and_keeps_checkpoint_paused() {
    let server = MockServer::start().await;
    let store = store().await;
    let wallet = store.create_wallet("Cancelled").await.unwrap();
    let account = store
        .add_account(&wallet.id, NetworkId::Solana, SOL, None)
        .await
        .unwrap();
    let b = Budget::limited_with_credits(50, 100);
    let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let engine = SyncEngine::new(
        store.clone(),
        Providers {
            helius: Some(helius(&server, b.clone())),
            ..Providers::default()
        },
        SyncOptions::default(),
    )
    .with_cancellation(flag);
    let r = engine.sync_account(&account).await;
    assert_eq!(r.coverage, Coverage::Paused);
    assert!(r.error.is_none());
    assert_eq!(b.used(), 0);
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn alchemy_persistent_block_cursor_avoids_expired_page_keys() {
    let server = MockServer::start().await;
    evm_defaults(&server, false).await;
    // Six indexed log events for one transaction; enrichment imports the entire
    // receipt once even when an event crosses the normalized page boundary.
    let rows:Vec<_>=(0..6).map(|i|json!({"hash":HASH,"uniqueId":format!("event-{i}"),"blockNum":"0xa","metadata":{"blockTimestamp":"2023-11-14T22:13:20Z"}})).collect();
    Mock::given(method("POST"))
        .and(body_partial_json(
            json!({"method":"alchemy_getAssetTransfers"}),
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"jsonrpc":"2.0","id":1,"result":{"transfers":rows}})),
        )
        .with_priority(1)
        .mount(&server)
        .await;
    let api = alchemy(&server, Budget::unlimited());
    let first = api
        .transactions(NetworkId::Ethereum, ADDR, true, None)
        .await
        .unwrap();
    assert_eq!(first.txs.len(), 1);
    let c = first.next.as_deref().unwrap();
    assert!(c.contains("event-4"));
    // A fresh adapter simulates a later process; no opaque provider key is saved.
    let second = alchemy(&server, Budget::unlimited())
        .transactions(NetworkId::Ethereum, ADDR, true, Some(c))
        .await
        .unwrap();
    assert!(second.txs.is_empty());
    assert!(second.next.is_none());
    let requests = server.received_requests().await.unwrap();
    let index: Vec<Value> = requests
        .iter()
        .map(|r| serde_json::from_slice::<Value>(&r.body).unwrap())
        .filter(|v| v["method"] == "alchemy_getAssetTransfers")
        .collect();
    assert_eq!(index[1]["params"][0]["toBlock"], "0xa");
    assert!(index[1]["params"][0].get("pageKey").is_none());
}

#[tokio::test]
async fn capped_token_scan_does_not_zero_omitted_cached_holdings() {
    let server = MockServer::start().await;
    evm_defaults(&server, false).await;
    let rows: Vec<_> = (1..=51)
        .map(|i| json!({"contractAddress":format!("0x{i:040x}"),"tokenBalance":"0x1"}))
        .collect();
    Mock::given(method("POST"))
        .and(body_partial_json(
            json!({"method":"alchemy_getTokenBalances"}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"jsonrpc":"2.0","id":1,"result":{"address":ADDR,"tokenBalances":rows}}),
        ))
        .with_priority(1)
        .mount(&server)
        .await;
    let store = store().await;
    let wallet = store.create_wallet("Capped").await.unwrap();
    let account = store
        .add_account(&wallet.id, NetworkId::Ethereum, ADDR, None)
        .await
        .unwrap();
    let mut old = portfolio_store::ingest::AssetSpec::native(NetworkId::Ethereum, "zerion");
    old.contract = Some(CONTRACT.into());
    old.decimals = 6;
    store
        .record_balance(&account.id, &old, &BigInt::from(777), None, "fresh")
        .await
        .unwrap();
    let engine = SyncEngine::new(
        store.clone(),
        Providers {
            alchemy: Some(alchemy(&server, Budget::unlimited())),
            ..Providers::default()
        },
        SyncOptions::default(),
    );
    let r = engine.sync_account(&account).await;
    assert!(r.error.is_none(), "{:?}", r.error);
    let balances = store.list_holdings(&Scope::All).await.unwrap();
    let prior = balances.iter().find(|b| b.asset_id == old.id()).unwrap();
    assert_eq!(prior.quantity, "0.000777");
    assert_eq!(prior.balance_status, portfolio_store::BalanceStatus::Stale);
}

#[tokio::test]
async fn fresh_alchemy_history_is_idempotent_and_reuses_persisted_metadata() {
    let server = MockServer::start().await;
    evm_defaults(&server, false).await;
    Mock::given(method("POST")).and(body_partial_json(json!({"method":"alchemy_getTokenMetadata"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"jsonrpc":"2.0","id":1,"result":{"decimals":6,"symbol":"USDC","name":"USD Coin"}})))
        .with_priority(1).expect(1).mount(&server).await;
    let store = store().await;
    let wallet = store.create_wallet("Fresh").await.unwrap();
    let account = store
        .add_account(&wallet.id, NetworkId::Ethereum, ADDR, None)
        .await
        .unwrap();
    for run in 0..2 {
        let engine = SyncEngine::new(
            store.clone(),
            Providers {
                alchemy: Some(alchemy(&server, Budget::unlimited())),
                ..Providers::default()
            },
            SyncOptions::default(),
        );
        let r = engine.sync_account(&account).await;
        assert!(r.error.is_none(), "{:?}", r.error);
        assert_eq!(r.new_transactions, if run == 0 { 1 } else { 0 });
        assert_eq!(r.coverage, Coverage::Partial);
    }
    let rows = store
        .list_activity(&Scope::All, &Default::default(), None, 50)
        .await
        .unwrap()
        .rows;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].legs.len(), 3);
    assert_eq!(rows[0].fee_quantity.as_deref(), Some("0.000021"));
    let incoming = store
        .checkpoint(&account.id, "alchemy", "history:incoming")
        .await
        .unwrap();
    let outgoing = store
        .checkpoint(&account.id, "alchemy", "history:outgoing")
        .await
        .unwrap();
    assert!(incoming.state.completed_once && outgoing.state.completed_once);
}

#[tokio::test]
async fn helius_minimum_context_lag_retries_with_accounted_cost() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).respond_with(ResponseTemplate::new(200).set_body_json(json!({"jsonrpc":"2.0","id":1,"error":{"code":-32016,"message":format!("Minimum context slot has not been reached {SENTINEL_KEY}")}}))).up_to_n_times(1).mount(&server).await;
    mount(&server, "getSlot", json!(200)).await;
    let b = Budget::limited_with_credits(50, 100);
    let api = helius(&server, b.clone());
    assert_eq!(api.slot().await.unwrap(), 200);
    assert_eq!(b.used(), 2);
    assert_eq!(b.credits(), 2);
    assert!(api.http().take_usage().last_error.is_none());
}

#[tokio::test]
async fn helius_unclassified_zero_decimal_assets_are_not_fungible_holdings() {
    let server = MockServer::start().await;
    sol_defaults(&server).await;
    Mock::given(method("POST")).and(body_partial_json(json!({"method":"getTokenAccountsByOwner"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"jsonrpc":"2.0","id":1,"result":{"context":{"slot":101},"value":[{"account":{"data":{"parsed":{"info":{"owner":SOL,"mint":MINT,"tokenAmount":{"amount":"1","decimals":0}}}}}}]}})))
        .with_priority(1).mount(&server).await;
    let holdings = helius(&server, Budget::unlimited())
        .holdings(SOL)
        .await
        .unwrap();
    assert!(!holdings.complete);
    assert_eq!(holdings.assets.len(), 1);
    assert!(holdings.assets[0].0.contract.is_none());
    let mut tx = sol_tx(false);
    tx["meta"]["preTokenBalances"][0]["uiTokenAmount"]["decimals"] = json!(0);
    tx["meta"]["postTokenBalances"][0]["uiTokenAmount"]["decimals"] = json!(0);
    assert_eq!(helius::normalize(&tx, SOL).unwrap().legs.len(), 1);
}

#[tokio::test]
async fn alchemy_forbidden_network_does_not_stop_other_enabled_mainnets() {
    let ethereum = MockServer::start().await;
    let base = MockServer::start().await;
    mount(&ethereum, "eth_chainId", json!("0x1")).await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(403))
        .expect(1)
        .mount(&base)
        .await;
    let b = Budget::limited_with_credits(50, 10000);
    let api = Alchemy::with_config(
        &[
            (NetworkId::Ethereum, ethereum.uri()),
            (NetworkId::Base, base.uri()),
        ],
        SENTINEL_KEY,
        b.clone(),
        fast(),
    )
    .unwrap();
    for _ in 0..2 {
        api.check_chain(NetworkId::Ethereum).await.unwrap();
        let error = api.check_chain(NetworkId::Base).await.unwrap_err();
        assert!(matches!(error, ProviderError::NetworkForbidden { .. }));
    }
    assert_eq!(b.used(), 3);
    assert_eq!(b.credits(), 1500);
}
