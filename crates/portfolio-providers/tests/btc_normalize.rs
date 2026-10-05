//! Bitcoin normalization fixtures (TESTING.md Layer A, "BTC" row).
//!
//! Transactions follow the documented Esplora schema; values are synthetic and
//! every expected amount is worked out by hand in the comments.

use std::collections::BTreeSet;
use std::sync::Arc;

use num_bigint::BigInt;
use portfolio_core::clock::FixedClock;
use portfolio_core::network::NetworkId;
use portfolio_providers::esplora::{Tx, tx_for_account};
use portfolio_store::ingest::{Direction, FeeAttribution, TxStatus};
use portfolio_store::{ProfileKind, Scope, Store};
use serde_json::json;

// Valid mainnet addresses (independently known public examples).
const A: &str = "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa";
const C: &str = "17SkEw2md5avVNyYgj6RiXuQKNwkXaxFyQ";
const X: &str = "1XPTgDRhN8RFnzniWCddobD9iKZatrvH4";
const Y: &str = "bc1qq2mvrp4g3ugd424dw4xv53rgsf8szkrv853jrc";
const NOW: i64 = 1_790_000_000;

fn input(addr: &str, value: u64) -> serde_json::Value {
    json!({"txid": "00".repeat(32), "vout": 0, "is_coinbase": false,
           "prevout": {"scriptpubkey_address": addr, "value": value}})
}

fn output(addr: &str, value: u64) -> serde_json::Value {
    json!({"scriptpubkey_address": addr, "value": value})
}

fn tx(id: &str, vin: Vec<serde_json::Value>, vout: Vec<serde_json::Value>, fee: u64) -> Tx {
    serde_json::from_value(json!({
        "txid": id, "vin": vin, "vout": vout, "fee": fee,
        "status": {"confirmed": true, "block_height": 800_000, "block_hash": "ab", "block_time": 1_700_000_000}
    }))
    .unwrap()
}

fn owned(list: &[&str]) -> BTreeSet<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn incoming_payment() {
    // X pays A 50,000 sat and takes 49,000 change; X pays the 1,000 fee.
    let t = tx(
        "aa01",
        vec![input(X, 100_000)],
        vec![output(A, 50_000), output(X, 49_000)],
        1_000,
    );
    let s = tx_for_account(&t, A, &owned(&[A]), NOW).unwrap();
    assert_eq!(s.operation, "receive");
    assert_eq!(s.legs.len(), 1);
    assert_eq!(s.legs[0].signed_raw, BigInt::from(50_000));
    assert_eq!(s.legs[0].direction, Direction::In);
    assert!(s.fee.is_none(), "the sender paid the fee");
    assert_eq!(s.status, TxStatus::Confirmed);
    assert_eq!(s.occurred_at, 1_700_000_000);
}

#[test]
fn outgoing_with_owned_change() {
    // A spends 100,000: 60,000 to Y, 39,000 change back to A, fee 1,000.
    // Net change -61,000 = send -60,000 + fee -1,000.
    let t = tx(
        "aa02",
        vec![input(A, 100_000)],
        vec![output(Y, 60_000), output(A, 39_000)],
        1_000,
    );
    let s = tx_for_account(&t, A, &owned(&[A]), NOW).unwrap();
    assert_eq!(s.operation, "send");
    assert_eq!(s.legs[0].signed_raw, BigInt::from(-60_000));
    let fee = s.fee.unwrap();
    assert_eq!(fee.raw, BigInt::from(1_000));
    assert_eq!(fee.attribution, FeeAttribution::Exact);
}

#[test]
fn multiple_owned_inputs_share_the_fee_pro_rata() {
    // A 70,000 + C 30,000 in; 90,000 to X, 9,000 change to A; fee 1,001.
    // Shares: A floor(1001*0.7)=700, C floor(1001*0.3)=300, remainder 1 to A (largest).
    let t = tx(
        "aa03",
        vec![input(A, 70_000), input(C, 30_000)],
        vec![output(X, 90_000), output(A, 8_999)],
        1_001,
    );
    let both = owned(&[A, C]);
    let a = tx_for_account(&t, A, &both, NOW).unwrap();
    let c = tx_for_account(&t, C, &both, NOW).unwrap();
    let fa = a.fee.unwrap();
    let fc = c.fee.unwrap();
    assert_eq!(fa.raw, BigInt::from(701));
    assert_eq!(fc.raw, BigInt::from(300));
    assert_eq!(fa.attribution, FeeAttribution::Shared);
    assert_eq!(fc.attribution, FeeAttribution::Shared);
    // A: 8,999 - 70,000 + 701 = -60,300; C: 0 - 30,000 + 300 = -29,700.
    assert_eq!(a.legs[0].signed_raw, BigInt::from(-60_300));
    assert_eq!(c.legs[0].signed_raw, BigInt::from(-29_700));
    // Conservation: principals sum to what left the owned set; fees sum to the fee.
    assert_eq!(
        &a.legs[0].signed_raw + &c.legs[0].signed_raw,
        BigInt::from(-90_000)
    );
    assert_eq!(fa.raw + fc.raw, BigInt::from(1_001));
}

#[test]
fn mixed_owner_inputs_make_the_fee_unknown() {
    // A 50,000 + X 50,000 (not tracked) in; 99,000 to Y; fee 1,000.
    let t = tx(
        "aa04",
        vec![input(A, 50_000), input(X, 50_000)],
        vec![output(Y, 99_000)],
        1_000,
    );
    let s = tx_for_account(&t, A, &owned(&[A]), NOW).unwrap();
    let fee = s.fee.unwrap();
    assert_eq!(fee.attribution, FeeAttribution::Unknown);
    assert_eq!(fee.raw, BigInt::from(500));
    assert_eq!(s.legs[0].signed_raw, BigInt::from(-49_500));
}

#[test]
fn pending_transaction_uses_observation_time() {
    let mut t = tx(
        "aa05",
        vec![input(X, 10_000)],
        vec![output(A, 9_000)],
        1_000,
    );
    t.status.confirmed = false;
    t.status.block_time = None;
    t.status.block_height = None;
    let s = tx_for_account(&t, A, &owned(&[A]), NOW).unwrap();
    assert_eq!(s.status, TxStatus::Pending);
    assert_eq!(s.occurred_at, NOW);
    assert_eq!(s.block_height, None);
}

#[test]
fn coinbase_and_untouched_and_missing_prevout() {
    let coinbase: Tx = serde_json::from_value(json!({
        "txid": "aa06", "fee": 0,
        "vin": [{"txid": "00".repeat(32), "vout": 4294967295u32, "is_coinbase": true, "prevout": null}],
        "vout": [output(A, 312_500_000)],
        "status": {"confirmed": true, "block_height": 900_000, "block_hash": "cd", "block_time": 1_750_000_000}
    }))
    .unwrap();
    let s = tx_for_account(&coinbase, A, &owned(&[A]), NOW).unwrap();
    assert_eq!(s.legs[0].leg_type, "coinbase");
    assert_eq!(s.legs[0].signed_raw, BigInt::from(312_500_000));
    assert!(s.fee.is_none());

    let other = tx("aa07", vec![input(X, 5_000)], vec![output(Y, 4_000)], 1_000);
    assert!(tx_for_account(&other, A, &owned(&[A]), NOW).is_none());

    let mut broken = tx(
        "aa08",
        vec![input(A, 5_000), input(X, 5_000)],
        vec![output(Y, 9_000)],
        1_000,
    );
    broken.vin[1].prevout = None;
    let s = tx_for_account(&broken, A, &owned(&[A]), NOW).unwrap();
    assert_eq!(s.decoding.as_str(), "partial");
    assert!(s.legs[0].unresolved);
}

#[tokio::test]
async fn same_transaction_at_two_owned_accounts_is_stored_once() {
    let store = Store::open_in_memory(ProfileKind::Test, Arc::new(FixedClock(NOW)))
        .await
        .unwrap();
    let w = store.create_wallet("Cold").await.unwrap();
    let a = store
        .add_account(&w.id, NetworkId::Bitcoin, A, None)
        .await
        .unwrap();
    let c = store
        .add_account(&w.id, NetworkId::Bitcoin, C, None)
        .await
        .unwrap();
    // A sends 40,000 to C, 59,000 change, fee 1,000.
    let t = tx(
        "aa09",
        vec![input(A, 100_000)],
        vec![output(C, 40_000), output(A, 59_000)],
        1_000,
    );
    let both = owned(&[A, C]);
    for _ in 0..2 {
        // Ingesting twice (overlapping sweeps) must not duplicate anything.
        store
            .ingest_transaction(&a.id, &tx_for_account(&t, A, &both, NOW).unwrap())
            .await
            .unwrap();
        store
            .ingest_transaction(&c.id, &tx_for_account(&t, C, &both, NOW).unwrap())
            .await
            .unwrap();
    }
    let page = store
        .list_activity(&Scope::All, &Default::default(), None, 50)
        .await
        .unwrap();
    assert_eq!(page.rows.len(), 2, "one row per (transaction, account)");
    let ra = page.rows.iter().find(|r| r.account_id == a.id).unwrap();
    let rc = page.rows.iter().find(|r| r.account_id == c.id).unwrap();
    assert_eq!(ra.transaction_id, rc.transaction_id);
    assert_eq!(ra.legs[0].signed_quantity, "-0.0004");
    assert_eq!(ra.fee_quantity.as_deref(), Some("0.00001"));
    assert_eq!(rc.legs[0].signed_quantity, "0.0004");
    assert_eq!(
        rc.fee_quantity, None,
        "the fee is counted once, for the payer"
    );
}
