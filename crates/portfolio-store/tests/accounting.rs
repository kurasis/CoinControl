//! Accounting replay over stored chain evidence (stage C).
//!
//! Expected values are worked out by hand in the comments, independently of
//! the code under test (TESTING.md §3).

use std::sync::Arc;

use num_bigint::BigInt;
use portfolio_core::accounting::{BasisKind, UnavailableReason};
use portfolio_core::clock::FixedClock;
use portfolio_core::decimal::{Dec, parse_dec, quantity_to_raw};
use portfolio_core::network::NetworkId;
use portfolio_store::ingest::{
    AssetSpec, Checkpoint, Coverage, Decoding, Direction, FeeAttribution, FeeSpec, LegSpec,
    PriceSpec, TxSpec, TxStatus, Verification,
};
use portfolio_store::*;

const NOW: i64 = 1_790_000_000;
const DAY: i64 = 86_400;
const T0: i64 = NOW - 100 * DAY;
const T1: i64 = NOW - 50 * DAY;
const T2: i64 = NOW - 10 * DAY;

const ADDR_A: &str = "0x5aaeb6053f3e94c9b9a09f33669435e7ef1beaed";
const ADDR_B: &str = "0xfb6916095ca1df60bb79ce92ce3ea74c37c5d359";
const USDC: &str = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";

fn d(s: &str) -> Dec {
    parse_dec(s).unwrap()
}

fn eth() -> AssetSpec {
    AssetSpec::native(NetworkId::Ethereum, "zerion")
}

fn usdc() -> AssetSpec {
    AssetSpec {
        network: NetworkId::Ethereum,
        contract: Some(USDC.into()),
        decimals: 6,
        symbol: Some("USDC".into()),
        name: Some("USD Coin".into()),
        verification: Verification::Verified,
        provider: "zerion",
    }
}

fn raw(asset: &AssetSpec, q: &str) -> BigInt {
    let neg = q.starts_with('-');
    let r = quantity_to_raw(&d(q.trim_start_matches('-')), asset.decimals).unwrap();
    if neg { -r } else { r }
}

fn leg(asset: AssetSpec, q: &str, op: &str) -> LegSpec {
    let signed = raw(&asset, q);
    LegSpec {
        counterparty: if op == "send" {
            Some(ADDR_B.into())
        } else if op == "receive" {
            Some(ADDR_A.into())
        } else {
            None
        },
        direction: if q.starts_with('-') {
            Direction::Out
        } else {
            Direction::In
        },
        signed_raw: signed,
        asset,
        leg_type: op.into(),
        decoding: Decoding::Interpreted,
        unresolved: !q.starts_with('-'),
    }
}

fn tx(hash: &str, at: i64, op: &str, legs: Vec<LegSpec>, fee: Option<&str>) -> TxSpec {
    TxSpec {
        network: NetworkId::Ethereum,
        part: None,
        hash: hash.into(),
        block_height: Some(1),
        position: None,
        occurred_at: at,
        status: TxStatus::Confirmed,
        provider: "zerion",
        operation: op.into(),
        legs,
        fee: fee.map(|f| FeeSpec {
            asset: eth(),
            raw: raw(&eth(), f),
            attribution: FeeAttribution::Exact,
        }),
        decoding: Decoding::Interpreted,
        evidence: serde_json::json!({}),
    }
}

async fn price(store: &Store, asset: &str, at: i64, p: &str, granularity: &'static str) {
    store
        .insert_price(&PriceSpec {
            asset_id: asset.into(),
            provider: "test",
            price_usd: p.into(),
            requested_at: at,
            observed_at: at,
            granularity,
            quality: if granularity == "day" {
                "estimated"
            } else {
                "current"
            },
            change_24h_percent: None,
        })
        .await
        .unwrap();
}

async fn balance(store: &Store, account: &str, asset: &AssetSpec, q: &str) {
    store
        .record_balance(account, asset, &raw(asset, q), None, "fresh")
        .await
        .unwrap();
}

async fn complete_history(store: &Store, account: &str) {
    store
        .save_checkpoint(
            account,
            "zerion",
            "history",
            &Checkpoint {
                coverage: Coverage::Complete,
                ..Checkpoint::default()
            },
        )
        .await
        .unwrap();
}

struct World {
    store: Store,
    a: String,
    b: String,
    wallet: String,
}

fn leg_id(hash: &str, account: &str, index: usize) -> String {
    format!("ethereum:{hash}:{account}:{index}")
}

/// Two owned Ethereum accounts:
/// - T0: A receives 2 ETH from outside.
/// - T1: A sends 1 ETH to B (one transaction seen by both), A pays a 0.01 ETH fee.
/// - T2: B sends 0.5 ETH to an untracked address.
///
/// ETH = 2000 at T0, 3000 at T1, 2500 at T2, 3000 now.
async fn world() -> World {
    let store = Store::open_in_memory(ProfileKind::Test, Arc::new(FixedClock(NOW)))
        .await
        .unwrap();
    let w = store.create_wallet("Main").await.unwrap();
    let a = store
        .add_account(&w.id, NetworkId::Ethereum, ADDR_A, None)
        .await
        .unwrap()
        .id;
    let b = store
        .add_account(&w.id, NetworkId::Ethereum, ADDR_B, None)
        .await
        .unwrap()
        .id;
    store
        .ingest_transaction(
            &a,
            &tx(
                "0x01",
                T0,
                "receive",
                vec![leg(eth(), "2", "receive")],
                None,
            ),
        )
        .await
        .unwrap();
    store
        .ingest_transaction(
            &a,
            &tx(
                "0x02",
                T1,
                "send",
                vec![leg(eth(), "-1", "send")],
                Some("0.01"),
            ),
        )
        .await
        .unwrap();
    store
        .ingest_transaction(
            &b,
            &tx(
                "0x02",
                T1,
                "receive",
                vec![leg(eth(), "1", "receive")],
                None,
            ),
        )
        .await
        .unwrap();
    store
        .ingest_transaction(
            &b,
            &tx("0x03", T2, "send", vec![leg(eth(), "-0.5", "send")], None),
        )
        .await
        .unwrap();
    let eth_id = eth().id();
    price(&store, &eth_id, T0, "2000", "day").await;
    price(&store, &eth_id, T1, "3000", "day").await;
    price(&store, &eth_id, T2, "2500", "day").await;
    price(&store, &eth_id, NOW, "3000", "tick").await;
    balance(&store, &a, &eth(), "0.99").await;
    balance(&store, &b, &eth(), "0.5").await;
    complete_history(&store, &a).await;
    complete_history(&store, &b).await;
    World {
        store,
        a,
        b,
        wallet: w.id,
    }
}

fn basis(q: &str, usd: Option<&str>, at: i64) -> BasisLotInput {
    BasisLotInput {
        quantity: q.into(),
        basis_usd: usd.map(Into::into),
        basis_kind: if usd.is_some() {
            BasisKind::Known
        } else {
            BasisKind::Unknown
        },
        acquired_at: at,
    }
}

async fn classify_world(w: &World) {
    w.store
        .save_leg_override(
            &leg_id("0x01", &w.a, 0),
            &LegOverride {
                basis_lots: Some(vec![basis("2", Some("4000"), T0 - 10 * DAY)]),
                ..LegOverride::default()
            },
        )
        .await
        .unwrap();
    w.store
        .save_leg_override(
            &leg_id("0x03", &w.b, 0),
            &LegOverride {
                classification: Some(LegClassification::Sale),
                proceeds_usd: Some("1300".into()),
                ..LegOverride::default()
            },
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn unreviewed_history_keeps_basis_unknown_and_lists_review_items() {
    let w = world().await;
    let report = w.store.replay_accounting().await.unwrap();
    // Unknown-basis receipt at T0 and the unclassified send at T2.
    assert_eq!(report.review_items, 2);
    let review = w.store.list_review_items(&Scope::All, None).await.unwrap();
    let reasons: Vec<_> = review.items.iter().map(|i| i.reason.as_str()).collect();
    assert!(reasons.contains(&"unknown_basis"));
    assert!(reasons.contains(&"unclassified_outgoing"));
    // The own transfer is recognized from shared transaction evidence, not reviewed.
    assert!(
        !review
            .items
            .iter()
            .any(|i| i.transaction_id.ends_with("0x02"))
    );

    let s = w.store.portfolio_summary(&Scope::All).await.unwrap();
    assert_eq!(s.total_value_usd.as_deref(), Some("4470"));
    assert_eq!(
        s.accounting.unrealized_pnl_usd, None,
        "never zero for unknown basis"
    );
    assert_eq!(
        s.accounting.unrealized_reason,
        Some(UnavailableReason::MissingBasis)
    );
    // Realized is incomplete while the T2 send is unclassified; the fee realization is known.
    assert!(!s.accounting.realized.complete);
    assert_eq!(s.accounting.expenses.known_usd, "30");
    assert_eq!(s.accounting.total_accounted_pnl_usd, None);
}

#[tokio::test]
async fn reviewed_history_produces_the_hand_computed_figures() {
    let w = world().await;
    classify_world(&w).await;
    let s = w
        .store
        .portfolio_summary(&Scope::All)
        .await
        .unwrap()
        .accounting;
    // A: 2 ETH @ $4000 basis; sends 1 ETH (basis 2000) to B; fee 0.01 ETH
    //    consumes basis 20 and is worth 30 -> fee realized 10, expense 30.
    //    A keeps 0.99 ETH with basis 1980.
    // B: 1 ETH basis 2000; sells 0.5 for 1300 -> realized 1300 - 1000 = 300; keeps 0.5 / 1000.
    // Now ETH = 3000: value 1.49 * 3000 = 4470, basis 2980, unrealized 1490 (50 %).
    assert_eq!(s.unrealized_pnl_usd.as_deref(), Some("1490"));
    assert_eq!(s.unrealized_return_percent.as_deref(), Some("50"));
    assert_eq!(s.remaining_basis_usd.as_deref(), Some("2980"));
    assert_eq!(s.realized.known_usd, "310");
    assert!(s.realized.complete);
    assert_eq!(s.income.known_usd, "0");
    assert_eq!(s.expenses.known_usd, "30");
    assert_eq!(s.fee_charges, 1, "the fee is charged exactly once");
    // 1490 + 310 + 0 - 30
    assert_eq!(s.total_accounted_pnl_usd.as_deref(), Some("1770"));
    assert_eq!(s.review_count, 0);
    assert_eq!(
        s.reconciliation_count, 0,
        "replayed inventory matches balances"
    );
    assert_eq!(s.basis_coverage_percent.as_deref(), Some("100"));

    // B alone: 0.5 ETH worth 1500 with basis 1000; realized 300; the fee was A's.
    let b = w
        .store
        .portfolio_summary(&Scope::Accounts {
            ids: vec![w.b.clone()],
        })
        .await
        .unwrap()
        .accounting;
    assert_eq!(b.unrealized_pnl_usd.as_deref(), Some("500"));
    assert_eq!(b.realized.known_usd, "300");
    assert_eq!(b.expenses.known_usd, "0");

    // A wallet containing both accounts equals the portfolio: nothing double counted.
    let wallet = w
        .store
        .portfolio_summary(&Scope::Wallet {
            id: w.wallet.clone(),
        })
        .await
        .unwrap()
        .accounting;
    assert_eq!(wallet, s);
}

#[tokio::test]
async fn own_transfer_preserves_acquisition_date_and_lineage() {
    let w = world().await;
    classify_world(&w).await;
    let detail = w
        .store
        .asset_detail(&Scope::All, &eth().id())
        .await
        .unwrap();
    let b_lot = detail
        .lots
        .iter()
        .find(|l| l.account_id == w.b)
        .expect("B holds a lot");
    assert_eq!(
        b_lot.acquired_at,
        T0 - 10 * DAY,
        "original acquisition date"
    );
    assert_eq!(b_lot.arrived_at, T1, "arrival in B");
    assert!(b_lot.parent_lot_id.is_some(), "lineage to A's lot");
    assert_eq!(b_lot.remaining_quantity, "0.5");
    assert_eq!(b_lot.remaining_basis_usd.as_deref(), Some("1000"));
    assert_eq!(detail.realized.known_usd, "310");
    assert_eq!(detail.unrealized_pnl_usd.as_deref(), Some("1490"));
}

#[tokio::test]
async fn replay_is_deterministic_and_reingestion_has_no_economic_effect() {
    let w = world().await;
    classify_world(&w).await;
    let first = w.store.portfolio_summary(&Scope::All).await.unwrap();
    let lots1 = w
        .store
        .asset_detail(&Scope::All, &eth().id())
        .await
        .unwrap()
        .lots;
    // Same pages again, then two more replays.
    w.store
        .ingest_transaction(
            &w.a,
            &tx(
                "0x02",
                T1,
                "send",
                vec![leg(eth(), "-1", "send")],
                Some("0.01"),
            ),
        )
        .await
        .unwrap();
    w.store.replay_accounting().await.unwrap();
    w.store.replay_accounting().await.unwrap();
    assert_eq!(w.store.portfolio_summary(&Scope::All).await.unwrap(), first);
    assert_eq!(
        w.store
            .asset_detail(&Scope::All, &eth().id())
            .await
            .unwrap()
            .lots,
        lots1
    );
}

#[tokio::test]
async fn overrides_are_versioned_and_the_original_observation_is_kept() {
    let w = world().await;
    let id = leg_id("0x01", &w.a, 0);
    for usd in ["4000", "3900"] {
        w.store
            .save_leg_override(
                &id,
                &LegOverride {
                    basis_lots: Some(vec![basis("2", Some(usd), T0)]),
                    ..LegOverride::default()
                },
            )
            .await
            .unwrap();
    }
    let detail = w.store.leg_detail(&id).await.unwrap();
    assert_eq!(detail.history.len(), 2);
    assert_eq!(
        detail.history[0].payload.basis_lots.as_ref().unwrap()[0]
            .basis_usd
            .as_deref(),
        Some("4000")
    );
    assert_eq!(
        detail.basis_usd.as_deref(),
        Some("3900"),
        "latest version applies"
    );
    assert_eq!(detail.quantity, "2", "evidence unchanged");

    // Reverting to the default interpretation is a new version, not a deletion.
    w.store
        .save_leg_override(&id, &LegOverride::default())
        .await
        .unwrap();
    let detail = w.store.leg_detail(&id).await.unwrap();
    assert_eq!(detail.history.len(), 3);
    assert!(detail.current.is_none());
    assert_eq!(detail.review.as_deref(), Some("unknown_basis"));
}

#[tokio::test]
async fn invalid_decisions_are_rejected() {
    let w = world().await;
    let receipt = leg_id("0x01", &w.a, 0);
    // Lots above the receipt's original quantity.
    let err = w
        .store
        .save_leg_override(
            &receipt,
            &LegOverride {
                basis_lots: Some(vec![basis("1.5", Some("1"), T0), basis("1", Some("1"), T0)]),
                ..LegOverride::default()
            },
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("lots total 2.5"));
    // A lot acquired after it arrived.
    assert!(
        w.store
            .save_leg_override(
                &receipt,
                &LegOverride {
                    basis_lots: Some(vec![basis("1", Some("1"), T0 + DAY)]),
                    ..LegOverride::default()
                },
            )
            .await
            .is_err()
    );
    // An incoming movement cannot be a sale.
    assert!(
        w.store
            .save_leg_override(
                &receipt,
                &LegOverride {
                    classification: Some(LegClassification::Sale),
                    ..LegOverride::default()
                },
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn group_scope_counts_transfers_as_flows_but_portfolio_does_not() {
    let w = world().await;
    classify_world(&w).await;
    let all = w
        .store
        .get_chart(&Scope::All, ChartRange::All)
        .await
        .unwrap();
    let perf = all.performance.expect("valued points exist");
    // Portfolio: the deposit at T0 is in the beginning value; the internal
    // transfer cancels; only the sale's proceeds leave the scope.
    assert_eq!(perf.flow_count, 1);
    assert_eq!(perf.net_flows_usd, "-1300");
    let gain = d(perf.gain_usd.as_deref().unwrap());
    assert_eq!(
        gain,
        d(&perf.ending_value_usd) - d(&perf.beginning_value_usd) - d("-1300")
    );

    let b = w
        .store
        .get_chart(
            &Scope::Accounts {
                ids: vec![w.b.clone()],
            },
            ChartRange::All,
        )
        .await
        .unwrap()
        .performance
        .unwrap();
    // B: the transfer from A at T1 is an inflow at market value (1 ETH * 3000),
    // the sale an outflow of its proceeds; inherited basis is irrelevant here.
    assert_eq!(b.flow_count, 1, "the T1 transfer is B's first valued point");
    assert_eq!(b.net_flows_usd, "-1300");
}

#[tokio::test]
async fn unclassified_outflow_makes_period_return_unavailable() {
    let w = world().await;
    w.store.replay_accounting().await.unwrap();
    let perf = w
        .store
        .get_chart(&Scope::All, ChartRange::All)
        .await
        .unwrap()
        .performance
        .unwrap();
    assert_eq!(
        perf.reason,
        Some(UnavailableReason::IncompleteClassification)
    );
    assert!(perf.return_percent.is_none() && perf.gain_usd.is_none());
}

#[tokio::test]
async fn swap_assigns_one_gross_value_to_both_sides() {
    let w = world().await;
    classify_world(&w).await;
    // A swaps 0.5 ETH for 1500 USDC at T2 (ETH 2500 -> gross 1250) and pays no fee.
    w.store
        .ingest_transaction(
            &w.a,
            &tx(
                "0x04",
                T2 + 60,
                "trade",
                vec![leg(eth(), "-0.5", "trade"), leg(usdc(), "1500", "trade")],
                None,
            ),
        )
        .await
        .unwrap();
    balance(&w.store, &w.a, &eth(), "0.49").await;
    balance(&w.store, &w.a, &usdc(), "1500").await;
    price(&w.store, &usdc().id(), NOW, "1", "tick").await;
    w.store.replay_accounting().await.unwrap();
    let s = w
        .store
        .portfolio_summary(&Scope::All)
        .await
        .unwrap()
        .accounting;
    // A's 0.5 ETH had basis 1980 * 0.5 / 0.99 = 1000 -> realized 1250 - 1000 = 250.
    // Total realized 10 + 300 + 250 = 560.
    assert_eq!(s.realized.known_usd, "560");
    let usdc_detail = w
        .store
        .asset_detail(&Scope::All, &usdc().id())
        .await
        .unwrap();
    assert_eq!(usdc_detail.remaining_basis_usd.as_deref(), Some("1250"));
    assert!(
        usdc_detail.has_estimated_basis,
        "daily-quote valuation is an estimate"
    );
    // 1500 USDC now worth 1500 -> unrealized 250 on the stablecoin leg (not forced to $1 basis).
    assert_eq!(usdc_detail.unrealized_pnl_usd.as_deref(), Some("250"));
}

#[tokio::test]
async fn manual_pairing_moves_basis_and_expenses_the_difference() {
    let w = world().await;
    classify_world(&w).await;
    // A sends 0.4 ETH to an exchange at T2+1h; 0.39 ETH arrives at B a day later.
    w.store
        .ingest_transaction(
            &w.a,
            &tx(
                "0x05",
                T2 + 3_600,
                "send",
                vec![leg(eth(), "-0.4", "send")],
                None,
            ),
        )
        .await
        .unwrap();
    w.store
        .ingest_transaction(
            &w.b,
            &tx(
                "0x06",
                T2 + DAY,
                "receive",
                vec![leg(eth(), "0.39", "receive")],
                None,
            ),
        )
        .await
        .unwrap();
    balance(&w.store, &w.a, &eth(), "0.59").await;
    balance(&w.store, &w.b, &eth(), "0.89").await;
    w.store.replay_accounting().await.unwrap();
    let out = leg_id("0x05", &w.a, 0);
    let detail = w.store.leg_detail(&out).await.unwrap();
    let candidate = detail
        .pair_candidates
        .iter()
        .find(|c| c.account_id == w.b)
        .expect("B's receipt is offered");
    w.store
        .save_leg_override(
            &out,
            &LegOverride {
                pair_with: Some(candidate.leg_id.clone()),
                ..LegOverride::default()
            },
        )
        .await
        .unwrap();
    let s = w
        .store
        .portfolio_summary(&Scope::All)
        .await
        .unwrap()
        .accounting;
    assert_eq!(s.review_count, 0);
    // The 0.01 ETH kept by the intermediary: worth 0.01 * 2500 = 25, basis 20 -> realized 5.
    assert_eq!(s.expenses.known_usd, "55");
    assert_eq!(s.realized.known_usd, "315");
    // Remaining basis is conserved apart from the consumed fee lots: 2980 - 20.
    assert_eq!(s.remaining_basis_usd.as_deref(), Some("2960"));
    // A receipt can be the destination of only one pairing.
    let again = w
        .store
        .save_leg_override(
            &leg_id("0x02", &w.a, 0),
            &LegOverride {
                pair_with: Some(candidate.leg_id.clone()),
                ..LegOverride::default()
            },
        )
        .await;
    assert!(again.is_err());
}

#[tokio::test]
async fn overspending_and_balance_mismatch_become_reconciliation_items() {
    let w = world().await;
    classify_world(&w).await;
    // B sends 1 ETH more than its known history holds.
    w.store
        .ingest_transaction(
            &w.b,
            &tx(
                "0x07",
                NOW - 60,
                "send",
                vec![leg(eth(), "-1", "send")],
                None,
            ),
        )
        .await
        .unwrap();
    w.store.replay_accounting().await.unwrap();
    let review = w.store.list_review_items(&Scope::All, None).await.unwrap();
    let kinds: Vec<_> = review
        .reconciliation
        .iter()
        .map(|r| r.kind.as_str())
        .collect();
    assert!(kinds.contains(&"inventory_gap"), "{kinds:?}");
    let gap = review
        .reconciliation
        .iter()
        .find(|r| r.kind == "inventory_gap")
        .unwrap();
    assert_eq!(gap.quantity.as_deref(), Some("0.5"));
    // The provider still reports 0.5 ETH at B, the ledger holds none.
    let mismatch = review
        .reconciliation
        .iter()
        .find(|r| r.kind == "balance_mismatch")
        .expect("mismatch reported");
    assert_eq!(mismatch.quantity.as_deref(), Some("0.5"));
    assert!(mismatch.detail.contains("differs"), "{}", mismatch.detail);
    // No hidden negative lot, no fabricated purchase: B's position has unknown basis.
    let b = w
        .store
        .portfolio_summary(&Scope::Accounts {
            ids: vec![w.b.clone()],
        })
        .await
        .unwrap();
    assert_eq!(b.accounting.unrealized_pnl_usd, None);
    assert_eq!(b.total_value_usd.as_deref(), Some("1500"));
}

#[tokio::test]
async fn historical_chart_uses_historical_quantity_not_todays() {
    let store = Store::open_in_memory(ProfileKind::Test, Arc::new(FixedClock(NOW)))
        .await
        .unwrap();
    let wallet = store.create_wallet("W").await.unwrap();
    let a = store
        .add_account(&wallet.id, NetworkId::Ethereum, ADDR_A, None)
        .await
        .unwrap()
        .id;
    store
        .ingest_transaction(
            &a,
            &tx(
                "0x10",
                T0,
                "receive",
                vec![leg(eth(), "1", "receive")],
                None,
            ),
        )
        .await
        .unwrap();
    store
        .ingest_transaction(
            &a,
            &tx(
                "0x11",
                T1,
                "receive",
                vec![leg(eth(), "1", "receive")],
                None,
            ),
        )
        .await
        .unwrap();
    balance(&store, &a, &eth(), "2").await;
    for day in 0..=120 {
        let at = NOW - NOW.rem_euclid(DAY) - day * DAY;
        price(&store, &eth().id(), at, "2000", "day").await;
    }
    let chart = store
        .get_chart(&Scope::All, ChartRange::Year)
        .await
        .unwrap();
    for p in &chart.points {
        let expected = if p.t < T0 {
            None
        } else if p.t < T1 {
            Some("2000")
        } else {
            Some("4000")
        };
        assert_eq!(p.value_usd.as_deref(), expected, "at {}", p.t);
    }
    assert_eq!(chart.history_available_since, Some(T0));
    // Asset chart: the market price series covers dates before the holding started.
    let asset = store
        .asset_chart(&Scope::All, &eth().id(), ChartRange::Year)
        .await
        .unwrap();
    let before = asset
        .price
        .iter()
        .find(|p| p.t < T0 && p.t > NOW - 120 * DAY)
        .unwrap();
    assert_eq!(before.price_usd.as_deref(), Some("2000"));
    assert!(before.estimated);
    let held_before = asset
        .holdings
        .points
        .iter()
        .find(|p| p.t == before.t)
        .unwrap();
    assert_eq!(held_before.value_usd, None);
}

#[tokio::test]
async fn csv_import_previews_validates_and_commits_transactionally() {
    let w = world().await;
    w.store.replay_accounting().await.unwrap();
    let csv = format!(
        "external_row_id,network_id,account_address,transaction_id,leg_id,asset_identifier,quantity,acquired_at_utc,total_basis_usd,basis_kind,classification,note\n\
         r1,ethereum,{ADDR_A},0x01,,native,1.5,2023-01-02T00:00:00Z,2400.50,known,,bought on exchange\n\
         r2,ethereum,{ADDR_A},0x01,,native,0.5,2023-02-01T00:00:00+02:00,,unknown,,\n\
         r3,ethereum,{ADDR_B},0x03,,native,,,,,sale,\n\
         r4,ethereum,0x0000000000000000000000000000000000000001,0x01,,native,1,2023-01-01T00:00:00Z,1,known,,\n\
         r4,ethereum,{ADDR_A},0x01,,native,1,2023-01-01T00:00:00,1,known,,\n"
    );
    let preview = w
        .store
        .preview_basis_import("basis.csv", &csv, None)
        .await
        .unwrap();
    assert_eq!(preview.ok_count, 3);
    assert_eq!(preview.error_count, 2, "{:#?}", preview.rows);
    assert!(!preview.can_commit, "errors block the commit");
    let r4 = &preview.rows[3];
    assert!(r4.messages.iter().any(|m| m.contains("not tracked")));
    let r5 = &preview.rows[4];
    assert!(r5.messages.iter().any(|m| m.contains("more than once")));
    assert!(r5.messages.iter().any(|m| m.contains("timezone")));
    // Nothing was applied by the preview.
    assert_eq!(
        w.store
            .portfolio_summary(&Scope::All)
            .await
            .unwrap()
            .accounting
            .review_count,
        2
    );
    assert!(
        w.store
            .commit_basis_import(&preview.batch_id)
            .await
            .is_err()
    );

    // Fixed file: rows r1-r3 only.
    let fixed: String = csv.lines().take(4).map(|l| format!("{l}\n")).collect();
    let preview = w
        .store
        .preview_basis_import("basis.csv", &fixed, None)
        .await
        .unwrap();
    assert!(preview.can_commit, "{:#?}", preview.rows);
    assert_eq!(preview.before.review_items, 2);
    // After: the receipt is partially known (0.5 ETH unknown) and the sale lacks proceeds.
    assert_eq!(preview.after.review_items, 2);
    let result = w
        .store
        .commit_basis_import(&preview.batch_id)
        .await
        .unwrap();
    assert_eq!(result.applied_rows, 3);
    let review = w.store.list_review_items(&Scope::All, None).await.unwrap();
    let reasons: Vec<_> = review.items.iter().map(|i| i.reason.as_str()).collect();
    assert_eq!(reasons, vec!["missing_proceeds", "unknown_basis"]);
    let lots = w
        .store
        .asset_detail(&Scope::All, &eth().id())
        .await
        .unwrap()
        .lots;
    // FIFO: the 2023-01-02 lot (1.5 ETH, $2400.50) went first to B (1 ETH) and
    // A's fee; the earliest remaining lot keeps its acquisition date.
    assert!(lots.iter().any(|l| l.acquired_at == 1_672_617_600));

    // The same file again is detected as a duplicate.
    let again = w
        .store
        .preview_basis_import("copy.csv", &fixed, None)
        .await
        .unwrap();
    assert!(again.duplicate_file);
    assert_eq!(again.duplicate_count, 3);
    assert!(!again.can_commit);
    // A committed batch cannot be committed twice.
    assert!(
        w.store
            .commit_basis_import(&preview.batch_id)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn csv_opening_lot_replaces_history_before_its_cutoff() {
    let w = world().await;
    let csv = format!(
        "external_row_id,network_id,account_address,transaction_id,asset_identifier,quantity,acquired_at_utc,total_basis_usd,basis_kind,classification,opening_cutoff_utc\n\
         o1,ethereum,{ADDR_A},,native,2,2020-01-01T00:00:00Z,500,known,opening,2026-09-30T00:00:00Z\n"
    );
    let preview = w
        .store
        .preview_basis_import("opening.csv", &csv, None)
        .await
        .unwrap();
    assert!(preview.can_commit, "{:#?}", preview.rows);
    assert!(preview.rows[0].opening);
    w.store
        .commit_basis_import(&preview.batch_id)
        .await
        .unwrap();
    let review = w.store.list_review_items(&Scope::All, None).await.unwrap();
    // The T0 receipt predates the cutoff and is replaced by the opening lot.
    assert!(
        review
            .reconciliation
            .iter()
            .any(|r| r.kind == "opening_overlap")
    );
    // A's own receipt is covered by the opening lot. B's receipt from A came
    // from history the opening lot replaces, so its basis is asked for.
    let unknown: Vec<_> = review
        .items
        .iter()
        .filter(|i| i.reason == "unknown_basis")
        .map(|i| i.transaction_id.as_str())
        .collect();
    assert_eq!(unknown, vec!["ethereum:0x02"], "{:#?}", review.items);
}

#[tokio::test]
async fn activity_rows_carry_historical_value_and_review_state() {
    let w = world().await;
    w.store.replay_accounting().await.unwrap();
    let page = w
        .store
        .list_activity(
            &Scope::All,
            &ActivityFilter {
                asset_id: None,
                unresolved_only: true,
                ..Default::default()
            },
            None,
            50,
        )
        .await
        .unwrap();
    // Only the receipt at T0 and the send at T2 need review.
    assert_eq!(page.rows.len(), 2);
    let receipt = page
        .rows
        .iter()
        .find(|r| r.transaction_id.ends_with("0x01"))
        .unwrap();
    assert_eq!(receipt.legs[0].value_usd.as_deref(), Some("4000"));
    assert_eq!(receipt.legs[0].treatment.as_deref(), Some("deposit"));
    let all = w
        .store
        .list_activity(&Scope::All, &ActivityFilter::default(), None, 50)
        .await
        .unwrap();
    let send = all
        .rows
        .iter()
        .find(|r| r.transaction_id.ends_with("0x02") && r.account_id == w.a)
        .unwrap();
    assert_eq!(send.fee_value_usd.as_deref(), Some("30"));
    assert_eq!(send.legs[0].treatment.as_deref(), Some("own_transfer_out"));
    assert!(!send.unresolved);
}

#[tokio::test]
async fn demo_profile_shows_known_estimated_and_missing_basis() {
    let store = Store::open_in_memory(ProfileKind::Demo, Arc::new(FixedClock(NOW)))
        .await
        .unwrap();
    store.seed_demo().await.unwrap();
    assert!(store.accounting_dirty().await.unwrap());
    store.replay_if_dirty().await.unwrap().unwrap();
    let a = store
        .portfolio_summary(&Scope::All)
        .await
        .unwrap()
        .accounting;
    // One receipt is left for review, so the total stays partial.
    assert!(a.review_count >= 1);
    assert_eq!(a.unrealized_pnl_usd, None);
    assert!(a.has_estimated_basis);
    assert_ne!(a.known_subset_basis_usd, "0");
    // The ETH sale is valued at market and its FIFO basis is known.
    assert!(a.realized.complete);
    assert_ne!(a.realized.known_usd, "0");
    assert!(a.expenses.complete && a.fee_charges >= 1);
    let review = store.list_review_items(&Scope::All, None).await.unwrap();
    assert!(review.items.iter().any(|i| i.reason == "unknown_basis"));
    assert!(
        !review
            .items
            .iter()
            .any(|i| i.reason == "unclassified_outgoing")
    );
}

#[tokio::test]
async fn movement_identity_survives_reordering_and_never_recycles() {
    let w = world().await;
    let mut record = tx(
        "0xidentity",
        T0,
        "receive",
        vec![leg(eth(), "1", "receive"), leg(usdc(), "5", "receive")],
        None,
    );
    w.store.ingest_transaction(&w.a, &record).await.unwrap();
    let original = leg_id("0xidentity", &w.a, 0);
    w.store
        .save_leg_override(
            &original,
            &LegOverride {
                basis_lots: Some(vec![basis("1", Some("123"), T0)]),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    record.legs.reverse();
    w.store.ingest_transaction(&w.a, &record).await.unwrap();
    w.store.replay_accounting().await.unwrap();
    let detail = w.store.leg_detail(&original).await.unwrap();
    assert_eq!(detail.asset_id, eth().id());
    assert_eq!(detail.basis_usd.as_deref(), Some("123"));
    record.legs.retain(|l| l.asset.id() != eth().id());
    w.store.ingest_transaction(&w.a, &record).await.unwrap();
    record.legs.push(leg(eth(), "2", "receive"));
    w.store.ingest_transaction(&w.a, &record).await.unwrap();
    w.store.replay_accounting().await.unwrap();
    assert!(w.store.leg_detail(&original).await.is_err());
    assert!(
        w.store
            .list_review_items(&Scope::All, None)
            .await
            .unwrap()
            .reconciliation
            .iter()
            .any(|r| r.kind == "orphaned_override")
    );
}

#[tokio::test]
async fn chain_position_orders_a_receipt_before_a_same_second_sale() {
    let store = Store::open_in_memory(ProfileKind::Test, Arc::new(FixedClock(NOW)))
        .await
        .unwrap();
    let wallet = store.create_wallet("Order").await.unwrap();
    let account = store
        .add_account(&wallet.id, NetworkId::Ethereum, ADDR_A, None)
        .await
        .unwrap();
    // Lexical hash order would put the sale first. Chain index must win.
    let mut buy = tx(
        "0xffff",
        T0,
        "receive",
        vec![leg(eth(), "1", "receive")],
        None,
    );
    buy.position = Some("2".into());
    let mut sale = tx("0x0000", T0, "send", vec![leg(eth(), "-1", "send")], None);
    sale.position = Some("10".into());
    store.ingest_transaction(&account.id, &sale).await.unwrap();
    store.ingest_transaction(&account.id, &buy).await.unwrap();
    store
        .save_leg_override(
            &leg_id("0xffff", &account.id, 0),
            &LegOverride {
                basis_lots: Some(vec![basis("1", Some("100"), T0)]),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    store
        .save_leg_override(
            &leg_id("0x0000", &account.id, 0),
            &LegOverride {
                classification: Some(LegClassification::Sale),
                proceeds_usd: Some("150".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let report = store.replay_accounting().await.unwrap();
    assert_eq!(report.reconciliation_items, 0);
    let summary = store.portfolio_summary(&Scope::All).await.unwrap();
    assert_eq!(summary.accounting.realized.known_usd, "50");
}

#[tokio::test]
async fn hidden_assets_keep_totals_but_accounting_exclusion_is_reversible() {
    let w = world().await;
    w.store.replay_accounting().await.unwrap();
    let before = w.store.portfolio_summary(&Scope::All).await.unwrap();
    w.store
        .set_asset_policy("ethereum:native", true, None)
        .await
        .unwrap();
    w.store.replay_accounting().await.unwrap();
    assert_eq!(
        w.store.portfolio_summary(&Scope::All).await.unwrap(),
        before
    );
    assert!(
        w.store
            .list_asset_policies()
            .await
            .unwrap()
            .iter()
            .any(|p| p.asset_id == "ethereum:native" && p.hidden)
    );
    w.store
        .set_asset_policy("ethereum:native", false, Some(true))
        .await
        .unwrap();
    w.store.replay_accounting().await.unwrap();
    assert!(w.store.list_holdings(&Scope::All).await.unwrap().is_empty());
    assert!(
        !w.store
            .list_activity(&Scope::All, &Default::default(), None, 200)
            .await
            .unwrap()
            .rows
            .is_empty()
    );
    w.store
        .set_asset_policy("ethereum:native", false, None)
        .await
        .unwrap();
    w.store.replay_accounting().await.unwrap();
    assert_eq!(
        w.store.portfolio_summary(&Scope::All).await.unwrap(),
        before
    );
}

#[tokio::test]
async fn custom_dates_have_exact_endpoints_and_filters_cover_only_the_selected_account() {
    let w = world().await;
    w.store.replay_accounting().await.unwrap();
    let custom = w
        .store
        .get_chart_window(&Scope::All, ChartRange::All, Some((T1 + 123, T2 - 321)))
        .await
        .unwrap();
    assert_eq!(custom.points.first().unwrap().t, T1 + 123);
    assert_eq!(custom.points.last().unwrap().t, T2 - 321);
    assert!(custom.points.len() <= 1000);
    assert_eq!(custom.performance.as_ref().unwrap().start, T1 + 123);
    assert!(
        w.store
            .get_chart_window(&Scope::All, ChartRange::All, Some((T2, T1)))
            .await
            .is_err()
    );
    let filter = ActivityFilter {
        account_id: Some(w.b.clone()),
        network: Some(NetworkId::Ethereum),
        operation: Some("send".into()),
        status: Some("confirmed".into()),
        start: Some(T2),
        end: Some(T2),
        ..Default::default()
    };
    let rows = w
        .store
        .list_activity(&Scope::All, &filter, None, 1)
        .await
        .unwrap()
        .rows;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].account_id, w.b);
    assert_eq!(rows[0].occurred_at, T2);
}

#[tokio::test]
async fn cooccurrence_without_matching_payment_edges_never_moves_owned_basis() {
    let w = world().await;
    let mut sent = leg(eth(), "-1", "send");
    sent.counterparty = Some("0x0000000000000000000000000000000000000001".into());
    let mut received = leg(eth(), "1", "receive");
    received.counterparty = Some("0x0000000000000000000000000000000000000002".into());
    w.store
        .ingest_transaction(&w.a, &tx("0x02", T1, "send", vec![sent], Some("0.01")))
        .await
        .unwrap();
    w.store
        .ingest_transaction(&w.b, &tx("0x02", T1, "receive", vec![received], None))
        .await
        .unwrap();
    w.store.replay_accounting().await.unwrap();
    let rows = w
        .store
        .list_activity(&Scope::All, &Default::default(), None, 200)
        .await
        .unwrap()
        .rows;
    for row in rows.iter().filter(|r| r.transaction_id == "ethereum:0x02") {
        assert!(
            row.legs
                .iter()
                .all(|l| l.treatment.as_deref() != Some("own_transfer_in")
                    && l.treatment.as_deref() != Some("own_transfer_out"))
        );
    }
}
