use thiserror::Error;

/// Errors surfaced by the persistence layer. Messages never contain secrets.
#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("this database was created by a newer version (schema {found}, supported {supported})")]
    UnsupportedFutureSchema { found: i64, supported: i64 },
    #[error("this database belongs to the {found} profile, not {expected}")]
    ProfileMismatch { expected: String, found: String },
    #[error(transparent)]
    Core(#[from] portfolio_core::CoreError),
    #[error("{0} not found")]
    NotFound(&'static str),
    #[error("this address is already tracked in wallet {wallet_label:?}")]
    AccountExists {
        account_id: String,
        wallet_label: String,
    },
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("stored data is corrupt: {0}")]
    Corrupt(String),
}
