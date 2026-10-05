//! Versioned, checksummed SQLite snapshots. Credentials are never stored here.
use crate::{Result, Store, StoreError};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::path::Path;

const MAX_DATABASE: usize = 128 * 1024 * 1024;
const FORMAT: &str = "coincontrol-backup-v1";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Backup {
    format: String,
    schema_version: i64,
    profile: String,
    created_at: i64,
    sha256: String,
    database_base64: String,
}
fn error(s: impl Into<String>) -> StoreError {
    StoreError::Invalid(s.into())
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn decode(text: &str) -> Result<(Backup, Vec<u8>)> {
    if text.len() > MAX_DATABASE * 4 / 3 + 4096 {
        return Err(error("backup exceeds 128 MiB database limit"));
    }
    let backup: Backup =
        serde_json::from_str(text).map_err(|_| error("invalid backup manifest"))?;
    if backup.format != FORMAT || backup.schema_version > crate::latest_schema_version() {
        return Err(error("unsupported backup format or future database schema"));
    }
    let bytes = STANDARD
        .decode(&backup.database_base64)
        .map_err(|_| error("invalid backup payload"))?;
    if bytes.len() > MAX_DATABASE
        || !bytes.starts_with(b"SQLite format 3\0")
        || digest(&bytes) != backup.sha256
    {
        return Err(error("backup checksum, size or SQLite header is invalid"));
    }
    Ok((backup, bytes))
}

impl Store {
    async fn snapshot_locked(&self) -> Result<String> {
        let dir = tempfile::tempdir().map_err(|e| error(e.to_string()))?;
        let path = dir.path().join("snapshot.sqlite");
        // VACUUM INTO copies committed pages including WAL and creates a
        // standalone, consistent SQLite image. Never copy the live DB file.
        sqlx::query("VACUUM INTO ?")
            .bind(path.to_string_lossy().as_ref())
            .execute(&self.pool)
            .await?;
        let bytes =
            std::fs::read(&path).map_err(|e| error(format!("reading VACUUM snapshot: {e}")))?;
        if bytes.len() > MAX_DATABASE {
            return Err(error("database exceeds backup size limit"));
        }
        let backup = Backup {
            format: FORMAT.into(),
            schema_version: self.schema_version().await?,
            profile: self.profile().as_str().into(),
            created_at: self.now(),
            sha256: digest(&bytes),
            database_base64: STANDARD.encode(bytes),
        };
        serde_json::to_string(&backup).map_err(|e| error(e.to_string()))
    }

    pub async fn export_backup(&self) -> Result<String> {
        let _guard = self.write_lock.lock().await;
        self.snapshot_locked().await
    }

    /// Validate the actual database, not only caller-supplied manifest fields.
    pub async fn inspect_backup(&self, text: &str) -> Result<String> {
        let (backup, bytes) = decode(text)?;
        if backup.profile != self.profile().as_str() {
            return Err(error("backup belongs to another profile"));
        }
        let dir = tempfile::tempdir().map_err(|e| error(e.to_string()))?;
        let path = dir.path().join("validate.sqlite");
        std::fs::write(&path, bytes).map_err(|e| error(e.to_string()))?;
        let restored = Store::open(&path, self.profile(), self.clock.clone()).await?;
        let valid = restored.integrity_ok().await?;
        let foreign_keys = sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(&restored.pool)
            .await?
            .is_empty();
        restored.close().await;
        if !valid || !foreign_keys {
            return Err(error("backup database integrity check failed"));
        }
        Ok(
            serde_json::json!({"profile":backup.profile,"created_at":backup.created_at,
            "schema_version":backup.schema_version,"sha256":backup.sha256})
            .to_string(),
        )
    }

    /// Restore rows atomically into the existing schema. Readers see the old
    /// transaction or the new one, never a half-replaced database. A safety
    /// snapshot is persisted before applying a restore.
    pub async fn restore_backup(&self, text: &str, safety_path: &Path) -> Result<()> {
        self.inspect_backup(text).await?;
        let (_, bytes) = decode(text)?;
        let dir = tempfile::tempdir().map_err(|e| error(e.to_string()))?;
        let path = dir.path().join("restore.sqlite");
        std::fs::write(&path, bytes).map_err(|e| error(e.to_string()))?;
        let restored = Store::open(&path, self.profile(), self.clock.clone()).await?;
        restored.close().await;
        let _guard = self.write_lock.lock().await;
        let safety = self.snapshot_locked().await?;
        // create_new preserves previous safety snapshots; never overwrite.
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(safety_path)
            .map_err(|e| error(format!("cannot create safety snapshot: {e}")))?;
        file.write_all(safety.as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|e| error(e.to_string()))?;
        let mut conn = self.pool.acquire().await?;
        sqlx::query("ATTACH DATABASE ? AS recovery")
            .bind(path.to_string_lossy().as_ref())
            .execute(&mut *conn)
            .await?;
        let result = async {
            let current = sqlx::query("SELECT name, sql FROM main.sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
                .fetch_all(&mut *conn).await?;
            let incoming = sqlx::query("SELECT name, sql FROM recovery.sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
                .fetch_all(&mut *conn).await?;
            let schema = |rows: &[sqlx::sqlite::SqliteRow]| -> Vec<(String, String)> {
                rows.iter().map(|r| (r.get("name"), r.get::<String,_>("sql").split_whitespace().collect::<Vec<_>>().join(" "))).collect()
            };
            if schema(&current) != schema(&incoming) { return Err(error("backup schema differs from supported schema")); }
            // Respect dependencies for RESTRICT/CASCADE actions as well as
            // deferred constraints. Copying alphabetically is not sufficient.
            let mut dependencies=std::collections::BTreeMap::new();
            for row in &current {
                let name:String=row.get("name");
                let quoted=format!("\"{}\"",name.replace('"',"\"\""));
                let parents=sqlx::QueryBuilder::<sqlx::Sqlite>::new("PRAGMA main.foreign_key_list(").push(quoted).push(")").build().fetch_all(&mut *conn).await?;
                dependencies.insert(name.clone(),parents.iter().map(|r|r.get::<String,_>("table")).filter(|p|p!=&name).collect::<std::collections::BTreeSet<_>>());
            }
            let mut order:Vec<String>=Vec::new();
            while !dependencies.is_empty() {
                let next=dependencies.iter().find(|(_,parents)|parents.iter().all(|p|order.contains(p))).map(|(name,_)|name.clone())
                    .ok_or_else(||error("unsupported cyclic backup schema"))?;
                dependencies.remove(&next);order.push(next);
            }
            let mut tx = sqlx::Connection::begin(&mut *conn).await?;
            for name in order.iter().rev() {
                let quoted = format!("\"{}\"", name.replace('"', "\"\""));
                sqlx::QueryBuilder::<sqlx::Sqlite>::new("DELETE FROM main.").push(&quoted).build().execute(&mut *tx).await.map_err(|e|error(format!("restore deleting {name}: {e}")))?;
            }
            for name in &order {
                let quoted = format!("\"{}\"", name.replace('"', "\"\""));
                sqlx::QueryBuilder::<sqlx::Sqlite>::new("INSERT INTO main.").push(&quoted).push(" SELECT * FROM recovery.").push(&quoted).build().execute(&mut *tx).await.map_err(|e|error(format!("restore inserting {name}: {e}")))?;
            }
            crate::ingest::mark_dirty_in(&mut tx).await?;
            let violations=sqlx::query("PRAGMA main.foreign_key_check").fetch_all(&mut *tx).await?;
            if !violations.is_empty() {
                return Err(error(format!("restored rows violate foreign keys: {:?}",violations.iter().map(|r|(r.get::<String,_>("table"),r.get::<String,_>("parent"))).collect::<Vec<_>>())));
            }
            tx.commit().await?;
            Ok(())
        }.await;
        let detached = sqlx::query("DETACH DATABASE recovery")
            .execute(&mut *conn)
            .await;
        result?;
        detached?;
        Ok(())
    }

    /// Export exact lots and audit decisions as CSV. Text cells are escaped
    /// against spreadsheet formula interpretation; monetary strings stay exact.
    pub async fn export_csv(&self, kind: &str) -> Result<String> {
        let _guard = self.write_lock.lock().await;
        let mut csv = csv::Writer::from_writer(Vec::new());
        let safe = |s: String| {
            if s.trim_start().starts_with(['=', '+', '-', '@', '\t', '\r']) {
                format!("'{s}")
            } else {
                s
            }
        };
        match kind {
            "holdings" => {
                csv.write_record([
                    "asset_id",
                    "network",
                    "symbol",
                    "quantity",
                    "price_usd",
                    "value_usd",
                    "balance_status",
                    "basis_usd",
                    "unrealized_pnl_usd",
                ])
                .map_err(|e| error(e.to_string()))?;
                for h in self.list_holdings(&crate::Scope::All).await? {
                    csv.write_record([
                        safe(h.asset_id),
                        h.network.as_str().into(),
                        safe(h.symbol.unwrap_or_default()),
                        h.quantity,
                        h.price_usd.unwrap_or_default(),
                        h.value_usd.unwrap_or_default(),
                        serde_json::to_string(&h.balance_status).unwrap_or_default(),
                        h.basis_usd.unwrap_or_default(),
                        h.unrealized_pnl_usd.unwrap_or_default(),
                    ])
                    .map_err(|e| error(e.to_string()))?;
                }
            }
            "activity" => {
                csv.write_record([
                    "transaction_id",
                    "account_id",
                    "network",
                    "occurred_at_unix_seconds",
                    "operation",
                    "status",
                    "leg_id",
                    "asset_id",
                    "signed_quantity",
                    "treatment",
                    "value_usd",
                    "fee_quantity",
                    "fee_symbol",
                ])
                .map_err(|e| error(e.to_string()))?;
                let mut cursor = None;
                loop {
                    let page = self
                        .list_activity(
                            &crate::Scope::All,
                            &crate::ActivityFilter::default(),
                            cursor.as_deref(),
                            200,
                        )
                        .await?;
                    for r in page.rows {
                        let mut legs = r.legs.into_iter().map(Some).collect::<Vec<_>>();
                        if legs.is_empty() {
                            legs.push(None);
                        }
                        for (i, leg) in legs.into_iter().enumerate() {
                            let (id, asset, quantity, treatment, value) = leg
                                .map(|l| {
                                    (
                                        l.leg_id,
                                        l.asset_id,
                                        l.signed_quantity,
                                        l.treatment.unwrap_or_default(),
                                        l.value_usd.unwrap_or_default(),
                                    )
                                })
                                .unwrap_or_default();
                            csv.write_record([
                                safe(r.transaction_id.clone()),
                                safe(r.account_id.clone()),
                                r.network.as_str().into(),
                                r.occurred_at.to_string(),
                                safe(r.operation.clone()),
                                safe(r.status.clone()),
                                safe(id),
                                safe(asset),
                                quantity,
                                safe(treatment),
                                value,
                                if i == 0 {
                                    r.fee_quantity.clone().unwrap_or_default()
                                } else {
                                    String::new()
                                },
                                if i == 0 {
                                    safe(r.fee_symbol.clone().unwrap_or_default())
                                } else {
                                    String::new()
                                },
                            ])
                            .map_err(|e| error(e.to_string()))?;
                        }
                    }
                    cursor = page.next_cursor;
                    if cursor.is_none() {
                        break;
                    }
                }
            }
            "lots" => {
                csv.write_record([
                    "lot_id",
                    "account_id",
                    "asset_id",
                    "acquired_at_unix_seconds",
                    "arrived_at_unix_seconds",
                    "quantity",
                    "remaining_quantity",
                    "basis_usd",
                    "remaining_basis_usd",
                    "basis_kind",
                    "source_event",
                ])
                .map_err(|e| error(e.to_string()))?;
                for r in
                    sqlx::query("SELECT * FROM lots ORDER BY account_id, asset_id, acquired_at, id")
                        .fetch_all(&self.pool)
                        .await?
                {
                    let mut cells = Vec::new();
                    for key in ["id", "account_id", "asset_id"] {
                        cells.push(safe(r.get(key)));
                    }
                    for key in ["acquired_at", "arrived_at"] {
                        cells.push(r.get::<i64, _>(key).to_string());
                    }
                    for key in ["quantity", "remaining_quantity"] {
                        cells.push(r.get::<String, _>(key));
                    }
                    for key in ["basis_usd", "remaining_basis_usd"] {
                        cells.push(r.get::<Option<String>, _>(key).unwrap_or_default());
                    }
                    for key in ["basis_kind", "source_event"] {
                        cells.push(safe(r.get(key)));
                    }
                    csv.write_record(cells).map_err(|e| error(e.to_string()))?;
                }
            }
            "decisions" => {
                csv.write_record([
                    "target_kind",
                    "target_id",
                    "version",
                    "created_at_unix_seconds",
                    "orphaned",
                    "decision_json",
                ])
                .map_err(|e| error(e.to_string()))?;
                for r in sqlx::query(
                    "SELECT * FROM accounting_overrides ORDER BY target_kind, target_id, version",
                )
                .fetch_all(&self.pool)
                .await?
                {
                    csv.write_record([
                        safe(r.get("target_kind")),
                        safe(r.get("target_id")),
                        r.get::<i64, _>("version").to_string(),
                        r.get::<i64, _>("created_at").to_string(),
                        r.get::<i64, _>("orphaned").to_string(),
                        safe(r.get("payload")),
                    ])
                    .map_err(|e| error(e.to_string()))?;
                }
            }
            _ => return Err(error("unknown CSV export kind")),
        }
        let bytes = csv.into_inner().map_err(|e| error(e.to_string()))?;
        String::from_utf8(bytes).map_err(|e| error(e.to_string()))
    }
}
