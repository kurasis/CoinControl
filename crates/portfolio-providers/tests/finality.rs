mod common;
use common::fast;
use portfolio_core::network::NetworkId;
use portfolio_providers::{
    finality::{self, Validation},
    http::{Budget, HttpClient},
};
use reqwest::header::HeaderMap;
use serde_json::{Value, json};
use url::Url;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_partial_json, method},
};
fn hash() -> String {
    format!("0x{}", "ab".repeat(32))
}
async fn answer(s: &MockServer, m: &str, params: Option<Value>, result: Value) {
    let mut matcher = json!({"method":m});
    if let Some(params) = params {
        matcher["params"] = params;
    }
    Mock::given(method("POST"))
        .and(body_partial_json(matcher))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"jsonrpc":"2.0","id":1,"result":result})),
        )
        .mount(s)
        .await;
}
fn client() -> HttpClient {
    HttpClient::new("publicnode", fast(), Budget::limited(20), HeaderMap::new()).unwrap()
}
async fn evm_header(s: &MockServer) {
    answer(s, "eth_chainId", None, json!("0x1")).await;
    answer(
        s,
        "eth_getBlockByNumber",
        Some(json!(["finalized", false])),
        json!({"number":"0x20","hash":hash()}),
    )
    .await;
}
#[tokio::test]
async fn matching_canonical_receipt_is_final_but_a_fork_receipt_is_reorged() {
    for mismatch in [false, true] {
        let s = MockServer::start().await;
        evm_header(&s).await;
        answer(&s,"eth_getTransactionReceipt",None,json!({"transactionHash":hash(),"blockNumber":"0x10","blockHash":hash(),"status":"0x1"})).await;
        answer(
            &s,
            "eth_getBlockByNumber",
            Some(json!(["0x10", false])),
            json!({"number":"0x10","hash":if mismatch {format!("0x{}","cd".repeat(32))} else {hash()}}),
        )
        .await;
        let result = finality::evm(
            &client(),
            Url::parse(&s.uri()).unwrap(),
            NetworkId::Ethereum,
            &[(hash(), 16)],
        )
        .await
        .unwrap();
        assert_eq!(
            result.results[0].1,
            if mismatch {
                Validation::Reorged
            } else {
                Validation::Final
            }
        );
    }
}
#[tokio::test]
async fn missing_index_entry_is_not_a_reorg_when_block_still_contains_the_transaction() {
    let s = MockServer::start().await;
    evm_header(&s).await;
    answer(&s, "eth_getTransactionReceipt", None, Value::Null).await;
    answer(&s, "eth_getTransactionByHash", None, Value::Null).await;
    answer(
        &s,
        "eth_getBlockByNumber",
        Some(json!(["0x10", false])),
        json!({"number":"0x10","hash":hash(),"transactions":[hash()]}),
    )
    .await;
    let result = finality::evm(
        &client(),
        Url::parse(&s.uri()).unwrap(),
        NetworkId::Ethereum,
        &[(hash(), 16)],
    )
    .await
    .unwrap();
    assert_eq!(result.results[0].1, Validation::Unavailable);
}
#[tokio::test]
async fn authoritative_canonical_block_membership_proves_a_dropped_confirmation() {
    let s = MockServer::start().await;
    evm_header(&s).await;
    answer(&s, "eth_getTransactionReceipt", None, Value::Null).await;
    answer(&s, "eth_getTransactionByHash", None, Value::Null).await;
    answer(
        &s,
        "eth_getBlockByNumber",
        Some(json!(["0x10", false])),
        json!({"number":"0x10","hash":hash(),"transactions":[]}),
    )
    .await;
    let result = finality::evm(
        &client(),
        Url::parse(&s.uri()).unwrap(),
        NetworkId::Ethereum,
        &[(hash(), 16)],
    )
    .await
    .unwrap();
    assert_eq!(result.results[0].1, Validation::Reorged);
}
#[tokio::test]
async fn wrong_network_and_malformed_membership_never_produce_a_reorg() {
    let s = MockServer::start().await;
    answer(&s, "eth_chainId", None, json!("0x89")).await;
    assert!(
        finality::evm(
            &client(),
            Url::parse(&s.uri()).unwrap(),
            NetworkId::Ethereum,
            &[(hash(), 16)]
        )
        .await
        .is_err()
    );
    let s = MockServer::start().await;
    evm_header(&s).await;
    answer(&s, "eth_getTransactionReceipt", None, Value::Null).await;
    answer(&s, "eth_getTransactionByHash", None, Value::Null).await;
    answer(
        &s,
        "eth_getBlockByNumber",
        Some(json!(["0x10", false])),
        json!({"number":"0x10","hash":hash(),"transactions":[42]}),
    )
    .await;
    assert!(
        finality::evm(
            &client(),
            Url::parse(&s.uri()).unwrap(),
            NetworkId::Ethereum,
            &[(hash(), 16)]
        )
        .await
        .is_err()
    );
}
#[tokio::test]
async fn solana_finalized_status_and_missing_index_have_distinct_meanings() {
    for indexed in [true, false] {
        let s = MockServer::start().await;
        answer(&s, "getSlot", None, json!(100)).await;
        answer(&s,"getSignatureStatuses",None,json!({"value":[if indexed {json!({"slot":10,"confirmationStatus":"finalized","err":null})} else {Value::Null}]})).await;
        if !indexed {
            answer(&s, "getBlock", None, json!({"signatures":["signature"]})).await;
        }
        let result = finality::solana(
            &client(),
            Url::parse(&s.uri()).unwrap(),
            &[("signature".into(), 10)],
        )
        .await
        .unwrap();
        assert_eq!(
            result.results[0].1,
            if indexed {
                Validation::Final
            } else {
                Validation::Unavailable
            }
        );
    }
}

#[tokio::test]
async fn ton_masterchain_anchor_and_complete_membership_distinguish_finality_from_missing_data() {
    use portfolio_providers::tonapi::TonApi;
    use wiremock::matchers::path;
    for (included, quantity) in [(true, 1), (false, 0), (false, 1)] {
        let s = MockServer::start().await;
        let hash = "ab".repeat(32);
        let id = "(0,8000000000000000,16)";
        let header = json!({"global_id":-239,"workchain_id":0,"shard":"8000000000000000","seqno":16,"tx_quantity":quantity});
        for (route, body) in [
            (
                "/blockchain/masterchain-head".to_string(),
                json!({"global_id":-239,"workchain_id":-1,"seqno":100}),
            ),
            (
                "/blockchain/masterchain/100/shards".to_string(),
                json!({"shards":[{"last_known_block":header}]}),
            ),
            (
                format!("/blockchain/transactions/{hash}"),
                json!({"hash":hash,"block":id}),
            ),
            (format!("/blockchain/blocks/{id}"), header),
            (
                format!("/blockchain/blocks/{id}/transactions"),
                json!({"transactions":if included {vec![json!({"hash":hash})]} else {vec![]}}),
            ),
        ] {
            Mock::given(method("GET"))
                .and(path(route))
                .respond_with(ResponseTemplate::new(200).set_body_json(body))
                .mount(&s)
                .await;
        }
        let api =
            TonApi::with_config(&format!("{}/", s.uri()), "", Budget::limited(10), fast()).unwrap();
        let result = api.validate_finality(&[(hash, 0)]).await.unwrap();
        assert_eq!(
            result.results[0].1,
            if included {
                Validation::Final
            } else if quantity == 0 {
                Validation::Reorged
            } else {
                Validation::Unavailable
            }
        );
    }
}
