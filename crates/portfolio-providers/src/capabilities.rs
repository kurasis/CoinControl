//! Per-network capability and coverage report (SPECIFICATION.md §3.2, §13 D).
//!
//! What this build's adapters actually read for each required network, as a
//! typed table the UI renders next to each account's live coverage. The
//! entries describe implemented behavior and its known gaps; they are not
//! provider marketing claims. Limitation codes are translated by the UI.

use portfolio_core::network::NetworkId;
use serde::Serialize;

use crate::{alchemy, esplora, helius, tonapi, trongrid, zerion};

/// How completely a capability is covered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum Support {
    Full,
    Partial,
    None,
}

/// A history category read by the adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum HistoryCategory {
    /// Native-asset transfers and fees.
    Native,
    /// Fungible token transfers (ERC-20/BEP-20, SPL, TRC-20, Jettons).
    Tokens,
    /// Swaps and other multi-leg activity interpreted by the source.
    Trades,
    /// Failed transactions (fee only).
    Failed,
    /// Value moved by contract execution (internal transfers).
    Internal,
    /// Pending (unconfirmed) activity.
    Pending,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct NetworkCapability {
    pub fallback_providers: Vec<String>,
    pub network: NetworkId,
    pub provider: String,
    /// The source needs an API key in this build.
    pub key_required: bool,
    pub balances: Support,
    pub token_discovery: Support,
    pub history: Vec<HistoryCategory>,
    pub fees: Support,
    pub internal_transfers: Support,
    /// Limitation codes; the UI explains each one.
    pub limitations: Vec<String>,
    /// Date of the last successful live check recorded in the repository's
    /// test report (`docs/NETWORK_COVERAGE.md`), if any.
    pub live_verified_on: Option<String>,
}

/// Live evidence date for the networks verified in stage D.
const VERIFIED: &str = "2026-10-05";

fn cap(
    network: NetworkId,
    provider: &str,
    key_required: bool,
    history: &[HistoryCategory],
    (balances, tokens, fees, internal): (Support, Support, Support, Support),
    limitations: &[&str],
) -> NetworkCapability {
    let reserves: &[&str] = match network {
        NetworkId::Bitcoin => &["mempool"],
        NetworkId::Ethereum | NetworkId::Arbitrum => {
            &["blockscout", "etherscan", "drpc", "publicnode"]
        }
        NetworkId::Polygon => &["etherscan", "drpc", "publicnode"],
        NetworkId::Optimism => &["blockscout", "drpc", "publicnode"],
        NetworkId::Base => &["drpc", "publicnode"],
        NetworkId::Bsc => &["drpc", "publicnode"],
        NetworkId::Solana => &["alchemy", "chainstack", "publicnode"],
        NetworkId::Tron => &["publicnode"],
        NetworkId::Ton => &["toncenter"],
    };
    NetworkCapability {
        fallback_providers: reserves.iter().map(|p| (*p).to_owned()).collect(),
        network,
        provider: provider.to_owned(),
        key_required,
        balances,
        token_discovery: tokens,
        history: history.to_vec(),
        fees,
        internal_transfers: internal,
        limitations: limitations.iter().map(|s| (*s).to_owned()).collect(),
        live_verified_on: Some(VERIFIED.to_owned()),
    }
}

/// The capability table for every required network, in matrix order.
pub fn network_capabilities() -> Vec<NetworkCapability> {
    use HistoryCategory::*;
    use Support::{Full, Partial};
    const EVM_LIMITS: &[&str] = &["zerion_simple_positions", "zerion_interpreted_history"];
    NetworkId::ALL
        .into_iter()
        .map(|n| match n {
            NetworkId::Bitcoin => cap(
                n,
                esplora::PROVIDER,
                false,
                &[Native, Failed, Pending],
                (Full, Support::None, Full, Support::None),
                &["btc_single_address", "btc_pending_cap"],
            ),
            NetworkId::Ethereum
            | NetworkId::Base
            | NetworkId::Arbitrum
            | NetworkId::Optimism
            | NetworkId::Bsc => cap(
                n,
                zerion::PROVIDER,
                true,
                &[Native, Tokens, Trades, Failed, Internal],
                (Full, Full, Full, Partial),
                EVM_LIMITS,
            ),
            NetworkId::Polygon => cap(
                n,
                zerion::PROVIDER,
                true,
                &[Native, Tokens, Trades, Failed, Internal],
                (Full, Full, Full, Partial),
                &[
                    "zerion_simple_positions",
                    "zerion_interpreted_history",
                    "polygon_native_contract",
                ],
            ),
            NetworkId::Solana => cap(
                n,
                zerion::PROVIDER,
                true,
                &[Native, Tokens, Trades, Failed],
                (Full, Full, Full, Partial),
                &[
                    "zerion_simple_positions",
                    "solana_zerion_interpreted",
                    "solana_token_2022",
                ],
            ),
            NetworkId::Tron => cap(
                n,
                trongrid::PROVIDER,
                true,
                &[Native, Tokens, Failed, Internal],
                (Full, Partial, Full, Partial),
                &[
                    "tron_staked_included",
                    "tron_trc10_unsupported",
                    "tron_token_metadata",
                    "tron_unverified_tokens",
                    "confirmed_only",
                ],
            ),
            NetworkId::Ton => cap(
                n,
                tonapi::PROVIDER,
                false,
                &[Native, Tokens, Trades, Failed, Pending],
                (Full, Full, Full, Partial),
                &[
                    "ton_events_canonical",
                    "ton_contract_calls_partial",
                    "ton_nft_excluded",
                    "ton_key_recommended",
                ],
            ),
        })
        .collect()
}

/// Reflects the configured primary adapters; new sources never inherit another's live evidence.
pub fn configured_capabilities(use_alchemy: bool, use_helius: bool) -> Vec<NetworkCapability> {
    let mut caps = network_capabilities();
    for c in &mut caps {
        if use_alchemy && alchemy::NETWORKS.contains(&c.network) {
            c.provider = alchemy::PROVIDER.into();
            c.fallback_providers.insert(0, "zerion".into());
            c.token_discovery = Support::Partial;
            c.history = vec![HistoryCategory::Native, HistoryCategory::Tokens];
            c.fees = Support::Partial;
            c.internal_transfers = Support::None;
            c.limitations = vec![
                "alchemy_transfer_index".into(),
                "alchemy_bounded_discovery".into(),
                "rpc_unverified_tokens".into(),
                "confirmed_only".into(),
            ];
            c.live_verified_on = None;
        } else if use_helius && c.network == NetworkId::Solana {
            c.provider = helius::PROVIDER.into();
            c.fallback_providers.insert(0, "zerion".into());
            c.balances = Support::Partial;
            c.token_discovery = Support::Partial;
            c.history = vec![
                HistoryCategory::Native,
                HistoryCategory::Tokens,
                HistoryCategory::Failed,
            ];
            c.limitations = vec![
                "helius_full_history".into(),
                "helius_program_effects".into(),
                "helius_fungible_scope".into(),
                "rpc_unverified_tokens".into(),
                "confirmed_only".into(),
            ];
            c.live_verified_on = None;
        }
    }
    caps
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::SyncEngine;

    #[test]
    fn configured_sources_keep_bnb_on_zerion_and_do_not_inherit_live_evidence() {
        let caps = configured_capabilities(true, true);
        for n in alchemy::NETWORKS {
            let c = caps.iter().find(|c| c.network == n).unwrap();
            assert_eq!(c.provider, "alchemy");
            assert!(c.live_verified_on.is_none());
            assert_eq!(c.internal_transfers, Support::None);
        }
        assert_eq!(
            caps.iter()
                .find(|c| c.network == NetworkId::Solana)
                .unwrap()
                .provider,
            "helius"
        );
        assert_eq!(
            caps.iter()
                .find(|c| c.network == NetworkId::Bsc)
                .unwrap()
                .provider,
            "zerion"
        );
    }

    #[test]
    fn every_required_network_has_a_working_adapter() {
        let caps = network_capabilities();
        assert_eq!(caps.len(), NetworkId::ALL.len());
        for c in &caps {
            assert_eq!(
                SyncEngine::provider_for(c.network),
                Some(c.provider.as_str()),
                "{:?}",
                c.network
            );
            assert!(c.history.contains(&HistoryCategory::Native));
            assert_eq!(c.balances, Support::Full);
        }
    }
}
