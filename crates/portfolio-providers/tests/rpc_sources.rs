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
