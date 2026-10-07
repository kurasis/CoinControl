//! Read-only provider adapters and synchronization for Portfolio Desk.
//!
//! Adapters translate documented provider responses into the storage crate's
//! normalized `TxSpec`/`AssetSpec` types. Nothing here signs, sends, or writes
//! to any blockchain. Credentials are passed in by the caller and never appear
//! in errors or logs.

pub mod alchemy;
pub mod capabilities;
pub mod defillama;
pub mod error;
pub mod esplora;
pub mod finality;
pub mod helius;
pub mod http;
pub mod livecoinwatch;
pub mod mirrors;
pub mod network_log;
mod rpc;
pub mod sync;
pub mod tonapi;
pub mod trongrid;
mod util;
pub mod zerion;

pub use error::ProviderError;
pub use sync::{
    AccountSyncReport, PriceHistoryReport, PriceReport, Providers, SyncEngine, SyncOptions,
};
