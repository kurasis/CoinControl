//! Data source catalog (docs/spec/API_PROVIDERS.md §1-§2).
//!
//! This catalog drives the Data Sources settings screen so the user can enter
//! keys only for features they enable.

use portfolio_core::network::NetworkId;
use serde::Serialize;

use crate::secrets::KeyStorage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum ProviderRole {
    MarketPrices,
    AccountData,
    SupplementalPrices,
    Supplemental,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum KeyRequirement {
    /// The feature this provider serves cannot run without a key.
    Required,
    /// Works anonymously at a stricter limit; a free key is recommended.
    Recommended,
    Optional,
    NotNeeded,
}

pub struct ProviderSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub role: ProviderRole,
    pub key: KeyRequirement,
    pub networks: &'static [NetworkId],
    pub docs_url: &'static str,
    pub key_url: Option<&'static str>,
    pub free_allowance: &'static str,
}

const EVM_AND_SOLANA: &[NetworkId] = &[
    NetworkId::Ethereum,
    NetworkId::Base,
    NetworkId::Arbitrum,
    NetworkId::Optimism,
    NetworkId::Polygon,
    NetworkId::Bsc,
    NetworkId::Solana,
];

pub const PROVIDERS: &[ProviderSpec] = &[
    ProviderSpec {
        id: "livecoinwatch",
        name: "Live Coin Watch",
        role: ProviderRole::MarketPrices,
        key: KeyRequirement::Required,
        networks: &[],
        docs_url: "https://livecoinwatch.github.io/lcw-api-docs/",
        key_url: Some("https://www.livecoinwatch.com/tools/api"),
        free_allowance: "10,000 requests/day",
    },
    ProviderSpec {
        id: "zerion",
        name: "Zerion",
        role: ProviderRole::AccountData,
        key: KeyRequirement::Required,
        networks: EVM_AND_SOLANA,
        docs_url: "https://developers.zerion.io/",
        key_url: Some("https://dashboard.zerion.io/"),
        free_allowance: "2,000 requests/day, 3 requests/sec",
    },
    ProviderSpec {
        id: "esplora",
        name: "Blockstream Esplora",
        role: ProviderRole::AccountData,
        key: KeyRequirement::NotNeeded,
        networks: &[NetworkId::Bitcoin],
        docs_url: "https://github.com/Blockstream/esplora/blob/master/API.md",
        key_url: None,
        free_allowance: "Public service, fair use",
    },
    ProviderSpec {
        id: "trongrid",
        name: "TronGrid",
        role: ProviderRole::AccountData,
        key: KeyRequirement::Required,
        networks: &[NetworkId::Tron],
        docs_url: "https://developers.tron.network/reference/select-network",
        key_url: Some("https://www.trongrid.io/"),
        free_allowance: "Per your TronGrid console",
    },
    ProviderSpec {
        id: "tonapi",
        name: "TonAPI",
        role: ProviderRole::AccountData,
        key: KeyRequirement::Recommended,
        networks: &[NetworkId::Ton],
        docs_url: "https://docs.tonapi.io/tonapi",
        key_url: Some("https://tonconsole.com/"),
        free_allowance: "1 request/sec with a free key",
    },
    ProviderSpec {
        id: "defillama",
        name: "DefiLlama",
        role: ProviderRole::SupplementalPrices,
        key: KeyRequirement::NotNeeded,
        networks: &[],
        docs_url: "https://api-docs.defillama.com/",
        key_url: None,
        free_allowance: "Public free access",
    },
    ProviderSpec {
        id: "helius",
        name: "Helius",
        role: ProviderRole::AccountData,
        key: KeyRequirement::Optional,
        networks: &[NetworkId::Solana],
        docs_url: "https://www.helius.dev/docs",
        key_url: Some("https://dashboard.helius.dev/"),
        free_allowance: "1M credits/month",
    },
    ProviderSpec {
        id: "alchemy",
        name: "Alchemy",
        role: ProviderRole::AccountData,
        key: KeyRequirement::Optional,
        networks: &[
            NetworkId::Ethereum,
            NetworkId::Base,
            NetworkId::Arbitrum,
            NetworkId::Optimism,
            NetworkId::Polygon,
        ],
        docs_url: "https://www.alchemy.com/docs",
        key_url: Some("https://dashboard.alchemy.com/"),
        free_allowance: "30M compute units/month",
    },
    ProviderSpec {
        id: "mempool",
        name: "mempool.space",
        role: ProviderRole::Supplemental,
        key: KeyRequirement::NotNeeded,
        networks: &[NetworkId::Bitcoin],
        docs_url: "https://mempool.space/docs/api/rest",
        key_url: None,
        free_allowance: "Public service, fair use",
    },
    ProviderSpec {
        id: "blockscout",
        name: "Blockscout PRO",
        role: ProviderRole::Supplemental,
        key: KeyRequirement::Optional,
        networks: &[
            NetworkId::Ethereum,
            NetworkId::Base,
            NetworkId::Arbitrum,
            NetworkId::Optimism,
            NetworkId::Polygon,
        ],
        docs_url: "https://docs.blockscout.com/devs/pro-api",
        key_url: Some("https://dev.blockscout.com/"),
        free_allowance: "100K credits/day, 5 requests/sec; most calls cost 20 credits",
    },
    ProviderSpec {
        id: "etherscan",
        name: "Etherscan V2",
        role: ProviderRole::Supplemental,
        key: KeyRequirement::Optional,
        networks: &[NetworkId::Ethereum, NetworkId::Arbitrum, NetworkId::Polygon],
        docs_url: "https://docs.etherscan.io/supported-chains",
        key_url: Some("https://etherscan.io/myapikey"),
        free_allowance: "100K calls/day, 3 requests/sec; selected free chains",
    },
    ProviderSpec {
        id: "drpc",
        name: "dRPC",
        role: ProviderRole::Supplemental,
        key: KeyRequirement::Optional,
        networks: &[
            NetworkId::Ethereum,
            NetworkId::Base,
            NetworkId::Arbitrum,
            NetworkId::Optimism,
            NetworkId::Polygon,
            NetworkId::Bsc,
            NetworkId::Solana,
        ],
        docs_url: "https://drpc.org/docs/gettingstarted/firstrequest",
        key_url: Some("https://drpc.org/"),
        free_allowance: "210M CU/month; free public nodes",
    },
    ProviderSpec {
        id: "chainstack",
        name: "Chainstack Solana RPC",
        role: ProviderRole::Supplemental,
        key: KeyRequirement::Optional,
        networks: &[NetworkId::Solana],
        docs_url: "https://docs.chainstack.com/docs/authentication-methods-for-different-scenarios",
        key_url: Some("https://console.chainstack.com/"),
        free_allowance: "3M requests/month, 25 requests/sec, one node; node auth token or HTTPS endpoint",
    },
    ProviderSpec {
        id: "toncenter",
        name: "TON Center v3",
        role: ProviderRole::Supplemental,
        key: KeyRequirement::Recommended,
        networks: &[NetworkId::Ton],
        docs_url: "https://docs.ton.org/api/v3/overview",
        key_url: Some("https://t.me/toncenter"),
        free_allowance: "10 requests/sec with a free key; 1/sec without",
    },
    ProviderSpec {
        id: "publicnode",
        name: "PublicNode",
        role: ProviderRole::Supplemental,
        key: KeyRequirement::NotNeeded,
        networks: &[
            NetworkId::Ethereum,
            NetworkId::Base,
            NetworkId::Arbitrum,
            NetworkId::Optimism,
            NetworkId::Polygon,
            NetworkId::Bsc,
            NetworkId::Solana,
            NetworkId::Tron,
        ],
        docs_url: "https://www.publicnode.com/",
        key_url: None,
        free_allowance: "Public RPC, fair use; no guaranteed quota",
    },
    ProviderSpec {
        id: "ankr",
        name: "Ankr",
        role: ProviderRole::Supplemental,
        key: KeyRequirement::Optional,
        networks: &[],
        docs_url: "https://www.ankr.com/docs/rpc-service/service-plans/",
        key_url: Some("https://www.ankr.com/rpc/"),
        free_allowance: "200M credits/month",
    },
];

/// Providers with a working adapter in this build.
const ADAPTERS: &[&str] = &[
    "mempool",
    "blockscout",
    "etherscan",
    "drpc",
    "chainstack",
    "toncenter",
    "publicnode",
    "livecoinwatch",
    "zerion",
    "esplora",
    "defillama",
    "trongrid",
    "tonapi",
    "helius",
    "alchemy",
];

pub fn find(id: &str) -> Option<&'static ProviderSpec> {
    PROVIDERS.iter().find(|p| p.id == id)
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ProviderStatus {
    pub id: String,
    pub name: String,
    pub role: ProviderRole,
    pub key_requirement: KeyRequirement,
    pub networks: Vec<NetworkId>,
    pub docs_url: String,
    pub key_url: Option<String>,
    pub free_allowance: String,
    /// `None` when no key is stored. A stored key is never returned.
    pub key_storage: Option<KeyStorage>,
    /// Requests counted by this app today (UTC); other apps sharing the key are not included.
    pub requests_today: u32,
    pub estimated_credits_today: u32,
    pub last_error: Option<String>,
    /// Adapter availability in this build.
    pub adapter_available: bool,
}

impl ProviderStatus {
    pub fn from_spec(spec: &ProviderSpec, key_storage: Option<KeyStorage>) -> Self {
        ProviderStatus {
            id: spec.id.into(),
            name: spec.name.into(),
            role: spec.role,
            key_requirement: spec.key,
            networks: spec.networks.to_vec(),
            docs_url: spec.docs_url.into(),
            key_url: spec.key_url.map(Into::into),
            free_allowance: spec.free_allowance.into(),
            key_storage,
            requests_today: 0,
            estimated_credits_today: 0,
            last_error: None,
            adapter_available: ADAPTERS.contains(&spec.id),
        }
    }
}
