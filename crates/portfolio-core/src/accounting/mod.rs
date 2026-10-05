//! Accounting engine. `docs/spec/ACCOUNTING.md` owns the semantics.

pub mod dietz;
pub mod flows;
pub mod ledger;
pub mod valuation;

pub use dietz::{ExternalFlow, PeriodPerformance, modified_dietz};
pub use flows::{ScopeFlows, scope_external_flows};
pub use ledger::{
    AccountKey, AccountTotals, AssetKey, BasisKind, ChainOrder, ConsumptionKind, Event, EventId,
    EventKind, Ledger, Lot, LotConsumption, LotOrigin, PartialSum, ReconciliationGap,
};
pub use valuation::{
    KnownSubset, PositionSummary, UnavailableReason, price_change_percent, ratio_to_percent,
    summarize_position,
};

/// Version of the accounting semantics; persisted with derived results.
pub const ACCOUNTING_ENGINE_VERSION: u32 = 3;

use crate::decimal::Dec;

/// `Unrealized + Realized + Income - Expenses`, only when every component is known.
pub fn total_accounted_pnl(unrealized: Option<&Dec>, totals: &AccountTotals) -> Option<Dec> {
    let realized = totals.realized_usd.value()?;
    let income = totals.income_usd.value()?;
    let expense = totals.expense_usd.value()?;
    Some(unrealized? + realized + income - expense)
}
