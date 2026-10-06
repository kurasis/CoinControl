//! Write path for provider synchronization (SPECIFICATION.md §6).
//!
//! Provider adapters normalize responses into the `*Spec` types below; this
//! module persists them idempotently. Re-ingesting the same transaction for the
//! same account replaces that account's legs and fee instead of adding a second
//! copy, so repeated pages, overlapping sweeps, and resumed backfills never
//! double count.

use std::collections::BTreeSet;

use num_bigint::BigInt;
use portfolio_core::network::NetworkId;
use serde::{Deserialize, Serialize};
use sqlx::{QueryBuilder, Row, Sqlite};

use crate::demo::asset_id;
use crate::{Account, Result, Store, StoreError};

/// A chain-specific asset as reported by a provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetSpec {
    pub network: NetworkId,
    /// Contract/mint/master identity; `None` for the native asset. Lowercase
    /// hex for EVM, exact case for Solana mints and TRON Base58 contracts,
    /// raw `workchain:hex` for TON Jetton masters.
    pub contract: Option<String>,
    pub decimals: u32,
    pub symbol: Option<String>,
    pub name: Option<String>,
    pub verification: Verification,
    pub provider: &'static str,
}

impl AssetSpec {
    pub fn native(network: NetworkId, provider: &'static str) -> Self {
        AssetSpec {
            network,
            contract: None,
            decimals: network.native_decimals(),
            symbol: Some(network.native_symbol().to_owned()),
            name: Some(network.display_name().to_owned()),
            verification: Verification::Verified,
            provider,
        }
    }

    pub fn id(&self) -> String {
        asset_id(self.network, self.contract.as_deref())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verification {
    Verified,
    Unverified,
    Spam,
}

impl Verification {
    pub fn as_str(self) -> &'static str {
        match self {
            Verification::Verified => "verified",
            Verification::Unverified => "unverified",
            Verification::Spam => "spam",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxStatus {
    Pending,
    Confirmed,
    Failed,
}

impl TxStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            TxStatus::Pending => "pending",
            TxStatus::Confirmed => "confirmed",
            TxStatus::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    In,
    Out,
    SelfTransfer,
}

impl Direction {
    pub fn as_str(self) -> &'static str {
        match self {
            Direction::In => "in",
            Direction::Out => "out",
            Direction::SelfTransfer => "self",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decoding {
    Interpreted,
    Partial,
    RawOnly,
}

impl Decoding {
    pub fn as_str(self) -> &'static str {
        match self {
            Decoding::Interpreted => "interpreted",
            Decoding::Partial => "partial",
            Decoding::RawOnly => "raw_only",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeeAttribution {
    Exact,
    Shared,
    Unknown,
    Sponsored,
}

impl FeeAttribution {
    pub fn as_str(self) -> &'static str {
        match self {
            FeeAttribution::Exact => "exact",
            FeeAttribution::Shared => "shared",
            FeeAttribution::Unknown => "unknown",
            FeeAttribution::Sponsored => "sponsored",
        }
    }
}

/// One asset movement of one owned account, excluding the network fee.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegSpec {
    /// Canonical public address of the other party, only when directly evidenced.
    pub counterparty: Option<String>,
    pub asset: AssetSpec,
    /// Smallest units; positive = received, negative = sent.
    pub signed_raw: BigInt,
    pub direction: Direction,
    pub leg_type: String,
    pub decoding: Decoding,
    /// Needs user review before its accounting meaning is known.
    pub unresolved: bool,
}

impl LegSpec {
    pub fn with_counterparty(mut self, address: Option<String>) -> Self {
        self.counterparty = address;
        self
    }
}

/// The part of a transaction fee borne by the synchronized account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeeSpec {
    pub asset: AssetSpec,
    pub raw: BigInt,
    pub attribution: FeeAttribution,
}

/// A transaction as seen from one owned account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxSpec {
    pub network: NetworkId,
    /// Canonical chain identifier: lowercase hex for BTC/EVM/TRON/TON,
    /// case-sensitive base58 signature for Solana.
    pub hash: String,
    /// `None`: the complete view of the transaction from one history category;
    /// re-ingesting it replaces the account's main legs and fee. `Some(key)`:
    /// one independently listed component (for example one TRC-20 transfer
    /// event) that replaces only its own legs, so categories that describe
    /// the same transaction add up instead of overwriting each other.
    pub part: Option<String>,
    pub block_height: Option<i64>,
    pub position: Option<String>,
    pub occurred_at: i64,
    pub status: TxStatus,
    pub provider: &'static str,
    /// Operation from this account's point of view (send, receive, trade...).
    pub operation: String,
    pub legs: Vec<LegSpec>,
    pub fee: Option<FeeSpec>,
    pub decoding: Decoding,
    /// Sanitized evidence references (no credentials, no full URLs).
    pub evidence: serde_json::Value,
}

/// Synchronization progress of one (account, provider, category).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Checkpoint {
    /// Continuation for older history not yet downloaded.
    pub backfill_cursor: Option<String>,
    /// Query fingerprint the cursor belongs to; a different fingerprint resets it.
    pub boundary: Option<String>,
    pub coverage: Coverage,
    pub earliest_covered_at: Option<i64>,
    pub state: CheckpointState,
}

/// Free-form progress details persisted as JSON in `retry_state`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CheckpointState {
    #[serde(default)]
    pub last_attempt_at: Option<i64>,
    #[serde(default)]
    pub last_success_at: Option<i64>,
    #[serde(default)]
    pub last_error: Option<String>,
    /// Full history was downloaded at least once.
    #[serde(default)]
    pub completed_once: bool,
    /// Pending records may be hidden by a provider cap.
    #[serde(default)]
    pub pending_incomplete: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum Coverage {
    #[default]
    Loading,
    Paused,
    Complete,
    Partial,
    Unsupported,
}

impl Coverage {
    pub fn as_str(self) -> &'static str {
        match self {
            Coverage::Loading => "loading",
            Coverage::Paused => "paused",
            Coverage::Complete => "complete",
            Coverage::Partial => "partial",
            Coverage::Unsupported => "unsupported",
        }
    }

    fn parse(text: &str) -> Result<Self> {
        Ok(match text {
            "loading" => Coverage::Loading,
            "paused" => Coverage::Paused,
            "complete" => Coverage::Complete,
            "partial" => Coverage::Partial,
            "unsupported" => Coverage::Unsupported,
            other => return Err(StoreError::Corrupt(format!("coverage {other:?}"))),
        })
    }
}

/// A price observation to persist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriceSpec {
    pub asset_id: String,
    pub provider: &'static str,
    /// Exact canonical decimal string.
    pub price_usd: String,
    pub requested_at: i64,
    pub observed_at: i64,
    pub granularity: &'static str,
    pub quality: &'static str,
    pub change_24h_percent: Option<String>,
}

/// A currently held asset that needs a quote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldAsset {
    pub asset_id: String,
    pub network: NetworkId,
    pub contract: Option<String>,
    pub symbol: Option<String>,
}

/// Synchronization state of one account for the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AccountSyncStatus {
    pub account_id: String,
    pub provider: Option<String>,
    pub coverage: Option<Coverage>,
    pub last_attempt_at: Option<i64>,
    pub last_success_at: Option<i64>,
    pub last_error: Option<String>,
    pub earliest_covered_at: Option<i64>,
    pub transaction_count: u32,
    pub pending_incomplete: bool,
}

/// Requests this app counted for a provider on one UTC day.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderUsage {
    pub provider: String,
    pub requests: u32,
    pub credits: u32,
    pub last_error: Option<String>,
}

/// Spacing of retained live price ticks.
const TICK_RETENTION_SECONDS: i64 = 300;

fn tx_row_id(network: NetworkId, hash: &str) -> String {
    format!("{}:{}", network.as_str(), hash)
}

impl Store {
    pub async fn account(&self, id: &str) -> Result<Account> {
        self.list_accounts(None)
            .await?
            .into_iter()
            .find(|a| a.id == id)
            .ok_or(StoreError::NotFound("account"))
    }

    /// Every account on `network`, archived ones included: archived accounts
    /// are still owned counterparties for transfer and fee attribution.
    pub async fn accounts_on_network(&self, network: NetworkId) -> Result<Vec<Account>> {
        Ok(self
            .list_accounts(None)
            .await?
            .into_iter()
            .filter(|a| a.network == network)
            .collect())
    }

    /// Inserts or refreshes an asset identity and returns its ID.
    ///
    /// Decimals of an existing identity never change silently; a different
    /// value is a data conflict. A `verified`/`spam` verdict replaces
    /// `unverified`, but an `unverified` observation never downgrades one.
    pub async fn upsert_asset(&self, asset: &AssetSpec) -> Result<String> {
        let _guard = self.write_lock.lock().await;
        let mut tx = self.pool.begin().await?;

        let id = upsert_asset_in(&mut tx, asset, self.now()).await?;
        tx.commit().await?;
        Ok(id)
    }

    /// A stored asset identity, attributed to `provider` when re-recorded.
    pub async fn asset_spec(
        &self,
        asset_id: &str,
        provider: &'static str,
    ) -> Result<Option<AssetSpec>> {
        let row = sqlx::query(
            "SELECT network_id, asset_kind, canonical_identifier, decimals, symbol, name, verification
             FROM assets WHERE id = ?",
        )
        .bind(asset_id)
        .fetch_optional(&self.pool)
        .await?;
        let Some(r) = row else { return Ok(None) };
        let network = NetworkId::parse(&r.get::<String, _>("network_id"))?;
        let kind: String = r.get("asset_kind");
        Ok(Some(AssetSpec {
            network,
            contract: (kind == "token").then(|| r.get("canonical_identifier")),
            decimals: u32::try_from(r.get::<i64, _>("decimals"))
                .map_err(|_| StoreError::Corrupt(format!("decimals of {asset_id}")))?,
            symbol: r.get("symbol"),
            name: r.get("name"),
            verification: match r.get::<String, _>("verification").as_str() {
                "verified" => Verification::Verified,
                "spam" => Verification::Spam,
                _ => Verification::Unverified,
            },
            provider,
        }))
    }

    pub async fn record_balance(
        &self,
        account_id: &str,
        asset: &AssetSpec,
        raw: &BigInt,
        chain_height: Option<i64>,
        status: &str,
    ) -> Result<()> {
        let now = self.now();
        let _guard = self.write_lock.lock().await;
        let mut tx = self.pool.begin().await?;
        let asset_id = upsert_asset_in(&mut tx, asset, now).await?;
        sqlx::query(
            "INSERT INTO balance_observations (account_id, asset_id, raw_quantity, observed_at, chain_height, provider, status)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(account_id)
        .bind(&asset_id)
        .bind(raw.to_string())
        .bind(now)
        .bind(chain_height)
        .bind(asset.provider)
        .bind(status)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Marks the latest balances of an account stale (a refresh failed), keeping
    /// the last known quantities instead of replacing them with zero.
    pub async fn mark_balances_stale(&self, account_id: &str) -> Result<()> {
        let now = self.now();
        let _guard = self.write_lock.lock().await;
        sqlx::query(
            "INSERT INTO balance_observations (account_id, asset_id, raw_quantity, observed_at, chain_height, provider, status)
             SELECT b.account_id, b.asset_id, b.raw_quantity, ?, b.chain_height, b.provider, 'stale'
             FROM balance_observations b
             WHERE b.account_id = ? AND b.status = 'fresh' AND b.id = (
                 SELECT b2.id FROM balance_observations b2
                 WHERE b2.account_id = b.account_id AND b2.asset_id = b.asset_id
                 ORDER BY b2.observed_at DESC, b2.id DESC LIMIT 1)",
        )
        .bind(now)
        .bind(account_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Assets whose latest observation for this account is non-zero.
    pub async fn nonzero_balance_assets(&self, account_id: &str) -> Result<BTreeSet<String>> {
        let rows: Vec<String> = sqlx::query_scalar(
            "SELECT b.asset_id FROM balance_observations b
             WHERE b.account_id = ? AND b.raw_quantity != '0' AND b.id = (
                 SELECT b2.id FROM balance_observations b2
                 WHERE b2.account_id = b.account_id AND b2.asset_id = b.asset_id
                 ORDER BY b2.observed_at DESC, b2.id DESC LIMIT 1)",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().collect())
    }

    /// Records a zero balance for an asset that the provider no longer reports.
    pub async fn record_zero_balance(
        &self,
        account_id: &str,
        asset_id: &str,
        provider: &str,
    ) -> Result<()> {
        let now = self.now();
        let _guard = self.write_lock.lock().await;
        sqlx::query(
            "INSERT INTO balance_observations (account_id, asset_id, raw_quantity, observed_at, provider, status)
             VALUES (?, ?, '0', ?, ?, 'fresh')",
        )
        .bind(account_id)
        .bind(asset_id)
        .bind(now)
        .bind(provider)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Persists one transaction for one account. Returns `true` when the
    /// account had not seen this transaction before.
    pub async fn ingest_transaction(&self, account_id: &str, spec: &TxSpec) -> Result<bool> {
        let now = self.now();
        let tx_id = tx_row_id(spec.network, &spec.hash);
        let part = spec.part.as_deref();
        let _guard = self.write_lock.lock().await;
        let mut tx = self.pool.begin().await?;

        // Match economic facts, not response array positions. Preserve legacy
        // IDs where the facts still agree, so existing decisions stay attached.
        let prefix = match part {
            None => format!("{tx_id}:{account_id}"),
            Some(p) => format!("{tx_id}:{account_id}:{p}"),
        };
        let previous = sqlx::query(
            "SELECT id, asset_id, signed_raw_quantity, direction, leg_type,json_extract(evidence,'$.counterparty') AS counterparty FROM activity_legs
             WHERE transaction_id = ? AND account_id = ?
             AND COALESCE(json_extract(evidence, '$.part'), '') = ? ORDER BY id",
        )
        .bind(&tx_id)
        .bind(account_id)
        .bind(part.unwrap_or(""))
        .fetch_all(&mut *tx)
        .await?;
        let mut next_index: i64 =
            sqlx::query_scalar("SELECT next_index FROM movement_slots WHERE prefix = ?")
                .bind(&prefix)
                .fetch_optional(&mut *tx)
                .await?
                .unwrap_or(0);
        for old in &previous {
            if let Some(index) = old
                .get::<String, _>("id")
                .strip_prefix(&format!("{prefix}:"))
                .and_then(|i| i.parse::<i64>().ok())
            {
                next_index = next_index.max(index + 1);
            }
        }
        // Also reserve removed legacy IDs that still have user decisions.
        let decided: Vec<String> = sqlx::query_scalar(
            "SELECT target_id FROM accounting_overrides WHERE target_kind = 'leg' AND target_id >= ? AND target_id < ?",
        ).bind(format!("{prefix}:")).bind(format!("{prefix};"))
        .fetch_all(&mut *tx)
        .await?;
        for id in decided {
            if let Some(index) = id
                .strip_prefix(&format!("{prefix}:"))
                .and_then(|s| s.parse::<i64>().ok())
            {
                next_index = next_index.max(index + 1);
            }
        }
        let mut reused = BTreeSet::new();

        if part.is_none() {
            // A final/failed/confirmed record is never downgraded to pending by a
            // late mempool observation.
            sqlx::query(
                "INSERT INTO chain_transactions (id, network_id, canonical_tx_id, block_height, position, occurred_at, status, source_provider, source_refs)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(network_id, canonical_tx_id) DO UPDATE SET
                     block_height = COALESCE(excluded.block_height, chain_transactions.block_height),
                     position = COALESCE(excluded.position, chain_transactions.position),
                     occurred_at = CASE WHEN excluded.status = 'pending' AND chain_transactions.status != 'pending'
                                        THEN chain_transactions.occurred_at ELSE excluded.occurred_at END,
                     status = CASE WHEN excluded.status = 'pending' AND chain_transactions.status IN ('confirmed', 'final', 'failed')
                                   THEN chain_transactions.status ELSE excluded.status END,
                     source_provider = excluded.source_provider,
                     source_refs = excluded.source_refs",
            )
            .bind(&tx_id)
            .bind(spec.network.as_str())
            .bind(&spec.hash)
            .bind(spec.block_height)
            .bind(&spec.position)
            .bind(spec.occurred_at)
            .bind(spec.status.as_str())
            .bind(spec.provider)
            .bind(spec.evidence.to_string())
            .execute(&mut *tx)
            .await?;
        } else {
            // A component never overrides the transaction-level record written
            // by the main category; it only creates it when it is first.
            sqlx::query(
                "INSERT INTO chain_transactions (id, network_id, canonical_tx_id, block_height, position, occurred_at, status, source_provider, source_refs)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(network_id, canonical_tx_id) DO UPDATE SET
                     block_height = COALESCE(chain_transactions.block_height, excluded.block_height)",
            )
            .bind(&tx_id)
            .bind(spec.network.as_str())
            .bind(&spec.hash)
            .bind(spec.block_height)
            .bind(&spec.position)
            .bind(spec.occurred_at)
            .bind(spec.status.as_str())
            .bind(spec.provider)
            .bind(spec.evidence.to_string())
            .execute(&mut *tx)
            .await?;
        }

        let known: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM account_transactions WHERE account_id = ? AND transaction_id = ?",
        )
        .bind(account_id)
        .bind(&tx_id)
        .fetch_optional(&mut *tx)
        .await?;
        if part.is_none() {
            sqlx::query(
                "INSERT INTO account_transactions (account_id, transaction_id, operation, provider, decoding, first_seen_at)
                 VALUES (?, ?, ?, ?, ?, ?)
                 ON CONFLICT(account_id, transaction_id) DO UPDATE SET
                     operation = excluded.operation, provider = excluded.provider, decoding = excluded.decoding",
            )
            .bind(account_id)
            .bind(&tx_id)
            .bind(&spec.operation)
            .bind(spec.provider)
            .bind(spec.decoding.as_str())
            .bind(now)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "DELETE FROM activity_legs WHERE transaction_id = ? AND account_id = ?
                 AND json_extract(evidence, '$.part') IS NULL",
            )
            .bind(&tx_id)
            .bind(account_id)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query(
                "INSERT INTO account_transactions (account_id, transaction_id, operation, provider, decoding, first_seen_at)
                 VALUES (?, ?, ?, ?, ?, ?)
                 ON CONFLICT(account_id, transaction_id) DO NOTHING",
            )
            .bind(account_id)
            .bind(&tx_id)
            .bind(&spec.operation)
            .bind(spec.provider)
            .bind(spec.decoding.as_str())
            .bind(now)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "DELETE FROM activity_legs WHERE transaction_id = ? AND account_id = ?
                 AND json_extract(evidence, '$.part') = ?",
            )
            .bind(&tx_id)
            .bind(account_id)
            .bind(part)
            .execute(&mut *tx)
            .await?;
        }
        if part.is_none() || spec.fee.is_some() {
            sqlx::query(
                "DELETE FROM transaction_fees WHERE transaction_id = ? AND payer_account_id = ?",
            )
            .bind(&tx_id)
            .bind(account_id)
            .execute(&mut *tx)
            .await?;
        }

        let (id_prefix, base_evidence) = match part {
            None => (format!("{tx_id}:{account_id}"), "{}".to_owned()),
            Some(p) => (
                format!("{tx_id}:{account_id}:{p}"),
                serde_json::json!({ "part": p }).to_string(),
            ),
        };
        for leg in &spec.legs {
            let asset_id = upsert_asset_in(&mut tx, &leg.asset, now).await?;
            let existing = previous.iter().find(|old| {
                !reused.contains(&old.get::<String, _>("id"))
                    && old.get::<String, _>("asset_id") == asset_id
                    && old.get::<String, _>("signed_raw_quantity") == leg.signed_raw.to_string()
                    && old.get::<String, _>("direction") == leg.direction.as_str()
                    && old.get::<String, _>("leg_type") == leg.leg_type
                    && old
                        .get::<Option<String>, _>("counterparty")
                        .is_none_or(|p| Some(p) == leg.counterparty)
            });
            let leg_id = match existing {
                Some(old) => old.get::<String, _>("id"),
                None => {
                    let id = format!("{id_prefix}:{next_index}");
                    next_index += 1;
                    id
                }
            };
            let mut evidence: serde_json::Value =
                serde_json::from_str(&base_evidence).expect("internal evidence JSON");
            evidence["counterparty"] = serde_json::json!(leg.counterparty);
            let evidence = evidence.to_string();
            reused.insert(leg_id.clone());
            sqlx::query(
                "INSERT INTO activity_legs (id, transaction_id, account_id, asset_id, signed_raw_quantity, direction, leg_type, decoding, unresolved, evidence)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(leg_id)
            .bind(&tx_id)
            .bind(account_id)
            .bind(&asset_id)
            .bind(leg.signed_raw.to_string())
            .bind(leg.direction.as_str())
            .bind(&leg.leg_type)
            .bind(leg.decoding.as_str())
            .bind(i64::from(leg.unresolved))
            .bind(&evidence)
            .execute(&mut *tx)
            .await?;
        }
        if let Some(fee) = &spec.fee {
            let asset_id = upsert_asset_in(&mut tx, &fee.asset, now).await?;
            sqlx::query(
                "INSERT INTO transaction_fees (id, transaction_id, payer_account_id, asset_id, raw_quantity, attribution)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(format!("{tx_id}:{account_id}:fee"))
            .bind(&tx_id)
            .bind(account_id)
            .bind(&asset_id)
            .bind(fee.raw.to_string())
            .bind(fee.attribution.as_str())
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query(
            "INSERT INTO movement_slots (prefix, next_index) VALUES (?, ?)
            ON CONFLICT(prefix) DO UPDATE SET next_index = excluded.next_index",
        )
        .bind(&prefix)
        .bind(next_index)
        .execute(&mut *tx)
        .await?;
        mark_dirty_in(&mut tx).await?;
        tx.commit().await?;
        Ok(known.is_none())
    }

    /// Which `(hash, part)` components this account already has (see
    /// [`TxSpec::part`]).
    pub async fn known_account_parts(
        &self,
        account_id: &str,
        network: NetworkId,
        parts: &[(String, String)],
    ) -> Result<BTreeSet<(String, String)>> {
        let mut out = BTreeSet::new();
        for (hash, part) in parts {
            let found: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM activity_legs
                 WHERE transaction_id = ? AND account_id = ? AND json_extract(evidence, '$.part') = ?
                 LIMIT 1",
            )
            .bind(tx_row_id(network, hash))
            .bind(account_id)
            .bind(part)
            .fetch_optional(&self.pool)
            .await?;
            if found.is_some() {
                out.insert((hash.clone(), part.clone()));
            }
        }
        Ok(out)
    }

    /// Which of `hashes` this account already has.
    pub async fn known_account_transactions(
        &self,
        account_id: &str,
        network: NetworkId,
        hashes: &[String],
    ) -> Result<BTreeSet<String>> {
        if hashes.is_empty() {
            return Ok(BTreeSet::new());
        }
        let mut query = QueryBuilder::<Sqlite>::new(
            "SELECT t.canonical_tx_id FROM account_transactions x
             JOIN chain_transactions t ON t.id = x.transaction_id
             WHERE x.account_id = ",
        );
        query
            .push_bind(account_id)
            .push(" AND t.network_id = ")
            .push_bind(network.as_str())
            .push(" AND t.canonical_tx_id IN (");
        let mut list = query.separated(", ");
        for h in hashes {
            list.push_bind(h);
        }
        list.push_unseparated(")");
        let rows: Vec<String> = query.build_query_scalar().fetch_all(&self.pool).await?;
        Ok(rows.into_iter().collect())
    }

    /// Hashes of this account's transactions still recorded as pending.
    pub async fn known_foreign_account_transactions(
        &self,
        account: &str,
        network: NetworkId,
        hashes: &[String],
        provider: &str,
    ) -> Result<BTreeSet<String>> {
        if hashes.is_empty() {
            return Ok(BTreeSet::new());
        }
        let mut q = QueryBuilder::<Sqlite>::new(
            "SELECT t.canonical_tx_id FROM account_transactions x JOIN chain_transactions t ON t.id=x.transaction_id WHERE x.account_id=",
        );
        q.push_bind(account)
            .push(" AND t.network_id=")
            .push_bind(network.as_str())
            .push(" AND x.provider != ")
            .push_bind(provider)
            .push(" AND t.canonical_tx_id IN (");
        let mut list = q.separated(", ");
        for hash in hashes {
            list.push_bind(hash);
        }
        list.push_unseparated(")");
        Ok(q.build_query_scalar::<String>()
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .collect())
    }

    /// Hashes of this account's transactions still recorded as pending.
    pub async fn pending_account_transactions(&self, account_id: &str) -> Result<Vec<String>> {
        Ok(sqlx::query_scalar(
            "SELECT t.canonical_tx_id FROM account_transactions x
             JOIN chain_transactions t ON t.id = x.transaction_id
             WHERE x.account_id = ? AND t.status = 'pending' ORDER BY t.occurred_at, t.id",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?)
    }

    /// Bounded confirmed tail for authoritative reorg checks.
    pub async fn recent_confirmed_transactions(
        &self,
        account_id: &str,
        min_height: i64,
        limit: u32,
    ) -> Result<Vec<String>> {
        Ok(sqlx::query_scalar("SELECT t.canonical_tx_id FROM account_transactions x JOIN chain_transactions t ON t.id=x.transaction_id WHERE x.account_id=? AND t.status='confirmed' AND t.block_height>=? ORDER BY t.block_height DESC,t.id LIMIT ?")
            .bind(account_id).bind(min_height).bind(i64::from(limit)).fetch_all(&self.pool).await?)
    }

    /// Only call after an authoritative transaction lookup, never on a timeout
    /// or because a paginated address listing omitted the transaction.
    pub async fn invalidate_confirmation(
        &self,
        network: NetworkId,
        hash: &str,
        pending: bool,
    ) -> Result<()> {
        let _guard = self.write_lock.lock().await;
        let mut tx = self.pool.begin().await?;
        sqlx::query("UPDATE chain_transactions SET status=?,block_height=NULL,position=NULL WHERE network_id=? AND canonical_tx_id=?")
            .bind(if pending {"pending"} else {"reorged"}).bind(network.as_str()).bind(hash).execute(&mut *tx).await?;
        mark_dirty_in(&mut tx).await?;
        tx.commit().await?;
        Ok(())
    }

    /// A pending transaction that disappeared (replaced or dropped) is kept as
    /// evidence with status `reorged`; it no longer counts as activity.
    pub async fn mark_transaction_dropped(&self, network: NetworkId, hash: &str) -> Result<()> {
        let _guard = self.write_lock.lock().await;
        let done = sqlx::query(
            "UPDATE chain_transactions SET status = 'reorged'
             WHERE network_id = ? AND canonical_tx_id = ? AND status = 'pending'",
        )
        .bind(network.as_str())
        .bind(hash)
        .execute(&self.pool)
        .await?;
        if done.rows_affected() > 0 {
            self.mark_accounting_dirty().await?;
        }
        Ok(())
    }

    pub async fn checkpoint(
        &self,
        account_id: &str,
        provider: &str,
        category: &str,
    ) -> Result<Checkpoint> {
        let row = sqlx::query(
            "SELECT backfill_cursor, boundary, coverage, earliest_covered_at, retry_state
             FROM sync_checkpoints WHERE account_id = ? AND provider = ? AND category = ?",
        )
        .bind(account_id)
        .bind(provider)
        .bind(category)
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else {
            return Ok(Checkpoint::default());
        };
        Ok(Checkpoint {
            backfill_cursor: row.get("backfill_cursor"),
            boundary: row.get("boundary"),
            coverage: Coverage::parse(&row.get::<String, _>("coverage"))?,
            earliest_covered_at: row.get("earliest_covered_at"),
            state: serde_json::from_str(&row.get::<String, _>("retry_state")).unwrap_or_default(),
        })
    }

    pub async fn save_checkpoint(
        &self,
        account_id: &str,
        provider: &str,
        category: &str,
        checkpoint: &Checkpoint,
    ) -> Result<()> {
        let state = serde_json::to_string(&checkpoint.state)
            .map_err(|e| StoreError::Invalid(e.to_string()))?;
        let _guard = self.write_lock.lock().await;
        sqlx::query(
            "INSERT INTO sync_checkpoints (id, account_id, provider, category, backfill_cursor, forward_cursor, boundary, coverage, earliest_covered_at, retry_state, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(account_id, provider, category) DO UPDATE SET
                 backfill_cursor = excluded.backfill_cursor, boundary = excluded.boundary,
                 coverage = excluded.coverage, earliest_covered_at = excluded.earliest_covered_at,
                 retry_state = excluded.retry_state, updated_at = excluded.updated_at",
        )
        .bind(format!("{account_id}:{provider}:{category}"))
        .bind(account_id)
        .bind(provider)
        .bind(category)
        .bind(&checkpoint.backfill_cursor)
        .bind(&checkpoint.boundary)
        .bind(checkpoint.coverage.as_str())
        .bind(checkpoint.earliest_covered_at)
        .bind(state)
        .bind(self.now())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Oldest transaction time stored for an account, if any.
    pub async fn earliest_account_transaction(&self, account_id: &str) -> Result<Option<i64>> {
        Ok(sqlx::query_scalar(
            "SELECT MIN(t.occurred_at) FROM account_transactions x
             JOIN chain_transactions t ON t.id = x.transaction_id
             WHERE x.account_id = ? AND t.status != 'pending'",
        )
        .bind(account_id)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn sync_status(&self) -> Result<Vec<AccountSyncStatus>> {
        let rows = sqlx::query(
            "SELECT a.id,
                    c.provider, c.coverage, c.earliest_covered_at, c.retry_state,
                    (SELECT COUNT(*) FROM account_transactions x WHERE x.account_id = a.id) AS tx_count
             FROM accounts a
             LEFT JOIN sync_checkpoints c ON c.account_id = a.id AND c.category = 'history'
             ORDER BY a.created_at, a.id",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|r| {
                let state: CheckpointState = r
                    .get::<Option<String>, _>("retry_state")
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or_default();
                Ok(AccountSyncStatus {
                    account_id: r.get("id"),
                    provider: r.get("provider"),
                    coverage: r
                        .get::<Option<String>, _>("coverage")
                        .map(|c| Coverage::parse(&c))
                        .transpose()?,
                    last_attempt_at: state.last_attempt_at,
                    last_success_at: state.last_success_at,
                    last_error: state.last_error,
                    earliest_covered_at: r.get("earliest_covered_at"),
                    transaction_count: u32::try_from(r.get::<i64, _>("tx_count")).unwrap_or(0),
                    pending_incomplete: state.pending_incomplete,
                })
            })
            .collect()
    }

    /// Stores a quote. Live ticks are thinned to one per asset and provider per
    /// five minutes (the newest wins), which is the resolution charts use.
    pub async fn insert_price(&self, price: &PriceSpec) -> Result<()> {
        let _guard = self.write_lock.lock().await;
        let mut tx = self.pool.begin().await?;
        if price.granularity == "tick" {
            sqlx::query(
                "DELETE FROM prices WHERE asset_id = ? AND provider = ? AND granularity = 'tick'
                 AND observed_at > ? AND observed_at <= ?",
            )
            .bind(&price.asset_id)
            .bind(price.provider)
            .bind(price.observed_at - TICK_RETENTION_SECONDS)
            .bind(price.observed_at)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query(
            "INSERT INTO prices (asset_id, provider, price_usd, requested_at, observed_at, granularity, quality, change_24h_percent)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&price.asset_id)
        .bind(price.provider)
        .bind(&price.price_usd)
        .bind(price.requested_at)
        .bind(price.observed_at)
        .bind(price.granularity)
        .bind(price.quality)
        .bind(&price.change_24h_percent)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Records the provider identity used to price an asset (one row per
    /// asset/provider; a changed identity bumps the version).
    pub async fn upsert_asset_mapping(
        &self,
        asset_id: &str,
        provider: &str,
        provider_asset_id: &str,
        confidence: &str,
        manually_verified: bool,
    ) -> Result<()> {
        let _guard = self.write_lock.lock().await;
        let mut tx = self.pool.begin().await?;
        let current = sqlx::query(
            "SELECT provider_asset_id, version FROM asset_mappings
             WHERE asset_id = ? AND provider = ? ORDER BY version DESC LIMIT 1",
        )
        .bind(asset_id)
        .bind(provider)
        .fetch_optional(&mut *tx)
        .await?;
        let version = match current {
            Some(row) if row.get::<String, _>(0) == provider_asset_id => return Ok(()),
            Some(row) => row.get::<i64, _>(1) + 1,
            None => 1,
        };
        sqlx::query(
            "INSERT INTO asset_mappings (id, asset_id, provider, provider_asset_id, confidence, manually_verified, version, effective_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(format!("{asset_id}:{provider}:{version}"))
        .bind(asset_id)
        .bind(provider)
        .bind(provider_asset_id)
        .bind(confidence)
        .bind(i64::from(manually_verified))
        .bind(version)
        .bind(self.now())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Non-spam assets with a non-zero latest balance in any active account.
    pub async fn held_assets(&self) -> Result<Vec<HeldAsset>> {
        let rows = sqlx::query(
            "SELECT DISTINCT s.id, s.network_id, s.asset_kind, s.canonical_identifier, s.symbol
             FROM balance_observations b
             JOIN assets s ON s.id = b.asset_id
             JOIN accounts a ON a.id = b.account_id
             WHERE a.archived = 0 AND s.verification != 'spam' AND b.raw_quantity != '0'
               AND b.id = (SELECT b2.id FROM balance_observations b2
                           WHERE b2.account_id = b.account_id AND b2.asset_id = b.asset_id
                           ORDER BY b2.observed_at DESC, b2.id DESC LIMIT 1)
             ORDER BY s.id",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|r| {
                let network: String = r.get("network_id");
                let kind: String = r.get("asset_kind");
                Ok(HeldAsset {
                    asset_id: r.get("id"),
                    network: NetworkId::parse(&network)?,
                    contract: (kind == "token").then(|| r.get("canonical_identifier")),
                    symbol: r.get("symbol"),
                })
            })
            .collect()
    }

    /// Assets `provider` had no quote for at or after `since`.
    pub async fn recent_quote_misses(
        &self,
        provider: &str,
        since: i64,
    ) -> Result<BTreeSet<String>> {
        let rows: Vec<String> = sqlx::query_scalar(
            "SELECT asset_id FROM quote_misses WHERE provider = ? AND missed_at >= ?",
        )
        .bind(provider)
        .bind(since)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().collect())
    }

    /// Records which requested assets got no quote and forgets earlier misses
    /// of the ones that did.
    pub async fn record_quote_results(
        &self,
        provider: &str,
        missed: &[(String, String)],
        quoted: &[String],
    ) -> Result<()> {
        let now = self.now();
        let _guard = self.write_lock.lock().await;
        let mut tx = self.pool.begin().await?;
        for (asset_id, provider_asset_id) in missed {
            sqlx::query(
                "INSERT INTO quote_misses (asset_id, provider, provider_asset_id, missed_at) VALUES (?, ?, ?, ?)
                 ON CONFLICT(asset_id, provider) DO UPDATE SET
                     provider_asset_id = excluded.provider_asset_id, missed_at = excluded.missed_at",
            )
            .bind(asset_id)
            .bind(provider)
            .bind(provider_asset_id)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }
        for asset_id in quoted {
            sqlx::query("DELETE FROM quote_misses WHERE asset_id = ? AND provider = ?")
                .bind(asset_id)
                .bind(provider)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Adds locally counted requests for a provider on a UTC day.
    pub async fn add_provider_usage(
        &self,
        provider: &str,
        day_utc: &str,
        requests: u32,
        last_error: Option<&str>,
    ) -> Result<()> {
        self.add_provider_usage_cost(provider, day_utc, requests, 0, last_error)
            .await
    }

    pub async fn add_provider_usage_cost(
        &self,
        provider: &str,
        day_utc: &str,
        requests: u32,
        credits: u32,
        last_error: Option<&str>,
    ) -> Result<()> {
        let _guard = self.write_lock.lock().await;
        sqlx::query(
            "INSERT INTO provider_usage (provider, day_utc, requests, last_error, credits) VALUES (?1, ?2, ?3, ?4, CAST(?5 AS TEXT))
             ON CONFLICT(provider, day_utc) DO UPDATE SET
                 requests = provider_usage.requests + excluded.requests,
                 credits = CAST(CAST(provider_usage.credits AS INTEGER) + CAST(excluded.credits AS INTEGER) AS TEXT),
                 last_error = excluded.last_error",
        )
        .bind(provider)
        .bind(day_utc)
        .bind(i64::from(requests))
        .bind(last_error)
        .bind(i64::from(credits))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn provider_usage(&self, day_utc: &str) -> Result<Vec<ProviderUsage>> {
        let rows = sqlx::query(
            "SELECT provider, requests, credits, last_error FROM provider_usage WHERE day_utc = ?",
        )
        .bind(day_utc)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| ProviderUsage {
                provider: r.get("provider"),
                requests: u32::try_from(r.get::<i64, _>("requests")).unwrap_or(u32::MAX),
                credits: r.get::<String, _>("credits").parse().unwrap_or(u32::MAX),
                last_error: r.get("last_error"),
            })
            .collect())
    }
}

/// Derived accounting is out of date after new evidence arrives.
pub(crate) async fn mark_dirty_in(tx: &mut sqlx::Transaction<'_, Sqlite>) -> Result<()> {
    sqlx::query(
        "INSERT INTO app_meta (key, value) VALUES ('accounting_dirty', '1')
         ON CONFLICT(key) DO UPDATE SET value = '1'",
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn upsert_asset_in(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    asset: &AssetSpec,
    now: i64,
) -> Result<String> {
    let id = asset.id();
    let existing = sqlx::query("SELECT decimals FROM assets WHERE id = ?")
        .bind(&id)
        .fetch_optional(&mut **tx)
        .await?;
    if let Some(row) = existing {
        let decimals: i64 = row.get(0);
        if decimals != i64::from(asset.decimals) {
            return Err(StoreError::Invalid(format!(
                "provider {} reports {} decimals for {id}, stored {decimals}",
                asset.provider, asset.decimals
            )));
        }
        sqlx::query(
            "UPDATE assets SET
                 symbol = COALESCE(?2, symbol), name = COALESCE(?3, name),
                 verification = CASE WHEN ?4 = 'unverified' THEN verification ELSE ?4 END,
                 metadata_provider = ?5, metadata_updated_at = ?6
             WHERE id = ?1",
        )
        .bind(&id)
        .bind(&asset.symbol)
        .bind(&asset.name)
        .bind(asset.verification.as_str())
        .bind(asset.provider)
        .bind(now)
        .execute(&mut **tx)
        .await?;
    } else {
        sqlx::query(
            "INSERT INTO assets (id, network_id, asset_kind, canonical_identifier, decimals, symbol, name, verification, metadata_provider, metadata_updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(asset.network.as_str())
        .bind(if asset.contract.is_some() { "token" } else { "native" })
        .bind(asset.contract.as_deref().unwrap_or(""))
        .bind(i64::from(asset.decimals))
        .bind(&asset.symbol)
        .bind(&asset.name)
        .bind(asset.verification.as_str())
        .bind(asset.provider)
        .bind(now)
        .execute(&mut **tx)
        .await?;
    }
    Ok(id)
}
