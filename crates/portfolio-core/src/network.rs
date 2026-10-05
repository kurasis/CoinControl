//! The finite first-release network matrix (SPECIFICATION.md §3.2).

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

/// Address/transaction model shared by a set of networks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum NetworkFamily {
    Bitcoin,
    Evm,
    Solana,
    Tron,
    Ton,
}

/// A supported mainnet. Testnets are separate configurations and never enter a
/// mainnet portfolio total, so they are deliberately absent here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum NetworkId {
    Bitcoin,
    Ethereum,
    Base,
    Arbitrum,
    Optimism,
    Polygon,
    Bsc,
    Solana,
    Tron,
    Ton,
}

impl NetworkId {
    pub const ALL: [NetworkId; 10] = [
        NetworkId::Bitcoin,
        NetworkId::Ethereum,
        NetworkId::Base,
        NetworkId::Arbitrum,
        NetworkId::Optimism,
        NetworkId::Polygon,
        NetworkId::Bsc,
        NetworkId::Solana,
        NetworkId::Tron,
        NetworkId::Ton,
    ];

    /// Stable identifier persisted in the database and used over IPC.
    pub fn as_str(self) -> &'static str {
        match self {
            NetworkId::Bitcoin => "bitcoin",
            NetworkId::Ethereum => "ethereum",
            NetworkId::Base => "base",
            NetworkId::Arbitrum => "arbitrum",
            NetworkId::Optimism => "optimism",
            NetworkId::Polygon => "polygon",
            NetworkId::Bsc => "bsc",
            NetworkId::Solana => "solana",
            NetworkId::Tron => "tron",
            NetworkId::Ton => "ton",
        }
    }

    pub fn parse(text: &str) -> Result<Self, CoreError> {
        NetworkId::ALL
            .into_iter()
            .find(|n| n.as_str() == text)
            .ok_or_else(|| CoreError::UnknownNetwork(text.to_owned()))
    }

    pub fn family(self) -> NetworkFamily {
        match self {
            NetworkId::Bitcoin => NetworkFamily::Bitcoin,
            NetworkId::Solana => NetworkFamily::Solana,
            NetworkId::Tron => NetworkFamily::Tron,
            NetworkId::Ton => NetworkFamily::Ton,
            _ => NetworkFamily::Evm,
        }
    }

    /// EVM chain ID, when applicable.
    pub fn evm_chain_id(self) -> Option<u64> {
        match self {
            NetworkId::Ethereum => Some(1),
            NetworkId::Base => Some(8453),
            NetworkId::Arbitrum => Some(42161),
            NetworkId::Optimism => Some(10),
            NetworkId::Polygon => Some(137),
            NetworkId::Bsc => Some(56),
            _ => None,
        }
    }

    /// English display name. Localized names live in the frontend.
    pub fn display_name(self) -> &'static str {
        match self {
            NetworkId::Bitcoin => "Bitcoin",
            NetworkId::Ethereum => "Ethereum",
            NetworkId::Base => "Base",
            NetworkId::Arbitrum => "Arbitrum One",
            NetworkId::Optimism => "Optimism",
            NetworkId::Polygon => "Polygon PoS",
            NetworkId::Bsc => "BNB Smart Chain",
            NetworkId::Solana => "Solana",
            NetworkId::Tron => "TRON",
            NetworkId::Ton => "TON",
        }
    }

    /// Symbol of the native fee asset.
    pub fn native_symbol(self) -> &'static str {
        match self {
            NetworkId::Bitcoin => "BTC",
            NetworkId::Ethereum | NetworkId::Base | NetworkId::Arbitrum | NetworkId::Optimism => {
                "ETH"
            }
            NetworkId::Polygon => "POL",
            NetworkId::Bsc => "BNB",
            NetworkId::Solana => "SOL",
            NetworkId::Tron => "TRX",
            NetworkId::Ton => "TON",
        }
    }

    /// Public block explorer origin (HTTPS, no trailing slash).
    pub fn explorer_origin(self) -> &'static str {
        match self {
            NetworkId::Bitcoin => "https://mempool.space",
            NetworkId::Ethereum => "https://etherscan.io",
            NetworkId::Base => "https://basescan.org",
            NetworkId::Arbitrum => "https://arbiscan.io",
            NetworkId::Optimism => "https://optimistic.etherscan.io",
            NetworkId::Polygon => "https://polygonscan.com",
            NetworkId::Bsc => "https://bscscan.com",
            NetworkId::Solana => "https://solscan.io",
            NetworkId::Tron => "https://tronscan.org",
            NetworkId::Ton => "https://tonviewer.com",
        }
    }

    /// Explorer page of a transaction. Only identifiers made of explorer-safe
    /// characters are linked; anything else yields `None`.
    pub fn explorer_tx_url(self, tx: &str) -> Option<String> {
        if tx.is_empty()
            || !tx
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
        {
            return None;
        }
        let origin = self.explorer_origin();
        Some(match self {
            NetworkId::Tron => format!("{origin}/#/transaction/{tx}"),
            NetworkId::Ton => format!("{origin}/transaction/{tx}"),
            _ => format!("{origin}/tx/{tx}"),
        })
    }

    /// Explorer page of an address or token contract.
    pub fn explorer_address_url(self, address: &str) -> Option<String> {
        if address.is_empty()
            || !address
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b':'))
        {
            return None;
        }
        let origin = self.explorer_origin();
        Some(match self {
            NetworkId::Tron => format!("{origin}/#/address/{address}"),
            NetworkId::Ton => format!("{origin}/{address}"),
            NetworkId::Solana => format!("{origin}/account/{address}"),
            _ => format!("{origin}/address/{address}"),
        })
    }

    /// Decimals of the native asset's smallest unit.
    pub fn native_decimals(self) -> u32 {
        match self {
            NetworkId::Bitcoin => 8,
            NetworkId::Solana | NetworkId::Ton => 9,
            NetworkId::Tron => 6,
            _ => 18,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_round_trip() {
        for n in NetworkId::ALL {
            assert_eq!(NetworkId::parse(n.as_str()).unwrap(), n);
        }
        assert!(NetworkId::parse("sepolia").is_err());
    }

    #[test]
    fn explorer_links_reject_unsafe_identifiers() {
        assert_eq!(
            NetworkId::Ethereum.explorer_tx_url("0xabc").as_deref(),
            Some("https://etherscan.io/tx/0xabc")
        );
        assert!(NetworkId::Ethereum.explorer_tx_url("0xabc/../x").is_none());
        assert!(NetworkId::Bitcoin.explorer_tx_url("").is_none());
        assert!(NetworkId::Ton.explorer_address_url("0:ab?x=1").is_none());
        for n in NetworkId::ALL {
            assert!(n.explorer_origin().starts_with("https://"));
        }
    }

    #[test]
    fn evm_chain_ids_match_the_spec() {
        let ids: Vec<_> = NetworkId::ALL
            .iter()
            .filter_map(|n| n.evm_chain_id())
            .collect();
        assert_eq!(ids, vec![1, 8453, 42161, 10, 137, 56]);
    }
}
