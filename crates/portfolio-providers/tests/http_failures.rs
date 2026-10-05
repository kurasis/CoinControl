//! Controlled HTTP failure handling (TESTING.md Layer A, "Reliability").
//! A local mock server stands in for providers; no external network is used.

mod common;

use std::time::Duration;

use common::{SENTINEL_KEY, fast};
use portfolio_core::network::NetworkId;
use portfolio_providers::ProviderError;
use portfolio_providers::esplora::Esplora;
use portfolio_providers::http::{Budget, HttpConfig};
use portfolio_providers::livecoinwatch::LiveCoinWatch;
use portfolio_providers::zerion::{Window, Zerion};
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const ADDR: &str = "0x1db3439a222c519ab44bb1144fc28167b4fa6ee6";

fn assert_no_secret(e: &ProviderError) {
    let text = format!("{e} {e:?}");
    assert!(!text.contains(SENTINEL_KEY), "credential leaked: {text}");
    assert!(!text.contains("http://"), "URL leaked: {text}");
}

async fn lcw(server: &MockServer, config: HttpConfig) -> LiveCoinWatch {
    LiveCoinWatch::with_config(&server.uri(), SENTINEL_KEY, Budget::unlimited(), config).unwrap()
}

#[tokio::test]
async fn unauthorized_fails_once_without_retry() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/coins/map"))
        .and(header("x-api-key", SENTINEL_KEY))
        .respond_with(ResponseTemplate::new(401).set_body_string(format!(
            r#"{{"error":{{"code":401,"description":"invalid key {SENTINEL_KEY}"}}}}"#
        )))
        .expect(1)
        .mount(&server)
        .await;
    let e = lcw(&server, fast())
        .await
        .quotes(&["BTC"])
        .await
        .unwrap_err();
    assert!(
        matches!(e, ProviderError::Auth { status: 401, .. }),
        "{e:?}"
    );
    assert!(e.stops_provider());
    assert_no_secret(&e);
}

#[tokio::test]
async fn rate_limit_with_retry_after_is_retried() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/coins/map"))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "0"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/coins/map"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(
                r#"[{"code":"BTC","rate":85042.5048971402,"delta":{"day":1.0051}}]"#,
            ),
        )
        .mount(&server)
        .await;
    let client = lcw(&server, fast()).await;
    let quotes = client.quotes(&["BTC"]).await.unwrap();
    assert_eq!(quotes[0].rate_usd.to_string(), "85042.5048971402");
    assert_eq!(
        client.http().take_usage().requests,
        2,
        "retries are counted"
    );
}

#[tokio::test]
async fn long_retry_after_and_exhausted_retries_fail_fast() {
    let server = MockServer::start().await;
    // Daily quota exhausted: the provider asks to come back in an hour.
    Mock::given(method("POST"))
        .and(path("/credits"))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "3600"))
        .expect(1)
        .mount(&server)
        .await;
    // Throttled without Retry-After: backoff, then give up after max_retries.
    Mock::given(method("POST"))
        .and(path("/coins/map"))
        .respond_with(ResponseTemplate::new(429))
        .expect(3)
        .mount(&server)
        .await;
    let client = lcw(&server, fast()).await;
    let e = client.credits().await.unwrap_err();
    assert!(
        matches!(
            e,
            ProviderError::RateLimited {
                retry_after_secs: Some(3600),
                ..
            }
        ),
        "{e:?}"
    );
    // Independently exercise short throttling with a fresh credential budget.
    let e = lcw(&server, fast())
        .await
        .quotes(&["BTC"])
        .await
        .unwrap_err();
    assert!(matches!(
        e,
        ProviderError::RateLimited {
            retry_after_secs: None,
            ..
        }
    ));
}

#[tokio::test]
async fn server_errors_are_retried_then_succeed() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/blocks/tip/height"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/blocks/tip/height"))
        .respond_with(ResponseTemplate::new(200).set_body_string("969794"))
        .mount(&server)
        .await;
    let e = Esplora::with_config(&server.uri(), Budget::unlimited(), fast()).unwrap();
    assert_eq!(e.tip_height().await.unwrap(), 969_794);
}

#[tokio::test]
async fn timeout_invalid_json_too_large_and_not_found() {
    let server = MockServer::start().await;
    Mock::given(path("/blocks/tip/height"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(3)))
        .mount(&server)
        .await;
    Mock::given(path("/address/1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{not json"))
        .mount(&server)
        .await;
    Mock::given(path(
        "/address/1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa/txs/mempool",
    ))
    .respond_with(ResponseTemplate::new(200).set_body_string("x".repeat(2 * 1024 * 1024)))
    .mount(&server)
    .await;
    Mock::given(path(format!("/tx/{}", "ab".repeat(32))))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    let config = HttpConfig {
        timeout: Duration::from_millis(300),
        max_retries: 0,
        ..fast()
    };
    let e = Esplora::with_config(&server.uri(), Budget::unlimited(), config).unwrap();
    assert!(matches!(
        e.tip_height().await.unwrap_err(),
        ProviderError::Timeout { .. }
    ));
    let err = e
        .address("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa")
        .await
        .unwrap_err();
    assert!(
        matches!(err, ProviderError::InvalidResponse { .. }),
        "{err:?}"
    );
    let err = e
        .mempool_txs("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa")
        .await
        .unwrap_err();
    assert!(matches!(err, ProviderError::TooLarge { .. }), "{err:?}");
    // A dropped/replaced unconfirmed transaction is "gone", not an error.
    assert!(e.tx(&"ab".repeat(32)).await.unwrap().is_none());
}

#[tokio::test]
async fn provider_error_inside_http_200_is_an_error_not_empty_data() {
    let server = MockServer::start().await;
    Mock::given(path(format!("/wallets/{ADDR}/positions/")))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"errors":[{"title":"Bad request","detail":"unsupported filter"}]}"#,
        ))
        .mount(&server)
        .await;
    let z = Zerion::with_config(&server.uri(), SENTINEL_KEY, Budget::unlimited(), fast()).unwrap();
    let e = z.positions(NetworkId::Ethereum, ADDR).await.unwrap_err();
    assert!(matches!(e, ProviderError::InvalidResponse { .. }), "{e:?}");
    assert_no_secret(&e);
}

#[tokio::test]
async fn repeated_cursor_is_detected() {
    let server = MockServer::start().await;
    let body = format!(
        r#"{{"links":{{"next":"{}/wallets/{ADDR}/transactions/?page%5Bafter%5D=SAME"}},"data":[]}}"#,
        server.uri()
    );
    Mock::given(path(format!("/wallets/{ADDR}/transactions/")))
        .and(query_param("page[after]", "SAME"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(&server)
        .await;
    let z = Zerion::with_config(&server.uri(), SENTINEL_KEY, Budget::unlimited(), fast()).unwrap();
    let e = z
        .transactions(
            NetworkId::Ethereum,
            ADDR,
            Some("SAME"),
            10,
            Window::default(),
        )
        .await
        .unwrap_err();
    assert!(matches!(e, ProviderError::RepeatedCursor { .. }), "{e:?}");
}

#[tokio::test]
async fn indexing_wallet_is_loading_not_empty() {
    let server = MockServer::start().await;
    Mock::given(path(format!("/wallets/{ADDR}/positions/")))
        .respond_with(ResponseTemplate::new(202).set_body_string(r#"{"data":[]}"#))
        .mount(&server)
        .await;
    let z = Zerion::with_config(&server.uri(), SENTINEL_KEY, Budget::unlimited(), fast()).unwrap();
    assert_eq!(
        z.positions(NetworkId::Ethereum, ADDR).await.unwrap(),
        portfolio_providers::zerion::Positions::Indexing
    );
}

#[tokio::test]
async fn budget_counts_retries_and_stops() {
    let server = MockServer::start().await;
    Mock::given(path("/blocks/tip/height"))
        .respond_with(ResponseTemplate::new(500))
        .expect(2)
        .mount(&server)
        .await;
    let e = Esplora::with_config(&server.uri(), Budget::limited(2), fast()).unwrap();
    let err = e.tip_height().await.unwrap_err();
    assert!(
        matches!(err, ProviderError::BudgetExhausted { .. }),
        "{err:?}"
    );
    assert_eq!(e.http().budget().used(), 2);
}

#[test]
fn missing_keys_are_reported_without_requests() {
    let z = Zerion::new("http://127.0.0.1:9", "  ", Budget::unlimited());
    assert!(matches!(z.err(), Some(ProviderError::MissingKey { .. })));
    let l = LiveCoinWatch::new("http://127.0.0.1:9", "", Budget::unlimited());
    assert!(matches!(l.err(), Some(ProviderError::MissingKey { .. })));
}

#[tokio::test]
async fn authentication_failure_stops_every_client_sharing_the_credential_budget() {
    let server = MockServer::start().await;
    Mock::given(path("/coins/map"))
        .respond_with(ResponseTemplate::new(403))
        .expect(1)
        .mount(&server)
        .await;
    let budget = Budget::limited(50);
    let first =
        LiveCoinWatch::with_config(&server.uri(), SENTINEL_KEY, budget.clone(), fast()).unwrap();
    let second =
        LiveCoinWatch::with_config(&server.uri(), SENTINEL_KEY, budget.clone(), fast()).unwrap();
    assert!(matches!(
        first.quotes(&["BTC"]).await,
        Err(ProviderError::Auth { .. })
    ));
    assert!(matches!(
        second.quotes(&["ETH"]).await,
        Err(ProviderError::Auth { status: 403, .. })
    ));
    assert_eq!(budget.used(), 1);
}
