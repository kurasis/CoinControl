//! Provider errors. Messages name the provider and endpoint only; they never
//! contain credentials, request URLs, or response bodies beyond a short,
//! sanitized provider message.

use thiserror::Error;

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum ProviderError {
    #[error("{provider} {endpoint}: network access forbidden (HTTP 403)")]
    NetworkForbidden {
        provider: &'static str,
        endpoint: &'static str,
    },
    #[error("{provider} {endpoint}: RPC node temporarily unavailable (code {code})")]
    RpcUnavailable {
        provider: &'static str,
        endpoint: &'static str,
        code: i64,
    },
    #[error("{provider}: synchronization cancelled")]
    Cancelled { provider: &'static str },
    #[error("{provider} {endpoint}: credential rejected (HTTP {status})")]
    Auth {
        provider: &'static str,
        endpoint: &'static str,
        status: u16,
    },
    #[error("{provider}: no API key configured")]
    MissingKey { provider: &'static str },
    #[error("{provider} {endpoint}: rate limited{}", retry_after_secs.map(|s| format!(" (retry after {s}s)")).unwrap_or_default())]
    RateLimited {
        provider: &'static str,
        endpoint: &'static str,
        retry_after_secs: Option<u64>,
    },
    #[error("{provider}: local request budget exhausted")]
    BudgetExhausted { provider: &'static str },
    #[error("{provider} {endpoint}: not found")]
    NotFound {
        provider: &'static str,
        endpoint: &'static str,
    },
    #[error("{provider} {endpoint}: server error (HTTP {status})")]
    Server {
        provider: &'static str,
        endpoint: &'static str,
        status: u16,
    },
    #[error("{provider} {endpoint}: HTTP {status} {detail}")]
    Http {
        provider: &'static str,
        endpoint: &'static str,
        status: u16,
        detail: String,
    },
    #[error("{provider} {endpoint}: timed out")]
    Timeout {
        provider: &'static str,
        endpoint: &'static str,
    },
    #[error("{provider} {endpoint}: {detail}")]
    Network {
        provider: &'static str,
        endpoint: &'static str,
        detail: String,
    },
    #[error("{provider} {endpoint}: response too large")]
    TooLarge {
        provider: &'static str,
        endpoint: &'static str,
    },
    #[error("{provider} {endpoint}: invalid response: {detail}")]
    InvalidResponse {
        provider: &'static str,
        endpoint: &'static str,
        detail: String,
    },
    #[error("{provider} {endpoint}: pagination did not advance")]
    RepeatedCursor {
        provider: &'static str,
        endpoint: &'static str,
    },
}

impl ProviderError {
    /// Safe to retry the same request: throttling, server faults, timeouts.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            ProviderError::RateLimited { .. }
                | ProviderError::RpcUnavailable { .. }
                | ProviderError::Server { .. }
                | ProviderError::Timeout { .. }
                | ProviderError::Network { .. }
        )
    }

    /// Stop every further request to this provider in the current run.
    pub fn stops_provider(&self) -> bool {
        matches!(
            self,
            ProviderError::Auth { .. }
                | ProviderError::MissingKey { .. }
                | ProviderError::BudgetExhausted { .. }
                | ProviderError::RateLimited { .. }
        )
    }
}
