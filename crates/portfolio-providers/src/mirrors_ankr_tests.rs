//! Private adapter composition tests; no test-only public constructor is needed.
use super::*;
use crate::{Providers, SyncEngine, SyncOptions};
use portfolio_core::clock::FixedClock;
use portfolio_store::{ProfileKind, Scope, Store};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_partial_json, method},
};
const ADDRESS: &str = "0x1db3439a222c519ab44bb1144fc28167b4fa6ee6";
const TOKEN: &str = "0x55d398326f99059ff775485246999027b3197955";
fn config() -> HttpConfig {
    HttpConfig {
        min_interval: Duration::ZERO,
        max_retries: 1,
        backoff: Duration::ZERO,
        ..HttpConfig::default()
    }
}
fn reserve(server: &MockServer, budget: Arc<Budget>, advanced: bool) -> Reserve {
    let url = format!("{}/test-token", server.uri());
    let mut api = Reserve::with_config(
        "ankr",
        Kind::Rpc,
        &[(NetworkId::Bsc, url.clone())],
        budget.clone(),
        HeaderMap::new(),
        config(),
    )
    .unwrap();
    if advanced {
        api.ankr = Some(Ankr::with_config(Url::parse(&url).unwrap(), budget, config()).unwrap());
    }
    api
}
async fn rpc_result(server: &MockServer, name: &str, result: Value) {
    Mock::given(method("POST"))
        .and(body_partial_json(json!({"method":name})))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"jsonrpc":"2.0","id":1,"result":result})),
        )
        .mount(server)
        .await;
}
async fn node(server: &MockServer) {
    rpc_result(server, "eth_chainId", json!("0x38")).await;
    rpc_result(server, "eth_blockNumber", json!("0xabc")).await;
    rpc_result(server, "eth_getBalance", json!("0x20000000000001")).await;
}
fn token_page(next: &str, contract: &str) -> Value {
    json!({"assets":[{"blockchain":"bsc","holderAddress":ADDRESS,
        "tokenType":"ERC20","contractAddress":contract,"tokenDecimals":18,
        "tokenSymbol":"USDT","balanceRawInteger":"9007199254740993123456789"}],"nextPageToken":next})
}
#[tokio::test]
async fn discovery_pages_share_node_credits_and_never_invent_a_common_height() {
    let server = MockServer::start().await;
    node(&server).await;
    Mock::given(method("POST"))
        .and(body_partial_json(
            json!({"method":"ankr_getAccountBalance"}),
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"jsonrpc":"2.0","id":1,"result":token_page("next",TOKEN)})),
        )
        .with_priority(2)
        .mount(&server)
        .await;
    Mock::given(method("POST")).and(body_partial_json(json!({"method":"ankr_getAccountBalance","params":{"pageToken":"next"}})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"jsonrpc":"2.0","id":1,"result":token_page("", "0x0000000000000000000000000000000000000001")})))
        .with_priority(1).mount(&server).await;
    let budget = Budget::limited_with_credits(5, 2000);
    let api = reserve(&server, budget.clone(), true);
    let h = api.snapshot(NetworkId::Bsc, ADDRESS, &[]).await.unwrap();
    assert_eq!(h.assets.len(), 3);
    assert_eq!(h.assets[0].1.to_string(), "9007199254740993");
    assert_eq!(h.assets[1].1.to_string(), "9007199254740993123456789");
    assert_eq!(h.height, None);
    assert_eq!(budget.used(), 5);
    assert_eq!(budget.credits(), 2000);
    assert!(matches!(
        api.snapshot(NetworkId::Bsc, ADDRESS, &[]).await,
        Err(ProviderError::BudgetExhausted { .. })
    ));
    assert_eq!(server.received_requests().await.unwrap().len(), 5);
}
#[tokio::test]
async fn discovery_bounds_preserve_unseen_tokens_and_reject_repeated_cursors() {
    for repeated in [false, true] {
        let server = MockServer::start().await;
        node(&server).await;
        Mock::given(method("POST"))
            .and(body_partial_json(
                json!({"method":"ankr_getAccountBalance"}),
            ))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(
                    json!({"jsonrpc":"2.0","id":1,"result":token_page("page-2",TOKEN)}),
                ),
            )
            .with_priority(2)
            .mount(&server)
            .await;
        Mock::given(method("POST")).and(body_partial_json(json!({"method":"ankr_getAccountBalance","params":{"pageToken":"page-2"}})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"jsonrpc":"2.0","id":1,"result":token_page(if repeated {"page-2"} else {"page-3"}, "0x0000000000000000000000000000000000000001")})))
            .with_priority(1).mount(&server).await;
        let api = reserve(&server, Budget::limited(10), true);
        let h = api.snapshot(NetworkId::Bsc, ADDRESS, &[]).await;
        if repeated {
            assert!(matches!(h, Err(ProviderError::InvalidResponse { .. })));
        } else {
            assert!(h.unwrap().warnings.iter().any(|w| w.contains("two pages")));
        }
        assert_eq!(server.received_requests().await.unwrap().len(), 5);
    }
}
#[tokio::test]
async fn advanced_method_unavailable_retains_node_balance_without_claiming_discovery() {
    let server = MockServer::start().await;
    node(&server).await;
    Mock::given(method("POST")).and(body_partial_json(json!({"method":"ankr_getAccountBalance"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"method not available on free plan"}})))
        .mount(&server).await;
    let api = reserve(&server, Budget::limited(10), true);
    let h = api.snapshot(NetworkId::Bsc, ADDRESS, &[]).await.unwrap();
    assert_eq!(h.assets.len(), 1);
    assert!(
        h.warnings
            .iter()
            .any(|w| w.contains("unavailable on this plan"))
    );
}
#[tokio::test]
async fn http_and_semantic_429_switch_to_independent_mirror_and_remain_neutral() {
    for semantic in [false, true] {
        let primary = MockServer::start().await;
        node(&primary).await;
        let response = if semantic {
            ResponseTemplate::new(200).set_body_json(json!({"jsonrpc":"2.0","id":1,"error":{"code":429,"message":"rate limit with https://rpc.ankr.com/multichain/test-token"}}))
        } else {
            ResponseTemplate::new(429).insert_header("retry-after", "120")
        };
        Mock::given(method("POST"))
            .and(body_partial_json(
                json!({"method":"ankr_getAccountBalance"}),
            ))
            .respond_with(response)
            .expect(1)
            .mount(&primary)
            .await;
        let backup = MockServer::start().await;
        node(&backup).await;
        let store = Store::open_in_memory(ProfileKind::Test, Arc::new(FixedClock(1791100000)))
            .await
            .unwrap();
        let wallet = store.create_wallet("test").await.unwrap();
        let account = store
            .add_account(&wallet.id, NetworkId::Bsc, ADDRESS, None)
            .await
            .unwrap();
        let api = reserve(&primary, Budget::limited(10), true);
        let mirror = Reserve::with_config(
            "publicnode",
            Kind::Rpc,
            &[(NetworkId::Bsc, backup.uri())],
            Budget::limited(10),
            HeaderMap::new(),
            config(),
        )
        .unwrap();
        let engine = SyncEngine::new(
            store.clone(),
            Providers {
                reserves: vec![api, mirror],
                ..Providers::default()
            },
            SyncOptions::default(),
        );
        let rep = engine.sync_account(&account).await;
        assert!(rep.error.is_none(), "{:?}", rep.error);
        assert_eq!(rep.provider.as_deref(), Some("publicnode"));
        assert!(rep.balance_refreshed && rep.balance_only);
        let usage = store
            .provider_usage(&portfolio_core::clock::utc_day(1791100000))
            .await
            .unwrap();
        let ankr = usage.iter().find(|u| u.provider == "ankr").unwrap();
        assert_eq!(ankr.requests, 4);
        assert!(ankr.last_error.is_none());
        assert!(store.provider_pause("ankr").await.unwrap().is_some());
        assert!(
            store
                .list_activity(&Scope::All, &Default::default(), None, 20)
                .await
                .unwrap()
                .rows
                .is_empty()
        );
    }
}
#[tokio::test]
async fn transport_retries_count_separate_advanced_attempts_and_credits() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    rpc_result(&server, "ankr_getAccountBalance", json!({"assets":[]})).await;
    let budget = Budget::limited_with_credits(2, 1400);
    let api = Ankr::with_config(
        Url::parse(&format!("{}/retry-credential", server.uri())).unwrap(),
        budget.clone(),
        HttpConfig {
            min_interval: Duration::from_millis(40),
            ..config()
        },
    )
    .unwrap();
    let started = std::time::Instant::now();
    api.balances_page(NetworkId::Bsc, ADDRESS, None)
        .await
        .unwrap();
    assert!(started.elapsed() >= Duration::from_millis(40));
    assert_eq!(budget.used(), 2);
    assert_eq!(budget.credits(), 1400);
}

#[tokio::test]
async fn http_errors_never_echo_even_short_authenticated_tokens() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(400).set_body_json(json!({"message":"short-key rejected"})),
        )
        .mount(&server)
        .await;
    let api = Ankr::with_config(
        Url::parse(&format!("{}/short-key", server.uri())).unwrap(),
        Budget::limited(1),
        config(),
    )
    .unwrap();
    let error = match api.balances_page(NetworkId::Bsc, ADDRESS, None).await {
        Err(error) => error,
        Ok(_) => panic!("HTTP failure was accepted"),
    };
    assert!(matches!(error, ProviderError::Http { status: 400, .. }));
    assert!(!error.to_string().contains("short-key"));
    assert!(
        !api.http()
            .take_usage()
            .last_error
            .unwrap()
            .contains("short-key")
    );
}
