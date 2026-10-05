//! Verify the shared Windows acceptance fixture against production Store APIs.
use portfolio_core::clock::SystemClock;
use portfolio_store::{ProfileKind, Scope, Store};
use std::{path::PathBuf, process::Command, sync::Arc};

#[tokio::test]
async fn windows_fixture_has_hand_computed_owned_transfer_and_group_totals() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("real.sqlite");
    let store = Store::open(&path, ProfileKind::Real, Arc::new(SystemClock))
        .await
        .unwrap();
    store.close().await;
    let script =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/acceptance-fixture.mjs");
    let result = Command::new("node")
        .arg(&script)
        .arg("seed")
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let store = Store::open(&path, ProfileKind::Real, Arc::new(SystemClock))
        .await
        .unwrap();
    store.replay_if_dirty().await.unwrap();
    let total = store.portfolio_summary(&Scope::All).await.unwrap();
    // 2 ETH received; internal transfer of 1 ETH; 0.01 ETH network fee.
    // A holds 0.99 ETH, B holds 1 ETH; current price $3000 -> $5970.
    assert_eq!(total.total_value_usd.as_deref(), Some("5970"));
    assert_eq!(total.accounting.expenses.known_usd, "30");
    assert_eq!(total.accounting.fee_charges, 1);
    for (id, value) in [("acceptance-both", "5970"), ("acceptance-a", "2970")] {
        let group = store
            .portfolio_summary(&Scope::Group { id: id.into() })
            .await
            .unwrap();
        assert_eq!(group.total_value_usd.as_deref(), Some(value));
        assert_eq!(group.accounting.expenses.known_usd, "30");
        assert_eq!(group.accounting.fee_charges, 1);
    }
    let decision = store.leg_detail("acceptance-receipt:in").await.unwrap();
    assert_eq!(decision.history.len(), 2);
    assert_eq!(decision.basis_usd.as_deref(), Some("4000"));
    assert!(store.integrity_ok().await.unwrap());
    store.close().await;
    let report = dir.path().join("snapshot.json");
    let result = Command::new("node")
        .arg(&script)
        .arg("snapshot")
        .arg(&path)
        .arg(&report)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let snapshot: serde_json::Value =
        serde_json::from_slice(&std::fs::read(report).unwrap()).unwrap();
    assert_eq!(snapshot["ownTransferLegs"], 2);
    assert_eq!(snapshot["feeCharges"], 1);
    assert_eq!(snapshot["accountingDirty"], "0");
}
