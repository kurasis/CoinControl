//! FIFO lot ledger (ACCOUNTING.md §3, §5, §6, §7).
//!
//! The ledger replays normalized economic events in a deterministic order and
//! maintains per-account, per-asset FIFO lots plus realized P&L, income and
//! expense totals. Unknown values stay unknown: a component that cannot be
//! computed marks its total incomplete instead of being treated as zero.

use std::collections::{BTreeMap, BTreeSet};

use bigdecimal::Zero;
use serde::{Deserialize, Serialize};

use crate::decimal::{Dec, div};

/// Opaque, stable account identifier (one address on one network).
pub type AccountKey = String;
/// Opaque, stable chain-specific asset identifier. Never a bare ticker.
pub type AssetKey = String;
/// Stable identity of a normalized economic event.
pub type EventId = String;

/// Certainty of a lot's acquisition basis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum BasisKind {
    Known,
    Estimated,
    Unknown,
}

/// Position of an event in chain order. Ties are broken by event ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ChainOrder {
    /// Unix seconds (UTC).
    pub time: i64,
    /// Network-specific position within the same second (block/index/logical time).
    pub seq: u64,
}

/// A FIFO inventory lot held by one account for one exact asset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lot {
    pub id: u64,
    pub account: AccountKey,
    pub asset: AssetKey,
    pub quantity: Dec,
    /// `None` means unknown basis, which is different from an explicit zero.
    pub basis_usd: Option<Dec>,
    pub basis_kind: BasisKind,
    /// Original acquisition position; preserved through own transfers.
    pub acquired: ChainOrder,
    /// When the inventory arrived in this account.
    pub arrived: ChainOrder,
    pub source_event: EventId,
    /// Lot this one was split off by an own transfer, if any.
    pub parent: Option<u64>,
}

/// A lot as it was created, kept after it is consumed for lineage reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LotOrigin {
    pub lot: Lot,
    /// Quantity and basis when created (the live lot shows what remains).
    pub quantity: Dec,
    pub basis_usd: Option<Dec>,
}

/// Why inventory left a lot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsumptionKind {
    Disposal,
    Fee,
    Transfer,
    Withdrawal,
}

impl ConsumptionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ConsumptionKind::Disposal => "disposal",
            ConsumptionKind::Fee => "fee",
            ConsumptionKind::Transfer => "transfer",
            ConsumptionKind::Withdrawal => "withdrawal",
        }
    }
}

/// One fragment taken from a lot by an event; links disposals to acquisitions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LotConsumption {
    pub lot_id: u64,
    pub event: EventId,
    pub quantity: Dec,
    pub basis_usd: Option<Dec>,
    pub kind: ConsumptionKind,
}

impl Lot {
    fn fifo_key(&self) -> (ChainOrder, u64) {
        (self.acquired, self.id)
    }
}

/// A normalized economic event. Quantities are exact decimal asset units.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventKind {
    /// Inventory entering an account: a purchase, an opening lot, or a receipt
    /// from an untracked sender. `basis_usd = None` means unknown basis.
    Acquire {
        account: AccountKey,
        asset: AssetKey,
        quantity: Dec,
        basis_usd: Option<Dec>,
        basis_kind: BasisKind,
        /// Original acquisition time when it differs from arrival (e.g. CSV basis).
        acquired_at: Option<ChainOrder>,
        /// Market value when this receipt is an external capital inflow.
        external_inflow_usd: Option<Dec>,
        is_external_inflow: bool,
    },
    /// Explicitly classified reward/airdrop received at a reliable value.
    Reward {
        account: AccountKey,
        asset: AssetKey,
        quantity: Dec,
        value_usd: Option<Dec>,
    },
    /// Sale or payment with gross proceeds (`None` = unresolved proceeds).
    Dispose {
        account: AccountKey,
        asset: AssetKey,
        quantity: Dec,
        proceeds_usd: Option<Dec>,
    },
    /// Network/trading fee actually paid by `account`.
    Fee {
        account: AccountKey,
        asset: AssetKey,
        quantity: Dec,
        value_usd: Option<Dec>,
    },
    /// Movement between two owned accounts. Basis and acquisition order move with the lots.
    OwnTransfer {
        from: AccountKey,
        to: AccountKey,
        asset: AssetKey,
        quantity: Dec,
        /// Fair market value at transfer time, used for scope-crossing flows.
        market_value_usd: Option<Dec>,
    },
    /// Token swap inside one account with a consistent gross trade value.
    Swap {
        account: AccountKey,
        give_asset: AssetKey,
        give_quantity: Dec,
        get_asset: AssetKey,
        get_quantity: Dec,
        gross_value_usd: Option<Dec>,
        value_kind: BasisKind,
    },
    /// Outgoing movement to an untracked address. `classified = false` means
    /// the user has not yet said what it was; period performance is then incomplete.
    Withdraw {
        account: AccountKey,
        asset: AssetKey,
        quantity: Dec,
        market_value_usd: Option<Dec>,
        classified: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub id: EventId,
    pub order: ChainOrder,
    pub kind: EventKind,
}

/// A sum whose components may be partially unknown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartialSum {
    /// Sum of the components that are known.
    pub known: Dec,
    /// False when at least one component could not be computed.
    pub complete: bool,
}

impl Default for PartialSum {
    fn default() -> Self {
        PartialSum {
            known: Dec::zero(),
            complete: true,
        }
    }
}

impl PartialSum {
    fn add(&mut self, value: Option<Dec>) {
        match value {
            Some(v) => self.known += v,
            None => self.complete = false,
        }
    }

    fn merge(&mut self, other: &PartialSum) {
        self.known += &other.known;
        self.complete &= other.complete;
    }

    /// The total when fully known; `None` otherwise.
    pub fn value(&self) -> Option<Dec> {
        self.complete.then(|| self.known.clone())
    }
}

/// Lifetime accounting totals for one account (or a deduplicated union).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AccountTotals {
    pub realized_usd: PartialSum,
    pub income_usd: PartialSum,
    pub expense_usd: PartialSum,
    /// Number of fee events charged; each fee is charged exactly once.
    pub fee_charges: u32,
}

/// Inventory that was needed but not present: never a hidden negative lot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconciliationGap {
    pub event: EventId,
    pub account: AccountKey,
    pub asset: AssetKey,
    pub missing_quantity: Dec,
}

/// One consumed fragment of a lot.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Consumed {
    lot_id: u64,
    quantity: Dec,
    basis_usd: Option<Dec>,
    basis_kind: BasisKind,
    acquired: ChainOrder,
}

struct Consumption {
    fragments: Vec<Consumed>,
    shortfall: Dec,
}

impl Consumption {
    /// Total consumed basis if every fragment's basis is known and nothing was missing.
    fn known_basis(&self) -> Option<Dec> {
        if !self.shortfall.is_zero() {
            return None;
        }
        self.fragments
            .iter()
            .try_fold(Dec::zero(), |acc, f| f.basis_usd.as_ref().map(|b| acc + b))
    }
}

/// Deterministic FIFO ledger.
#[derive(Debug, Default)]
pub struct Ledger {
    lots: BTreeMap<(AccountKey, AssetKey), Vec<Lot>>,
    totals: BTreeMap<AccountKey, AccountTotals>,
    asset_totals: BTreeMap<(AccountKey, AssetKey), AccountTotals>,
    gaps: Vec<ReconciliationGap>,
    applied: BTreeSet<EventId>,
    next_lot_id: u64,
    origins: BTreeMap<u64, LotOrigin>,
    consumptions: Vec<LotConsumption>,
}

impl Ledger {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replays events in deterministic chain order (time, sequence, event ID).
    pub fn replay(events: &[Event]) -> Self {
        let mut sorted: Vec<&Event> = events.iter().collect();
        sorted.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.id.cmp(&b.id)));
        let mut ledger = Ledger::new();
        for event in sorted {
            ledger.apply(event);
        }
        ledger
    }

    /// Applies one event. Re-applying an event with the same ID has no effect.
    pub fn apply(&mut self, event: &Event) {
        if !self.applied.insert(event.id.clone()) {
            return;
        }
        match &event.kind {
            EventKind::Acquire {
                account,
                asset,
                quantity,
                basis_usd,
                basis_kind,
                acquired_at,
                ..
            } => {
                let kind = if basis_usd.is_none() {
                    BasisKind::Unknown
                } else {
                    *basis_kind
                };
                self.push_lot(
                    account,
                    asset,
                    quantity.clone(),
                    basis_usd.clone(),
                    kind,
                    acquired_at.unwrap_or(event.order),
                    event,
                    None,
                );
            }
            EventKind::Reward {
                account,
                asset,
                quantity,
                value_usd,
            } => {
                self.with_totals(account, asset, |t| t.income_usd.add(value_usd.clone()));
                let kind = if value_usd.is_some() {
                    BasisKind::Known
                } else {
                    BasisKind::Unknown
                };
                self.push_lot(
                    account,
                    asset,
                    quantity.clone(),
                    value_usd.clone(),
                    kind,
                    event.order,
                    event,
                    None,
                );
            }
            EventKind::Dispose {
                account,
                asset,
                quantity,
                proceeds_usd,
            } => {
                let consumed = self.consume(
                    account,
                    asset,
                    quantity,
                    &event.id,
                    ConsumptionKind::Disposal,
                );
                let realized = match (proceeds_usd, consumed.known_basis()) {
                    (Some(p), Some(b)) => Some(p - b),
                    _ => None,
                };
                self.with_totals(account, asset, |t| t.realized_usd.add(realized.clone()));
            }
            EventKind::Fee {
                account,
                asset,
                quantity,
                value_usd,
            } => {
                let consumed =
                    self.consume(account, asset, quantity, &event.id, ConsumptionKind::Fee);
                let realized = match (value_usd, consumed.known_basis()) {
                    (Some(v), Some(b)) => Some(v - b),
                    _ => None,
                };
                self.with_totals(account, asset, |t| {
                    t.realized_usd.add(realized.clone());
                    t.expense_usd.add(value_usd.clone());
                    t.fee_charges += 1;
                });
            }
            EventKind::OwnTransfer {
                from,
                to,
                asset,
                quantity,
                ..
            } => {
                let consumed =
                    self.consume(from, asset, quantity, &event.id, ConsumptionKind::Transfer);
                for fragment in consumed.fragments {
                    self.push_lot(
                        to,
                        asset,
                        fragment.quantity,
                        fragment.basis_usd,
                        fragment.basis_kind,
                        fragment.acquired,
                        event,
                        Some(fragment.lot_id),
                    );
                }
                if !consumed.shortfall.is_zero() {
                    // The coins did arrive on-chain; their lineage is unknown.
                    self.push_lot(
                        to,
                        asset,
                        consumed.shortfall,
                        None,
                        BasisKind::Unknown,
                        event.order,
                        event,
                        None,
                    );
                }
            }
            EventKind::Swap {
                account,
                give_asset,
                give_quantity,
                get_asset,
                get_quantity,
                gross_value_usd,
                value_kind,
            } => {
                let consumed = self.consume(
                    account,
                    give_asset,
                    give_quantity,
                    &event.id,
                    ConsumptionKind::Disposal,
                );
                let realized = match (gross_value_usd, consumed.known_basis()) {
                    (Some(v), Some(b)) => Some(v - b),
                    _ => None,
                };
                self.with_totals(account, give_asset, |t| {
                    t.realized_usd.add(realized.clone())
                });
                let kind = if gross_value_usd.is_some() {
                    *value_kind
                } else {
                    BasisKind::Unknown
                };
                self.push_lot(
                    account,
                    get_asset,
                    get_quantity.clone(),
                    gross_value_usd.clone(),
                    kind,
                    event.order,
                    event,
                    None,
                );
            }
            EventKind::Withdraw {
                account,
                asset,
                quantity,
                classified,
                ..
            } => {
                // Lots leave the tracked scope without a realized sale.
                self.consume(
                    account,
                    asset,
                    quantity,
                    &event.id,
                    ConsumptionKind::Withdrawal,
                );
                if !classified {
                    self.with_totals(account, asset, |t| t.realized_usd.complete = false);
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn push_lot(
        &mut self,
        account: &AccountKey,
        asset: &AssetKey,
        quantity: Dec,
        basis_usd: Option<Dec>,
        basis_kind: BasisKind,
        acquired: ChainOrder,
        event: &Event,
        parent: Option<u64>,
    ) {
        if quantity.is_zero() {
            return;
        }
        self.next_lot_id += 1;
        let lot = Lot {
            id: self.next_lot_id,
            account: account.clone(),
            asset: asset.clone(),
            quantity,
            basis_usd,
            basis_kind,
            acquired,
            arrived: event.order,
            source_event: event.id.clone(),
            parent,
        };
        self.origins.insert(
            lot.id,
            LotOrigin {
                quantity: lot.quantity.clone(),
                basis_usd: lot.basis_usd.clone(),
                lot: lot.clone(),
            },
        );
        let lots = self
            .lots
            .entry((account.clone(), asset.clone()))
            .or_default();
        let at = lots.partition_point(|l| l.fifo_key() <= lot.fifo_key());
        lots.insert(at, lot);
    }

    /// Consumes `quantity` FIFO from exactly this account's lots of this asset.
    fn consume(
        &mut self,
        account: &AccountKey,
        asset: &AssetKey,
        quantity: &Dec,
        event: &EventId,
        kind: ConsumptionKind,
    ) -> Consumption {
        let mut remaining = quantity.clone();
        let mut fragments = Vec::new();
        if let Some(lots) = self.lots.get_mut(&(account.clone(), asset.clone())) {
            while remaining > Dec::zero() && !lots.is_empty() {
                let lot = &mut lots[0];
                if lot.quantity <= remaining {
                    remaining -= &lot.quantity;
                    let whole = lots.remove(0);
                    fragments.push(Consumed {
                        lot_id: whole.id,
                        quantity: whole.quantity,
                        basis_usd: whole.basis_usd,
                        basis_kind: whole.basis_kind,
                        acquired: whole.acquired,
                    });
                } else {
                    // Split proportionally; the residual stays with the parent so
                    // the two children always sum exactly to the original basis.
                    let taken_basis = lot.basis_usd.as_ref().map(|basis| {
                        div(&(basis * &remaining), &lot.quantity).unwrap_or_else(Dec::zero)
                    });
                    if let (Some(basis), Some(taken)) = (lot.basis_usd.as_mut(), &taken_basis) {
                        *basis -= taken;
                    }
                    lot.quantity -= &remaining;
                    fragments.push(Consumed {
                        lot_id: lot.id,
                        quantity: remaining.clone(),
                        basis_usd: taken_basis,
                        basis_kind: lot.basis_kind,
                        acquired: lot.acquired,
                    });
                    remaining = Dec::zero();
                }
            }
        }
        for f in &fragments {
            self.consumptions.push(LotConsumption {
                lot_id: f.lot_id,
                event: event.clone(),
                quantity: f.quantity.clone(),
                basis_usd: f.basis_usd.clone(),
                kind,
            });
        }
        if remaining > Dec::zero() {
            self.gaps.push(ReconciliationGap {
                event: event.clone(),
                account: account.clone(),
                asset: asset.clone(),
                missing_quantity: remaining.clone(),
            });
        }
        Consumption {
            fragments,
            shortfall: remaining,
        }
    }

    /// Applies `update` to the account's totals and to its per-asset totals.
    fn with_totals(
        &mut self,
        account: &AccountKey,
        asset: &AssetKey,
        mut update: impl FnMut(&mut AccountTotals),
    ) {
        update(self.totals.entry(account.clone()).or_default());
        update(
            self.asset_totals
                .entry((account.clone(), asset.clone()))
                .or_default(),
        );
    }

    /// Remaining lots of `asset` across a deduplicated set of accounts.
    pub fn lots_for<'a>(
        &'a self,
        accounts: &'a BTreeSet<AccountKey>,
        asset: &'a str,
    ) -> impl Iterator<Item = &'a Lot> + 'a {
        accounts.iter().flat_map(move |account| {
            self.lots
                .get(&(account.clone(), asset.to_owned()))
                .into_iter()
                .flatten()
        })
    }

    /// All remaining lots, in deterministic account/asset/FIFO order.
    pub fn all_lots(&self) -> impl Iterator<Item = &Lot> {
        self.lots.values().flatten()
    }

    /// Lifetime totals for a deduplicated union of accounts.
    pub fn totals_for(&self, accounts: &BTreeSet<AccountKey>) -> AccountTotals {
        let mut out = AccountTotals::default();
        for account in accounts {
            if let Some(t) = self.totals.get(account) {
                out.realized_usd.merge(&t.realized_usd);
                out.income_usd.merge(&t.income_usd);
                out.expense_usd.merge(&t.expense_usd);
                out.fee_charges += t.fee_charges;
            }
        }
        out
    }

    pub fn gaps(&self) -> &[ReconciliationGap] {
        &self.gaps
    }

    /// Every lot ever created, with its original quantity and basis, by lot ID.
    pub fn lot_origins(&self) -> impl Iterator<Item = &LotOrigin> {
        self.origins.values()
    }

    /// Every consumed lot fragment, in application order.
    pub fn consumptions(&self) -> &[LotConsumption] {
        &self.consumptions
    }

    /// Lifetime totals per (account, asset): realized P&L and expenses belong
    /// to the disposed or fee asset, income to the received asset.
    pub fn asset_totals(&self) -> impl Iterator<Item = (&(AccountKey, AssetKey), &AccountTotals)> {
        self.asset_totals.iter()
    }

    /// Accounts with lifetime totals.
    pub fn accounts_with_totals(&self) -> impl Iterator<Item = (&AccountKey, &AccountTotals)> {
        self.totals.iter()
    }
}
