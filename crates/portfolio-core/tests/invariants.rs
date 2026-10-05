//! Accounting invariants from ACCOUNTING.md §11 and TESTING.md Layer A.

use std::collections::BTreeSet;

use portfolio_core::accounting::*;
use portfolio_core::decimal::{Dec, parse_dec};

fn dec(s: &str) -> Dec {
    parse_dec(s).unwrap()
}

fn at(time: i64) -> ChainOrder {
    ChainOrder { time, seq: 0 }
}

fn buy(id: &str, t: i64, account: &str, asset: &str, qty: &str, basis: Option<&str>) -> Event {
    Event {
        id: id.into(),
        order: at(t),
        kind: EventKind::Acquire {
            account: account.into(),
            asset: asset.into(),
            quantity: dec(qty),
            basis_usd: basis.map(dec),
            basis_kind: BasisKind::Known,
            acquired_at: None,
            external_inflow_usd: None,
            is_external_inflow: false,
        },
    }
}

fn sell(id: &str, t: i64, account: &str, asset: &str, qty: &str, proceeds: Option<&str>) -> Event {
    Event {
        id: id.into(),
        order: at(t),
        kind: EventKind::Dispose {
            account: account.into(),
            asset: asset.into(),
            quantity: dec(qty),
            proceeds_usd: proceeds.map(dec),
        },
    }
}

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn split_lots_conserve_basis_exactly() {
    let events = vec![
        buy("b", 0, "a", "X", "3", Some("100")),
        sell("s1", 1, "a", "X", "1", Some("50")),
        sell("s2", 2, "a", "X", "1", Some("50")),
    ];
    let ledger = Ledger::replay(&events);
    let totals = ledger.totals_for(&set(&["a"]));
    let remaining = summarize_position(ledger.lots_for(&set(&["a"]), "X"), Some(&dec("1")));
    let consumed = dec("100") - remaining.basis_usd.clone().unwrap();
    // realized = proceeds - consumed basis, so consumed + remaining must equal 100 exactly.
    assert_eq!(dec("100") - totals.realized_usd.value().unwrap(), consumed);
    assert_eq!(consumed + remaining.basis_usd.unwrap(), dec("100"));
}

#[test]
fn replay_is_deterministic_regardless_of_input_order() {
    let mut events = vec![
        buy("b1", 0, "a", "X", "1", Some("10")),
        buy("b2", 1, "a", "X", "1", Some("20")),
        sell("s", 2, "a", "X", "1.5", Some("60")),
    ];
    let forward = Ledger::replay(&events);
    events.reverse();
    let backward = Ledger::replay(&events);
    let f: Vec<_> = forward.all_lots().cloned().collect();
    let b: Vec<_> = backward.all_lots().cloned().collect();
    assert_eq!(f, b);
    assert_eq!(
        forward.totals_for(&set(&["a"])),
        backward.totals_for(&set(&["a"]))
    );
}

#[test]
fn reimporting_identical_events_has_no_economic_effect() {
    let events = vec![
        buy("b", 0, "a", "X", "1", Some("10")),
        sell("s", 1, "a", "X", "0.5", Some("8")),
    ];
    let once = Ledger::replay(&events);
    let mut doubled = events.clone();
    doubled.extend(events);
    let twice = Ledger::replay(&doubled);
    assert_eq!(
        once.all_lots().cloned().collect::<Vec<_>>(),
        twice.all_lots().cloned().collect::<Vec<_>>()
    );
    assert_eq!(
        once.totals_for(&set(&["a"])),
        twice.totals_for(&set(&["a"]))
    );
}

#[test]
fn fifo_never_consumes_another_accounts_lots_or_same_symbol_other_asset() {
    let events = vec![
        buy("b1", 0, "a", "eth:usdt", "100", Some("100")),
        buy("b2", 0, "b", "eth:usdt", "100", Some("100")),
        buy("b3", 0, "a", "tron:usdt", "100", Some("100")),
        sell("s", 1, "a", "eth:usdt", "150", Some("150")),
    ];
    let ledger = Ledger::replay(&events);
    assert_eq!(
        summarize_position(ledger.lots_for(&set(&["b"]), "eth:usdt"), None).quantity,
        dec("100")
    );
    assert_eq!(
        summarize_position(ledger.lots_for(&set(&["a"]), "tron:usdt"), None).quantity,
        dec("100")
    );
    assert_eq!(
        summarize_position(ledger.lots_for(&set(&["a"]), "eth:usdt"), None).quantity,
        dec("0")
    );
    let gap = &ledger.gaps()[0];
    assert_eq!(
        (gap.account.as_str(), gap.missing_quantity.clone()),
        ("a", dec("50"))
    );
    // Shortfall makes realized P&L unresolved, not computed against zero cost.
    assert!(
        ledger
            .totals_for(&set(&["a"]))
            .realized_usd
            .value()
            .is_none()
    );
}

#[test]
fn unknown_basis_is_not_zero_basis() {
    let ledger = Ledger::replay(&[buy("b", 0, "a", "X", "1", None)]);
    let p = summarize_position(ledger.lots_for(&set(&["a"]), "X"), Some(&dec("5")));
    assert_eq!(p.basis_usd, None);
    assert_eq!(p.unrealized_usd, None);
    assert_eq!(p.reason, Some(UnavailableReason::MissingBasis));

    let ledger = Ledger::replay(&[buy("b", 0, "a", "X", "1", Some("0"))]);
    let p = summarize_position(ledger.lots_for(&set(&["a"]), "X"), Some(&dec("5")));
    assert_eq!(p.basis_usd, Some(dec("0")));
    assert_eq!(p.unrealized_usd, Some(dec("5")));
    assert_eq!(p.unrealized_percent, None);
}

#[test]
fn own_transfer_preserves_acquisition_order_at_recipient() {
    let events = vec![
        buy("old", 0, "a", "X", "1", Some("10")),
        buy("new", 5, "b", "X", "1", Some("50")),
        Event {
            id: "t".into(),
            order: at(10),
            kind: EventKind::OwnTransfer {
                from: "a".into(),
                to: "b".into(),
                asset: "X".into(),
                quantity: dec("1"),
                market_value_usd: Some(dec("60")),
            },
        },
        sell("s", 20, "b", "X", "1", Some("60")),
    ];
    let ledger = Ledger::replay(&events);
    // FIFO at b must consume the lot acquired at t=0 (basis 10), not b's own t=5 lot.
    assert_eq!(
        ledger.totals_for(&set(&["b"])).realized_usd.value(),
        Some(dec("50"))
    );
    let b = set(&["b"]);
    let left: Vec<_> = ledger.lots_for(&b, "X").collect();
    assert_eq!(left[0].basis_usd, Some(dec("50")));
}

#[test]
fn unclassified_withdrawal_makes_period_flows_incomplete() {
    let events = vec![
        buy("b", 0, "a", "X", "1", Some("10")),
        Event {
            id: "w".into(),
            order: at(10),
            kind: EventKind::Withdraw {
                account: "a".into(),
                asset: "X".into(),
                quantity: dec("1"),
                market_value_usd: Some(dec("20")),
                classified: false,
            },
        },
    ];
    let flows = scope_external_flows(&events, &set(&["a"]));
    assert!(!flows.complete);
    assert_eq!(flows.flows[0].amount_usd, dec("-20"));
}

#[test]
fn live_coin_watch_ratio_conversion() {
    assert_eq!(ratio_to_percent(&dec("1.05")), dec("5"));
    assert_eq!(ratio_to_percent(&dec("0.8")), dec("-20"));
    assert_eq!(ratio_to_percent(&dec("1")), dec("0"));
}

#[test]
fn price_change_requires_positive_reference() {
    assert_eq!(
        price_change_percent(&dec("110"), &dec("100")),
        Some(dec("10"))
    );
    assert_eq!(price_change_percent(&dec("110"), &dec("0")), None);
}

#[test]
fn dietz_zero_duration_and_flow_at_start_are_handled() {
    let r = modified_dietz(10, 10, &dec("100"), &dec("100"), &[]);
    assert_eq!(r.return_percent, None);
    assert_eq!(r.reason, Some(UnavailableReason::ZeroDuration));
    // A flow exactly at t0 is part of V0 and must not be counted again.
    let flows = [ExternalFlow {
        time: 0,
        amount_usd: dec("500"),
    }];
    let r = modified_dietz(0, 100, &dec("1500"), &dec("1650"), &flows);
    assert_eq!(r.gain_usd, dec("150"));
    assert_eq!(r.return_percent, Some(dec("10")));
}
