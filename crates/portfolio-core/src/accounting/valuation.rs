//! Current valuation and unrealized return (ACCOUNTING.md §4).

use bigdecimal::Zero;
use serde::{Deserialize, Serialize};

use super::ledger::{BasisKind, Lot};
use crate::decimal::{Dec, div};

/// Why a metric is not shown as a number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum UnavailableReason {
    MissingPrice,
    MissingBasis,
    ZeroBasis,
    ZeroDuration,
    NonpositiveDenominator,
    IncompleteClassification,
}

/// P&L limited to lots whose basis is known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownSubset {
    pub quantity: Dec,
    pub value_usd: Option<Dec>,
    pub basis_usd: Dec,
    pub unrealized_usd: Option<Dec>,
    pub unrealized_percent: Option<Dec>,
}

/// Valuation of a set of lots of one asset at one price.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PositionSummary {
    pub quantity: Dec,
    pub value_usd: Option<Dec>,
    /// Remaining basis when every lot's basis is known.
    pub basis_usd: Option<Dec>,
    pub unrealized_usd: Option<Dec>,
    pub unrealized_percent: Option<Dec>,
    /// First reason the whole-position figures are incomplete, if any.
    pub reason: Option<UnavailableReason>,
    pub known_subset: KnownSubset,
    /// Share of quantity with known (or estimated) basis, in percent.
    pub basis_coverage_quantity_percent: Option<Dec>,
    /// True when any included basis is estimated rather than known.
    pub has_estimated_basis: bool,
}

fn percent(numerator: &Dec, denominator: &Dec) -> Option<Dec> {
    div(&(numerator * Dec::from(100)), denominator)
}

/// Values lots at `price_usd`. A missing price is never treated as zero.
pub fn summarize_position<'a>(
    lots: impl IntoIterator<Item = &'a Lot>,
    price_usd: Option<&Dec>,
) -> PositionSummary {
    let mut quantity = Dec::zero();
    let mut known_quantity = Dec::zero();
    let mut known_basis = Dec::zero();
    let mut all_known = true;
    let mut has_estimated = false;
    for lot in lots {
        quantity += &lot.quantity;
        match &lot.basis_usd {
            Some(b) => {
                known_quantity += &lot.quantity;
                known_basis += b;
                has_estimated |= lot.basis_kind == BasisKind::Estimated;
            }
            None => all_known = false,
        }
    }

    let value = price_usd.map(|p| &quantity * p);
    let known_value = price_usd.map(|p| &known_quantity * p);
    let known_unrealized = known_value.as_ref().map(|v| v - &known_basis);
    let known_percent = known_unrealized
        .as_ref()
        .and_then(|u| percent(u, &known_basis));

    let basis = all_known.then(|| known_basis.clone());
    let unrealized = match (&value, &basis) {
        (Some(v), Some(b)) => Some(v - b),
        _ => None,
    };
    let unrealized_percent = match (&unrealized, &basis) {
        (Some(u), Some(b)) => percent(u, b),
        _ => None,
    };
    let reason = if price_usd.is_none() {
        Some(UnavailableReason::MissingPrice)
    } else if !all_known {
        Some(UnavailableReason::MissingBasis)
    } else if basis.as_ref().is_some_and(Zero::is_zero) {
        Some(UnavailableReason::ZeroBasis)
    } else {
        None
    };

    PositionSummary {
        basis_coverage_quantity_percent: percent(&known_quantity, &quantity),
        quantity,
        value_usd: value,
        basis_usd: basis,
        unrealized_usd: unrealized,
        unrealized_percent,
        reason,
        known_subset: KnownSubset {
            quantity: known_quantity,
            value_usd: known_value,
            basis_usd: known_basis,
            unrealized_usd: known_unrealized,
            unrealized_percent: known_percent,
        },
        has_estimated_basis: has_estimated,
    }
}

/// Converts a Live Coin Watch ratio-style change (`delta.day`) into percent.
/// `1.05 -> 5`, `0.8 -> -20`, `1 -> 0`.
pub fn ratio_to_percent(ratio: &Dec) -> Dec {
    (ratio - Dec::from(1)) * Dec::from(100)
}

/// Market price change `(P_now / P_then - 1) * 100`; `None` unless `P_then > 0`.
pub fn price_change_percent(now: &Dec, then: &Dec) -> Option<Dec> {
    if *then <= Dec::zero() {
        return None;
    }
    div(now, then).map(|r| ratio_to_percent(&r))
}
