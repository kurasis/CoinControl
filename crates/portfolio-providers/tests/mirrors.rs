//! Failure recovery with independently bounded sources, exact balances and retained evidence.
mod common;
use common::{NOW, SENTINEL_KEY, fast, store};
use num_bigint::BigInt;
use portfolio_core::network::NetworkId;
use portfolio_providers::{
    ProviderError, Providers, SyncEngine, SyncOptions,
    alchemy::Alchemy,
    esplora::Esplora,
    http::Budget,
    mirrors::{Kind, Reserve},
};
use portfolio_store::{
    BalanceStatus, Coverage, Scope,
    ingest::{AssetSpec, Verification},
};
use reqwest::header::HeaderMap;
use serde_json::{Value, json};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_partial_json, method, path, query_param},
};
const ETH: &str = "0x1db3439a222c519ab44bb1144fc28167b4fa6ee6";
const TOKEN: &str = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";
const BTC: &str = "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa";
const TON: &str = "0:0000000000000000000000000000000000000000000000000000000000000000";
fn reserve(
    server: &MockServer,
    p: &'static str,
    kind: Kind,
    n: NetworkId,
    b: std::sync::Arc<Budget>,
) -> Reserve {
    Reserve::with_config(
        p,
        kind,
        &[(n, format!("{}/", server.uri()))],
        b,
        HeaderMap::new(),
        fast(),
    )
    .unwrap()
}
async fn rpc(server: &MockServer, m: &str, result: Value) {
    Mock::given(method("POST"))
        .and(body_partial_json(json!({"method":m})))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"jsonrpc":"2.0","id":1,"result":result})),
        )
        .mount(server)
        .await;
}
async fn evm(server: &MockServer) {
    rpc(server, "eth_chainId", json!("0x1")).await;
    rpc(server, "eth_blockNumber", json!("0xabc")).await;
    rpc(server, "eth_getBalance", json!("0x20000000000001")).await;
}
fn alchemy(server: &MockServer, b: std::sync::Arc<Budget>) -> Alchemy {
    Alchemy::with_config(
        &[(NetworkId::Ethereum, server.uri())],
        SENTINEL_KEY,
        b,
        fast(),
    )
    .unwrap()
}
fn token() -> AssetSpec {
    AssetSpec {
        network: NetworkId::Ethereum,
        contract: Some(TOKEN.into()),
        decimals: 6,
        symbol: Some("USDC".into()),
        name: None,
        verification: Verification::Verified,
        provider: "zerion",
    }
}
#[tokio::test]
async fn failover_429_403_500_and_timeout_keeps_unknown_tokens_and_persists_breaker() {
    for code in [429, 403, 500, 408] {
        let primary = MockServer::start().await;
        let mut response = ResponseTemplate::new(if code == 408 { 200 } else { code });
        if code == 429 {
            response = response.insert_header("retry-after", "120");
        }
        if code == 408 {
            response = response.set_delay(std::time::Duration::from_millis(200));
        }
        Mock::given(method("POST"))
            .respond_with(response)
            .mount(&primary)
            .await;
        let backup = MockServer::start().await;
        evm(&backup).await;
        let s = store().await;
        let w = s.create_wallet("test").await.unwrap();
        let a = s
            .add_account(&w.id, NetworkId::Ethereum, ETH, None)
            .await
            .unwrap();
        use portfolio_store::ingest::{
            Decoding, Direction, FeeAttribution, FeeSpec, LegSpec, TxSpec, TxStatus,
        };
        let historical = TxSpec {
            network: NetworkId::Ethereum,
            hash: format!("0x{}", "a".repeat(64)),
            part: None,
            block_height: Some(1),
            position: None,
            occurred_at: NOW - 100,
            status: TxStatus::Confirmed,
            provider: "zerion",
            operation: "receive".into(),
            legs: vec![LegSpec {
                counterparty: None,
                asset: AssetSpec::native(NetworkId::Ethereum, "zerion"),
                signed_raw: BigInt::from(9007199254740993_u64),
                direction: Direction::In,
                leg_type: "receive".into(),
                decoding: Decoding::Interpreted,
                unresolved: false,
            }],
            fee: Some(FeeSpec {
                asset: AssetSpec::native(NetworkId::Ethereum, "zerion"),
                raw: BigInt::from(21000),
                attribution: FeeAttribution::Exact,
            }),
            decoding: Decoding::Interpreted,
            evidence: json!({"source":"controlled richer history"}),
        };
        s.ingest_transaction(&a.id, &historical).await.unwrap();
        let before = s
            .list_activity(&Scope::All, &Default::default(), None, 50)
            .await
            .unwrap()
            .rows;
        // Put a previously discovered token outside the reserve's bounded known-token lookup.
        for i in 1..=21 {
            let mut t = token();
            t.contract = Some(format!("0x{i:040x}"));
            s.record_balance(&a.id, &t, &BigInt::from(99), None, "fresh")
                .await
                .unwrap();
        }
        rpc(&backup, "eth_call", json!("0x2")).await;
        let mut config = fast();
        config.timeout = std::time::Duration::from_millis(25);
        config.max_retries = 0;
        let make = || Providers {
            alchemy: Some(
                Alchemy::with_config(
                    &[(NetworkId::Ethereum, primary.uri())],
                    SENTINEL_KEY,
                    Budget::limited(10),
                    config.clone(),
                )
                .unwrap(),
            ),
            reserves: vec![reserve(
                &backup,
                "publicnode",
                Kind::Rpc,
                NetworkId::Ethereum,
                Budget::limited(50),
            )],
            ..Providers::default()
        };
        let first = SyncEngine::new(s.clone(), make(), SyncOptions::default())
            .sync_account(&a)
            .await;
        assert!(first.error.is_none(), "{code}: {:?}", first.error);
        assert_eq!(first.provider.as_deref(), Some("publicnode"));
        assert!(first.balance_only);
        assert_eq!(first.coverage, Coverage::Partial);
        assert!(!first.fallback_reasons.is_empty());
        let assets = s.list_holdings(&Scope::All).await.unwrap();
        assert_eq!(
            assets
                .iter()
                .filter(|a| a.balance_status == BalanceStatus::Stale)
                .count(),
            1
        );
        let old = assets
            .iter()
            .find(|a| a.balance_status == BalanceStatus::Stale)
            .unwrap();
        assert_eq!(old.quantity, "0.000099");
        let n = primary.received_requests().await.unwrap().len();
        let second = SyncEngine::new(s.clone(), make(), SyncOptions::default())
            .sync_account(&a)
            .await;
        assert!(second.error.is_none());
        assert_eq!(
            primary.received_requests().await.unwrap().len(),
            n,
            "cooldown survives new engine"
        );
        assert_eq!(
            s.list_activity(&Scope::All, &Default::default(), None, 50)
                .await
                .unwrap()
                .rows,
            before,
            "reserve cannot rewrite cached legs or fee"
        );
        let status = s.sync_status().await.unwrap();
        assert_eq!(status.len(), 1);
        assert_eq!(status[0].provider.as_deref(), Some("publicnode"));
        assert!(status[0].balance_only);
        assert_eq!(
            s.checkpoint(&a.id, "publicnode", "history")
                .await
                .unwrap()
                .backfill_cursor,
            None
        );
        assert_eq!(
            s.provider_cooldown("alchemy", NetworkId::Ethereum)
                .await
                .unwrap(),
            Some(
                NOW + if code == 429 {
                    120
                } else if code == 403 {
                    3600
                } else {
                    30
                }
            )
        );
        let manual = SyncEngine::new(s.clone(), make(), SyncOptions::default())
            .with_transient_retry(true)
            .sync_account(&a)
            .await;
        assert!(manual.error.is_none());
        let attempted = primary.received_requests().await.unwrap().len();
        assert_eq!(
            attempted > n,
            matches!(code, 500 | 408),
            "manual retry bypasses only transient errors, never Retry-After or denied access"
        );
        s.clear_provider_cooldown("alchemy").await.unwrap();
        assert_eq!(
            s.provider_cooldown("alchemy", NetworkId::Ethereum)
                .await
                .unwrap(),
            None
        );
    }
}
#[tokio::test]
async fn wrong_chain_and_cancel_do_not_use_a_reserve() {
    let server = MockServer::start().await;
    rpc(&server, "eth_chainId", json!("0xa")).await;
    let backup = MockServer::start().await;
    evm(&backup).await;
    let s = store().await;
    let w = s.create_wallet("test").await.unwrap();
    let a = s
        .add_account(&w.id, NetworkId::Ethereum, ETH, None)
        .await
        .unwrap();
    let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let e = SyncEngine::new(
        s.clone(),
        Providers {
            alchemy: Some(alchemy(&server, Budget::limited(10))),
            reserves: vec![reserve(
                &backup,
                "publicnode",
                Kind::Rpc,
                NetworkId::Ethereum,
                Budget::limited(10),
            )],
            ..Providers::default()
        },
        SyncOptions::default(),
    );
    let e = e.with_cancellation(cancelled.clone());
    let r = e.sync_account(&a).await;
    assert!(r.error.unwrap().contains("wrong mainnet"));
    assert!(backup.received_requests().await.unwrap().is_empty());
    cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
    assert_eq!(e.sync_account(&a).await.coverage, Coverage::Paused);
    assert!(backup.received_requests().await.unwrap().is_empty());
}
fn btc_tx(n: u32) -> Value {
    json!({"txid":format!("{n:064x}"),"vin":[],"vout":[{"scriptpubkey_address":BTC,"value":1000}],"fee":0,"status":{"confirmed":true,"block_height":800000+n,"block_time":1700000000+i64::from(n),"block_hash":"ff"}})
}
#[tokio::test]
async fn bitcoin_mirror_cursors_provenance_and_resume_are_independent() {
    let primary = MockServer::start().await;
    let backup = MockServer::start().await;
    Mock::given(path("/blocks/tip/height"))
        .respond_with(ResponseTemplate::new(200).set_body_string("900000"))
        .mount(&backup)
        .await;
    Mock::given(path(format!("/address/{BTC}"))).respond_with(ResponseTemplate::new(200).set_body_json(json!({"address":BTC,"chain_stats":{"funded_txo_sum":26000,"spent_txo_sum":0,"tx_count":26},"mempool_stats":{"funded_txo_sum":0,"spent_txo_sum":0,"tx_count":0}}))).mount(&backup).await;
    Mock::given(path(format!("/address/{BTC}/txs/chain")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json((2..=26).rev().map(btc_tx).collect::<Vec<_>>()),
        )
        .mount(&backup)
        .await;
    Mock::given(path(format!("/address/{BTC}/txs/chain/{:064x}", 2)))
        .respond_with(ResponseTemplate::new(200).set_body_json(vec![btc_tx(1)]))
        .mount(&backup)
        .await;
    Mock::given(path(format!("/address/{BTC}/txs/mempool")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&backup)
        .await;
    let s = store().await;
    let w = s.create_wallet("btc").await.unwrap();
    let a = s
        .add_account(&w.id, NetworkId::Bitcoin, BTC, None)
        .await
        .unwrap();
    for pages in [1, 2, 2] {
        let e = SyncEngine::new(
            s.clone(),
            Providers {
                esplora: Some(
                    Esplora::with_config(&primary.uri(), Budget::limited(0), fast()).unwrap(),
                ),
                mempool: Some(
                    Esplora::with_provider("mempool", &backup.uri(), Budget::limited(50), fast())
                        .unwrap(),
                ),
                ..Providers::default()
            },
            SyncOptions {
                max_history_pages: pages,
                ..SyncOptions::default()
            },
        );
        let r = e.sync_account(&a).await;
        assert!(r.error.is_none(), "{:?}", r.error);
        assert_eq!(r.provider.as_deref(), Some("mempool"));
        assert_eq!(
            s.checkpoint(&a.id, "esplora", "history")
                .await
                .unwrap()
                .backfill_cursor,
            None
        );
        if pages == 1 {
            assert!(
                s.checkpoint(&a.id, "mempool", "history")
                    .await
                    .unwrap()
                    .backfill_cursor
                    .is_some()
            );
        }
    }
    let status = s.sync_status().await.unwrap();
    assert_eq!(status[0].transaction_count, 26);
    assert_eq!(status[0].coverage, Some(Coverage::Complete));
    assert!(primary.received_requests().await.unwrap().is_empty());
}
#[tokio::test]
async fn blockscout_current_schema_keyset_and_credits_are_exact() {
    let server = MockServer::start().await;
    Mock::given(path(format!("/1/api/v2/addresses/{ETH}"))).respond_with(ResponseTemplate::new(200).set_body_json(json!({"hash":ETH,"coin_balance":"9007199254740993","block_number_balance_updated_at":123}))).mount(&server).await;
    let row = json!({"token":{"address_hash":TOKEN,"decimals":"6","symbol":"USDC","name":"USD Coin","type":"ERC-20"},"value":"9007199254740993"});
    Mock::given(path(format!("/1/api/v2/addresses/{ETH}/tokens")))
        .and(query_param("type", "ERC-20"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"items":[row],"next_page_params":{"id":3,"value":"0","fiat_value":null}}),
        ))
        .with_priority(2)
        .mount(&server)
        .await;
    Mock::given(path(format!("/1/api/v2/addresses/{ETH}/tokens")))
        .and(query_param("id", "3"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"items":[],"next_page_params":null})),
        )
        .with_priority(1)
        .mount(&server)
        .await;
    let b = Budget::limited_with_credits(10, 60);
    let api = reserve(
        &server,
        "blockscout",
        Kind::Blockscout,
        NetworkId::Ethereum,
        b.clone(),
    );
    let h = api.snapshot(NetworkId::Ethereum, ETH, &[]).await.unwrap();
    assert_eq!(h.assets.len(), 2);
    assert_eq!(h.assets[1].1.to_string(), "9007199254740993");
    assert_eq!(b.credits(), 60);
    assert_eq!(b.used(), 3);
    assert!(matches!(
        api.snapshot(NetworkId::Ethereum, ETH, &[]).await,
        Err(ProviderError::BudgetExhausted { .. })
    ));
    assert!(
        !Reserve::new("blockscout", SENTINEL_KEY, Budget::limited(0))
            .unwrap()
            .supports(NetworkId::Bsc)
    );
}
#[tokio::test]
async fn etherscan_selected_free_chains_semantic_rate_limits_and_secret_redaction() {
    let server = MockServer::start().await;
    Mock::given(query_param("action","balance")).respond_with(ResponseTemplate::new(200).set_body_json(json!({"status":"0","message":"NOTOK","result":format!("Max rate limit reached {SENTINEL_KEY}")}))).mount(&server).await;
    let api = reserve(
        &server,
        "etherscan",
        Kind::Etherscan,
        NetworkId::Ethereum,
        Budget::limited(5),
    );
    let e = api
        .snapshot(NetworkId::Ethereum, ETH, &[])
        .await
        .err()
        .unwrap();
    assert!(matches!(e, ProviderError::RateLimited { .. }));
    assert!(!e.to_string().contains(SENTINEL_KEY));
    let api = Reserve::new("etherscan", SENTINEL_KEY, Budget::limited(0)).unwrap();
    assert!(api.supports(NetworkId::Arbitrum));
    assert!(!api.supports(NetworkId::Base));
    assert!(!api.supports(NetworkId::Bsc));
}
#[tokio::test]
async fn toncenter_discovers_only_exact_metadata_never_uses_transaction_hashes() {
    let server = MockServer::start().await;
    let master = format!("0:{}", "a".repeat(64));
    Mock::given(path("/accountStates"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"accounts":[{"address":TON,"balance":"9007199254740993"}]})),
        )
        .mount(&server)
        .await;
    Mock::given(path("/jetton/wallets")).respond_with(ResponseTemplate::new(200).set_body_json(json!({"jetton_wallets":[{"owner":TON,"jetton":master,"balance":"9007199254740993"}],"metadata":{master.clone():{"token_info":[{"type":"jetton_masters","valid":true,"extra":{"decimals":"9"},"symbol":"T"}]}}}))).mount(&server).await;
    let api = reserve(
        &server,
        "toncenter",
        Kind::TonCenter,
        NetworkId::Ton,
        Budget::limited(10),
    );
    let h = api.snapshot(NetworkId::Ton, TON, &[]).await.unwrap();
    assert_eq!(h.assets.len(), 2);
    assert_eq!(h.assets[1].0.decimals, 9);
    assert_eq!(h.assets[1].1.to_string(), "9007199254740993");
    let s = store().await;
    let w = s.create_wallet("ton").await.unwrap();
    let a = s
        .add_account(&w.id, NetworkId::Ton, TON, None)
        .await
        .unwrap();
    let r = SyncEngine::new(
        s.clone(),
        Providers {
            reserves: vec![api],
            ..Providers::default()
        },
        SyncOptions::default(),
    )
    .sync_account(&a)
    .await;
    assert!(r.error.is_none());
    assert!(r.balance_only);
    assert_eq!(s.sync_status().await.unwrap()[0].transaction_count, 0);
}
#[tokio::test]
async fn solana_standard_rpc_checks_mainnet_and_aggregates_owned_token_accounts() {
    let server = MockServer::start().await;
    let address = "Vote111111111111111111111111111111111111111";
    rpc(
        &server,
        "getGenesisHash",
        json!("5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d"),
    )
    .await;
    rpc(
        &server,
        "getBalance",
        json!({"context":{"slot":5},"value":9007199254740993_u64}),
    )
    .await;
    rpc(&server, "getTokenAccountsByOwner", json!({"value":[]})).await;
    for provider in ["chainstack", "publicnode", "alchemy"] {
        let api = reserve(
            &server,
            provider,
            Kind::Rpc,
            NetworkId::Solana,
            Budget::limited(10),
        );
        let h = api.snapshot(NetworkId::Solana, address, &[]).await.unwrap();
        assert_eq!(h.assets[0].1.to_string(), "9007199254740993");
        assert_eq!(h.assets[0].0.provider, provider);
        assert_eq!(h.warnings.is_empty(), provider != "chainstack");
    }
    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.body_json::<Value>().unwrap()["method"] == "getTokenAccountsByOwner")
            .count(),
        4,
        "only PublicNode/Alchemy scan SPL and Token-2022 owners; free Chainstack never calls a paid method"
    );
    let e = Reserve::new(
        "chainstack",
        "cp_platform-management-key",
        Budget::limited(0),
    )
    .err()
    .unwrap();
    assert!(e.to_string().contains("node auth token"));
    assert!(
        Reserve::new(
            "chainstack",
            "https://attacker.example/key",
            Budget::limited(0)
        )
        .is_err()
    );
}
#[tokio::test]
async fn tron_publicnode_includes_owned_stake_and_does_not_infer_missing_accounts() {
    let server = MockServer::start().await;
    let address = "TT2T17KZhoDu47i2E4FWxfG79zdkEWkU9N";
    Mock::given(path("/walletsolidity/getaccount")).respond_with(ResponseTemplate::new(200).set_body_json(json!({"address":address,"balance":10,"frozenV2":[{"amount":20}],"unfrozenV2":[{"unfreeze_amount":30}],"delegated_frozenV2_balance_for_bandwidth":40,"account_resource":{"delegated_frozenV2_balance_for_energy":50}}))).mount(&server).await;
    let api = reserve(
        &server,
        "publicnode",
        Kind::Rpc,
        NetworkId::Tron,
        Budget::limited(10),
    );
    let h = api.snapshot(NetworkId::Tron, address, &[]).await.unwrap();
    assert_eq!(h.assets[0].1.to_string(), "150");
    server.reset().await;
    Mock::given(path("/walletsolidity/getaccount"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&server)
        .await;
    assert!(api.snapshot(NetworkId::Tron, address, &[]).await.is_err());
}

#[tokio::test]
async fn forbidden_solana_token_method_preserves_native_and_other_network_access() {
    let server = MockServer::start().await;
    rpc(
        &server,
        "getGenesisHash",
        json!("5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d"),
    )
    .await;
    rpc(
        &server,
        "getBalance",
        json!({"context":{"slot":5},"value":100}),
    )
    .await;
    Mock::given(body_partial_json(
        json!({"method":"getTokenAccountsByOwner"}),
    ))
    .respond_with(ResponseTemplate::new(403))
    .mount(&server)
    .await;
    evm(&server).await;
    let b = Budget::limited(20);
    let api = Reserve::with_config(
        "publicnode",
        Kind::Rpc,
        &[
            (NetworkId::Solana, server.uri()),
            (NetworkId::Ethereum, server.uri()),
        ],
        b.clone(),
        HeaderMap::new(),
        fast(),
    )
    .unwrap();
    let sol = api
        .snapshot(
            NetworkId::Solana,
            "Vote111111111111111111111111111111111111111",
            &[],
        )
        .await
        .unwrap();
    assert_eq!(sol.assets[0].1.to_string(), "100");
    assert!(!sol.warnings.is_empty());
    let eth = api.snapshot(NetworkId::Ethereum, ETH, &[]).await.unwrap();
    assert_eq!(eth.assets[0].1.to_string(), "9007199254740993");
    assert_eq!(b.used(), 6);
}
#[tokio::test]
async fn local_storage_failure_does_not_issue_backup_requests_and_monthly_counts_share_networks() {
    let s = store().await;
    let w = s.create_wallet("db").await.unwrap();
    let a = s
        .add_account(&w.id, NetworkId::Ethereum, ETH, None)
        .await
        .unwrap();
    s.add_provider_usage_cost("drpc", "2026-10-01", 12, 0, None)
        .await
        .unwrap();
    s.add_provider_usage_cost("drpc", "2026-10-02", 8, 0, None)
        .await
        .unwrap();
    s.add_provider_usage_cost("drpc", "2026-09-30", 99, 0, None)
        .await
        .unwrap();
    assert_eq!(
        s.provider_usage_month("2026-10", "drpc").await.unwrap(),
        (20, 0)
    );
    let server = MockServer::start().await;
    evm(&server).await;
    s.close().await;
    let engine = SyncEngine::new(
        s,
        Providers {
            reserves: vec![reserve(
                &server,
                "publicnode",
                Kind::Rpc,
                NetworkId::Ethereum,
                Budget::limited(10),
            )],
            ..Providers::default()
        },
        SyncOptions::default(),
    );
    let report = engine.sync_account(&a).await;
    assert!(report.error.is_some());
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn paid_plan_402_advances_to_available_balance_reserve() {
    let gated = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(402)
                .set_body_json(json!({"message":format!("plan unavailable {SENTINEL_KEY}")})),
        )
        .mount(&gated)
        .await;
    let available = MockServer::start().await;
    evm(&available).await;
    let s = store().await;
    let w = s.create_wallet("plan").await.unwrap();
    let a = s
        .add_account(&w.id, NetworkId::Ethereum, ETH, None)
        .await
        .unwrap();
    let engine = SyncEngine::new(
        s,
        Providers {
            reserves: vec![
                reserve(
                    &gated,
                    "blockscout",
                    Kind::Blockscout,
                    NetworkId::Ethereum,
                    Budget::limited(10),
                ),
                reserve(
                    &available,
                    "publicnode",
                    Kind::Rpc,
                    NetworkId::Ethereum,
                    Budget::limited(10),
                ),
            ],
            ..Providers::default()
        },
        SyncOptions::default(),
    );
    let r = engine.sync_account(&a).await;
    assert!(r.error.is_none());
    assert_eq!(r.provider.as_deref(), Some("publicnode"));
    assert!(
        r.fallback_reasons
            .iter()
            .any(|e| e.contains("capability unavailable"))
    );
    assert!(!r.fallback_reasons.join(" ").contains(SENTINEL_KEY));
    let api = Reserve::new("blockscout", SENTINEL_KEY, Budget::limited(0)).unwrap();
    assert!(!api.supports(NetworkId::Base));
    assert!(!api.supports(NetworkId::Polygon));
}
#[test]
fn drpc_free_scope_excludes_premium_solana() {
    let api = Reserve::new("drpc", SENTINEL_KEY, Budget::limited(0)).unwrap();
    for n in [
        NetworkId::Ethereum,
        NetworkId::Base,
        NetworkId::Arbitrum,
        NetworkId::Optimism,
        NetworkId::Polygon,
        NetworkId::Bsc,
    ] {
        assert!(api.supports(n));
    }
    assert!(!api.supports(NetworkId::Solana));
}

#[tokio::test]
async fn recovered_primary_has_one_healthy_status_while_reserve_keeps_its_pause() {
    let primary = MockServer::start().await;
    let backup = MockServer::start().await;
    for server in [&primary, &backup] {
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(500))
            .mount(server)
            .await;
    }
    let s = store().await;
    let w = s.create_wallet("reconnect").await.unwrap();
    let a = s
        .add_account(&w.id, NetworkId::Bitcoin, BTC, None)
        .await
        .unwrap();
    let make = || {
        SyncEngine::new(
            s.clone(),
            Providers {
                esplora: Some(
                    Esplora::with_config(&primary.uri(), Budget::limited(50), fast()).unwrap(),
                ),
                mempool: Some(
                    Esplora::with_provider("mempool", &backup.uri(), Budget::limited(50), fast())
                        .unwrap(),
                ),
                ..Providers::default()
            },
            SyncOptions::default(),
        )
        .with_transient_retry(true)
    };
    assert!(make().sync_account(&a).await.error.is_some());
    let reserve_error = s
        .checkpoint(&a.id, "mempool", "history")
        .await
        .unwrap()
        .state
        .last_error;
    assert!(reserve_error.is_some());
    let attempts = backup.received_requests().await.unwrap().len();
    primary.reset().await;
    Mock::given(path("/blocks/tip/height"))
        .respond_with(ResponseTemplate::new(200).set_body_string("900000"))
        .mount(&primary)
        .await;
    Mock::given(path(format!("/address/{BTC}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"address":BTC,"chain_stats":{"funded_txo_sum":1000,"spent_txo_sum":0,"tx_count":1},"mempool_stats":{"funded_txo_sum":0,"spent_txo_sum":0,"tx_count":0}})))
        .mount(&primary).await;
    Mock::given(path(format!("/address/{BTC}/txs/chain")))
        .respond_with(ResponseTemplate::new(200).set_body_json(vec![btc_tx(1)]))
        .mount(&primary)
        .await;
    Mock::given(path(format!("/address/{BTC}/txs/mempool")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&primary)
        .await;
    let r = make().sync_account(&a).await;
    assert!(r.error.is_none(), "{:?}", r.error);
    let status = s.sync_status().await.unwrap();
    assert_eq!(status.len(), 1);
    assert_eq!(status[0].provider.as_deref(), Some("esplora"));
    assert_eq!(status[0].last_error, None);
    assert_eq!(status[0].last_success_at, Some(NOW));
    assert_eq!(status[0].transaction_count, 1);
    assert_eq!(
        s.list_holdings(&Scope::All).await.unwrap()[0].balance_status,
        BalanceStatus::Fresh
    );
    assert_eq!(backup.received_requests().await.unwrap().len(), attempts);
    assert_eq!(
        s.checkpoint(&a.id, "mempool", "history")
            .await
            .unwrap()
            .state
            .last_error,
        reserve_error
    );
    assert!(
        s.provider_cooldown("mempool", NetworkId::Bitcoin)
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn free_chainstack_uses_token_mirror_and_keeps_sol_when_that_mirror_is_limited() {
    use portfolio_providers::helius::{TOKEN_2022, TOKEN_PROGRAM};
    const ADDRESS: &str = "Vote111111111111111111111111111111111111111";
    const MINT: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
    for limited in [false, true] {
        let primary = MockServer::start().await;
        rpc(
            &primary,
            "getGenesisHash",
            json!("5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d"),
        )
        .await;
        rpc(
            &primary,
            "getBalance",
            json!({"context":{"slot":50},"value":9007199254740993_u64}),
        )
        .await;
        let mirror = MockServer::start().await;
        if limited {
            Mock::given(method("POST"))
                .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "120"))
                .expect(1)
                .mount(&mirror)
                .await;
        } else {
            rpc(
                &mirror,
                "getGenesisHash",
                json!("5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d"),
            )
            .await;
            rpc(
                &mirror,
                "getBalance",
                json!({"context":{"slot":51},"value":9007199254740993_u64}),
            )
            .await;
            for (program, value) in [
                (
                    TOKEN_PROGRAM,
                    json!([{"account":{"data":{"parsed":{"info":{"owner":ADDRESS,"mint":MINT,"tokenAmount":{"amount":"7","decimals":6}}}}}}]),
                ),
                (TOKEN_2022, json!([])),
            ] {
                Mock::given(method("POST"))
                    .and(body_partial_json(json!({"method":"getTokenAccountsByOwner","params":[ADDRESS,{"programId":program},{"encoding":"jsonParsed","commitment":"finalized","minContextSlot":51}]})))
                    .respond_with(ResponseTemplate::new(200).set_body_json(json!({"jsonrpc":"2.0","id":1,"result":{"value":value}})))
                    .expect(1).mount(&mirror).await;
            }
        }
        let s = store().await;
        let w = s.create_wallet("Free Solana reserves").await.unwrap();
        let a = s
            .add_account(&w.id, NetworkId::Solana, ADDRESS, None)
            .await
            .unwrap();
        let token = AssetSpec {
            network: NetworkId::Solana,
            contract: Some(MINT.into()),
            decimals: 6,
            symbol: Some("USDC".into()),
            name: None,
            verification: Verification::Unverified,
            provider: "helius",
        };
        s.record_balance(&a.id, &token, &BigInt::from(9), None, "fresh")
            .await
            .unwrap();
        let r = SyncEngine::new(
            s.clone(),
            Providers {
                reserves: vec![
                    reserve(
                        &primary,
                        "chainstack",
                        Kind::Rpc,
                        NetworkId::Solana,
                        Budget::limited(10),
                    ),
                    reserve(
                        &mirror,
                        "publicnode",
                        Kind::Rpc,
                        NetworkId::Solana,
                        Budget::limited(10),
                    ),
                ],
                ..Providers::default()
            },
            SyncOptions::default(),
        )
        .sync_account(&a)
        .await;
        assert!(r.error.is_none(), "{:?}", r.error);
        assert!(r.balance_refreshed && r.balance_only);
        assert_eq!(r.coverage, Coverage::Partial);
        let provider = if limited { "chainstack" } else { "publicnode" };
        assert_eq!(r.provider.as_deref(), Some(provider));
        let status = s.sync_status().await.unwrap();
        assert_eq!(status[0].provider.as_deref(), Some(provider));
        assert!(status[0].last_error.is_none());
        let holdings = s.list_holdings(&Scope::All).await.unwrap();
        let usdc = holdings
            .iter()
            .find(|h| h.asset_id == format!("solana:token:{MINT}"))
            .unwrap();
        assert_eq!(usdc.quantity, if limited { "0.000009" } else { "0.000007" });
        assert_eq!(
            usdc.balance_status,
            if limited {
                BalanceStatus::Stale
            } else {
                BalanceStatus::Fresh
            }
        );
        assert!(
            r.fallback_reasons
                .iter()
                .any(|s| s.contains("free Solana plan"))
        );
        assert!(
            primary
                .received_requests()
                .await
                .unwrap()
                .iter()
                .all(|r| r.body_json::<Value>().unwrap()["method"] != "getTokenAccountsByOwner")
        );
        if limited {
            assert!(
                r.fallback_reasons
                    .iter()
                    .any(|s| s.contains("rate limited"))
            );
            assert_eq!(
                s.provider_cooldown("publicnode", NetworkId::Solana)
                    .await
                    .unwrap(),
                Some(NOW + 120)
            );
        }
    }
}

#[tokio::test]
async fn usable_sol_does_not_hide_a_wrong_mainnet_token_mirror() {
    const ADDRESS: &str = "Vote111111111111111111111111111111111111111";
    let primary = MockServer::start().await;
    rpc(
        &primary,
        "getGenesisHash",
        json!("5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d"),
    )
    .await;
    rpc(
        &primary,
        "getBalance",
        json!({"context":{"slot":5},"value":9}),
    )
    .await;
    let wrong = MockServer::start().await;
    rpc(&wrong, "getGenesisHash", json!("devnet-not-mainnet")).await;
    let s = store().await;
    let w = s.create_wallet("Wrong mainnet reserve").await.unwrap();
    let a = s
        .add_account(&w.id, NetworkId::Solana, ADDRESS, None)
        .await
        .unwrap();
    let r = SyncEngine::new(
        s,
        Providers {
            reserves: vec![
                reserve(
                    &primary,
                    "chainstack",
                    Kind::Rpc,
                    NetworkId::Solana,
                    Budget::limited(10),
                ),
                reserve(
                    &wrong,
                    "publicnode",
                    Kind::Rpc,
                    NetworkId::Solana,
                    Budget::limited(10),
                ),
            ],
            ..Providers::default()
        },
        SyncOptions::default(),
    )
    .sync_account(&a)
    .await;
    assert!(r.balance_refreshed);
    assert!(
        r.error
            .as_deref()
            .is_some_and(|s| s.contains("wrong Solana mainnet"))
    );
    assert_eq!(wrong.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn alchemy_solana_budget_is_shared_with_evm_and_denial_stays_network_scoped() {
    const ADDRESS: &str = "Vote111111111111111111111111111111111111111";
    let solana = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(403))
        .expect(1)
        .mount(&solana)
        .await;
    let evm_server = MockServer::start().await;
    rpc(&evm_server, "eth_chainId", json!("0x1")).await;
    let budget = Budget::limited_with_credits(50, 1000);
    let sol = reserve(
        &solana,
        "alchemy",
        Kind::Rpc,
        NetworkId::Solana,
        budget.clone(),
    );
    assert!(matches!(
        sol.snapshot(NetworkId::Solana, ADDRESS, &[])
            .await
            .err()
            .unwrap(),
        ProviderError::NetworkForbidden { .. }
    ));
    let eth = alchemy(&evm_server, budget.clone());
    eth.check_chain(NetworkId::Ethereum).await.unwrap();
    assert_eq!(
        budget.used(),
        2,
        "one failed Solana and one successful EVM request"
    );
    assert_eq!(
        budget.credits(),
        100 + portfolio_providers::alchemy::estimated_cost("eth_chainId")
    );
    let before = budget.used();
    assert!(!sol.validates_finality(NetworkId::Solana));
    assert!(sol.validate_finality(NetworkId::Solana, &[]).await.is_err());
    assert_eq!(
        budget.used(),
        before,
        "balance-only reserve must not issue unbudgeted finality reads"
    );
}

#[tokio::test]
async fn all_throttled_sources_pause_without_erasing_cached_balances() {
    let primary = MockServer::start().await;
    let backup = MockServer::start().await;
    for server in [&primary, &backup] {
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "120"))
            .expect(1)
            .mount(server)
            .await;
    }
    let s = store().await;
    let w = s.create_wallet("All sources paused").await.unwrap();
    let a = s
        .add_account(&w.id, NetworkId::Ethereum, ETH, None)
        .await
        .unwrap();
    s.record_balance(
        &a.id,
        &AssetSpec::native(NetworkId::Ethereum, "alchemy"),
        &BigInt::from(9007199254740993_u64),
        None,
        "fresh",
    )
    .await
    .unwrap();
    let engine = SyncEngine::new(
        s.clone(),
        Providers {
            alchemy: Some(alchemy(&primary, Budget::limited(10))),
            reserves: vec![reserve(
                &backup,
                "publicnode",
                Kind::Rpc,
                NetworkId::Ethereum,
                Budget::limited(10),
            )],
            ..Providers::default()
        },
        SyncOptions::default(),
    );
    let r = engine.sync_account(&a).await;
    assert!(r.error.is_none());
    assert!(!r.balance_refreshed);
    assert_eq!(r.coverage, Coverage::Paused);
    let h = s.list_holdings(&Scope::All).await.unwrap();
    assert_eq!(h[0].quantity, "0.009007199254740993");
    assert_eq!(h[0].balance_status, BalanceStatus::Stale);
    assert!(s.sync_status().await.unwrap()[0].last_error.is_none());
    let later = SyncEngine::new(
        s.clone(),
        Providers {
            alchemy: Some(alchemy(&primary, Budget::limited(10))),
            reserves: vec![reserve(
                &backup,
                "publicnode",
                Kind::Rpc,
                NetworkId::Ethereum,
                Budget::limited(10),
            )],
            ..Providers::default()
        },
        SyncOptions::default(),
    )
    .with_transient_retry(true)
    .sync_account(&a)
    .await;
    assert!(later.error.is_none());
    assert_eq!(later.coverage, Coverage::Paused);
    assert!(
        !later.balance_refreshed,
        "new client/run must respect both stored pauses"
    );
}
