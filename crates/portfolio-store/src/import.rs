//! Generic CSV basis import (ACCOUNTING.md §10).
//!
//! An import attaches acquisition lots and classifications to receipts that
//! already exist in the downloaded history, or declares opening lots at an
//! explicit history cutoff. It never creates a second balance for an on-chain
//! receipt. The flow is preview (column mapping, row-level errors, duplicate
//! detection, recalculation preview) followed by a transactional commit;
//! nothing is written to the ledger until the user commits.

use std::collections::{BTreeMap, BTreeSet};

use portfolio_core::accounting::BasisKind;
use portfolio_core::address::normalize_address;
use portfolio_core::clock::parse_rfc3339;
use portfolio_core::decimal::{Dec, parse_dec, to_canonical};
use portfolio_core::network::{NetworkFamily, NetworkId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::Row;

#[cfg(feature = "ts")]
use ts_rs::TS;

use crate::accounting::{BasisLotInput, LegClassification, LegOverride, OpeningLot, RecalcSummary};
use crate::demo::asset_id;
use crate::review::{insert_override, validate_override};
use crate::{ReplayReport, Result, Store, StoreError};

const MAX_FILE_BYTES: usize = 5 * 1024 * 1024;
const MAX_ROWS: usize = 10_000;

/// Recognized fields, in the documented column order.
pub const FIELDS: [&str; 13] = [
    "external_row_id",
    "network_id",
    "account_address",
    "transaction_id",
    "leg_id",
    "asset_identifier",
    "quantity",
    "acquired_at_utc",
    "total_basis_usd",
    "basis_kind",
    "classification",
    "note",
    "opening_cutoff_utc",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum ImportRowStatus {
    Ok,
    Error,
    /// Already imported in a committed batch; skipped.
    Duplicate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct ImportRowPreview {
    /// 1-based data row (the header is row 0).
    pub row: u32,
    pub external_row_id: Option<String>,
    pub status: ImportRowStatus,
    pub messages: Vec<String>,
    /// Matched receipt or movement, or `None` for opening lots and errors.
    pub leg_id: Option<String>,
    pub account_id: Option<String>,
    pub asset_id: Option<String>,
    pub quantity: Option<String>,
    pub total_basis_usd: Option<String>,
    pub basis_kind: Option<BasisKind>,
    pub acquired_at: Option<i64>,
    pub classification: Option<LegClassification>,
    pub opening: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct ImportPreview {
    pub batch_id: String,
    pub file_name: String,
    pub file_sha256: String,
    /// The identical file was already committed.
    pub duplicate_file: bool,
    pub columns: Vec<String>,
    /// Field -> column header actually used.
    pub mapping: BTreeMap<String, String>,
    pub missing_required: Vec<String>,
    pub rows: Vec<ImportRowPreview>,
    pub ok_count: u32,
    pub error_count: u32,
    pub duplicate_count: u32,
    /// Committing is possible only without errors and with something to apply.
    pub can_commit: bool,
    pub before: RecalcSummary,
    pub after: RecalcSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct ImportResult {
    pub batch_id: String,
    pub applied_rows: u32,
    pub replay: ReplayReport,
}

/// What a committed batch writes.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Planned {
    legs: BTreeMap<String, LegOverride>,
    openings: Vec<(String, OpeningLot)>,
    row_keys: Vec<String>,
    #[serde(default)]
    can_commit: bool,
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn normalize_header(h: &str) -> String {
    h.trim()
        .trim_start_matches('\u{feff}')
        .to_ascii_lowercase()
        .replace([' ', '-'], "_")
}

fn parse_kind(text: &str) -> Option<BasisKind> {
    match text.trim().to_ascii_lowercase().as_str() {
        "known" => Some(BasisKind::Known),
        "estimated" => Some(BasisKind::Estimated),
        "unknown" => Some(BasisKind::Unknown),
        _ => None,
    }
}

fn parse_time(text: &str) -> std::result::Result<i64, String> {
    parse_rfc3339(text.trim()).ok_or_else(|| {
        format!("{text:?} is not an ISO 8601 timestamp with a timezone (e.g. 2024-03-01T12:00:00Z)")
    })
}

fn parse_amount(text: &str, field: &str) -> std::result::Result<Dec, String> {
    let d = parse_dec(text).map_err(|_| format!("{field}: {text:?} is not an exact decimal"))?;
    if bigdecimal::Signed::is_negative(&d) {
        return Err(format!("{field} cannot be negative"));
    }
    Ok(d)
}

fn normalize_tx_hash(network: NetworkId, tx: &str) -> String {
    let tx = tx.trim();
    match network.family() {
        NetworkFamily::Evm | NetworkFamily::Bitcoin => tx.to_ascii_lowercase(),
        NetworkFamily::Tron => tx.trim_start_matches("0x").to_ascii_lowercase(),
        _ => tx.to_owned(),
    }
}

fn asset_for(network: NetworkId, identifier: &str) -> String {
    let id = identifier.trim();
    if id.is_empty() || id.eq_ignore_ascii_case("native") {
        return asset_id(network, None);
    }
    let contract = match network.family() {
        NetworkFamily::Evm => id.to_ascii_lowercase(),
        // Jetton masters are stored in raw form; accept the friendly form too.
        NetworkFamily::Ton => portfolio_core::address::normalize_address(network, id)
            .map_or_else(|_| id.to_owned(), |a| a.canonical),
        _ => id.to_owned(),
    };
    asset_id(network, Some(&contract))
}

impl Store {
    /// Parses and validates a CSV file and stores it as a preview batch.
    pub async fn preview_basis_import(
        &self,
        file_name: &str,
        content: &str,
        mapping: Option<BTreeMap<String, String>>,
    ) -> Result<ImportPreview> {
        if content.len() > MAX_FILE_BYTES {
            return Err(StoreError::Invalid("the file is larger than 5 MB".into()));
        }
        let file_sha256 = sha256_hex(content.as_bytes());
        let mut reader = csv::ReaderBuilder::new()
            .trim(csv::Trim::All)
            .flexible(false)
            .from_reader(content.as_bytes());
        let headers: Vec<String> = reader
            .headers()
            .map_err(|e| StoreError::Invalid(format!("cannot read the header row: {e}")))?
            .iter()
            .map(str::to_owned)
            .collect();

        // Field -> column index. Explicit mapping wins; otherwise headers match field names.
        let mut used: BTreeMap<String, String> = BTreeMap::new();
        let mut index: BTreeMap<&str, usize> = BTreeMap::new();
        for field in FIELDS {
            let header = match mapping.as_ref().and_then(|m| m.get(field)) {
                Some(h) if !h.is_empty() => Some(h.clone()),
                _ => headers
                    .iter()
                    .find(|h| normalize_header(h) == field)
                    .cloned(),
            };
            if let Some(h) = header
                && let Some(i) = headers.iter().position(|x| *x == h)
            {
                index.insert(field, i);
                used.insert(field.to_owned(), h);
            }
        }
        let missing_required: Vec<String> = ["network_id", "account_address", "quantity"]
            .into_iter()
            .filter(|f| !index.contains_key(f))
            .map(str::to_owned)
            .collect();

        let mut records = Vec::new();
        for (n, record) in reader.records().enumerate() {
            if n >= MAX_ROWS {
                return Err(StoreError::Invalid(format!("more than {MAX_ROWS} rows")));
            }
            let record = record.map_err(|e| StoreError::Invalid(format!("row {}: {e}", n + 1)))?;
            let get = |field: &str| -> String {
                index
                    .get(field)
                    .and_then(|i| record.get(*i))
                    .unwrap_or("")
                    .to_owned()
            };
            let values: BTreeMap<&str, String> = FIELDS.iter().map(|f| (*f, get(f))).collect();
            records.push(values);
        }

        let committed = self.committed_import_keys().await?;
        let duplicate_file = committed.1.contains(&file_sha256);
        let accounts: BTreeMap<(String, String), String> = self
            .list_accounts(None)
            .await?
            .into_iter()
            .map(|a| ((a.network.as_str().to_owned(), a.canonical_address), a.id))
            .collect();
        let (current, _) = self.current_overrides().await?;

        let mut rows: Vec<ImportRowPreview> = Vec::new();
        let mut keys: Vec<Option<String>> = Vec::new();
        let mut seen_ids: BTreeSet<String> = BTreeSet::new();
        let mut fragments: BTreeMap<String, Vec<(usize, BasisLotInput)>> = BTreeMap::new();
        let mut classes: BTreeMap<String, Vec<(usize, LegClassification)>> = BTreeMap::new();
        let mut notes: BTreeMap<String, String> = BTreeMap::new();
        let mut openings: Vec<(usize, OpeningLot)> = Vec::new();

        for (i, v) in records.iter().enumerate() {
            let mut p = ImportRowPreview {
                row: u32::try_from(i + 1).unwrap_or(u32::MAX),
                external_row_id: Some(v["external_row_id"].clone()).filter(|s| !s.is_empty()),
                status: ImportRowStatus::Ok,
                messages: Vec::new(),
                leg_id: None,
                account_id: None,
                asset_id: None,
                quantity: None,
                total_basis_usd: Some(v["total_basis_usd"].clone()).filter(|s| !s.is_empty()),
                basis_kind: None,
                acquired_at: None,
                classification: None,
                opening: false,
            };
            let key = p.external_row_id.clone().map_or_else(
                || {
                    let joined: Vec<&str> = FIELDS.iter().map(|f| v[f].as_str()).collect();
                    format!("hash:{}", sha256_hex(joined.join("\u{1f}").as_bytes()))
                },
                |id| format!("id:{id}"),
            );
            if let Some(id) = &p.external_row_id
                && !seen_ids.insert(id.clone())
            {
                p.messages
                    .push(format!("external_row_id {id:?} appears more than once"));
            }
            if duplicate_file || committed.0.contains(&key) {
                p.status = ImportRowStatus::Duplicate;
                p.messages.push("already imported".into());
                rows.push(p);
                keys.push(None);
                continue;
            }
            if let Err(e) = self
                .interpret_row(
                    i,
                    v,
                    &accounts,
                    &mut p,
                    &mut fragments,
                    &mut classes,
                    &mut notes,
                    &mut openings,
                )
                .await
            {
                p.messages.push(e);
            }
            if !p.messages.is_empty() {
                p.status = ImportRowStatus::Error;
            }
            rows.push(p);
            keys.push(Some(key));
        }

        // Combine rows per receipt and validate them together.
        let mut planned = Planned {
            legs: BTreeMap::new(),
            openings: Vec::new(),
            row_keys: Vec::new(),
            can_commit: false,
        };
        let targets: BTreeSet<String> = fragments.keys().chain(classes.keys()).cloned().collect();
        for leg_id in targets {
            let row_ids: Vec<usize> = fragments
                .get(&leg_id)
                .into_iter()
                .flatten()
                .map(|(i, _)| *i)
                .chain(classes.get(&leg_id).into_iter().flatten().map(|(i, _)| *i))
                .collect();
            if row_ids
                .iter()
                .any(|i| rows[*i].status != ImportRowStatus::Ok)
            {
                continue;
            }
            let mut o = current.get(&leg_id).cloned().unwrap_or_default();
            if let Some(f) = fragments.get(&leg_id) {
                o.basis_lots = Some(f.iter().map(|(_, l)| l.clone()).collect());
                o.basis_from_market = false;
            }
            if let Some(c) = classes.get(&leg_id) {
                let distinct: BTreeSet<_> = c.iter().map(|(_, c)| *c).collect();
                if distinct.len() > 1 {
                    for i in &row_ids {
                        rows[*i]
                            .messages
                            .push("rows for this movement disagree on classification".into());
                        rows[*i].status = ImportRowStatus::Error;
                    }
                    continue;
                }
                o.classification = distinct.into_iter().next();
            }
            if let Some(n) = notes.get(&leg_id) {
                o.note = Some(n.clone());
            }
            let facts = self
                .leg_facts(&leg_id)
                .await?
                .ok_or(StoreError::NotFound("movement"))?;
            let pair = match &o.pair_with {
                Some(id) => self.leg_facts(id).await?,
                None => None,
            };
            if let Err(e) = validate_override(&leg_id, &facts, &o, pair.as_ref()) {
                for i in &row_ids {
                    rows[*i].messages.push(e.to_string());
                    rows[*i].status = ImportRowStatus::Error;
                }
                continue;
            }
            planned.legs.insert(leg_id, o);
        }
        for (i, lot) in openings {
            if rows[i].status == ImportRowStatus::Ok {
                planned
                    .openings
                    .push((format!("{file_sha256}:{}", i + 1), lot));
            }
        }

        let ok_count = rows
            .iter()
            .filter(|r| r.status == ImportRowStatus::Ok)
            .count();
        let error_count = rows
            .iter()
            .filter(|r| r.status == ImportRowStatus::Error)
            .count();
        let duplicate_count = rows.len() - ok_count - error_count;
        planned.row_keys = rows
            .iter()
            .zip(&keys)
            .filter(|(r, _)| r.status == ImportRowStatus::Ok)
            .filter_map(|(_, k)| k.clone())
            .collect();

        let (before, after) = self
            .recalc_preview(&planned.legs, &planned.openings)
            .await?;
        let batch_id = uuid::Uuid::new_v4().to_string();
        let can_commit =
            missing_required.is_empty() && error_count == 0 && ok_count > 0 && !duplicate_file;
        planned.can_commit = can_commit;
        {
            let _guard = self.write_lock.lock().await;
            sqlx::query(
                "INSERT INTO import_batches (id, file_sha256, mapping, row_ids, status, created_at, file_name, rows_json)
                 VALUES (?, ?, ?, '[]', 'preview', ?, ?, ?)",
            )
            .bind(&batch_id)
            .bind(&file_sha256)
            .bind(serde_json::to_string(&used).map_err(|e| StoreError::Invalid(e.to_string()))?)
            .bind(self.now())
            .bind(file_name)
            .bind(serde_json::to_string(&planned).map_err(|e| StoreError::Invalid(e.to_string()))?)
            .execute(&self.pool)
            .await?;
        }
        let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        Ok(ImportPreview {
            batch_id,
            file_name: file_name.to_owned(),
            file_sha256,
            duplicate_file,
            columns: headers,
            mapping: used,
            missing_required,
            rows,
            ok_count: count(ok_count),
            error_count: count(error_count),
            duplicate_count: count(duplicate_count),
            can_commit,
            before,
            after,
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn interpret_row(
        &self,
        i: usize,
        v: &BTreeMap<&str, String>,
        accounts: &BTreeMap<(String, String), String>,
        p: &mut ImportRowPreview,
        fragments: &mut BTreeMap<String, Vec<(usize, BasisLotInput)>>,
        classes: &mut BTreeMap<String, Vec<(usize, LegClassification)>>,
        notes: &mut BTreeMap<String, String>,
        openings: &mut Vec<(usize, OpeningLot)>,
    ) -> std::result::Result<(), String> {
        let network = NetworkId::parse(v["network_id"].trim())
            .map_err(|_| format!("unknown network_id {:?}", v["network_id"]))?;
        let address = normalize_address(network, &v["account_address"])
            .map_err(|e| format!("account_address: {e}"))?;
        let account = accounts
            .get(&(network.as_str().to_owned(), address.canonical.clone()))
            .ok_or_else(|| "this address is not tracked; add it first".to_owned())?;
        p.account_id = Some(account.clone());
        let asset = asset_for(network, &v["asset_identifier"]);
        p.asset_id = Some(asset.clone());

        let class_text = v["classification"].trim();
        let opening = class_text.eq_ignore_ascii_case("opening");
        let classification = if class_text.is_empty() || opening {
            None
        } else {
            Some(
                LegClassification::parse(class_text)
                    .ok_or_else(|| format!("unknown classification {class_text:?}"))?,
            )
        };
        p.classification = classification;

        let quantity = match v["quantity"].trim() {
            "" => None,
            q => Some(parse_amount(q, "quantity")?),
        };
        p.quantity = quantity.as_ref().map(to_canonical);
        let basis = match v["total_basis_usd"].trim() {
            "" => None, // blank cost means unknown
            b => Some(parse_amount(b, "total_basis_usd")?),
        };
        let kind = match v["basis_kind"].trim() {
            "" if basis.is_none() => BasisKind::Unknown,
            "" => return Err("basis_kind is required when total_basis_usd is given".into()),
            k => parse_kind(k)
                .ok_or_else(|| format!("basis_kind {k:?} must be known, estimated or unknown"))?,
        };
        if basis.is_none() && kind != BasisKind::Unknown {
            return Err(
                "a blank total_basis_usd means unknown basis; use basis_kind unknown".into(),
            );
        }
        if basis.is_some() && kind == BasisKind::Unknown {
            return Err("basis_kind unknown cannot carry a total_basis_usd".into());
        }
        p.basis_kind = Some(kind);
        let acquired = match v["acquired_at_utc"].trim() {
            "" => None,
            t => Some(parse_time(t)?),
        };
        p.acquired_at = acquired;

        let tx = v["transaction_id"].trim();
        if tx.is_empty() {
            if !opening {
                return Err("rows without transaction_id must be classification opening".into());
            }
            let cutoff = parse_time(&v["opening_cutoff_utc"])
                .map_err(|e| format!("opening_cutoff_utc: {e}"))?;
            let quantity = quantity.ok_or("an opening lot needs a quantity")?;
            let acquired = acquired.ok_or("an opening lot needs acquired_at_utc")?;
            if acquired > cutoff {
                return Err("an opening lot cannot be acquired after the cutoff".into());
            }
            p.opening = true;
            openings.push((
                i,
                OpeningLot {
                    account_id: account.clone(),
                    asset_id: asset,
                    quantity: to_canonical(&quantity),
                    basis_usd: basis.as_ref().map(to_canonical),
                    basis_kind: kind,
                    acquired_at: acquired,
                    cutoff_at: cutoff,
                    note: Some(v["note"].clone()).filter(|n| !n.is_empty()),
                },
            ));
            return Ok(());
        }
        if opening {
            return Err("opening lots cannot reference a transaction".into());
        }

        // Match the movement: account + transaction + asset, then leg_id when ambiguous.
        let tx_row = format!("{}:{}", network.as_str(), normalize_tx_hash(network, tx));
        let candidates: Vec<(String, String)> = sqlx::query(
            "SELECT id, signed_raw_quantity FROM activity_legs
             WHERE transaction_id = ? AND account_id = ? AND asset_id = ? ORDER BY id",
        )
        .bind(&tx_row)
        .bind(account)
        .bind(&p.asset_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|r| (r.get(0), r.get(1)))
        .collect();
        let leg_hint = v["leg_id"].trim();
        let matched: Vec<&(String, String)> = candidates
            .iter()
            .filter(|(id, _)| {
                leg_hint.is_empty() || id == leg_hint || id.rsplit(':').next() == Some(leg_hint)
            })
            .collect();
        let (leg_id, raw) = match matched.as_slice() {
            [] if candidates.is_empty() => {
                return Err(
                    "no downloaded movement of this asset in that transaction for this account"
                        .into(),
                );
            }
            [] => {
                return Err(format!(
                    "leg_id {leg_hint:?} does not match this transaction"
                ));
            }
            [one] => (*one).clone(),
            _ => return Err("several movements match; add leg_id to choose one".into()),
        };
        p.leg_id = Some(leg_id.clone());
        let incoming = !raw.starts_with('-');
        if let Some(c) = classification {
            classes.entry(leg_id.clone()).or_default().push((i, c));
        }
        if !v["note"].is_empty() {
            notes.insert(leg_id.clone(), v["note"].clone());
        }
        if incoming {
            let Some(quantity) = quantity else {
                if classification.is_some() {
                    return Ok(());
                }
                return Err("quantity is required for an acquisition lot".into());
            };
            let acquired = acquired.ok_or("acquired_at_utc is required for an acquisition lot")?;
            fragments.entry(leg_id).or_default().push((
                i,
                BasisLotInput {
                    quantity: to_canonical(&quantity),
                    basis_usd: basis.as_ref().map(to_canonical),
                    basis_kind: kind,
                    acquired_at: acquired,
                },
            ));
        } else {
            if basis.is_some() {
                return Err("total_basis_usd applies to incoming movements".into());
            }
            if classification.is_none() {
                return Err("an outgoing movement needs a classification".into());
            }
        }
        Ok(())
    }

    /// Row keys and file hashes of committed batches.
    async fn committed_import_keys(&self) -> Result<(BTreeSet<String>, BTreeSet<String>)> {
        let rows = sqlx::query(
            "SELECT file_sha256, row_ids FROM import_batches WHERE status = 'committed'",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut keys = BTreeSet::new();
        let mut files = BTreeSet::new();
        for r in rows {
            files.insert(r.get::<String, _>("file_sha256"));
            let ids: Vec<String> =
                serde_json::from_str(&r.get::<String, _>("row_ids")).unwrap_or_default();
            keys.extend(ids);
        }
        Ok((keys, files))
    }

    /// Applies a previewed batch in one transaction, then replays accounting.
    pub async fn commit_basis_import(&self, batch_id: &str) -> Result<ImportResult> {
        let guard = self.write_lock.lock().await;
        let row =
            sqlx::query("SELECT file_sha256, status, rows_json FROM import_batches WHERE id = ?")
                .bind(batch_id)
                .fetch_optional(&self.pool)
                .await?
                .ok_or(StoreError::NotFound("import"))?;
        if row.get::<String, _>("status") != "preview" {
            return Err(StoreError::Invalid(
                "this import is no longer pending".into(),
            ));
        }
        let planned: Planned = serde_json::from_str(&row.get::<String, _>("rows_json"))
            .map_err(|e| StoreError::Corrupt(format!("import batch: {e}")))?;
        if !planned.can_commit {
            return Err(StoreError::Invalid(
                "fix the rows with errors and preview the file again".into(),
            ));
        }
        let (committed_keys, files) = self.committed_import_keys().await?;
        if files.contains(&row.get::<String, _>("file_sha256"))
            || planned.row_keys.iter().any(|k| committed_keys.contains(k))
        {
            return Err(StoreError::Invalid(
                "these rows were already imported".into(),
            ));
        }
        // Evidence may have changed since the preview: validate again.
        for (leg_id, o) in &planned.legs {
            let facts = self.leg_facts(leg_id).await?.ok_or_else(|| {
                StoreError::Invalid(format!("movement {leg_id} no longer exists; preview again"))
            })?;
            let pair = match &o.pair_with {
                Some(id) => self.leg_facts(id).await?,
                None => None,
            };
            validate_override(leg_id, &facts, o, pair.as_ref())?;
        }
        let source = format!("csv:{batch_id}");
        {
            let mut tx = self.pool.begin().await?;
            let now = self.now();
            for (leg_id, o) in &planned.legs {
                insert_override(&mut tx, "leg", leg_id, o, &source, now).await?;
            }
            for (id, lot) in &planned.openings {
                insert_override(&mut tx, "lot", id, lot, &source, now).await?;
            }
            sqlx::query(
                "UPDATE import_batches SET status = 'committed', committed_at = ?, row_ids = ? WHERE id = ?",
            )
            .bind(now)
            .bind(serde_json::to_string(&planned.row_keys).map_err(|e| StoreError::Invalid(e.to_string()))?)
            .bind(batch_id)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
        }
        drop(guard);
        let replay = self.replay_accounting().await?;
        Ok(ImportResult {
            batch_id: batch_id.to_owned(),
            applied_rows: u32::try_from(planned.row_keys.len()).unwrap_or(u32::MAX),
            replay,
        })
    }

    /// Discards a previewed batch; nothing was applied.
    pub async fn discard_basis_import(&self, batch_id: &str) -> Result<()> {
        let _guard = self.write_lock.lock().await;
        sqlx::query(
            "UPDATE import_batches SET status = 'rolled_back' WHERE id = ? AND status = 'preview'",
        )
        .bind(batch_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
