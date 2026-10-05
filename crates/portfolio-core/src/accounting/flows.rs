//! External capital flows for a selected scope (ACCOUNTING.md §7.1, §8).

use std::collections::BTreeSet;

use super::dietz::ExternalFlow;
use super::ledger::{AccountKey, Event, EventKind};
use crate::decimal::Dec;

/// External flows of a scope, plus whether every flow could be valued and classified.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScopeFlows {
    pub flows: Vec<ExternalFlow>,
    pub complete: bool,
}

/// Derives the signed external flows crossing the boundary of `scope`.
///
/// Own transfers inside the scope cancel; transfers crossing the scope boundary
/// count at fair market value. Trades, fees and rewards inside the scope are not
/// capital flows. Unclassified outgoing movements make the result incomplete.
pub fn scope_external_flows(events: &[Event], scope: &BTreeSet<AccountKey>) -> ScopeFlows {
    let mut out = ScopeFlows {
        flows: Vec::new(),
        complete: true,
    };
    let mut seen = BTreeSet::new();
    for event in events {
        if !seen.insert(event.id.as_str()) {
            continue;
        }
        match &event.kind {
            EventKind::OwnTransfer {
                from,
                to,
                market_value_usd,
                ..
            } => {
                let from_in = scope.contains(from);
                let to_in = scope.contains(to);
                if from_in != to_in {
                    push(event.order.time, market_value_usd, to_in, &mut out);
                }
            }
            EventKind::Acquire {
                account,
                is_external_inflow: true,
                external_inflow_usd,
                ..
            } if scope.contains(account) => {
                push(event.order.time, external_inflow_usd, true, &mut out);
            }
            EventKind::Withdraw {
                account,
                market_value_usd,
                classified,
                ..
            } if scope.contains(account) => {
                if !classified {
                    out.complete = false;
                }
                push(event.order.time, market_value_usd, false, &mut out);
            }
            _ => {}
        }
    }
    out
}

fn push(time: i64, value: &Option<Dec>, inflow: bool, out: &mut ScopeFlows) {
    match value {
        Some(v) => out.flows.push(ExternalFlow {
            time,
            amount_usd: if inflow { v.clone() } else { -v.clone() },
        }),
        None => out.complete = false,
    }
}
