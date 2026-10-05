//! Error type for domain operations.

use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CoreError {
    #[error("invalid decimal value: {0:?}")]
    InvalidDecimal(String),
    #[error("value {value} has more precision than {decimals} decimals allow")]
    ExcessPrecision { value: String, decimals: u32 },
    #[error("unknown network identifier: {0:?}")]
    UnknownNetwork(String),
    #[error("invalid {network} address: {reason}")]
    InvalidAddress {
        network: &'static str,
        reason: String,
    },
}
