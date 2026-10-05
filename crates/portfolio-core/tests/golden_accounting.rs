//! Golden acceptance cases from `docs/spec/fixtures/accounting_cases.json`.
//!
//! Each case is translated into ledger events and driven through the real
//! engine; expectations come only from the fixture, never from the code under test.

use std::collections::BTreeSet;

use portfolio_core::accounting::*;
use portfolio_core::decimal::{Dec, parse_dec};
use serde_json::Value;

const FIXTURE: &str = include_str!("../../../docs/spec/fixtures/accounting_cases.json");

fn d(v: &Value) -> Dec {
    parse_dec(
        v.as_str()
            .unwrap_or_else(|| panic!("expected decimal string, got {v}")),
    )
    .unwrap()
}

fn opt_d(v: &Value) -> Option<Dec> {
    if v.is_null() { None } else { Some(d(v)) }
}

/// Asserts an exact decimal (or null) expectation, with an optional absolute tolerance.
fn check(case: &str, field: &str, expected: &Value, actual: Option<&Dec>, tolerance: Option<&Dec>) {
    match (opt_d(expected), actual) {
        (None, None) => {}
        (Some(e), Some(a)) => {
            let diff = (a - &e).abs();
            let limit = tolerance.cloned().unwrap_or_default();
            assert!(diff <= limit, "{case}.{field}: expected {e}, got {a}");
        }
        (e, a) => panic!("{case}.{field}: expected {e:?}, got {a:?}"),
    }
}

fn order(time: i64) -> ChainOrder {
    ChainOrder { time, seq: 0 }
}

fn acquire(id: &str, t: i64, account: &str, asset: &str, qty: &Dec, basis: Option<Dec>) -> Event {
    Event {
        id: id.into(),
        order: order(t),
        kind: EventKind::Acquire {
            account: account.into(),
            asset: asset.into(),
            quantity: qty.clone(),
            basis_kind: if basis.is_some() {
                BasisKind::Known
            } else {
                BasisKind::Unknown
            },
            basis_usd: basis,
            acquired_at: None,
            external_inflow_usd: None,
            is_external_inflow: false,
        },
    }
}

fn one(account: &str) -> BTreeSet<String> {
    BTreeSet::from([account.to_owned()])
}

fn run_case(case: &Value) {
    let id = case["id"].as_str().unwrap();
    let e = &case["expected"];
    match case["kind"].as_str().unwrap() {
        "remaining_lots" => {
            let events: Vec<Event> = case["lots"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .map(|(i, l)| {
                    acquire(
                        &format!("lot{i}"),
                        i as i64,
                        "acct",
                        "X",
                        &d(&l["quantity"]),
                        opt_d(&l["basis_usd"]),
                    )
                })
                .collect();
            let ledger = Ledger::replay(&events);
            let scope = one("acct");
            let p = summarize_position(ledger.lots_for(&scope, "X"), Some(&d(&case["price_usd"])));
            check(id, "quantity", &e["quantity"], Some(&p.quantity), None);
            check(id, "value_usd", &e["value_usd"], p.value_usd.as_ref(), None);
            check(id, "basis_usd", &e["basis_usd"], p.basis_usd.as_ref(), None);
            check(
                id,
                "unrealized_usd",
                &e["unrealized_usd"],
                p.unrealized_usd.as_ref(),
                None,
            );
            check(
                id,
                "unrealized_percent",
                &e["unrealized_percent"],
                p.unrealized_percent.as_ref(),
                None,
            );
            if e["reason"] == "zero_basis" {
                assert_eq!(p.reason, Some(UnavailableReason::ZeroBasis), "{id}");
            }
        }
        "fifo_sale" => {
            let mut events: Vec<Event> = case["lots_oldest_first"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .map(|(i, l)| {
                    acquire(
                        &format!("lot{i}"),
                        i as i64,
                        "acct",
                        "X",
                        &d(&l["quantity"]),
                        opt_d(&l["basis_usd"]),
                    )
                })
                .collect();
            events.push(Event {
                id: "sale".into(),
                order: order(100),
                kind: EventKind::Dispose {
                    account: "acct".into(),
                    asset: "X".into(),
                    quantity: d(&case["sold_quantity"]),
                    proceeds_usd: Some(d(&case["gross_proceeds_usd"])),
                },
            });
            let ledger = Ledger::replay(&events);
            let scope = one("acct");
            let totals = ledger.totals_for(&scope);
            let p = summarize_position(ledger.lots_for(&scope, "X"), Some(&d(&case["price_usd"])));
            let proceeds = d(&case["gross_proceeds_usd"]);
            let realized = totals.realized_usd.value().unwrap();
            check(
                id,
                "disposed_basis_usd",
                &e["disposed_basis_usd"],
                Some(&(proceeds - &realized)),
                None,
            );
            check(
                id,
                "realized_usd",
                &e["realized_usd"],
                Some(&realized),
                None,
            );
            check(
                id,
                "remaining_quantity",
                &e["remaining_quantity"],
                Some(&p.quantity),
                None,
            );
            check(
                id,
                "remaining_basis_usd",
                &e["remaining_basis_usd"],
                p.basis_usd.as_ref(),
                None,
            );
            check(
                id,
                "remaining_value_usd",
                &e["remaining_value_usd"],
                p.value_usd.as_ref(),
                None,
            );
            check(
                id,
                "unrealized_usd",
                &e["unrealized_usd"],
                p.unrealized_usd.as_ref(),
                None,
            );
            check(
                id,
                "unrealized_percent",
                &e["unrealized_percent"],
                p.unrealized_percent.as_ref(),
                None,
            );
            let total = total_accounted_pnl(p.unrealized_usd.as_ref(), &totals);
            check(
                id,
                "total_accounted_pnl_usd",
                &e["total_accounted_pnl_usd"],
                total.as_ref(),
                None,
            );
        }
        "fee" => {
            let fee_qty = d(&case["fee_quantity"]);
            let events = vec![
                acquire(
                    "open",
                    0,
                    "acct",
                    "ETH",
                    &d(&case["opening_quantity"]),
                    Some(d(&case["opening_basis_usd"])),
                ),
                Event {
                    id: "fee".into(),
                    order: order(10),
                    kind: EventKind::Fee {
                        account: "acct".into(),
                        asset: "ETH".into(),
                        value_usd: Some(&fee_qty * d(&case["fee_price_usd"])),
                        quantity: fee_qty,
                    },
                },
            ];
            let ledger = Ledger::replay(&events);
            let scope = one("acct");
            let totals = ledger.totals_for(&scope);
            let p = summarize_position(
                ledger.lots_for(&scope, "ETH"),
                Some(&d(&case["current_price_usd"])),
            );
            check(
                id,
                "remaining_quantity",
                &e["remaining_quantity"],
                Some(&p.quantity),
                None,
            );
            check(
                id,
                "remaining_basis_usd",
                &e["remaining_basis_usd"],
                p.basis_usd.as_ref(),
                None,
            );
            check(
                id,
                "remaining_value_usd",
                &e["remaining_value_usd"],
                p.value_usd.as_ref(),
                None,
            );
            check(
                id,
                "unrealized_usd",
                &e["unrealized_usd"],
                p.unrealized_usd.as_ref(),
                None,
            );
            check(
                id,
                "fee_asset_realized_usd",
                &e["fee_asset_realized_usd"],
                totals.realized_usd.value().as_ref(),
                None,
            );
            check(
                id,
                "expense_usd",
                &e["expense_usd"],
                totals.expense_usd.value().as_ref(),
                None,
            );
            let total = total_accounted_pnl(p.unrealized_usd.as_ref(), &totals);
            check(
                id,
                "total_accounted_pnl_usd",
                &e["total_accounted_pnl_usd"],
                total.as_ref(),
                None,
            );
        }
        "owned_transfer" => {
            let price = d(&case["price_usd"]);
            let qty = d(&case["transfer_quantity"]);
            let fee_qty = d(&case["sender_fee_quantity"]);
            let mut events = vec![acquire(
                "open",
                0,
                "sender",
                "ETH",
                &d(&case["opening_sender_quantity"]),
                Some(d(&case["opening_sender_basis_usd"])),
            )];
            let recipient_opening = d(&case["recipient_opening_quantity"]);
            if recipient_opening > Dec::default() {
                events.push(acquire(
                    "open-r",
                    0,
                    "recipient",
                    "ETH",
                    &recipient_opening,
                    None,
                ));
            }
            let transfer = Event {
                id: "tx1:transfer".into(),
                order: ChainOrder { time: 50, seq: 1 },
                kind: EventKind::OwnTransfer {
                    from: "sender".into(),
                    to: "recipient".into(),
                    asset: "ETH".into(),
                    market_value_usd: Some(&qty * &price),
                    quantity: qty,
                },
            };
            let fee = Event {
                id: "tx1:fee".into(),
                order: ChainOrder { time: 50, seq: 0 },
                kind: EventKind::Fee {
                    account: "sender".into(),
                    asset: "ETH".into(),
                    value_usd: Some(&fee_qty * &price),
                    quantity: fee_qty,
                },
            };
            events.push(transfer.clone());
            events.push(fee.clone());
            // The same evidence observed twice (e.g. from both accounts) must not double count.
            events.push(transfer);
            events.push(fee);
            let ledger = Ledger::replay(&events);

            let sender = summarize_position(ledger.lots_for(&one("sender"), "ETH"), Some(&price));
            let recipient =
                summarize_position(ledger.lots_for(&one("recipient"), "ETH"), Some(&price));
            check(
                id,
                "sender_quantity",
                &e["sender_quantity"],
                Some(&sender.quantity),
                None,
            );
            check(
                id,
                "sender_basis_usd",
                &e["sender_basis_usd"],
                sender.basis_usd.as_ref(),
                None,
            );
            check(
                id,
                "recipient_quantity",
                &e["recipient_quantity"],
                Some(&recipient.quantity),
                None,
            );
            check(
                id,
                "recipient_basis_usd",
                &e["recipient_basis_usd"],
                recipient.basis_usd.as_ref(),
                None,
            );

            // Union of the selected (overlapping) groups.
            let mut union = BTreeSet::new();
            for g in case["selected_groups"].as_array().unwrap() {
                for member in case["groups"][g.as_str().unwrap()].as_array().unwrap() {
                    union.insert(member.as_str().unwrap().to_owned());
                }
            }
            let u = summarize_position(ledger.lots_for(&union, "ETH"), Some(&price));
            let totals = ledger.totals_for(&union);
            check(
                id,
                "union_quantity",
                &e["union_quantity"],
                Some(&u.quantity),
                None,
            );
            check(
                id,
                "union_basis_usd",
                &e["union_basis_usd"],
                u.basis_usd.as_ref(),
                None,
            );
            check(
                id,
                "union_value_usd",
                &e["union_value_usd"],
                u.value_usd.as_ref(),
                None,
            );
            let total = total_accounted_pnl(u.unrealized_usd.as_ref(), &totals);
            check(
                id,
                "union_total_accounted_pnl_usd",
                &e["union_total_accounted_pnl_usd"],
                total.as_ref(),
                None,
            );
            assert_eq!(
                totals.fee_charges,
                e["fee_charges"].as_u64().unwrap() as u32,
                "{id}.fee_charges"
            );

            let portfolio = BTreeSet::from(["sender".to_owned(), "recipient".to_owned()]);
            let pf = scope_external_flows(&events, &portfolio);
            let pf_sum: Dec = pf.flows.iter().map(|f| f.amount_usd.clone()).sum();
            check(
                id,
                "portfolio_external_transfer_flow_usd",
                &e["portfolio_external_transfer_flow_usd"],
                Some(&pf_sum),
                None,
            );
            let rf = scope_external_flows(&events, &one("recipient"));
            let rf_sum: Dec = rf.flows.iter().map(|f| f.amount_usd.clone()).sum();
            check(
                id,
                "recipient_scope_inflow_usd",
                &e["recipient_scope_inflow_usd"],
                Some(&rf_sum),
                None,
            );
        }
        "partial_basis" => {
            let events: Vec<Event> = case["lots"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .map(|(i, l)| {
                    acquire(
                        &format!("lot{i}"),
                        i as i64,
                        "acct",
                        "X",
                        &d(&l["quantity"]),
                        opt_d(&l["basis_usd"]),
                    )
                })
                .collect();
            let ledger = Ledger::replay(&events);
            let p = summarize_position(
                ledger.lots_for(&one("acct"), "X"),
                Some(&d(&case["price_usd"])),
            );
            check(id, "value_usd", &e["value_usd"], p.value_usd.as_ref(), None);
            check(
                id,
                "whole_position_unrealized_usd",
                &e["whole_position_unrealized_usd"],
                p.unrealized_usd.as_ref(),
                None,
            );
            check(
                id,
                "whole_position_unrealized_percent",
                &e["whole_position_unrealized_percent"],
                p.unrealized_percent.as_ref(),
                None,
            );
            let k = &p.known_subset;
            check(
                id,
                "known_subset_value_usd",
                &e["known_subset_value_usd"],
                k.value_usd.as_ref(),
                None,
            );
            check(
                id,
                "known_subset_basis_usd",
                &e["known_subset_basis_usd"],
                Some(&k.basis_usd),
                None,
            );
            check(
                id,
                "known_subset_unrealized_usd",
                &e["known_subset_unrealized_usd"],
                k.unrealized_usd.as_ref(),
                None,
            );
            check(
                id,
                "known_subset_unrealized_percent",
                &e["known_subset_unrealized_percent"],
                k.unrealized_percent.as_ref(),
                None,
            );
            check(
                id,
                "basis_coverage_quantity_percent",
                &e["basis_coverage_quantity_percent"],
                p.basis_coverage_quantity_percent.as_ref(),
                None,
            );
            assert_eq!(p.reason, Some(UnavailableReason::MissingBasis));
        }
        "unknown_basis_receipt" => {
            let events = vec![acquire(
                "rx",
                0,
                "acct",
                "BTC",
                &d(&case["quantity"]),
                opt_d(&case["basis_usd"]),
            )];
            let ledger = Ledger::replay(&events);
            let p = summarize_position(
                ledger.lots_for(&one("acct"), "BTC"),
                Some(&d(&case["price_usd"])),
            );
            let totals = ledger.totals_for(&one("acct"));
            check(id, "value_usd", &e["value_usd"], p.value_usd.as_ref(), None);
            check(id, "basis_usd", &e["basis_usd"], p.basis_usd.as_ref(), None);
            check(
                id,
                "unrealized_usd",
                &e["unrealized_usd"],
                p.unrealized_usd.as_ref(),
                None,
            );
            check(
                id,
                "unrealized_percent",
                &e["unrealized_percent"],
                p.unrealized_percent.as_ref(),
                None,
            );
            check(
                id,
                "recognized_income_usd",
                &e["recognized_income_usd"],
                totals.income_usd.value().as_ref(),
                None,
            );
        }
        "modified_dietz" => {
            let flows: Vec<ExternalFlow> = case["external_flows"]
                .as_array()
                .unwrap()
                .iter()
                .map(|f| ExternalFlow {
                    time: f["second"].as_i64().unwrap(),
                    amount_usd: d(&f["amount_usd"]),
                })
                .collect();
            let r = modified_dietz(
                case["start_second"].as_i64().unwrap(),
                case["end_second"].as_i64().unwrap(),
                &d(&case["beginning_value_usd"]),
                &d(&case["ending_value_usd"]),
                &flows,
            );
            let tol = e.get("return_percent_absolute_tolerance").map(d);
            check(id, "gain_usd", &e["gain_usd"], Some(&r.gain_usd), None);
            check(
                id,
                "denominator_usd",
                &e["denominator_usd"],
                Some(&r.denominator_usd),
                None,
            );
            check(
                id,
                "return_percent",
                &e["return_percent"],
                r.return_percent.as_ref(),
                tol.as_ref(),
            );
            if e["reason"] == "nonpositive_denominator" {
                assert_eq!(r.reason, Some(UnavailableReason::NonpositiveDenominator));
            }
        }
        "missing_price" => {
            let events = vec![acquire(
                "rx",
                0,
                "acct",
                "X",
                &d(&case["quantity"]),
                opt_d(&case["basis_usd"]),
            )];
            let ledger = Ledger::replay(&events);
            let p = summarize_position(
                ledger.lots_for(&one("acct"), "X"),
                opt_d(&case["price_usd"]).as_ref(),
            );
            check(id, "quantity", &e["quantity"], Some(&p.quantity), None);
            check(id, "value_usd", &e["value_usd"], p.value_usd.as_ref(), None);
            check(
                id,
                "unrealized_usd",
                &e["unrealized_usd"],
                p.unrealized_usd.as_ref(),
                None,
            );
            check(
                id,
                "unrealized_percent",
                &e["unrealized_percent"],
                p.unrealized_percent.as_ref(),
                None,
            );
            assert_eq!(p.reason, Some(UnavailableReason::MissingPrice));
        }
        "swap" => {
            let disposed = case["disposed_asset"].as_str().unwrap();
            let received = case["received_asset"].as_str().unwrap();
            let gross = d(&case["gross_trade_value_usd"]);
            let events = vec![
                acquire(
                    "open",
                    0,
                    "acct",
                    disposed,
                    &d(&case["disposed_quantity"]),
                    Some(d(&case["disposed_basis_usd"])),
                ),
                Event {
                    id: "swap".into(),
                    order: order(10),
                    kind: EventKind::Swap {
                        account: "acct".into(),
                        give_asset: disposed.into(),
                        give_quantity: d(&case["disposed_quantity"]),
                        get_asset: received.into(),
                        get_quantity: d(&case["received_quantity"]),
                        gross_value_usd: Some(gross),
                        value_kind: BasisKind::Known,
                    },
                },
            ];
            let ledger = Ledger::replay(&events);
            let scope = one("acct");
            let totals = ledger.totals_for(&scope);
            let p = summarize_position(
                ledger.lots_for(&scope, received),
                Some(&d(&case["received_current_price_usd"])),
            );
            check(
                id,
                "realized_usd",
                &e["realized_usd"],
                totals.realized_usd.value().as_ref(),
                None,
            );
            check(
                id,
                "new_lot_basis_usd",
                &e["new_lot_basis_usd"],
                p.basis_usd.as_ref(),
                None,
            );
            check(
                id,
                "new_lot_value_usd",
                &e["new_lot_value_usd"],
                p.value_usd.as_ref(),
                None,
            );
            check(
                id,
                "new_lot_unrealized_usd",
                &e["new_lot_unrealized_usd"],
                p.unrealized_usd.as_ref(),
                None,
            );
            let flows = scope_external_flows(&events, &scope);
            let sum: Dec = flows.flows.iter().map(|f| f.amount_usd.clone()).sum();
            check(
                id,
                "portfolio_external_flow_usd",
                &e["portfolio_external_flow_usd"],
                Some(&sum),
                None,
            );
            assert!(
                ledger.lots_for(&scope, disposed).next().is_none(),
                "{id}: disposed asset fully consumed"
            );
            let total = total_accounted_pnl(p.unrealized_usd.as_ref(), &totals);
            check(
                id,
                "total_accounted_pnl_usd",
                &e["total_accounted_pnl_usd"],
                total.as_ref(),
                None,
            );
        }
        "reward" => {
            let qty = d(&case["quantity"]);
            let events = vec![Event {
                id: "reward".into(),
                order: order(0),
                kind: EventKind::Reward {
                    account: "acct".into(),
                    asset: "X".into(),
                    value_usd: Some(&qty * d(&case["receipt_price_usd"])),
                    quantity: qty,
                },
            }];
            let ledger = Ledger::replay(&events);
            let scope = one("acct");
            let totals = ledger.totals_for(&scope);
            let p = summarize_position(
                ledger.lots_for(&scope, "X"),
                Some(&d(&case["current_price_usd"])),
            );
            check(
                id,
                "income_usd",
                &e["income_usd"],
                totals.income_usd.value().as_ref(),
                None,
            );
            check(id, "basis_usd", &e["basis_usd"], p.basis_usd.as_ref(), None);
            check(
                id,
                "current_value_usd",
                &e["current_value_usd"],
                p.value_usd.as_ref(),
                None,
            );
            check(
                id,
                "unrealized_usd",
                &e["unrealized_usd"],
                p.unrealized_usd.as_ref(),
                None,
            );
            let total = total_accounted_pnl(p.unrealized_usd.as_ref(), &totals);
            check(
                id,
                "total_accounted_pnl_usd",
                &e["total_accounted_pnl_usd"],
                total.as_ref(),
                None,
            );
            let flows = scope_external_flows(&events, &scope);
            let sum: Dec = flows.flows.iter().map(|f| f.amount_usd.clone()).sum();
            check(
                id,
                "external_capital_flow_usd",
                &e["external_capital_flow_usd"],
                Some(&sum),
                None,
            );
        }
        other => panic!(
            "{id}: unhandled fixture kind {other:?}; add support before accepting the fixture"
        ),
    }
}

#[test]
fn all_fixture_cases_pass() {
    let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(fixture["schema_version"], 1);
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(
        cases.len(),
        14,
        "fixture case count changed; review new cases"
    );
    for case in cases {
        run_case(case);
    }
}
