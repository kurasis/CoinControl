//! Accounting replay over synchronized history (ACCOUNTING.md, SPECIFICATION.md §7–§8).
//!
//! The replay turns stored chain evidence (legs and fees), stored prices and
//! the latest version of every user decision into normalized ledger events,
//! runs the FIFO [`Ledger`] and persists the derived results: lots and their
//! consumptions, lifetime totals, capital flows for period performance, a
//! per-leg interpretation for the activity and review lists, and
//! reconciliation items. Derived tables are rebuilt as a whole, so replaying
//! the same evidence, overrides and prices always produces the same rows.
//!
//! Interpretation rules, in order:
//! 1. Opening lots (CSV) replace the account/asset history before their cutoff.
//! 2. Reviewed manual pairings link an outgoing and an incoming leg of two
//!    owned accounts as one own transfer.
//! 3. Within one chain transaction, legs of the same account and asset are
//!    netted, then outgoing and incoming quantities of the same asset between
//!    owned accounts are paired as own transfers (shared transaction evidence).
//! 4. What remains per account is a trade when it both sends and receives
//!    assets, otherwise a disposal/withdrawal or a deposit/reward according to
//!    the user's classification. Unclassified movements stay unresolved.
//! 5. A fee actually paid by an owned account is charged once, at its market value.

use std::collections::{BTreeMap, BTreeSet};

use bigdecimal::Zero;
use portfolio_core::accounting::{
    AccountTotals, BasisKind, ChainOrder, Event, EventKind, Ledger, Lot, PartialSum,
};
use portfolio_core::decimal::{
    Dec, div, parse_dec, parse_raw_amount, raw_to_quantity, to_canonical,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;

#[cfg(feature = "ts")]
use ts_rs::TS;

use crate::{Result, Store, StoreError};

pub(crate) const MINUTE: i64 = 60;
pub(crate) const HOUR: i64 = 3_600;
pub(crate) const DAY: i64 = 86_400;

// ---------------------------------------------------------------- user decisions

/// What the user says an unpaired movement was.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum LegClassification {
    /// Incoming capital (purchase elsewhere, deposit from an exchange). Default for receipts.
    Deposit,
    /// Reward or airdrop received at a reliable value: income plus a lot at that value.
    Reward,
    /// Outgoing to an untracked address the user owns or withdrew to: not a sale.
    Withdrawal,
    Gift,
    OwnUntracked,
    /// Disposal with proceeds (sale for fiat, payment for goods).
    Sale,
    Payment,
    /// Not reviewed yet. Default for unpaired outgoing movements.
    Unclassified,
}

impl LegClassification {
    pub fn is_incoming(self) -> bool {
        matches!(self, LegClassification::Deposit | LegClassification::Reward)
    }

    pub fn is_outgoing(self) -> bool {
        !self.is_incoming()
    }

    pub fn parse(text: &str) -> Option<Self> {
        Some(match text.trim().to_ascii_lowercase().as_str() {
            "deposit" | "purchase" | "buy" => LegClassification::Deposit,
            "reward" | "income" | "airdrop" => LegClassification::Reward,
            "withdrawal" => LegClassification::Withdrawal,
            "gift" => LegClassification::Gift,
            "own_untracked" => LegClassification::OwnUntracked,
            "sale" | "sell" => LegClassification::Sale,
            "payment" => LegClassification::Payment,
            "unclassified" => LegClassification::Unclassified,
            _ => return None,
        })
    }
}

/// One acquisition fragment attached to a receipt (basis correction or CSV).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct BasisLotInput {
    /// Exact decimal asset units.
    pub quantity: String,
    /// Exact USD; `None` means unknown, `"0"` an explicit zero.
    pub basis_usd: Option<String>,
    pub basis_kind: BasisKind,
    /// Original acquisition time (may precede the on-chain receipt).
    pub acquired_at: i64,
}

/// A versioned user decision about one leg. Empty fields keep the default
/// interpretation; saving an empty override reverts to it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct LegOverride {
    #[serde(default)]
    pub classification: Option<LegClassification>,
    /// Acquisition lots for a receipt. Their quantity may be below the receipt;
    /// the rest keeps unknown basis.
    #[serde(default)]
    pub basis_lots: Option<Vec<BasisLotInput>>,
    /// Use the receipt-time market value as an *estimated* basis.
    #[serde(default)]
    pub basis_from_market: bool,
    /// Gross proceeds of a sale/payment in USD.
    #[serde(default)]
    pub proceeds_usd: Option<String>,
    /// Use the market value at disposal time as estimated proceeds.
    #[serde(default)]
    pub proceeds_from_market: bool,
    /// Historical unit price override for this leg's valuation.
    #[serde(default)]
    pub price_usd: Option<String>,
    /// Incoming leg of another owned account that received this outgoing leg.
    #[serde(default)]
    pub pair_with: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

impl LegOverride {
    pub fn is_empty(&self) -> bool {
        self.classification.is_none()
            && self.basis_lots.is_none()
            && !self.basis_from_market
            && self.proceeds_usd.is_none()
            && !self.proceeds_from_market
            && self.price_usd.is_none()
            && self.pair_with.is_none()
            && self.note.is_none()
    }
}

/// Opening inventory at a declared history start (CSV `opening` rows).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct OpeningLot {
    pub account_id: String,
    pub asset_id: String,
    pub quantity: String,
    pub basis_usd: Option<String>,
    pub basis_kind: BasisKind,
    pub acquired_at: i64,
    /// History of this account and asset before this instant is replaced by the lot.
    pub cutoff_at: i64,
    #[serde(default)]
    pub note: Option<String>,
}

// ---------------------------------------------------------------- inputs

#[derive(Debug, Clone)]
pub(crate) struct AssetInfo {
    pub decimals: u32,
    pub verification: String,
}

#[derive(Debug, Clone)]
struct LegRow {
    address: String,
    counterparty: Option<String>,
    id: String,
    tx_id: String,
    account_id: String,
    asset_id: String,
    /// Signed asset units.
    quantity: Dec,
    at: i64,
}

#[derive(Debug, Clone)]
struct FeeRow {
    id: String,
    tx_id: String,
    account_id: String,
    asset_id: String,
    quantity: Dec,
    at: i64,
}

#[derive(Debug, Clone)]
struct PricePoint {
    at: i64,
    price: String,
    tolerance: i64,
    estimated: bool,
}

/// Stored quotes per asset, for "nearest suitable point within tolerance" lookups.
#[derive(Debug, Default, Clone)]
pub(crate) struct PriceBook {
    series: BTreeMap<String, Vec<PricePoint>>,
}

/// Tolerance for using a quote of this resolution (ACCOUNTING.md §9).
pub(crate) fn tolerance_for(granularity: &str) -> i64 {
    match granularity {
        "day" => DAY,
        "hour" => HOUR,
        _ => 5 * MINUTE,
    }
}

impl PriceBook {
    pub(crate) fn push(&mut self, asset: &str, at: i64, price: String, granularity: &str) {
        self.series
            .entry(asset.to_owned())
            .or_default()
            .push(PricePoint {
                at,
                price,
                tolerance: tolerance_for(granularity),
                estimated: matches!(granularity, "day" | "hour"),
            });
    }

    pub(crate) fn finish(&mut self) {
        for points in self.series.values_mut() {
            points.sort_by_key(|p| p.at);
        }
    }

    /// The quote nearest to `t` whose own tolerance covers the distance.
    /// Ties prefer the earlier quote. `None` when nothing suitable exists:
    /// a later quote is never stretched to fill a long gap.
    pub(crate) fn at(&self, asset: &str, t: i64) -> Option<(Dec, bool)> {
        let points = self.series.get(asset)?;
        let i = points.partition_point(|p| p.at <= t);
        let mut best: Option<&PricePoint> = None;
        // Scan a few neighbours on each side: a coarse point can cover `t`
        // even when finer ones nearby do not.
        let lo = i.saturating_sub(4);
        let hi = (i + 4).min(points.len());
        for p in &points[lo..hi] {
            let distance = (t - p.at).abs();
            if distance > p.tolerance {
                continue;
            }
            let better = match best {
                None => true,
                Some(b) => {
                    let bd = (t - b.at).abs();
                    distance < bd || (distance == bd && p.at < b.at)
                }
            };
            if better {
                best = Some(p);
            }
        }
        if best.is_none() {
            // A daily point may be further than 4 ticks away.
            let before = points[..i].iter().rev().find(|p| t - p.at <= p.tolerance);
            let after = points[i..].iter().find(|p| p.at - t <= p.tolerance);
            best = match (before, after) {
                (Some(b), Some(a)) => Some(if a.at - t < t - b.at { a } else { b }),
                (b, a) => b.or(a),
            };
        }
        let p = best?;
        parse_dec(&p.price).ok().map(|d| (d, p.estimated))
    }
}

pub(crate) struct Inputs {
    transaction_order: BTreeMap<String, u64>,
    owned: BTreeSet<String>,
    assets: BTreeMap<String, AssetInfo>,
    legs: Vec<LegRow>,
    fees: Vec<FeeRow>,
    leg_overrides: BTreeMap<String, LegOverride>,
    openings: Vec<(String, OpeningLot)>,
    prices: PriceBook,
}

// ---------------------------------------------------------------- derived output

/// How the replay interpreted one leg or fee.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LegNote {
    pub tx_id: String,
    pub account_id: String,
    pub asset_id: String,
    pub at: i64,
    pub quantity: Dec,
    pub treatment: &'static str,
    pub counterparty: Option<String>,
    pub value_usd: Option<Dec>,
    pub value_estimated: bool,
    pub basis_usd: Option<Dec>,
    pub basis_kind: Option<BasisKind>,
    pub proceeds_usd: Option<Dec>,
    pub review: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FlowRow {
    pub event_id: String,
    pub at: i64,
    pub from: Option<String>,
    pub to: Option<String>,
    pub asset_id: String,
    pub quantity: Dec,
    pub value_usd: Option<Dec>,
    pub classified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReconItem {
    pub kind: &'static str,
    pub account_id: Option<String>,
    pub asset_id: Option<String>,
    pub event_id: Option<String>,
    pub quantity: Option<Dec>,
    pub detail: String,
}

pub(crate) struct Computation {
    pub ledger: Ledger,
    pub notes: BTreeMap<String, LegNote>,
    pub flows: Vec<FlowRow>,
    pub items: Vec<ReconItem>,
    pub orphaned_legs: BTreeSet<String>,
    pub event_count: usize,
}

/// Outcome of a replay, reported to the UI and tests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct ReplayReport {
    pub events: u32,
    pub lots: u32,
    pub review_items: u32,
    pub reconciliation_items: u32,
    pub orphaned_overrides: u32,
}

/// Compact totals used to preview the effect of an import before committing it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct RecalcSummary {
    pub realized_known_usd: String,
    pub realized_complete: bool,
    pub remaining_known_basis_usd: String,
    pub unknown_basis_lots: u32,
    pub review_items: u32,
    pub inventory_gaps: u32,
}

fn order(at: i64) -> ChainOrder {
    ChainOrder { time: at, seq: 0 }
}

fn mul(a: &Dec, b: &Dec) -> Dec {
    a * b
}

fn parse_opt(text: &Option<String>) -> Result<Option<Dec>> {
    text.as_deref()
        .map(|t| parse_dec(t).map_err(StoreError::from))
        .transpose()
}

/// Splits `total` across `weights` proportionally; the last share takes the
/// rounding residue so the parts always sum exactly to `total`.
fn split(total: &Dec, weights: &[Dec]) -> Option<Vec<Dec>> {
    let sum: Dec = weights.iter().cloned().sum();
    if sum.is_zero() {
        return None;
    }
    let mut out = Vec::with_capacity(weights.len());
    let mut allocated = Dec::zero();
    for (i, w) in weights.iter().enumerate() {
        if i + 1 == weights.len() {
            out.push(total - &allocated);
        } else {
            let part = div(&(total * w), &sum)?;
            allocated += &part;
            out.push(part);
        }
    }
    Some(out)
}

impl Inputs {
    fn leg_value(&self, leg_id: &str, asset: &str, quantity: &Dec, at: i64) -> (Option<Dec>, bool) {
        if let Some(price) = self
            .leg_overrides
            .get(leg_id)
            .and_then(|o| o.price_usd.as_deref())
            .and_then(|p| parse_dec(p).ok())
        {
            return (Some(mul(&quantity.abs(), &price)), false);
        }
        match self.prices.at(asset, at) {
            Some((price, estimated)) => (Some(mul(&quantity.abs(), &price)), estimated),
            None => (None, false),
        }
    }

    fn is_spam(&self, asset: &str) -> bool {
        self.assets
            .get(asset)
            .is_some_and(|a| a.verification == "spam")
    }

    /// Unverified, unpriced tokens are usually unsolicited; their unknown basis
    /// is kept but does not fill the review list.
    fn reviewable(&self, asset: &str, value: &Option<Dec>) -> bool {
        value.is_some()
            || self
                .assets
                .get(asset)
                .is_some_and(|a| a.verification == "verified")
    }
}

/// Builds ledger events and interpretations. Pure: no I/O, deterministic.
pub(crate) fn compute(inputs: &Inputs) -> Computation {
    let mut events: Vec<Event> = Vec::new();
    let mut notes: BTreeMap<String, LegNote> = BTreeMap::new();
    let mut flows: Vec<FlowRow> = Vec::new();
    let mut items: Vec<ReconItem> = Vec::new();

    let leg_ids: BTreeSet<&str> = inputs.legs.iter().map(|l| l.id.as_str()).collect();
    let orphaned_legs: BTreeSet<String> = inputs
        .leg_overrides
        .keys()
        .filter(|id| !leg_ids.contains(id.as_str()))
        .cloned()
        .collect();
    for id in &orphaned_legs {
        items.push(ReconItem {
            kind: "orphaned_override",
            account_id: None,
            asset_id: None,
            event_id: Some(id.clone()),
            quantity: None,
            detail: "the movement this decision refers to is no longer reported".into(),
        });
    }

    // 1. Opening lots and the history they replace.
    let mut cutoffs: BTreeMap<(String, String), i64> = BTreeMap::new();
    for (_, lot) in &inputs.openings {
        let key = (lot.account_id.clone(), lot.asset_id.clone());
        let entry = cutoffs.entry(key).or_insert(lot.cutoff_at);
        *entry = (*entry).max(lot.cutoff_at);
    }
    for (id, lot) in &inputs.openings {
        if !inputs.owned.contains(&lot.account_id) || inputs.is_spam(&lot.asset_id) {
            continue;
        }
        let Ok(quantity) = parse_dec(&lot.quantity) else {
            continue;
        };
        let basis = lot.basis_usd.as_deref().and_then(|b| parse_dec(b).ok());
        let value = inputs
            .prices
            .at(&lot.asset_id, lot.cutoff_at)
            .map(|(p, _)| mul(&quantity, &p));
        let event_id = format!("open|{id}");
        events.push(Event {
            id: event_id.clone(),
            order: order(lot.cutoff_at),
            kind: EventKind::Acquire {
                account: lot.account_id.clone(),
                asset: lot.asset_id.clone(),
                quantity: quantity.clone(),
                basis_kind: if basis.is_some() {
                    lot.basis_kind
                } else {
                    BasisKind::Unknown
                },
                basis_usd: basis,
                acquired_at: Some(order(lot.acquired_at)),
                external_inflow_usd: value.clone(),
                is_external_inflow: true,
            },
        });
        flows.push(FlowRow {
            event_id,
            at: lot.cutoff_at,
            from: None,
            to: Some(lot.account_id.clone()),
            asset_id: lot.asset_id.clone(),
            quantity,
            value_usd: value,
            classified: true,
        });
    }
    let before_opening = |account: &str, asset: &str, at: i64| -> bool {
        cutoffs
            .get(&(account.to_owned(), asset.to_owned()))
            .is_some_and(|c| at < *c)
    };
    let mut overlap: BTreeMap<(String, String), u32> = BTreeMap::new();
    let mut active: Vec<&LegRow> = Vec::new();
    for leg in &inputs.legs {
        if inputs.is_spam(&leg.asset_id) {
            notes.insert(leg.id.clone(), note(leg, "excluded_spam"));
        } else if before_opening(&leg.account_id, &leg.asset_id, leg.at) {
            *overlap
                .entry((leg.account_id.clone(), leg.asset_id.clone()))
                .or_default() += 1;
            notes.insert(leg.id.clone(), note(leg, "before_opening"));
        } else {
            active.push(leg);
        }
    }
    for ((account, asset), count) in overlap {
        items.push(ReconItem {
            kind: "opening_overlap",
            account_id: Some(account),
            asset_id: Some(asset),
            event_id: None,
            quantity: None,
            detail: format!(
                "{count} downloaded movement(s) predate the opening position and are not replayed"
            ),
        });
    }

    // 2. Reviewed manual pairings.
    let by_id: BTreeMap<&str, &LegRow> = active.iter().map(|l| (l.id.as_str(), *l)).collect();
    let mut paired: BTreeSet<String> = BTreeSet::new();
    for (out_id, o) in &inputs.leg_overrides {
        let Some(in_id) = &o.pair_with else { continue };
        let (Some(out), Some(inc)) = (by_id.get(out_id.as_str()), by_id.get(in_id.as_str())) else {
            continue;
        };
        let valid = out.quantity < Dec::zero()
            && inc.quantity > Dec::zero()
            && out.asset_id == inc.asset_id
            && out.account_id != inc.account_id
            && inc.quantity <= out.quantity.abs()
            && !paired.contains(&out.id)
            && !paired.contains(&inc.id);
        if !valid {
            items.push(ReconItem {
                kind: "invalid_pairing",
                account_id: Some(out.account_id.clone()),
                asset_id: Some(out.asset_id.clone()),
                event_id: Some(out.id.clone()),
                quantity: None,
                detail: "a transfer pairing needs an outgoing and an incoming movement of the same asset in two owned accounts, received quantity not above the sent quantity".into(),
            });
            continue;
        }
        paired.insert(out.id.clone());
        paired.insert(inc.id.clone());
        let event_id = format!("{}|1|pair|{}", out.tx_id, out.id);
        let (value, estimated) = inputs.leg_value(&inc.id, &inc.asset_id, &inc.quantity, out.at);
        events.push(Event {
            id: event_id.clone(),
            order: order(out.at),
            kind: EventKind::OwnTransfer {
                from: out.account_id.clone(),
                to: inc.account_id.clone(),
                asset: out.asset_id.clone(),
                quantity: inc.quantity.clone(),
                market_value_usd: value.clone(),
            },
        });
        flows.push(FlowRow {
            event_id,
            at: out.at,
            from: Some(out.account_id.clone()),
            to: Some(inc.account_id.clone()),
            asset_id: out.asset_id.clone(),
            quantity: inc.quantity.clone(),
            value_usd: value.clone(),
            classified: true,
        });
        let shortfall = out.quantity.abs() - &inc.quantity;
        let mut out_note = note(out, "own_transfer_out");
        out_note.counterparty = Some(inc.account_id.clone());
        out_note.value_usd = value.clone();
        out_note.value_estimated = estimated;
        notes.insert(out.id.clone(), out_note);
        let mut in_note = note(inc, "own_transfer_in");
        in_note.counterparty = Some(out.account_id.clone());
        in_note.value_usd = value;
        in_note.value_estimated = estimated;
        notes.insert(inc.id.clone(), in_note);
        if shortfall > Dec::zero() {
            // The intermediary kept part of it: a transfer cost, expensed once.
            let (fee_value, _) = inputs.leg_value(&out.id, &out.asset_id, &shortfall, out.at);
            events.push(Event {
                id: format!("{}|2|paircost|{}", out.tx_id, out.id),
                order: order(out.at),
                kind: EventKind::Fee {
                    account: out.account_id.clone(),
                    asset: out.asset_id.clone(),
                    quantity: shortfall,
                    value_usd: fee_value,
                },
            });
        }
    }

    // 3–5. Per chain transaction.
    let mut by_tx: BTreeMap<(i64, &str), Vec<&LegRow>> = BTreeMap::new();
    for leg in active.iter().filter(|l| !paired.contains(&l.id)) {
        by_tx
            .entry((leg.at, leg.tx_id.as_str()))
            .or_default()
            .push(leg);
    }
    let mut fees_by_tx: BTreeMap<(i64, &str), Vec<&FeeRow>> = BTreeMap::new();
    for fee in &inputs.fees {
        if before_opening(&fee.account_id, &fee.asset_id, fee.at) {
            notes.insert(
                fee.id.clone(),
                LegNote {
                    tx_id: fee.tx_id.clone(),
                    account_id: fee.account_id.clone(),
                    asset_id: fee.asset_id.clone(),
                    at: fee.at,
                    quantity: -fee.quantity.clone(),
                    treatment: "before_opening",
                    counterparty: None,
                    value_usd: None,
                    value_estimated: false,
                    basis_usd: None,
                    basis_kind: None,
                    proceeds_usd: None,
                    review: None,
                },
            );
            continue;
        }
        fees_by_tx
            .entry((fee.at, fee.tx_id.as_str()))
            .or_default()
            .push(fee);
    }
    let tx_keys: BTreeSet<(i64, &str)> = by_tx.keys().chain(fees_by_tx.keys()).copied().collect();
    for key in tx_keys {
        let legs = by_tx.get(&key).map(Vec::as_slice).unwrap_or(&[]);
        process_tx(
            inputs,
            key.1,
            key.0,
            legs,
            &mut events,
            &mut notes,
            &mut flows,
        );
        for fee in fees_by_tx.get(&key).map(Vec::as_slice).unwrap_or(&[]) {
            if inputs.is_spam(&fee.asset_id) {
                continue;
            }
            let (value, estimated) =
                inputs.leg_value(&fee.id, &fee.asset_id, &fee.quantity, fee.at);
            events.push(Event {
                id: format!("{}|2|fee|{}", fee.tx_id, fee.account_id),
                order: order(fee.at),
                kind: EventKind::Fee {
                    account: fee.account_id.clone(),
                    asset: fee.asset_id.clone(),
                    quantity: fee.quantity.clone(),
                    value_usd: value.clone(),
                },
            });
            let review = value.is_none().then_some("missing_price");
            notes.insert(
                fee.id.clone(),
                LegNote {
                    tx_id: fee.tx_id.clone(),
                    account_id: fee.account_id.clone(),
                    asset_id: fee.asset_id.clone(),
                    at: fee.at,
                    quantity: -fee.quantity.clone(),
                    treatment: "fee",
                    counterparty: None,
                    value_usd: value,
                    value_estimated: estimated,
                    basis_usd: None,
                    basis_kind: None,
                    proceeds_usd: None,
                    review,
                },
            );
        }
    }

    // Respect the provider's chain position within the same timestamp. Event
    // identifiers only break ties inside one transaction, not across blocks.
    for event in &mut events {
        if let Some((tx, _)) = event.id.split_once('|') {
            event.order.seq = inputs.transaction_order.get(tx).copied().unwrap_or(0);
        }
    }
    let event_count = events.len();
    let ledger = Ledger::replay(&events);
    for gap in ledger.gaps() {
        items.push(ReconItem {
            kind: "inventory_gap",
            account_id: Some(gap.account.clone()),
            asset_id: Some(gap.asset.clone()),
            event_id: Some(gap.event.clone()),
            quantity: Some(gap.missing_quantity.clone()),
            detail: "more was spent than the known history holds; earlier history or a basis lot is missing".into(),
        });
    }
    Computation {
        ledger,
        notes,
        flows,
        items,
        orphaned_legs,
        event_count,
    }
}

fn note(leg: &LegRow, treatment: &'static str) -> LegNote {
    LegNote {
        tx_id: leg.tx_id.clone(),
        account_id: leg.account_id.clone(),
        asset_id: leg.asset_id.clone(),
        at: leg.at,
        quantity: leg.quantity.clone(),
        treatment,
        counterparty: None,
        value_usd: None,
        value_estimated: false,
        basis_usd: None,
        basis_kind: None,
        proceeds_usd: None,
        review: None,
    }
}

/// One netted movement of one account and asset inside a transaction.
struct Movement<'a> {
    leg: &'a LegRow,
    direct_edge: bool,
    /// Positive amount still to be interpreted.
    remaining: Dec,
    counterparty: Option<String>,
}

#[allow(clippy::too_many_lines)]
fn process_tx(
    inputs: &Inputs,
    tx_id: &str,
    at: i64,
    legs: &[&LegRow],
    events: &mut Vec<Event>,
    notes: &mut BTreeMap<String, LegNote>,
    flows: &mut Vec<FlowRow>,
) {
    // Net legs of the same account and asset; the largest leg in the net
    // direction represents the movement, the others are marked netted.
    let mut grouped: BTreeMap<(&str, &str), Vec<&LegRow>> = BTreeMap::new();
    for leg in legs {
        if inputs.is_spam(&leg.asset_id) {
            notes.insert(leg.id.clone(), note(leg, "excluded_spam"));
            continue;
        }
        grouped
            .entry((leg.account_id.as_str(), leg.asset_id.as_str()))
            .or_default()
            .push(leg);
    }
    let mut outs: BTreeMap<&str, Vec<Movement>> = BTreeMap::new(); // asset -> movements
    let mut ins: BTreeMap<&str, Vec<Movement>> = BTreeMap::new();
    for ((_, asset), group) in &grouped {
        let net: Dec = group.iter().map(|l| l.quantity.clone()).sum();
        let representative = group
            .iter()
            .filter(|l| !net.is_zero() && (l.quantity > Dec::zero()) == (net > Dec::zero()))
            .max_by(|a, b| {
                a.quantity
                    .abs()
                    .cmp(&b.quantity.abs())
                    .then_with(|| b.id.cmp(&a.id))
            });
        for leg in group {
            if representative.is_none_or(|r| r.id != leg.id) {
                notes.insert(leg.id.clone(), note(leg, "netted"));
            }
        }
        let Some(rep) = representative else { continue };
        let m = Movement {
            leg: rep,
            direct_edge: group.iter().all(|l| {
                l.counterparty == rep.counterparty
                    && (l.quantity > Dec::zero()) == (net > Dec::zero())
            }),
            remaining: net.abs(),
            counterparty: None,
        };
        if net > Dec::zero() {
            ins.entry(asset).or_default().push(m);
        } else {
            outs.entry(asset).or_default().push(m);
        }
    }

    // Own transfers between owned accounts sharing this transaction.
    let mut transfer_n = 0u32;
    for (asset, senders) in outs.iter_mut() {
        let Some(receivers) = ins.get_mut(asset) else {
            continue;
        };
        // A shared transaction hash proves co-occurrence, not the payment
        // edges of a multi-party transaction. Never invent FIFO lot routes.
        if senders.len() != 1 || receivers.len() != 1 {
            continue;
        }
        let (mut i, mut j) = (0usize, 0usize);
        while i < senders.len() && j < receivers.len() {
            let (s, r) = (&senders[i], &receivers[j]);
            if !s.direct_edge
                || !r.direct_edge
                || s.leg.counterparty.as_deref() != Some(r.leg.address.as_str())
                || r.leg.counterparty.as_deref() != Some(s.leg.address.as_str())
            {
                break;
            }
            if s.leg.account_id == r.leg.account_id {
                j += 1;
                continue;
            }
            let quantity = s.remaining.clone().min(r.remaining.clone());
            let (value, estimated) = inputs.leg_value(&r.leg.id, asset, &quantity, at);
            let event_id = format!("{tx_id}|1|xfer|{transfer_n:04}");
            transfer_n += 1;
            events.push(Event {
                id: event_id.clone(),
                order: order(at),
                kind: EventKind::OwnTransfer {
                    from: s.leg.account_id.clone(),
                    to: r.leg.account_id.clone(),
                    asset: (*asset).to_owned(),
                    quantity: quantity.clone(),
                    market_value_usd: value.clone(),
                },
            });
            flows.push(FlowRow {
                event_id,
                at,
                from: Some(s.leg.account_id.clone()),
                to: Some(r.leg.account_id.clone()),
                asset_id: (*asset).to_owned(),
                quantity: quantity.clone(),
                value_usd: value.clone(),
                classified: true,
            });
            for (m, treatment, other) in [
                (
                    &senders[i],
                    "own_transfer_out",
                    &receivers[j].leg.account_id,
                ),
                (&receivers[j], "own_transfer_in", &senders[i].leg.account_id),
            ] {
                let mut n = note(m.leg, treatment);
                n.counterparty = Some(other.clone());
                let (v, e) = inputs.leg_value(&m.leg.id, asset, &m.leg.quantity, at);
                n.value_usd = v;
                n.value_estimated = e;
                notes.insert(m.leg.id.clone(), n);
            }
            let _ = estimated;
            let (s_acct, r_acct) = (
                senders[i].leg.account_id.clone(),
                receivers[j].leg.account_id.clone(),
            );
            senders[i].remaining -= &quantity;
            senders[i].counterparty = Some(r_acct);
            receivers[j].remaining -= &quantity;
            receivers[j].counterparty = Some(s_acct);
            if senders[i].remaining.is_zero() {
                i += 1;
            }
            if receivers[j].remaining.is_zero() {
                j += 1;
            }
        }
    }

    // Remaining movements per account.
    let mut per_account: BTreeMap<&str, (Vec<&Movement>, Vec<&Movement>)> = BTreeMap::new();
    for movements in outs.values() {
        for m in movements.iter().filter(|m| !m.remaining.is_zero()) {
            per_account.entry(&m.leg.account_id).or_default().0.push(m);
        }
    }
    for movements in ins.values() {
        for m in movements.iter().filter(|m| !m.remaining.is_zero()) {
            per_account.entry(&m.leg.account_id).or_default().1.push(m);
        }
    }

    for (account, (gives, gets)) in per_account {
        if !gives.is_empty() && !gets.is_empty() {
            trade(inputs, tx_id, at, account, &gives, &gets, events, notes);
            continue;
        }
        for m in gives {
            outgoing(inputs, tx_id, at, account, m, events, notes, flows);
        }
        for m in gets {
            incoming(inputs, tx_id, at, account, m, events, notes, flows);
        }
    }
}

fn movement_value(inputs: &Inputs, m: &Movement, at: i64) -> (Option<Dec>, bool) {
    inputs.leg_value(&m.leg.id, &m.leg.asset_id, &m.remaining, at)
}

#[allow(clippy::too_many_arguments)]
fn trade(
    inputs: &Inputs,
    tx_id: &str,
    at: i64,
    account: &str,
    gives: &[&Movement],
    gets: &[&Movement],
    events: &mut Vec<Event>,
    notes: &mut BTreeMap<String, LegNote>,
) {
    let give_values: Vec<(Option<Dec>, bool)> = gives
        .iter()
        .map(|m| movement_value(inputs, m, at))
        .collect();
    let get_values: Vec<(Option<Dec>, bool)> =
        gets.iter().map(|m| movement_value(inputs, m, at)).collect();
    let overridden_proceeds: Option<Vec<Dec>> = gives
        .iter()
        .map(|m| {
            inputs
                .leg_overrides
                .get(&m.leg.id)
                .and_then(|o| o.proceeds_usd.as_deref())
                .and_then(|p| parse_dec(p).ok())
        })
        .collect();
    let all = |v: &[(Option<Dec>, bool)]| -> Option<Vec<Dec>> {
        v.iter().map(|(x, _)| x.clone()).collect()
    };
    // One consistent gross trade value: user consideration first, then the
    // valued give side, then the valued get side (ACCOUNTING.md §3).
    let (gross, known) = if let Some(p) = &overridden_proceeds {
        (Some(p.iter().cloned().sum::<Dec>()), true)
    } else if let Some(v) = all(&give_values) {
        (Some(v.iter().cloned().sum::<Dec>()), false)
    } else if let Some(v) = all(&get_values) {
        (Some(v.iter().cloned().sum::<Dec>()), false)
    } else {
        (None, false)
    };
    let proceeds: Vec<Option<Dec>> = match (&overridden_proceeds, &gross) {
        (Some(p), _) => p.iter().cloned().map(Some).collect(),
        (None, Some(g)) if gives.len() == 1 => vec![Some(g.clone())],
        (None, Some(g)) => match all(&give_values).and_then(|w| split(g, &w)) {
            Some(parts) => parts.into_iter().map(Some).collect(),
            None => vec![None; gives.len()],
        },
        (None, None) => vec![None; gives.len()],
    };
    for (n, (m, p)) in gives.iter().zip(&proceeds).enumerate() {
        events.push(Event {
            id: format!("{tx_id}|1|trade|{account}|{n:03}"),
            order: order(at),
            kind: EventKind::Dispose {
                account: account.to_owned(),
                asset: m.leg.asset_id.clone(),
                quantity: m.remaining.clone(),
                proceeds_usd: p.clone(),
            },
        });
        let mut nt = note(m.leg, "trade_out");
        nt.counterparty = m.counterparty.clone();
        nt.value_usd = give_values[n].0.clone();
        nt.value_estimated = give_values[n].1;
        nt.proceeds_usd = p.clone();
        nt.review = p.is_none().then_some("missing_trade_value");
        notes.insert(m.leg.id.clone(), nt);
    }
    let bases: Vec<Option<Dec>> = match &gross {
        Some(g) if gets.len() == 1 => vec![Some(g.clone())],
        Some(g) => match all(&get_values).and_then(|w| split(g, &w)) {
            Some(parts) => parts.into_iter().map(Some).collect(),
            None => vec![None; gets.len()],
        },
        None => vec![None; gets.len()],
    };
    let derived_kind = if known {
        BasisKind::Known
    } else {
        BasisKind::Estimated
    };
    for (n, (m, basis)) in gets.iter().zip(bases).enumerate() {
        let user_lots = inputs
            .leg_overrides
            .get(&m.leg.id)
            .and_then(|o| o.basis_lots.clone());
        let mut nt = note(m.leg, "trade_in");
        nt.counterparty = m.counterparty.clone();
        nt.value_usd = get_values[n].0.clone();
        nt.value_estimated = get_values[n].1;
        if let Some(lots) = user_lots {
            let (b, k) = push_basis_lots(
                &format!("{tx_id}|3|trade|{account}|{n:03}"),
                at,
                account,
                &m.leg.asset_id,
                &m.remaining,
                &lots,
                None,
                events,
            );
            nt.basis_usd = b;
            nt.basis_kind = Some(k);
        } else {
            let kind = if basis.is_some() {
                derived_kind
            } else {
                BasisKind::Unknown
            };
            events.push(Event {
                id: format!("{tx_id}|3|trade|{account}|{n:03}"),
                order: order(at),
                kind: EventKind::Acquire {
                    account: account.to_owned(),
                    asset: m.leg.asset_id.clone(),
                    quantity: m.remaining.clone(),
                    basis_usd: basis.clone(),
                    basis_kind: kind,
                    acquired_at: None,
                    external_inflow_usd: None,
                    is_external_inflow: false,
                },
            });
            nt.review = basis.is_none().then_some("missing_trade_value");
            nt.basis_usd = basis;
            nt.basis_kind = Some(kind);
        }
        notes.insert(m.leg.id.clone(), nt);
    }
}

/// Emits acquisition fragments for a receipt; the unmatched remainder keeps
/// unknown basis. Returns the total basis (if fully known) and its kind.
#[allow(clippy::too_many_arguments)]
fn push_basis_lots(
    id_prefix: &str,
    at: i64,
    account: &str,
    asset: &str,
    quantity: &Dec,
    lots: &[BasisLotInput],
    inflow: Option<Option<Dec>>,
    events: &mut Vec<Event>,
) -> (Option<Dec>, BasisKind) {
    let mut assigned = Dec::zero();
    let mut total = Some(Dec::zero());
    let mut kind = BasisKind::Known;
    let mut first = true;
    for (k, lot) in lots.iter().enumerate() {
        let Ok(q) = parse_dec(&lot.quantity) else {
            continue;
        };
        if q <= Dec::zero() || &assigned + &q > *quantity {
            continue; // validated on save; never create inventory beyond the receipt
        }
        assigned += &q;
        let basis = lot.basis_usd.as_deref().and_then(|b| parse_dec(b).ok());
        let lot_kind = if basis.is_some() {
            lot.basis_kind
        } else {
            BasisKind::Unknown
        };
        total = match (total, &basis) {
            (Some(t), Some(b)) => Some(t + b),
            _ => None,
        };
        kind = weaker(kind, lot_kind);
        events.push(Event {
            id: format!("{id_prefix}|lot{k:03}"),
            order: order(at),
            kind: EventKind::Acquire {
                account: account.to_owned(),
                asset: asset.to_owned(),
                quantity: q,
                basis_usd: basis,
                basis_kind: lot_kind,
                acquired_at: Some(order(lot.acquired_at)),
                external_inflow_usd: if first {
                    inflow.clone().flatten()
                } else {
                    None
                },
                is_external_inflow: first && inflow.is_some(),
            },
        });
        first = false;
    }
    let rest = quantity - &assigned;
    if rest > Dec::zero() {
        total = None;
        kind = BasisKind::Unknown;
        events.push(Event {
            id: format!("{id_prefix}|rest"),
            order: order(at),
            kind: EventKind::Acquire {
                account: account.to_owned(),
                asset: asset.to_owned(),
                quantity: rest,
                basis_usd: None,
                basis_kind: BasisKind::Unknown,
                acquired_at: None,
                external_inflow_usd: if first {
                    inflow.clone().flatten()
                } else {
                    None
                },
                is_external_inflow: first && inflow.is_some(),
            },
        });
    }
    (total, kind)
}

fn weaker(a: BasisKind, b: BasisKind) -> BasisKind {
    match (a, b) {
        (BasisKind::Unknown, _) | (_, BasisKind::Unknown) => BasisKind::Unknown,
        (BasisKind::Estimated, _) | (_, BasisKind::Estimated) => BasisKind::Estimated,
        _ => BasisKind::Known,
    }
}

#[allow(clippy::too_many_arguments)]
fn outgoing(
    inputs: &Inputs,
    tx_id: &str,
    at: i64,
    account: &str,
    m: &Movement,
    events: &mut Vec<Event>,
    notes: &mut BTreeMap<String, LegNote>,
    flows: &mut Vec<FlowRow>,
) {
    let o = inputs.leg_overrides.get(&m.leg.id);
    let class = o
        .and_then(|o| o.classification)
        .filter(|c| c.is_outgoing())
        .unwrap_or(LegClassification::Unclassified);
    let (value, estimated) = movement_value(inputs, m, at);
    let event_id = format!("{tx_id}|1|out|{account}|{}", m.leg.asset_id);
    let mut nt = note(m.leg, "");
    nt.counterparty = m.counterparty.clone();
    nt.value_usd = value.clone();
    nt.value_estimated = estimated;
    match class {
        LegClassification::Sale | LegClassification::Payment => {
            let proceeds = match o {
                Some(o) if o.proceeds_usd.is_some() => {
                    o.proceeds_usd.as_deref().and_then(|p| parse_dec(p).ok())
                }
                Some(o) if o.proceeds_from_market => value.clone(),
                _ => None,
            };
            events.push(Event {
                id: event_id.clone(),
                order: order(at),
                kind: EventKind::Dispose {
                    account: account.to_owned(),
                    asset: m.leg.asset_id.clone(),
                    quantity: m.remaining.clone(),
                    proceeds_usd: proceeds.clone(),
                },
            });
            // Proceeds leave the tracked scope (fiat or goods).
            flows.push(FlowRow {
                event_id,
                at,
                from: Some(account.to_owned()),
                to: None,
                asset_id: m.leg.asset_id.clone(),
                quantity: m.remaining.clone(),
                value_usd: proceeds.clone(),
                classified: true,
            });
            nt.treatment = if class == LegClassification::Sale {
                "sale"
            } else {
                "payment"
            };
            nt.review = proceeds.is_none().then_some("missing_proceeds");
            nt.proceeds_usd = proceeds;
        }
        LegClassification::Withdrawal
        | LegClassification::Gift
        | LegClassification::OwnUntracked => {
            events.push(Event {
                id: event_id.clone(),
                order: order(at),
                kind: EventKind::Withdraw {
                    account: account.to_owned(),
                    asset: m.leg.asset_id.clone(),
                    quantity: m.remaining.clone(),
                    market_value_usd: value.clone(),
                    classified: true,
                },
            });
            flows.push(FlowRow {
                event_id,
                at,
                from: Some(account.to_owned()),
                to: None,
                asset_id: m.leg.asset_id.clone(),
                quantity: m.remaining.clone(),
                value_usd: value.clone(),
                classified: true,
            });
            nt.treatment = match class {
                LegClassification::Gift => "gift",
                LegClassification::OwnUntracked => "own_untracked",
                _ => "withdrawal",
            };
            nt.review = value.is_none().then_some("missing_price");
        }
        _ => {
            events.push(Event {
                id: event_id.clone(),
                order: order(at),
                kind: EventKind::Withdraw {
                    account: account.to_owned(),
                    asset: m.leg.asset_id.clone(),
                    quantity: m.remaining.clone(),
                    market_value_usd: value.clone(),
                    classified: false,
                },
            });
            flows.push(FlowRow {
                event_id,
                at,
                from: Some(account.to_owned()),
                to: None,
                asset_id: m.leg.asset_id.clone(),
                quantity: m.remaining.clone(),
                value_usd: value.clone(),
                classified: false,
            });
            nt.treatment = "unclassified_out";
            nt.review = inputs
                .reviewable(&m.leg.asset_id, &value)
                .then_some("unclassified_outgoing");
        }
    }
    notes.insert(m.leg.id.clone(), nt);
}

#[allow(clippy::too_many_arguments)]
fn incoming(
    inputs: &Inputs,
    tx_id: &str,
    at: i64,
    account: &str,
    m: &Movement,
    events: &mut Vec<Event>,
    notes: &mut BTreeMap<String, LegNote>,
    flows: &mut Vec<FlowRow>,
) {
    let o = inputs.leg_overrides.get(&m.leg.id);
    let class = o
        .and_then(|o| o.classification)
        .filter(|c| c.is_incoming())
        .unwrap_or(LegClassification::Deposit);
    let (value, estimated) = movement_value(inputs, m, at);
    let event_id = format!("{tx_id}|3|in|{account}|{}", m.leg.asset_id);
    let mut nt = note(m.leg, "");
    nt.counterparty = m.counterparty.clone();
    nt.value_usd = value.clone();
    nt.value_estimated = estimated;
    if class == LegClassification::Reward {
        events.push(Event {
            id: event_id,
            order: order(at),
            kind: EventKind::Reward {
                account: account.to_owned(),
                asset: m.leg.asset_id.clone(),
                quantity: m.remaining.clone(),
                value_usd: value.clone(),
            },
        });
        nt.treatment = "reward";
        nt.basis_usd = value.clone();
        nt.basis_kind = Some(if value.is_some() {
            BasisKind::Known
        } else {
            BasisKind::Unknown
        });
        nt.review = value.is_none().then_some("missing_price");
        notes.insert(m.leg.id.clone(), nt);
        return;
    }
    // Deposit: external capital inflow at market value; basis from the user.
    flows.push(FlowRow {
        event_id: event_id.clone(),
        at,
        from: None,
        to: Some(account.to_owned()),
        asset_id: m.leg.asset_id.clone(),
        quantity: m.remaining.clone(),
        value_usd: value.clone(),
        classified: true,
    });
    nt.treatment = "deposit";
    let lots = o.and_then(|o| o.basis_lots.clone());
    if let Some(lots) = lots {
        let (basis, kind) = push_basis_lots(
            &event_id,
            at,
            account,
            &m.leg.asset_id,
            &m.remaining,
            &lots,
            Some(value.clone()),
            events,
        );
        nt.basis_usd = basis;
        nt.basis_kind = Some(kind);
        nt.review = (kind == BasisKind::Unknown).then_some("unknown_basis");
    } else {
        let estimated_basis = o
            .is_some_and(|o| o.basis_from_market)
            .then(|| value.clone())
            .flatten();
        let kind = if estimated_basis.is_some() {
            BasisKind::Estimated
        } else {
            BasisKind::Unknown
        };
        events.push(Event {
            id: event_id,
            order: order(at),
            kind: EventKind::Acquire {
                account: account.to_owned(),
                asset: m.leg.asset_id.clone(),
                quantity: m.remaining.clone(),
                basis_usd: estimated_basis.clone(),
                basis_kind: kind,
                acquired_at: None,
                external_inflow_usd: value.clone(),
                is_external_inflow: true,
            },
        });
        nt.review = (estimated_basis.is_none() && inputs.reviewable(&m.leg.asset_id, &value))
            .then_some("unknown_basis");
        nt.basis_usd = estimated_basis;
        nt.basis_kind = Some(kind);
    }
    notes.insert(m.leg.id.clone(), nt);
}

// ---------------------------------------------------------------- persistence

fn basis_kind_str(kind: BasisKind) -> &'static str {
    match kind {
        BasisKind::Known => "known",
        BasisKind::Estimated => "estimated",
        BasisKind::Unknown => "unknown",
    }
}

pub(crate) fn parse_basis_kind(text: &str) -> BasisKind {
    match text {
        "known" => BasisKind::Known,
        "estimated" => BasisKind::Estimated,
        _ => BasisKind::Unknown,
    }
}

fn canon(d: &Dec) -> String {
    to_canonical(d)
}

fn opt(d: &Option<Dec>) -> Option<String> {
    d.as_ref().map(canon)
}

fn summarize(c: &Computation) -> RecalcSummary {
    let all: BTreeSet<String> = c
        .ledger
        .accounts_with_totals()
        .map(|(a, _)| a.clone())
        .collect();
    let totals = c.ledger.totals_for(&all);
    let mut remaining_known = Dec::zero();
    let mut unknown = 0u32;
    for lot in c.ledger.all_lots() {
        match &lot.basis_usd {
            Some(b) => remaining_known += b,
            None => unknown += 1,
        }
    }
    RecalcSummary {
        realized_known_usd: canon(&totals.realized_usd.known),
        realized_complete: totals.realized_usd.complete,
        remaining_known_basis_usd: canon(&remaining_known),
        unknown_basis_lots: unknown,
        review_items: u32::try_from(c.notes.values().filter(|n| n.review.is_some()).count())
            .unwrap_or(u32::MAX),
        inventory_gaps: u32::try_from(c.ledger.gaps().len()).unwrap_or(u32::MAX),
    }
}

impl Store {
    /// Marks derived accounting results out of date.
    pub(crate) async fn mark_accounting_dirty(&self) -> Result<()> {
        sqlx::query(
            "INSERT INTO app_meta (key, value) VALUES ('accounting_dirty', '1')
             ON CONFLICT(key) DO UPDATE SET value = '1'",
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn accounting_dirty(&self) -> Result<bool> {
        let v: Option<String> =
            sqlx::query_scalar("SELECT value FROM app_meta WHERE key = 'accounting_dirty'")
                .fetch_optional(&self.pool)
                .await?;
        Ok(v.as_deref() != Some("0"))
    }

    /// Replays when evidence, prices or decisions changed since the last replay.
    pub async fn replay_if_dirty(&self) -> Result<Option<ReplayReport>> {
        if self.accounting_dirty().await? {
            Ok(Some(self.replay_accounting().await?))
        } else {
            Ok(None)
        }
    }

    pub(crate) async fn load_inputs(&self) -> Result<Inputs> {
        let mut ordered = Vec::new();
        for row in sqlx::query(
            "SELECT id, network_id, occurred_at, block_height, position FROM chain_transactions",
        )
        .fetch_all(&self.pool)
        .await?
        {
            let position: Option<String> = row.get("position");
            ordered.push((
                row.get::<i64, _>("occurred_at"),
                row.get::<String, _>("network_id"),
                row.get::<Option<i64>, _>("block_height")
                    .unwrap_or(i64::MAX),
                position
                    .as_deref()
                    .and_then(|p| p.parse::<u64>().ok())
                    .unwrap_or(u64::MAX),
                row.get::<String, _>("id"),
            ));
        }
        ordered.sort();
        let transaction_order = ordered
            .into_iter()
            .enumerate()
            .map(|(i, (_, _, _, _, id))| (id, i as u64 + 1))
            .collect();
        let owned: BTreeSet<String> = sqlx::query_scalar("SELECT id FROM accounts")
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .collect();
        let mut assets = BTreeMap::new();
        for r in sqlx::query("SELECT a.id,a.decimals,CASE WHEN p.exclude_override=1 THEN 'spam' WHEN p.exclude_override=0 AND a.verification='spam' THEN 'unverified' ELSE a.verification END AS verification FROM assets a LEFT JOIN asset_preferences p ON p.asset_id=a.id")
            .fetch_all(&self.pool)
            .await?
        {
            assets.insert(
                r.get::<String, _>("id"),
                AssetInfo {
                    decimals: u32::try_from(r.get::<i64, _>("decimals"))
                        .map_err(|_| StoreError::Corrupt("asset decimals".into()))?,
                    verification: r.get("verification"),
                },
            );
        }
        let qty = |asset: &str, raw: &str| -> Result<Dec> {
            let info = assets
                .get(asset)
                .ok_or_else(|| StoreError::Corrupt(format!("unknown asset {asset}")))?;
            Ok(raw_to_quantity(&parse_raw_amount(raw)?, info.decimals))
        };
        // Pending and dropped transactions do not enter the ledger; failed
        // ones only through their actually paid fee.
        let mut legs = Vec::new();
        for r in sqlx::query(
            "SELECT l.id, l.transaction_id, l.account_id, l.asset_id, l.signed_raw_quantity, t.occurred_at,a.canonical_address,json_extract(l.evidence,'$.counterparty') AS counterparty
             FROM activity_legs l JOIN chain_transactions t ON t.id = l.transaction_id JOIN accounts a ON a.id=l.account_id
             WHERE t.status IN ('confirmed', 'final')
             ORDER BY t.occurred_at, l.transaction_id, l.id",
        )
        .fetch_all(&self.pool)
        .await?
        {
            let asset: String = r.get("asset_id");
            let quantity = qty(&asset, &r.get::<String, _>("signed_raw_quantity"))?;
            if quantity.is_zero() {
                continue;
            }
            legs.push(LegRow {
                address:r.get("canonical_address"),
                counterparty:r.get("counterparty"),
                id: r.get("id"),
                tx_id: r.get("transaction_id"),
                account_id: r.get("account_id"),
                asset_id: asset,
                quantity,
                at: r.get("occurred_at"),
            });
        }
        let mut fees = Vec::new();
        for r in sqlx::query(
            "SELECT f.id, f.transaction_id, f.payer_account_id, f.asset_id, f.raw_quantity, t.occurred_at
             FROM transaction_fees f JOIN chain_transactions t ON t.id = f.transaction_id
             WHERE t.status IN ('confirmed', 'final', 'failed') AND f.payer_account_id IS NOT NULL
               AND f.attribution != 'sponsored'
             ORDER BY t.occurred_at, f.transaction_id, f.id",
        )
        .fetch_all(&self.pool)
        .await?
        {
            let asset: String = r.get("asset_id");
            let quantity = qty(&asset, &r.get::<String, _>("raw_quantity"))?;
            if quantity.is_zero() {
                continue;
            }
            fees.push(FeeRow {
                id: r.get("id"),
                tx_id: r.get("transaction_id"),
                account_id: r.get("payer_account_id"),
                asset_id: asset,
                quantity,
                at: r.get("occurred_at"),
            });
        }
        let (leg_overrides, openings) = self.current_overrides().await?;

        let wanted: BTreeSet<&str> = legs
            .iter()
            .map(|l| l.asset_id.as_str())
            .chain(fees.iter().map(|f| f.asset_id.as_str()))
            .chain(openings.iter().map(|(_, o)| o.asset_id.as_str()))
            .collect();
        let mut prices = PriceBook::default();
        for r in sqlx::query(
            "SELECT asset_id, price_usd, observed_at, granularity FROM prices
             WHERE quality != 'low_confidence' ORDER BY asset_id, observed_at, id",
        )
        .fetch_all(&self.pool)
        .await?
        {
            let asset: String = r.get("asset_id");
            if !wanted.contains(asset.as_str()) {
                continue;
            }
            prices.push(
                &asset,
                r.get("observed_at"),
                r.get("price_usd"),
                &r.get::<String, _>("granularity"),
            );
        }
        prices.finish();
        Ok(Inputs {
            transaction_order,
            owned,
            assets,
            legs,
            fees,
            leg_overrides,
            openings,
            prices,
        })
    }

    /// Latest non-empty version of each leg override and each opening lot.
    pub(crate) async fn current_overrides(
        &self,
    ) -> Result<(BTreeMap<String, LegOverride>, Vec<(String, OpeningLot)>)> {
        let rows = sqlx::query(
            "SELECT o.target_kind, o.target_id, o.payload FROM accounting_overrides o
             WHERE o.target_kind IN ('leg', 'lot') AND o.version = (
                 SELECT MAX(o2.version) FROM accounting_overrides o2
                 WHERE o2.target_kind = o.target_kind AND o2.target_id = o.target_id)
             ORDER BY o.target_kind, o.target_id",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut legs = BTreeMap::new();
        let mut openings = Vec::new();
        for r in rows {
            let kind: String = r.get("target_kind");
            let target: String = r.get("target_id");
            let payload: String = r.get("payload");
            if kind == "leg" {
                let o: LegOverride = serde_json::from_str(&payload)
                    .map_err(|e| StoreError::Corrupt(format!("override {target}: {e}")))?;
                if !o.is_empty() {
                    legs.insert(target, o);
                }
            } else if payload != "null" {
                let lot: OpeningLot = serde_json::from_str(&payload)
                    .map_err(|e| StoreError::Corrupt(format!("opening lot {target}: {e}")))?;
                openings.push((target, lot));
            }
        }
        Ok((legs, openings))
    }

    /// Computes what the ledger would look like with extra, uncommitted decisions.
    pub(crate) async fn recalc_preview(
        &self,
        extra_legs: &BTreeMap<String, LegOverride>,
        extra_openings: &[(String, OpeningLot)],
    ) -> Result<(RecalcSummary, RecalcSummary)> {
        let inputs = self.load_inputs().await?;
        let (before, mut inputs) =
            tokio::task::spawn_blocking(move || (summarize(&compute(&inputs)), inputs))
                .await
                .map_err(|e| StoreError::Corrupt(format!("accounting preview worker: {e}")))?;
        for (k, v) in extra_legs {
            inputs.leg_overrides.insert(k.clone(), v.clone());
        }
        inputs.openings.extend(extra_openings.iter().cloned());
        let wanted: BTreeSet<String> = extra_openings
            .iter()
            .map(|(_, o)| o.asset_id.clone())
            .filter(|a| !inputs.prices.series.contains_key(a))
            .collect();
        if !wanted.is_empty() {
            for r in sqlx::query(
                "SELECT asset_id, price_usd, observed_at, granularity FROM prices
                 WHERE quality != 'low_confidence' ORDER BY asset_id, observed_at, id",
            )
            .fetch_all(&self.pool)
            .await?
            {
                let asset: String = r.get("asset_id");
                if wanted.contains(&asset) {
                    inputs.prices.push(
                        &asset,
                        r.get("observed_at"),
                        r.get("price_usd"),
                        &r.get::<String, _>("granularity"),
                    );
                }
            }
            inputs.prices.finish();
        }
        let after = tokio::task::spawn_blocking(move || summarize(&compute(&inputs)))
            .await
            .map_err(|e| StoreError::Corrupt(format!("accounting preview worker: {e}")))?;
        Ok((before, after))
    }

    /// Rebuilds every derived accounting table from evidence, prices and decisions.
    pub async fn replay_accounting(&self) -> Result<ReplayReport> {
        // Hold the same write gate from evidence loading through commit. A
        // sync/decision cannot be acknowledged by a replay that did not see it.
        let _guard = self.write_lock.lock().await;
        let inputs = self.load_inputs().await?;
        let (c, inputs) = tokio::task::spawn_blocking(move || (compute(&inputs), inputs))
            .await
            .map_err(|e| StoreError::Corrupt(format!("accounting worker: {e}")))?;
        let mismatches = self.balance_mismatches(&c.ledger).await?;
        let mut tx = self.pool.begin().await?;
        for statement in [
            "DELETE FROM lot_consumptions",
            "DELETE FROM lots",
            "DELETE FROM account_accounting",
            "DELETE FROM accounting_flows",
            "DELETE FROM leg_accounting",
            "DELETE FROM reconciliation_items",
        ] {
            sqlx::query(statement).execute(&mut *tx).await?;
        }

        let remaining: BTreeMap<u64, &Lot> = c.ledger.all_lots().map(|l| (l.id, l)).collect();
        let mut lot_count = 0u32;
        for origin in c.ledger.lot_origins() {
            let lot = &origin.lot;
            let live = remaining.get(&lot.id);
            sqlx::query(
                "INSERT INTO lots (id, account_id, asset_id, quantity, remaining_quantity, basis_usd,
                    remaining_basis_usd, basis_kind, acquired_at, arrived_at, parent_lot_id, source_event, method_version)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(format!("L{}", lot.id))
            .bind(&lot.account)
            .bind(&lot.asset)
            .bind(canon(&origin.quantity))
            .bind(live.map_or_else(|| "0".to_owned(), |l| canon(&l.quantity)))
            .bind(opt(&origin.basis_usd))
            .bind(match live {
                Some(l) => opt(&l.basis_usd),
                None => origin.basis_usd.as_ref().map(|_| "0".to_owned()),
            })
            .bind(basis_kind_str(lot.basis_kind))
            .bind(lot.acquired.time)
            .bind(lot.arrived.time)
            .bind(lot.parent.map(|p| format!("L{p}")))
            .bind(&lot.source_event)
            .bind(i64::from(portfolio_core::accounting::ACCOUNTING_ENGINE_VERSION))
            .execute(&mut *tx)
            .await?;
            lot_count += 1;
        }
        for (i, cons) in c.ledger.consumptions().iter().enumerate() {
            sqlx::query(
                "INSERT INTO lot_consumptions (id, lot_id, event_id, quantity, basis_usd, kind, method_version)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(format!("C{i:08}"))
            .bind(format!("L{}", cons.lot_id))
            .bind(&cons.event)
            .bind(canon(&cons.quantity))
            .bind(opt(&cons.basis_usd))
            .bind(cons.kind.as_str())
            .bind(i64::from(portfolio_core::accounting::ACCOUNTING_ENGINE_VERSION))
            .execute(&mut *tx)
            .await?;
        }
        let account_rows = c
            .ledger
            .accounts_with_totals()
            .map(|(a, t)| ((a.clone(), String::new()), t.clone()));
        let asset_rows = c
            .ledger
            .asset_totals()
            .map(|((a, s), t)| ((a.clone(), s.clone()), t.clone()));
        for ((account, asset), t) in account_rows.chain(asset_rows) {
            if !inputs.owned.contains(&account) {
                continue;
            }
            sqlx::query(
                "INSERT INTO account_accounting (account_id, asset_id, realized_usd, realized_complete,
                    income_usd, income_complete, expense_usd, expense_complete, fee_charges)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&account)
            .bind(&asset)
            .bind(canon(&t.realized_usd.known))
            .bind(i64::from(t.realized_usd.complete))
            .bind(canon(&t.income_usd.known))
            .bind(i64::from(t.income_usd.complete))
            .bind(canon(&t.expense_usd.known))
            .bind(i64::from(t.expense_usd.complete))
            .bind(i64::from(t.fee_charges))
            .execute(&mut *tx)
            .await?;
        }
        for f in &c.flows {
            sqlx::query(
                "INSERT INTO accounting_flows (event_id, at, from_account_id, to_account_id, asset_id, quantity, value_usd, classified)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&f.event_id)
            .bind(f.at)
            .bind(&f.from)
            .bind(&f.to)
            .bind(&f.asset_id)
            .bind(canon(&f.quantity))
            .bind(opt(&f.value_usd))
            .bind(i64::from(f.classified))
            .execute(&mut *tx)
            .await?;
        }
        let mut review = 0u32;
        for (leg_id, n) in &c.notes {
            review += u32::from(n.review.is_some());
            sqlx::query(
                "INSERT INTO leg_accounting (leg_id, transaction_id, account_id, asset_id, occurred_at, quantity,
                    treatment, counterparty_account_id, value_usd, value_estimated, basis_usd, basis_kind,
                    proceeds_usd, review)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(leg_id)
            .bind(&n.tx_id)
            .bind(&n.account_id)
            .bind(&n.asset_id)
            .bind(n.at)
            .bind(canon(&n.quantity))
            .bind(n.treatment)
            .bind(&n.counterparty)
            .bind(opt(&n.value_usd))
            .bind(i64::from(n.value_estimated))
            .bind(opt(&n.basis_usd))
            .bind(n.basis_kind.map(basis_kind_str))
            .bind(opt(&n.proceeds_usd))
            .bind(n.review)
            .execute(&mut *tx)
            .await?;
        }
        let mut recon = 0u32;
        for (i, item) in c.items.iter().chain(mismatches.iter()).enumerate() {
            recon += 1;
            sqlx::query(
                "INSERT INTO reconciliation_items (id, kind, account_id, asset_id, event_id, quantity, detail)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(format!("R{i:06}"))
            .bind(item.kind)
            .bind(&item.account_id)
            .bind(&item.asset_id)
            .bind(&item.event_id)
            .bind(opt(&item.quantity))
            .bind(&item.detail)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query("UPDATE accounting_overrides SET orphaned = 0 WHERE target_kind = 'leg'")
            .execute(&mut *tx)
            .await?;
        for id in &c.orphaned_legs {
            sqlx::query(
                "UPDATE accounting_overrides SET orphaned = 1 WHERE target_kind = 'leg' AND target_id = ?",
            )
            .bind(id)
            .execute(&mut *tx)
            .await?;
        }
        for (key, value) in [
            ("accounting_dirty", "0".to_owned()),
            ("accounting_replayed_at", self.now().to_string()),
        ] {
            sqlx::query(
                "INSERT INTO app_meta (key, value) VALUES (?, ?)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            )
            .bind(key)
            .bind(value)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(ReplayReport {
            events: u32::try_from(c.event_count).unwrap_or(u32::MAX),
            lots: lot_count,
            review_items: review,
            reconciliation_items: recon,
            orphaned_overrides: u32::try_from(c.orphaned_legs.len()).unwrap_or(u32::MAX),
        })
    }

    /// Compares the replayed inventory with the latest balance observation of
    /// every account that has downloaded history (SPECIFICATION.md §8.1 step 7).
    /// A difference is reported, never turned into a purchase or a sale.
    async fn balance_mismatches(&self, ledger: &Ledger) -> Result<Vec<ReconItem>> {
        let mut held: BTreeMap<(String, String), Dec> = BTreeMap::new();
        for lot in ledger.all_lots() {
            *held
                .entry((lot.account.clone(), lot.asset.clone()))
                .or_insert_with(Dec::zero) += &lot.quantity;
        }
        let rows = sqlx::query(
            "SELECT b.account_id, b.asset_id, b.raw_quantity, s.decimals, CASE WHEN p.exclude_override=1 THEN 'spam' WHEN p.exclude_override=0 AND s.verification='spam' THEN 'unverified' ELSE s.verification END AS verification,
                    (SELECT c.coverage FROM sync_checkpoints c WHERE c.account_id = b.account_id AND c.category = 'history') AS coverage,
                    (SELECT COUNT(*) FROM account_transactions x JOIN chain_transactions t ON t.id = x.transaction_id
                     WHERE x.account_id = b.account_id AND t.status = 'pending') AS pending,
                    (SELECT COUNT(*) FROM account_transactions x WHERE x.account_id = b.account_id) AS txs
             FROM balance_observations b JOIN assets s ON s.id = b.asset_id LEFT JOIN asset_preferences p ON p.asset_id=s.id
             WHERE b.status IN ('fresh', 'stale') AND b.id = (
                 SELECT b2.id FROM balance_observations b2
                 WHERE b2.account_id = b.account_id AND b2.asset_id = b.asset_id
                 ORDER BY b2.observed_at DESC, b2.id DESC LIMIT 1)
             ORDER BY b.account_id, b.asset_id",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut observed: BTreeMap<(String, String), (Dec, String)> = BTreeMap::new();
        let mut accounts_with_history: BTreeMap<String, (String, i64)> = BTreeMap::new();
        for r in rows {
            if r.get::<i64, _>("txs") == 0 || r.get::<String, _>("verification") == "spam" {
                continue;
            }
            let account: String = r.get("account_id");
            let decimals = u32::try_from(r.get::<i64, _>("decimals")).unwrap_or(0);
            let q = raw_to_quantity(
                &parse_raw_amount(&r.get::<String, _>("raw_quantity"))?,
                decimals,
            );
            let coverage: Option<String> = r.get("coverage");
            accounts_with_history.insert(
                account.clone(),
                (coverage.clone().unwrap_or_default(), r.get("pending")),
            );
            observed.insert(
                (account, r.get("asset_id")),
                (q, coverage.unwrap_or_default()),
            );
        }
        let keys: BTreeSet<(String, String)> = observed
            .keys()
            .cloned()
            .chain(
                held.keys()
                    .filter(|(a, _)| accounts_with_history.contains_key(a))
                    .cloned(),
            )
            .collect();
        let mut out = Vec::new();
        for key in keys {
            let (account_coverage, pending) = accounts_with_history
                .get(&key.0)
                .cloned()
                .unwrap_or_default();
            if pending > 0 {
                continue; // pending movements are outside the ledger; compare later
            }
            if account_coverage != "complete" {
                // While older history is still loading, a difference is expected:
                // the unexplained balance is valued as an unknown-basis lot instead.
                continue;
            }
            let obs = observed
                .get(&key)
                .map(|(q, _)| q.clone())
                .unwrap_or_default();
            let ledger_q = held.get(&key).cloned().unwrap_or_default();
            let diff = &obs - &ledger_q;
            if diff.is_zero() {
                continue;
            }
            out.push(ReconItem {
                kind: "balance_mismatch",
                account_id: Some(key.0),
                asset_id: Some(key.1),
                event_id: None,
                quantity: Some(diff),
                detail: "the reported balance differs from the complete synchronized history"
                    .into(),
            });
        }
        Ok(out)
    }

    /// Lifetime totals of a deduplicated set of accounts (optionally one asset).
    pub(crate) async fn scope_totals(
        &self,
        accounts: &BTreeSet<String>,
        asset: Option<&str>,
    ) -> Result<AccountTotals> {
        let rows = sqlx::query(
            "SELECT account_id, realized_usd, realized_complete, income_usd, income_complete,
                    expense_usd, expense_complete, fee_charges
             FROM account_accounting WHERE asset_id = ?",
        )
        .bind(asset.unwrap_or(""))
        .fetch_all(&self.pool)
        .await?;
        let mut out = AccountTotals::default();
        let add = |sum: &mut PartialSum, value: &str, complete: i64| -> Result<()> {
            sum.known += parse_dec(value)?;
            sum.complete &= complete != 0;
            Ok(())
        };
        for r in rows {
            if !accounts.contains(&r.get::<String, _>("account_id")) {
                continue;
            }
            add(
                &mut out.realized_usd,
                &r.get::<String, _>("realized_usd"),
                r.get("realized_complete"),
            )?;
            add(
                &mut out.income_usd,
                &r.get::<String, _>("income_usd"),
                r.get("income_complete"),
            )?;
            add(
                &mut out.expense_usd,
                &r.get::<String, _>("expense_usd"),
                r.get("expense_complete"),
            )?;
            out.fee_charges += u32::try_from(r.get::<i64, _>("fee_charges")).unwrap_or(0);
        }
        Ok(out)
    }

    /// Exact, compact lot inputs for valuation only. Identical decimal strings
    /// are counted by SQLite, then multiplied in Rust: SQL SUM would coerce
    /// monetary text to floating point. No FIFO identity/order is needed here.
    /// Detailed lot/audit views continue to use `scope_lots`.
    pub(crate) async fn scope_valuation_lots(
        &self,
        accounts: &BTreeSet<String>,
    ) -> Result<BTreeMap<(String, String), Vec<Lot>>> {
        if accounts.is_empty() {
            return Ok(BTreeMap::new());
        }
        let mut query = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
            "SELECT account_id, asset_id, remaining_quantity, remaining_basis_usd,
                    basis_kind, COUNT(*) AS copies
             FROM lots WHERE remaining_quantity != '0' AND account_id IN (",
        );
        let mut ids = query.separated(",");
        for account in accounts {
            ids.push_bind(account);
        }
        ids.push_unseparated(
            ") GROUP BY account_id, asset_id, remaining_quantity, remaining_basis_usd, basis_kind",
        );
        let rows = query.build().fetch_all(&self.pool).await?;
        let mut out: BTreeMap<(String, String), Vec<Lot>> = BTreeMap::new();
        for row in rows {
            let account: String = row.get("account_id");
            let asset: String = row.get("asset_id");
            let copies = Dec::from(row.get::<i64, _>("copies"));
            out.entry((account.clone(), asset.clone()))
                .or_default()
                .push(Lot {
                    id: 0,
                    account,
                    asset,
                    quantity: parse_dec(&row.get::<String, _>("remaining_quantity"))? * &copies,
                    basis_usd: parse_opt(&row.get("remaining_basis_usd"))?
                        .map(|basis| basis * &copies),
                    basis_kind: parse_basis_kind(&row.get::<String, _>("basis_kind")),
                    acquired: order(0),
                    arrived: order(0),
                    source_event: String::new(),
                    parent: None,
                });
        }
        Ok(out)
    }

    /// Remaining lots of the scope: `(account, asset) -> lots`.
    pub(crate) async fn scope_lots(
        &self,
        accounts: &BTreeSet<String>,
    ) -> Result<BTreeMap<(String, String), Vec<Lot>>> {
        if accounts.is_empty() {
            return Ok(BTreeMap::new());
        }
        let mut query = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
            "SELECT id, account_id, asset_id, remaining_quantity, remaining_basis_usd, basis_kind,
                    acquired_at, arrived_at, source_event
             FROM lots WHERE remaining_quantity != '0' AND account_id IN (",
        );
        let mut ids = query.separated(",");
        for account in accounts {
            ids.push_bind(account);
        }
        ids.push_unseparated(") ORDER BY account_id, asset_id, acquired_at, id");
        let rows = query.build().fetch_all(&self.pool).await?;
        let mut out: BTreeMap<(String, String), Vec<Lot>> = BTreeMap::new();
        for r in rows {
            let account: String = r.get("account_id");
            if !accounts.contains(&account) {
                continue;
            }
            let asset: String = r.get("asset_id");
            let id: String = r.get("id");
            out.entry((account.clone(), asset.clone()))
                .or_default()
                .push(Lot {
                    id: id.trim_start_matches('L').parse().unwrap_or(0),
                    account,
                    asset,
                    quantity: parse_dec(&r.get::<String, _>("remaining_quantity"))?,
                    basis_usd: parse_opt(&r.get("remaining_basis_usd"))?,
                    basis_kind: parse_basis_kind(&r.get::<String, _>("basis_kind")),
                    acquired: order(r.get("acquired_at")),
                    arrived: order(r.get("arrived_at")),
                    source_event: r.get("source_event"),
                    parent: None,
                });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Dec {
        parse_dec(s).unwrap()
    }

    #[test]
    fn split_conserves_the_total_exactly() {
        let parts = split(&d("100"), &[d("1"), d("1"), d("1")]).unwrap();
        assert_eq!(parts.iter().cloned().sum::<Dec>(), d("100"));
        assert!(split(&d("1"), &[d("0")]).is_none());
    }

    #[test]
    fn price_book_respects_tolerance_and_prefers_the_nearest_point() {
        let mut book = PriceBook::default();
        book.push("a", 1_000, "10".into(), "day");
        book.push("a", 1_000 + DAY, "20".into(), "day");
        book.push("a", 200_000, "30".into(), "tick");
        book.finish();
        assert_eq!(book.at("a", 1_000 + 100).unwrap(), (d("10"), true));
        assert_eq!(book.at("a", 1_000 + DAY - 100).unwrap(), (d("20"), true));
        assert_eq!(book.at("a", 200_100).unwrap(), (d("30"), false));
        // A tick does not cover a point 10 minutes away, and nothing else does.
        assert!(book.at("a", 200_000 + 600 + DAY).is_none());
        assert!(book.at("b", 1_000).is_none());
    }
}
