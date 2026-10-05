//! Flow-adjusted period performance with Modified Dietz (ACCOUNTING.md §8).

use bigdecimal::Zero;

use super::valuation::UnavailableReason;
use crate::decimal::{Dec, div};

/// A signed external capital flow: inflow positive, outflow negative.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalFlow {
    /// Unix seconds (UTC).
    pub time: i64,
    pub amount_usd: Dec,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeriodPerformance {
    pub gain_usd: Dec,
    pub denominator_usd: Dec,
    pub return_percent: Option<Dec>,
    pub reason: Option<UnavailableReason>,
}

/// Computes Modified Dietz for the interval `(start, end]`.
///
/// Only flows with `start < t <= end` are included; a flow at `start` is
/// already part of the beginning value. Weights use actual elapsed seconds.
pub fn modified_dietz(
    start: i64,
    end: i64,
    beginning_value_usd: &Dec,
    ending_value_usd: &Dec,
    flows: &[ExternalFlow],
) -> PeriodPerformance {
    let included: Vec<&ExternalFlow> = flows
        .iter()
        .filter(|f| start < f.time && f.time <= end)
        .collect();
    let flow_sum: Dec = included.iter().map(|f| f.amount_usd.clone()).sum();
    let gain = ending_value_usd - beginning_value_usd - &flow_sum;

    if end <= start {
        return PeriodPerformance {
            gain_usd: gain,
            denominator_usd: beginning_value_usd.clone(),
            return_percent: None,
            reason: Some(UnavailableReason::ZeroDuration),
        };
    }

    let duration = Dec::from(end - start);
    let mut denominator = beginning_value_usd.clone();
    for flow in included {
        let weight = div(&Dec::from(end - flow.time), &duration).unwrap_or_else(Dec::zero);
        denominator += weight * &flow.amount_usd;
    }

    if denominator <= Dec::zero() {
        return PeriodPerformance {
            gain_usd: gain,
            denominator_usd: denominator,
            return_percent: None,
            reason: Some(UnavailableReason::NonpositiveDenominator),
        };
    }

    let return_percent = div(&(&gain * Dec::from(100)), &denominator);
    PeriodPerformance {
        gain_usd: gain,
        denominator_usd: denominator,
        return_percent,
        reason: None,
    }
}
