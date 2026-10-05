//! Daily historical price coverage (ACCOUNTING.md §9, SPECIFICATION.md §9).
//!
//! Valuing fees, trades, transfers and past holdings needs historical quotes.
//! Daily series are downloaded once per asset and range and remembered in
//! `price_history_coverage`, so charts and replays never refetch them.

use portfolio_core::network::NetworkId;
use sqlx::Row;

use crate::accounting::DAY;
use crate::{Result, Store};

/// Earliest history kept for a held asset that has no recorded activity.
const HELD_LOOKBACK_DAYS: i64 = 365;
/// A provider without a series for an asset is asked again after this long.
pub const MISSING_RETRY_SECONDS: i64 = 7 * DAY;

/// An asset whose daily price history should cover `[need_from, now]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriceHistoryNeed {
    pub asset_id: String,
    pub network: NetworkId,
    pub contract: Option<String>,
    pub need_from: i64,
    pub covered_from: Option<i64>,
    pub covered_to: Option<i64>,
    pub retry_after: Option<i64>,
}

/// One daily quote from a historical series.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailyPrice {
    pub at: i64,
    /// Exact canonical decimal string.
    pub price_usd: String,
    pub low_confidence: bool,
}

impl Store {
    /// Non-spam, quoted assets with activity or a current balance, and the
    /// daily range each one needs.
    pub async fn price_history_needs(&self, provider: &str) -> Result<Vec<PriceHistoryNeed>> {
        let now = self.now();
        let rows = sqlx::query(
            "WITH used AS (
                 SELECT l.asset_id AS asset_id, MIN(t.occurred_at) AS first_at
                 FROM activity_legs l JOIN chain_transactions t ON t.id = l.transaction_id
                 WHERE t.status IN ('confirmed', 'final') GROUP BY l.asset_id
                 UNION ALL
                 SELECT f.asset_id, MIN(t.occurred_at)
                 FROM transaction_fees f JOIN chain_transactions t ON t.id = f.transaction_id
                 GROUP BY f.asset_id
                 UNION ALL
                 SELECT b.asset_id, ?1 FROM balance_observations b
                 WHERE b.raw_quantity != '0' GROUP BY b.asset_id
             )
             SELECT s.id, s.network_id, s.asset_kind, s.canonical_identifier, MIN(u.first_at) AS first_at,
                    c.covered_from, c.covered_to, c.retry_after
             FROM used u JOIN assets s ON s.id = u.asset_id
             LEFT JOIN price_history_coverage c ON c.asset_id = s.id AND c.provider = ?2
             WHERE s.verification != 'spam'
               -- Only assets with a market quote: a token no source prices today
               -- has no history either, and asking for it would waste the budget.
               AND EXISTS (SELECT 1 FROM prices p WHERE p.asset_id = s.id)
             GROUP BY s.id ORDER BY s.id",
        )
        .bind(now - HELD_LOOKBACK_DAYS * DAY)
        .bind(provider)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|r| {
                let network: String = r.get("network_id");
                let kind: String = r.get("asset_kind");
                let first: i64 = r.get("first_at");
                Ok(PriceHistoryNeed {
                    asset_id: r.get("id"),
                    network: NetworkId::parse(&network)?,
                    contract: (kind == "token").then(|| r.get("canonical_identifier")),
                    need_from: first - first.rem_euclid(DAY),
                    covered_from: r.get("covered_from"),
                    covered_to: r.get("covered_to"),
                    retry_after: r.get("retry_after"),
                })
            })
            .collect()
    }

    /// Replaces the provider's daily quotes in `[from, to]` and extends coverage.
    pub async fn store_price_history(
        &self,
        asset_id: &str,
        provider: &str,
        provider_asset_id: &str,
        from: i64,
        to: i64,
        points: &[DailyPrice],
    ) -> Result<()> {
        let now = self.now();
        let _guard = self.write_lock.lock().await;
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "DELETE FROM prices WHERE asset_id = ? AND provider = ? AND granularity = 'day'
             AND observed_at >= ? AND observed_at <= ?",
        )
        .bind(asset_id)
        .bind(provider)
        .bind(from)
        .bind(to)
        .execute(&mut *tx)
        .await?;
        for p in points.iter().filter(|p| p.at >= from && p.at <= to) {
            sqlx::query(
                "INSERT INTO prices (asset_id, provider, price_usd, requested_at, observed_at, granularity, quality)
                 VALUES (?, ?, ?, ?, ?, 'day', ?)",
            )
            .bind(asset_id)
            .bind(provider)
            .bind(&p.price_usd)
            .bind(now)
            .bind(p.at)
            .bind(if p.low_confidence { "low_confidence" } else { "estimated" })
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query(
            "INSERT INTO price_history_coverage (asset_id, provider, provider_asset_id, covered_from, covered_to, retry_after, last_error, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, NULL, NULL, ?6)
             ON CONFLICT(asset_id, provider) DO UPDATE SET
                 provider_asset_id = excluded.provider_asset_id,
                 covered_from = MIN(COALESCE(price_history_coverage.covered_from, excluded.covered_from), excluded.covered_from),
                 covered_to = MAX(COALESCE(price_history_coverage.covered_to, excluded.covered_to), excluded.covered_to),
                 retry_after = NULL, last_error = NULL, updated_at = excluded.updated_at",
        )
        .bind(asset_id)
        .bind(provider)
        .bind(provider_asset_id)
        .bind(from)
        .bind(to)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        crate::ingest::mark_dirty_in(&mut tx).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Records that the provider has no series for an asset (or failed), so it
    /// is not asked again before `retry_after`.
    pub async fn note_price_history_missing(
        &self,
        asset_id: &str,
        provider: &str,
        provider_asset_id: &str,
        error: &str,
    ) -> Result<()> {
        let now = self.now();
        let _guard = self.write_lock.lock().await;
        sqlx::query(
            "INSERT INTO price_history_coverage (asset_id, provider, provider_asset_id, retry_after, last_error, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(asset_id, provider) DO UPDATE SET
                 retry_after = excluded.retry_after, last_error = excluded.last_error, updated_at = excluded.updated_at",
        )
        .bind(asset_id)
        .bind(provider)
        .bind(provider_asset_id)
        .bind(now + MISSING_RETRY_SECONDS)
        .bind(error)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
