//! Errors returned to the frontend. Messages are user-facing and never contain secrets.

use portfolio_store::StoreError;
use serde::Serialize;

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct CommandError {
    /// Stable machine-readable code for the UI to localize.
    pub code: &'static str,
    pub message: String,
    /// Optional related entity (e.g. the existing account for a duplicate address).
    pub detail: Option<String>,
}

impl CommandError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        CommandError {
            code,
            message: message.into(),
            detail: None,
        }
    }
}

impl From<StoreError> for CommandError {
    fn from(err: StoreError) -> Self {
        match err {
            StoreError::AccountExists { ref account_id, .. } => CommandError {
                code: "account_exists",
                detail: Some(account_id.clone()),
                message: err.to_string(),
            },
            StoreError::Core(core) => CommandError::new("invalid_input", core.to_string()),
            StoreError::Invalid(_) => CommandError::new("invalid_input", err.to_string()),
            StoreError::NotFound(_) => CommandError::new("not_found", err.to_string()),
            StoreError::UnsupportedFutureSchema { .. } | StoreError::ProfileMismatch { .. } => {
                CommandError::new("profile_unavailable", err.to_string())
            }
            other => {
                tracing::error!(error = %other, "storage failure");
                CommandError::new("storage", "A local storage operation failed.")
            }
        }
    }
}

impl From<portfolio_core::CoreError> for CommandError {
    fn from(err: portfolio_core::CoreError) -> Self {
        CommandError::new("invalid_input", err.to_string())
    }
}

pub type CommandResult<T> = Result<T, CommandError>;
