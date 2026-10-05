//! SQLite persistence for Portfolio Desk.
//!
//! SQLx is the single database access layer. Writes are serialized through one
//! lock; reads use a small bounded pool. Every profile (real, demo, test) is a
//! separate database file so demo data can never mix with a real portfolio.

use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use portfolio_core::accounting::ACCOUNTING_ENGINE_VERSION;
use portfolio_core::clock::Clock;
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous,
};
use tokio::sync::Mutex;

pub mod accounting;
mod activity;
pub mod demo;
pub mod dto;
mod error;
pub mod import;
pub mod ingest;
mod portfolio;
pub mod prices;
mod review;
mod settings;
mod wallets;

pub use accounting::{
    BasisLotInput, LegClassification, LegOverride, OpeningLot, RecalcSummary, ReplayReport,
};
pub use dto::*;
pub use error::StoreError;
pub use import::{ImportPreview, ImportResult, ImportRowPreview, ImportRowStatus};
pub use ingest::{AccountSyncStatus, Coverage};

pub type Result<T> = std::result::Result<T, StoreError>;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Highest schema migration version this build understands.
pub fn latest_schema_version() -> i64 {
    MIGRATOR.iter().map(|m| m.version).max().unwrap_or(0)
}

/// An open profile database.
#[derive(Clone)]
pub struct Store {
    pool: SqlitePool,
    write_lock: Arc<Mutex<()>>,
    clock: Arc<dyn Clock>,
    profile: ProfileKind,
}

impl Store {
    /// Opens (creating if needed) the profile database at `path` and migrates it.
    pub async fn open(path: &Path, profile: ProfileKind, clock: Arc<dyn Clock>) -> Result<Self> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await?;
        Self::init(pool, profile, clock).await
    }

    /// Opens a private in-memory database (tests only).
    pub async fn open_in_memory(profile: ProfileKind, clock: Arc<dyn Clock>) -> Result<Self> {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true);
        // One connection: each in-memory connection would otherwise be its own database.
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;
        Self::init(pool, profile, clock).await
    }

    async fn init(pool: SqlitePool, profile: ProfileKind, clock: Arc<dyn Clock>) -> Result<Self> {
        reject_future_schema(&pool).await?;
        MIGRATOR.run(&pool).await?;
        let store = Store {
            pool,
            write_lock: Arc::new(Mutex::new(())),
            clock,
            profile,
        };
        store.ensure_profile_identity().await?;
        Ok(store)
    }

    pub fn profile(&self) -> ProfileKind {
        self.profile
    }

    /// Current time from the store's clock (fixed in tests).
    pub fn now(&self) -> i64 {
        self.clock.now()
    }

    pub async fn close(&self) {
        self.pool.close().await;
    }

    /// Stamps a new database with its profile kind and refuses to open a
    /// database created for a different profile (e.g. demo data as real).
    async fn ensure_profile_identity(&self) -> Result<()> {
        let _guard = self.write_lock.lock().await;
        let existing: Option<String> =
            sqlx::query_scalar("SELECT value FROM app_meta WHERE key = 'profile_kind'")
                .fetch_optional(&self.pool)
                .await?;
        match existing {
            Some(kind) if kind != self.profile.as_str() => {
                return Err(StoreError::ProfileMismatch {
                    expected: self.profile.as_str().to_owned(),
                    found: kind,
                });
            }
            Some(_) => {}
            None => {
                let mut tx = self.pool.begin().await?;
                for (key, value) in [
                    ("profile_kind", self.profile.as_str().to_owned()),
                    ("created_at", self.now().to_string()),
                ] {
                    sqlx::query("INSERT INTO app_meta (key, value) VALUES (?, ?)")
                        .bind(key)
                        .bind(value)
                        .execute(&mut *tx)
                        .await?;
                }
                tx.commit().await?;
            }
        }
        let stored: Option<String> = sqlx::query_scalar(
            "SELECT value FROM app_meta WHERE key = 'accounting_engine_version'",
        )
        .fetch_optional(&self.pool)
        .await?;
        if stored.as_deref() != Some(ACCOUNTING_ENGINE_VERSION.to_string().as_str()) {
            // Derived accounting was computed by another engine: rebuild it on open.
            self.mark_accounting_dirty().await?;
        }
        sqlx::query(
            "INSERT INTO app_meta (key, value) VALUES ('accounting_engine_version', ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(ACCOUNTING_ENGINE_VERSION.to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Current applied schema version.
    pub async fn schema_version(&self) -> Result<i64> {
        let v: Option<i64> =
            sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations WHERE success = 1")
                .fetch_one(&self.pool)
                .await?;
        Ok(v.unwrap_or(0))
    }

    /// Runs SQLite's integrity check.
    pub async fn integrity_ok(&self) -> Result<bool> {
        let result: String = sqlx::query_scalar("PRAGMA integrity_check")
            .fetch_one(&self.pool)
            .await?;
        Ok(result == "ok")
    }
}

/// A database written by a newer build must never be opened or downgraded.
async fn reject_future_schema(pool: &SqlitePool) -> Result<()> {
    let has_table: Option<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name = '_sqlx_migrations'",
    )
    .fetch_optional(pool)
    .await?;
    if has_table.is_none() {
        return Ok(());
    }
    let found: Option<i64> = sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations")
        .fetch_one(pool)
        .await?;
    let supported = latest_schema_version();
    match found {
        Some(found) if found > supported => {
            Err(StoreError::UnsupportedFutureSchema { found, supported })
        }
        _ => Ok(()),
    }
}
