use std::collections::BTreeSet;
use std::sync::Arc;

use portfolio_core::clock::FixedClock;
use portfolio_core::decimal::parse_dec;
use portfolio_core::network::NetworkId;
use portfolio_store::*;

const NOW: i64 = 1_790_000_000;

fn clock() -> Arc<FixedClock> {
    Arc::new(FixedClock(NOW))
}

async fn mem(profile: ProfileKind) -> Store {
    Store::open_in_memory(profile, clock()).await.unwrap()
}

#[tokio::test]
async fn accounts_are_unique_per_network_and_canonical_address() {
    let store = mem(ProfileKind::Test).await;
    let w = store.create_wallet("Main").await.unwrap();
    let a = store
        .add_account(
            &w.id,
            NetworkId::Ethereum,
            "0x5aaeb6053f3e94c9b9a09f33669435e7ef1beaed",
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        a.display_address,
        "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed"
    );

    // Same address, different casing: rejected as a duplicate with a pointer to the existing account.
    let dup = store
        .add_account(
            &w.id,
            NetworkId::Ethereum,
            "0x5AAEB6053F3E94C9B9A09F33669435E7EF1BEAED",
            None,
        )
        .await
        .unwrap_err();
    assert!(matches!(dup, StoreError::AccountExists { ref account_id, .. } if *account_id == a.id));

    // The same EVM address on another network is a different account.
    store
        .add_account(
            &w.id,
            NetworkId::Base,
            "0x5aaeb6053f3e94c9b9a09f33669435e7ef1beaed",
            None,
        )
        .await
        .unwrap();
    assert!(
        store
            .add_account(&w.id, NetworkId::Bitcoin, "not-an-address", None)
            .await
            .is_err()
    );
    assert_eq!(store.list_accounts(Some(&w.id)).await.unwrap().len(), 2);
}

#[tokio::test]
async fn overlapping_groups_resolve_to_a_deduplicated_union() {
    let store = mem(ProfileKind::Test).await;
    let cold = store.create_wallet("Cold").await.unwrap();
    let hot = store.create_wallet("Hot").await.unwrap();
    let a = store
        .add_account(
            &cold.id,
            NetworkId::Bitcoin,
            "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa",
            None,
        )
        .await
        .unwrap();
    let b = store
        .add_account(
            &hot.id,
            NetworkId::Solana,
            "11111111111111111111111111111112",
            None,
        )
        .await
        .unwrap();
    let g1 = store.create_group("G1").await.unwrap();
    let g2 = store.create_group("G2").await.unwrap();
    store
        .set_group_wallets(&g1.id, &[cold.id.clone(), hot.id.clone()])
        .await
        .unwrap();
    store
        .set_group_wallets(&g2.id, &[hot.id.clone(), hot.id.clone()])
        .await
        .unwrap();

    let all = store.resolve_scope(&Scope::All).await.unwrap();
    assert_eq!(all, BTreeSet::from([a.id.clone(), b.id.clone()]));
    let groups = store.list_groups().await.unwrap();
    let g2_loaded = groups.iter().find(|g| g.id == g2.id).unwrap();
    assert_eq!(g2_loaded.wallet_ids, vec![hot.id.clone()]);

    store.set_account_archived(&a.id, true).await.unwrap();
    assert_eq!(
        store
            .resolve_scope(&Scope::Group { id: g1.id.clone() })
            .await
            .unwrap(),
        BTreeSet::from([b.id.clone()])
    );

    // Deleting a group never deletes wallets or accounts.
    store.delete_group(&g1.id).await.unwrap();
    assert_eq!(store.list_wallets().await.unwrap().len(), 2);
    assert_eq!(store.list_accounts(None).await.unwrap().len(), 2);
}

#[tokio::test]
async fn empty_portfolio_reports_value_unavailable_not_zero() {
    let store = mem(ProfileKind::Real).await;
    let summary = store.portfolio_summary(&Scope::All).await.unwrap();
    assert_eq!(summary.total_value_usd, None);
    assert_eq!(summary.holding_count, 0);
    let chart = store
        .get_chart(&Scope::All, ChartRange::Month)
        .await
        .unwrap();
    assert!(chart.points.iter().all(|p| p.value_usd.is_none()));
    assert_eq!(chart.history_available_since, None);
}

#[tokio::test]
async fn demo_data_is_refused_outside_the_demo_profile() {
    for profile in [ProfileKind::Real, ProfileKind::Test] {
        assert!(mem(profile).await.seed_demo().await.is_err());
    }
}

#[tokio::test]
async fn demo_profile_exercises_required_states() {
    let store = mem(ProfileKind::Demo).await;
    store.seed_demo().await.unwrap();
    store.seed_demo().await.unwrap(); // idempotent

    let holdings = store.list_holdings(&Scope::All).await.unwrap();
    let summary = store.portfolio_summary(&Scope::All).await.unwrap();
    assert_eq!(summary.unpriced_count, 1, "one demo token has no quote");
    assert_eq!(
        summary.excluded_spam_count, 1,
        "spam is excluded from the valued total"
    );
    assert_eq!(summary.stale_count, 1, "one account shows a stale balance");
    assert!(holdings.iter().all(|h| h.verification != "spam"));

    // Total equals the sum of priced rows; the unpriced row stays `None`, not zero.
    let sum: bigdecimal::BigDecimal = holdings
        .iter()
        .filter_map(|h| h.value_usd.as_deref())
        .map(|v| parse_dec(v).unwrap())
        .sum();
    assert_eq!(
        parse_dec(summary.total_value_usd.as_deref().unwrap()).unwrap(),
        sum
    );
    let unpriced = holdings.iter().find(|h| h.price_usd.is_none()).unwrap();
    assert_eq!(unpriced.value_usd, None);
    assert_eq!(
        holdings.last().unwrap().asset_id,
        unpriced.asset_id,
        "unpriced rows sort last"
    );

    // Value descending among priced rows.
    let values: Vec<_> = holdings
        .iter()
        .filter_map(|h| h.value_usd.as_deref())
        .map(|v| parse_dec(v).unwrap())
        .collect();
    assert!(values.windows(2).all(|w| w[0] >= w[1]));

    // Same ETH symbol on two networks stays two rows.
    assert_eq!(
        holdings
            .iter()
            .filter(|h| h.symbol.as_deref() == Some("ETH"))
            .count(),
        2
    );
    assert_eq!(summary.unrealized_pnl_usd, None);
}

#[tokio::test]
async fn demo_chart_has_bounded_points_and_no_backward_extrapolation() {
    let store = mem(ProfileKind::Demo).await;
    store.seed_demo().await.unwrap();
    for range in [
        ChartRange::Day,
        ChartRange::Week,
        ChartRange::Month,
        ChartRange::Quarter,
        ChartRange::Year,
        ChartRange::All,
    ] {
        let chart = store.get_chart(&Scope::All, range).await.unwrap();
        assert!(
            !chart.points.is_empty() && chart.points.len() <= 1_001,
            "{range:?}"
        );
        assert!(chart.points.windows(2).all(|w| w[0].t < w[1].t));
        let since = chart.history_available_since.unwrap();
        for p in &chart.points {
            if p.t < since {
                assert!(
                    p.value_usd.is_none(),
                    "{range:?}: value before first observation"
                );
            }
        }
        assert!(
            chart.points.last().unwrap().value_usd.is_some(),
            "{range:?}"
        );
    }
}

#[tokio::test]
async fn activity_pages_do_not_overlap() {
    let store = mem(ProfileKind::Demo).await;
    store.seed_demo().await.unwrap();
    let mut seen = BTreeSet::new();
    let mut cursor: Option<String> = None;
    let mut pages = 0;
    loop {
        let page = store
            .list_activity(&Scope::All, &Default::default(), cursor.as_deref(), 5)
            .await
            .unwrap();
        for row in &page.rows {
            assert!(
                seen.insert((row.transaction_id.clone(), row.account_id.clone())),
                "duplicate row"
            );
        }
        pages += 1;
        match page.next_cursor {
            Some(c) => cursor = Some(c),
            None => break,
        }
    }
    assert!(pages >= 2);
    assert_eq!(seen.len(), 12);

    // The demo send shows its own movement and its fee separately (fee charged once).
    let page = store
        .list_activity(&Scope::All, &Default::default(), None, 50)
        .await
        .unwrap();
    let send = page.rows.iter().find(|r| r.operation == "send").unwrap();
    assert_eq!(send.legs[0].signed_quantity, "-0.01");
    assert_eq!(send.fee_quantity.as_deref(), Some("0.00042"));
}

#[tokio::test]
async fn settings_round_trip_and_validate() {
    let store = mem(ProfileKind::Test).await;
    assert_eq!(store.get_settings().await.unwrap(), Settings::default());
    let mut s = Settings {
        language: Some("ru".into()),
        theme: ThemePreference::Light,
        ..Settings::default()
    };
    store.update_settings(&s).await.unwrap();
    assert_eq!(store.get_settings().await.unwrap(), s);
    s.language = Some("de".into());
    assert!(store.update_settings(&s).await.is_err());
}

#[tokio::test]
async fn file_database_reopens_and_guards_profile_and_schema() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("profile.sqlite");
    {
        let store = Store::open(&path, ProfileKind::Demo, clock())
            .await
            .unwrap();
        store.create_wallet("Persisted").await.unwrap();
        assert!(store.integrity_ok().await.unwrap());
        assert_eq!(
            store.schema_version().await.unwrap(),
            latest_schema_version()
        );
        store.close().await;
    }
    {
        let store = Store::open(&path, ProfileKind::Demo, clock())
            .await
            .unwrap();
        assert_eq!(store.list_wallets().await.unwrap()[0].label, "Persisted");
        store.close().await;
    }
    // A demo database must never be opened as the real profile.
    let err = Store::open(&path, ProfileKind::Real, clock())
        .await
        .err()
        .unwrap();
    assert!(matches!(err, StoreError::ProfileMismatch { .. }), "{err}");

    // Simulate a database written by a newer build.
    {
        let store = Store::open(&path, ProfileKind::Demo, clock())
            .await
            .unwrap();
        store.close().await;
        let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}", path.display()))
            .await
            .unwrap();
        sqlx::query("INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) VALUES (9999, 'future', 1, x'00', 0)")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
    }
    let err = Store::open(&path, ProfileKind::Demo, clock())
        .await
        .err()
        .unwrap();
    assert!(
        matches!(err, StoreError::UnsupportedFutureSchema { found: 9999, .. }),
        "{err}"
    );
}

mod ingest {
    use super::*;
    use num_bigint::BigInt;
    use portfolio_store::ingest::*;

    fn eth_tx(status: TxStatus, occurred_at: i64, amount: i64) -> TxSpec {
        TxSpec {
            network: NetworkId::Ethereum,
            part: None,
            hash: "0xabc".into(),
            block_height: (status != TxStatus::Pending).then_some(100),
            position: None,
            occurred_at,
            status,
            provider: "test",
            operation: "receive".into(),
            legs: vec![LegSpec {
                counterparty: None,
                asset: AssetSpec::native(NetworkId::Ethereum, "test"),
                signed_raw: BigInt::from(amount),
                direction: Direction::In,
                leg_type: "receive".into(),
                decoding: Decoding::Interpreted,
                unresolved: true,
            }],
            fee: None,
            decoding: Decoding::Interpreted,
            evidence: serde_json::json!({}),
        }
    }

    async fn eth_account(store: &Store) -> Account {
        let w = store.create_wallet("W").await.unwrap();
        store
            .add_account(
                &w.id,
                NetworkId::Ethereum,
                "0x5aaeb6053f3e94c9b9a09f33669435e7ef1beaed",
                None,
            )
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn reingest_replaces_legs_and_never_downgrades_status() {
        let store = mem(ProfileKind::Test).await;
        let a = eth_account(&store).await;
        assert!(
            store
                .ingest_transaction(&a.id, &eth_tx(TxStatus::Pending, NOW, 5))
                .await
                .unwrap()
        );
        // Confirmation updates the same record in place.
        assert!(
            !store
                .ingest_transaction(&a.id, &eth_tx(TxStatus::Confirmed, NOW - 60, 5))
                .await
                .unwrap()
        );
        // A late mempool observation does not turn it back into pending.
        store
            .ingest_transaction(&a.id, &eth_tx(TxStatus::Pending, NOW, 5))
            .await
            .unwrap();
        let page = store
            .list_activity(&Scope::All, &Default::default(), None, 10)
            .await
            .unwrap();
        assert_eq!(page.rows.len(), 1);
        assert_eq!(page.rows[0].status, "confirmed");
        assert_eq!(page.rows[0].occurred_at, NOW - 60);
        assert_eq!(page.rows[0].legs.len(), 1);
        assert_eq!(page.rows[0].legs[0].signed_quantity, "0.000000000000000005");
    }

    #[tokio::test]
    async fn changed_decimals_are_a_conflict_not_a_silent_rescale() {
        let store = mem(ProfileKind::Test).await;
        let mut token = AssetSpec::native(NetworkId::Ethereum, "test");
        token.contract = Some("0xtoken".into());
        token.decimals = 6;
        store.upsert_asset(&token).await.unwrap();
        token.decimals = 18;
        assert!(matches!(
            store.upsert_asset(&token).await,
            Err(StoreError::Invalid(_))
        ));
    }

    #[tokio::test]
    async fn newest_live_tick_values_holdings() {
        let store = mem(ProfileKind::Test).await;
        let a = eth_account(&store).await;
        let native = AssetSpec::native(NetworkId::Ethereum, "test");
        store
            .record_balance(&a.id, &native, &BigInt::from(10u64.pow(18)), None, "fresh")
            .await
            .unwrap();
        for (i, price) in ["100", "101", "102"].iter().enumerate() {
            store
                .insert_price(&PriceSpec {
                    asset_id: native.id(),
                    provider: "test",
                    price_usd: (*price).into(),
                    requested_at: NOW,
                    observed_at: NOW - 120 + 60 * i as i64,
                    granularity: "tick",
                    quality: "current",
                    change_24h_percent: None,
                })
                .await
                .unwrap();
        }
        let holdings = store.list_holdings(&Scope::All).await.unwrap();
        assert_eq!(holdings[0].price_usd.as_deref(), Some("102"));
        assert_eq!(holdings[0].value_usd.as_deref(), Some("102"));
        let chart = store.get_chart(&Scope::All, ChartRange::Day).await.unwrap();
        assert!(chart.points.last().unwrap().value_usd.is_some());
    }
}

#[tokio::test]
async fn address_batches_validate_all_rows_and_commit_atomically() {
    let store = mem(ProfileKind::Test).await;
    let wallet = store.create_wallet("Batch").await.unwrap();
    let a = "0x5aaeb6053f3e94c9b9a09f33669435e7ef1beaed".to_owned();
    let b = "0xfb6916095ca1df60bb79ce92ce3ea74c37c5d359".to_owned();
    assert!(
        store
            .add_accounts(
                &wallet.id,
                NetworkId::Ethereum,
                &[a.clone(), "invalid".into()],
                None
            )
            .await
            .is_err()
    );
    assert!(store.list_accounts(None).await.unwrap().is_empty());
    assert!(
        store
            .add_accounts(
                &wallet.id,
                NetworkId::Ethereum,
                &[a.clone(), a.to_uppercase()],
                None
            )
            .await
            .is_err()
    );
    assert!(store.list_accounts(None).await.unwrap().is_empty());
    store
        .add_accounts(
            &wallet.id,
            NetworkId::Ethereum,
            &[a.clone(), b.clone()],
            None,
        )
        .await
        .unwrap();
    assert!(
        store
            .add_accounts(
                &wallet.id,
                NetworkId::Ethereum,
                &["0x0000000000000000000000000000000000000001".into(), b],
                None
            )
            .await
            .is_err()
    );
    assert_eq!(store.list_accounts(None).await.unwrap().len(), 2);
}
