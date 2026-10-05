use portfolio_core::clock::FixedClock;
use portfolio_store::{ProfileKind, Store};
use std::sync::Arc;

#[tokio::test]
async fn backup_round_trip_is_atomic_and_preserves_a_safety_snapshot() {
    let clock = Arc::new(FixedClock(1_790_000_000));
    let dir = tempfile::tempdir().unwrap();
    let original = Store::open(
        &dir.path().join("original.sqlite"),
        ProfileKind::Test,
        clock.clone(),
    )
    .await
    .unwrap();
    original.create_wallet("Saved wallet").await.unwrap();
    let backup = original.export_backup().await.unwrap();
    let restored = Store::open(
        &dir.path().join("restored.sqlite"),
        ProfileKind::Test,
        clock,
    )
    .await
    .unwrap();
    restored.create_wallet("Before restore").await.unwrap();
    let safety = dir.path().join("before.ccbackup");
    restored.restore_backup(&backup, &safety).await.unwrap();
    assert_eq!(
        restored.list_wallets().await.unwrap()[0].label,
        "Saved wallet"
    );
    assert!(restored.integrity_ok().await.unwrap());
    assert!(
        std::fs::read_to_string(safety)
            .unwrap()
            .contains("coincontrol-backup-v1")
    );
    let csv = restored.export_csv("decisions").await.unwrap();
    assert!(csv.contains("decision_json"));
}

#[tokio::test]
async fn corrupt_future_and_wrong_profile_backups_cannot_replace_data() {
    let clock = Arc::new(FixedClock(1_790_000_000));
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(
        &dir.path().join("original.sqlite"),
        ProfileKind::Test,
        clock.clone(),
    )
    .await
    .unwrap();
    store.create_wallet("Keep me").await.unwrap();
    let good = store.export_backup().await.unwrap();
    let mut bad: serde_json::Value = serde_json::from_str(&good).unwrap();
    bad["sha256"] = serde_json::json!("wrong");
    assert!(store.inspect_backup(&bad.to_string()).await.is_err());
    bad = serde_json::from_str(&good).unwrap();
    bad["schema_version"] = serde_json::json!(999);
    assert!(store.inspect_backup(&bad.to_string()).await.is_err());
    bad = serde_json::from_str(&good).unwrap();
    bad["path"] = serde_json::json!("../../escape");
    assert!(store.inspect_backup(&bad.to_string()).await.is_err());
    let real = Store::open_in_memory(ProfileKind::Real, clock)
        .await
        .unwrap();
    assert!(real.inspect_backup(&good).await.is_err());
    assert_eq!(store.list_wallets().await.unwrap()[0].label, "Keep me");
}

#[tokio::test]
async fn recovery_preserves_basis_audit_groups_preferences_and_exact_csv() {
    use portfolio_core::accounting::BasisKind;
    use portfolio_store::{BasisLotInput, LegOverride, Scope};
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(FixedClock(1_790_000_000));
    let original = Store::open(
        &dir.path().join("demo.sqlite"),
        ProfileKind::Demo,
        clock.clone(),
    )
    .await
    .unwrap();
    original.seed_demo().await.unwrap();
    original.replay_accounting().await.unwrap();
    let item = original
        .list_review_items(&Scope::All, Some(500))
        .await
        .unwrap()
        .items
        .into_iter()
        .find(|r| r.reason == "unknown_basis")
        .unwrap();
    let leg = original.leg_detail(&item.leg_id).await.unwrap();
    original
        .save_leg_override(
            &item.leg_id,
            &LegOverride {
                basis_lots: Some(vec![BasisLotInput {
                    quantity: leg.quantity,
                    basis_usd: Some("123.45".into()),
                    basis_kind: BasisKind::Known,
                    acquired_at: leg.occurred_at,
                }]),
                note: Some("Recovery audit".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    original
        .set_asset_policy(&leg.asset_id, true, None)
        .await
        .unwrap();
    let wallets = original.list_wallets().await.unwrap();
    let group = original.create_group("Recovery group").await.unwrap();
    original
        .set_group_wallets(
            &group.id,
            &wallets.iter().map(|w| w.id.clone()).collect::<Vec<_>>(),
        )
        .await
        .unwrap();
    original.replay_accounting().await.unwrap();
    let backup = original.export_backup().await.unwrap();
    let restored = Store::open(
        &dir.path().join("restored.sqlite"),
        ProfileKind::Demo,
        clock,
    )
    .await
    .unwrap();
    restored.seed_demo().await.unwrap();
    restored.replay_accounting().await.unwrap();
    restored.create_wallet("Replaced wallet").await.unwrap();
    restored
        .restore_backup(&backup, &dir.path().join("safety.ccbackup"))
        .await
        .unwrap();
    restored.replay_accounting().await.unwrap();
    assert_eq!(
        restored.portfolio_summary(&Scope::All).await.unwrap(),
        original.portfolio_summary(&Scope::All).await.unwrap()
    );
    assert_eq!(
        restored.list_groups().await.unwrap(),
        original.list_groups().await.unwrap()
    );
    assert_eq!(
        restored.list_asset_policies().await.unwrap(),
        original.list_asset_policies().await.unwrap()
    );
    let decision = restored.leg_detail(&item.leg_id).await.unwrap();
    assert_eq!(decision.basis_usd.as_deref(), Some("123.45"));
    assert_eq!(decision.history.len(), 1);
    for kind in ["lots", "decisions", "activity", "holdings"] {
        assert_eq!(
            restored.export_csv(kind).await.unwrap(),
            original.export_csv(kind).await.unwrap()
        );
    }
}
