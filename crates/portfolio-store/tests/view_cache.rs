//! File-backed regressions: cache invalidation across commits and restores.
use portfolio_core::clock::FixedClock;
use portfolio_core::network::NetworkId;
use portfolio_store::ingest::{AssetSpec, PriceSpec};
use portfolio_store::{ProfileKind, Scope, Store};
use std::sync::Arc;

#[tokio::test]
async fn committed_updates_from_clones_external_writers_and_restore_refresh_views() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("profile.sqlite");
    let clock = Arc::new(FixedClock(1_791_158_400));
    let store = Store::open(&path, ProfileKind::Test, clock.clone())
        .await
        .unwrap();
    let wallet = store.create_wallet("Cache regression").await.unwrap();
    let account = store
        .add_account(
            &wallet.id,
            NetworkId::Ethereum,
            "0x0000000000000000000000000000000000000001",
            None,
        )
        .await
        .unwrap();
    let asset = AssetSpec::native(NetworkId::Ethereum, "test");
    store
        .record_balance(
            &account.id,
            &asset,
            &1_000_000_000_000_000_000_i64.into(),
            None,
            "fresh",
        )
        .await
        .unwrap();
    store
        .insert_price(&PriceSpec {
            asset_id: asset.id(),
            provider: "test",
            price_usd: "2000".into(),
            requested_at: store.now(),
            observed_at: store.now(),
            granularity: "tick",
            quality: "current",
            change_24h_percent: None,
        })
        .await
        .unwrap();
    assert_eq!(
        store
            .portfolio_summary(&Scope::All)
            .await
            .unwrap()
            .total_value_usd
            .as_deref(),
        Some("2000")
    );
    let backup = store.export_backup().await.unwrap();
    store
        .clone()
        .record_balance(
            &account.id,
            &asset,
            &2_000_000_000_000_000_000_i64.into(),
            None,
            "fresh",
        )
        .await
        .unwrap();
    assert_eq!(
        store
            .portfolio_summary(&Scope::All)
            .await
            .unwrap()
            .total_value_usd
            .as_deref(),
        Some("4000")
    );
    let other = Store::open(&path, ProfileKind::Test, clock).await.unwrap();
    other.mark_balances_stale(&account.id).await.unwrap();
    let summary = store.portfolio_summary(&Scope::All).await.unwrap();
    assert_eq!(summary.total_value_usd.as_deref(), Some("4000"));
    assert_eq!(summary.stale_count, 1);
    assert_eq!(
        store.list_holdings(&Scope::All).await.unwrap()[0].quantity,
        "2"
    );
    store
        .restore_backup(&backup, &temp.path().join("safety.ccbackup"))
        .await
        .unwrap();
    assert_eq!(
        store
            .portfolio_summary(&Scope::All)
            .await
            .unwrap()
            .total_value_usd
            .as_deref(),
        Some("2000")
    );
    assert_eq!(
        store.list_holdings(&Scope::All).await.unwrap()[0].quantity,
        "1"
    );
    other.close().await;
    store.close().await;
}

#[tokio::test]
async fn uncommitted_prices_do_not_pollute_cached_values() {
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("profile.sqlite");
    let store = Store::open(
        &path,
        ProfileKind::Test,
        Arc::new(FixedClock(1_791_158_400)),
    )
    .await
    .unwrap();
    let wallet = store.create_wallet("Rollback").await.unwrap();
    let account = store
        .add_account(
            &wallet.id,
            NetworkId::Ethereum,
            "0x0000000000000000000000000000000000000001",
            None,
        )
        .await
        .unwrap();
    let asset = AssetSpec::native(NetworkId::Ethereum, "test");
    store
        .record_balance(
            &account.id,
            &asset,
            &1_000_000_000_000_000_000_i64.into(),
            None,
            "fresh",
        )
        .await
        .unwrap();
    let original = store.list_holdings(&Scope::All).await.unwrap();
    assert!(original[0].price_usd.is_none());
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::new().filename(&path))
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO prices(asset_id,provider,price_usd,requested_at,observed_at,granularity,quality) VALUES(?,'test','123',?,?,'tick','current')")
        .bind(asset.id()).bind(store.now()).bind(store.now()).execute(&mut *tx).await.unwrap();
    assert_eq!(store.list_holdings(&Scope::All).await.unwrap(), original);
    tx.rollback().await.unwrap();
    assert_eq!(store.list_holdings(&Scope::All).await.unwrap(), original);
    pool.close().await;
    store.close().await;
}
