//! Explicit local maintenance. Evidence and user decisions survive cache cleanup/rescans.
use crate::{Account, Result, Store, StoreError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::Row;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct RemovalPreview {
    pub account: Account,
    /// Conservative dependency set: ownership changes may affect any account on this network.
    pub related_accounts: Vec<Account>,
    pub movements: u32,
    pub fees: u32,
    pub transactions: u32,
    pub decisions: u32,
    pub revision: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ProviderQuota {
    pub daily_requests: u32,
    pub daily_credits: u32,
    pub monthly_requests: u32,
}
impl ProviderQuota {
    pub fn defaults(provider: &str) -> Self {
        let daily_requests = match provider {
            "zerion" => 1600,
            "livecoinwatch" => 9000,
            "esplora" | "mempool" | "trongrid" | "tonapi" => 5000,
            "defillama" => 10000,
            "alchemy" | "helius" => 5000,
            _ => 1000,
        };
        Self {
            daily_requests,
            daily_credits: match provider {
                "alchemy" => 150000,
                "helius" => 20000,
                "drpc" | "blockscout" => 80000,
                "ankr" => 700000,
                _ => daily_requests,
            },
            monthly_requests: if matches!(provider, "drpc" | "chainstack") {
                30000
            } else {
                daily_requests * 31
            },
        }
    }
}

impl Store {
    async fn removal_preview_locked(&self, id: &str) -> Result<RemovalPreview> {
        let account = self.account(id).await?;
        let related_accounts = self
            .list_accounts(None)
            .await?
            .into_iter()
            .filter(|a| a.id != id && a.network == account.network)
            .collect();
        let rows = sqlx::query("SELECT l.id,l.signed_raw_quantity,l.evidence,t.status,t.block_height FROM activity_legs l JOIN chain_transactions t ON t.id=l.transaction_id WHERE l.account_id=? ORDER BY l.id")
            .bind(id).fetch_all(&self.pool).await?;
        let fees = sqlx::query(
            "SELECT id,raw_quantity FROM transaction_fees WHERE payer_account_id=? ORDER BY id",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?;
        let transactions=sqlx::query("SELECT a.transaction_id,t.status FROM account_transactions a JOIN chain_transactions t ON t.id=a.transaction_id WHERE a.account_id=? ORDER BY a.transaction_id").bind(id).fetch_all(&self.pool).await?;
        let ids: std::collections::BTreeSet<String> = rows
            .iter()
            .chain(fees.iter())
            .map(|r| r.get("id"))
            .collect();
        let overrides = sqlx::query(
            "SELECT target_kind,target_id,version,payload FROM accounting_overrides ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut digest = Sha256::new();
        digest
            .update(serde_json::to_vec(&account).map_err(|e| StoreError::Invalid(e.to_string()))?);
        digest.update(
            serde_json::to_vec(&related_accounts)
                .map_err(|e| StoreError::Invalid(e.to_string()))?,
        );
        // Include imported evidence and all decisions, including cross-account pairing edits.
        for r in &rows {
            digest.update(
                serde_json::to_vec(&(
                    r.get::<String, _>("id"),
                    r.get::<String, _>("signed_raw_quantity"),
                    r.get::<String, _>("evidence"),
                    r.get::<String, _>("status"),
                    r.get::<Option<i64>, _>("block_height"),
                ))
                .unwrap(),
            );
        }
        for fee in &fees {
            digest.update(
                serde_json::to_vec(&(
                    fee.get::<String, _>("id"),
                    fee.get::<String, _>("raw_quantity"),
                ))
                .unwrap(),
            );
        }
        for t in &transactions {
            digest.update(
                serde_json::to_vec(&(
                    t.get::<String, _>("transaction_id"),
                    t.get::<String, _>("status"),
                ))
                .unwrap(),
            );
        }
        let mut decisions = 0;
        for r in overrides {
            let kind: String = r.get("target_kind");
            let target: String = r.get("target_id");
            let payload: String = r.get("payload");
            let own = (kind == "leg" && ids.contains(&target))
                || (kind == "lot"
                    && serde_json::from_str::<serde_json::Value>(&payload)
                        .ok()
                        .and_then(|v| {
                            v.get("account_id")
                                .and_then(|v| v.as_str())
                                .map(str::to_owned)
                        })
                        .as_deref()
                        == Some(id));
            decisions += u32::from(own);
            digest.update(
                serde_json::to_vec(&(kind, target, r.get::<i64, _>("version"), payload)).unwrap(),
            );
        }
        Ok(RemovalPreview {
            account,
            related_accounts,
            movements: rows.len().try_into().unwrap_or(u32::MAX),
            fees: fees.len().try_into().unwrap_or(u32::MAX),
            transactions: transactions.len().try_into().unwrap_or(u32::MAX),
            decisions,
            revision: format!("{:x}", digest.finalize()),
        })
    }
    pub async fn preview_account_removal(&self, id: &str) -> Result<RemovalPreview> {
        let _guard = self.write_lock.lock().await;
        self.removal_preview_locked(id).await
    }
    pub async fn remove_account(&self, id: &str, revision: &str) -> Result<()> {
        let _guard = self.write_lock.lock().await;
        if self.removal_preview_locked(id).await?.revision != revision {
            return Err(StoreError::Invalid(
                "data changed; review the removal preview again".into(),
            ));
        }
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM accounting_overrides WHERE (target_kind='leg' AND target_id IN (SELECT id FROM activity_legs WHERE account_id=? UNION SELECT id FROM transaction_fees WHERE payer_account_id=?)) OR (target_kind='lot' AND json_extract(payload,'$.account_id')=?)")
            .bind(id).bind(id).bind(id).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM transaction_fees WHERE payer_account_id=?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM accounts WHERE id=?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM chain_transactions WHERE NOT EXISTS (SELECT 1 FROM account_transactions a WHERE a.transaction_id=chain_transactions.id)").execute(&mut *tx).await?;
        crate::ingest::mark_dirty_in(&mut tx).await?;
        tx.commit().await?;
        Ok(())
    }
    /// Reset pagination without deleting observations or stable leg IDs/decisions.
    pub async fn prepare_rescan(&self, ids: &[String]) -> Result<()> {
        if ids.is_empty() || ids.len() > 50 {
            return Err(StoreError::Invalid("select 1-50 accounts".into()));
        }
        let _guard = self.write_lock.lock().await;
        for id in ids {
            self.account(id).await?;
        }
        let mut tx = self.pool.begin().await?;
        for id in ids {
            sqlx::query("DELETE FROM sync_checkpoints WHERE account_id=?")
                .bind(id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("UPDATE balance_observations SET status='stale' WHERE account_id=?")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        crate::ingest::mark_dirty_in(&mut tx).await?;
        tx.commit().await?;
        Ok(())
    }
    /// Retain historical prices and lot lineage; only discard retry/derived caches.
    pub async fn clear_caches(&self) -> Result<()> {
        let _guard = self.write_lock.lock().await;
        sqlx::query("DELETE FROM quote_misses")
            .execute(&self.pool)
            .await?;
        *self.view_cache.lock().await = Default::default();
        Ok(())
    }
    pub async fn provider_quota(&self, provider: &str) -> Result<ProviderQuota> {
        let raw: Option<String> = sqlx::query_scalar("SELECT value_json FROM settings WHERE key=?")
            .bind(format!("quota:{provider}"))
            .fetch_optional(&self.pool)
            .await?;
        Ok(raw
            .and_then(|v| serde_json::from_str(&v).ok())
            .unwrap_or_else(|| ProviderQuota::defaults(provider)))
    }
    /// Soft caps can be lowered; increasing above conservative defaults requires a future plan-aware design.
    pub async fn set_provider_quota(&self, provider: &str, quota: &ProviderQuota) -> Result<()> {
        let max = ProviderQuota::defaults(provider);
        if quota.daily_requests == 0
            || quota.daily_requests > max.daily_requests
            || quota.daily_credits == 0
            || quota.daily_credits > max.daily_credits
            || quota.monthly_requests == 0
            || quota.monthly_requests > max.monthly_requests
        {
            return Err(StoreError::Invalid(
                "quota must be positive and no higher than the conservative defaults".into(),
            ));
        }
        let _guard = self.write_lock.lock().await;
        sqlx::query("INSERT INTO settings(key,value_json,updated_at) VALUES(?,?,?) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json,updated_at=excluded.updated_at")
            .bind(format!("quota:{provider}")).bind(serde_json::to_string(quota).map_err(|e|StoreError::Invalid(e.to_string()))?).bind(self.now()).execute(&self.pool).await?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AccountCoverage {
    pub provider: String,
    pub category: String,
    pub coverage: crate::Coverage,
    pub earliest_covered_at: Option<i64>,
    pub updated_at: i64,
}
impl Store {
    pub async fn account_coverage(&self, id: &str) -> Result<Vec<AccountCoverage>> {
        self.account(id).await?;
        let rows = sqlx::query("SELECT provider, category, coverage, earliest_covered_at, updated_at FROM sync_checkpoints WHERE account_id=? ORDER BY provider,category")
            .bind(id).fetch_all(&self.pool).await?;
        rows.into_iter()
            .map(|r| {
                Ok(AccountCoverage {
                    provider: r.get("provider"),
                    category: r.get("category"),
                    coverage: crate::Coverage::parse(&r.get::<String, _>("coverage"))?,
                    earliest_covered_at: r.get("earliest_covered_at"),
                    updated_at: r.get("updated_at"),
                })
            })
            .collect()
    }
    pub async fn remove_empty_wallet(&self, id: &str) -> Result<()> {
        let _guard = self.write_lock.lock().await;
        let done = sqlx::query("DELETE FROM wallets WHERE id=? AND NOT EXISTS(SELECT 1 FROM accounts WHERE wallet_id=?)").bind(id).bind(id).execute(&self.pool).await?;
        if done.rows_affected() == 0 {
            return Err(StoreError::Invalid(
                "remove the wallet's accounts first".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ProfileKind;
    use portfolio_core::clock::FixedClock;
    use std::sync::Arc;
    async fn demo() -> Store {
        let store = Store::open_in_memory(ProfileKind::Demo, Arc::new(FixedClock(1790000000)))
            .await
            .unwrap();
        store.seed_demo().await.unwrap();
        store
    }
    #[tokio::test]
    async fn stale_removal_preview_is_rejected_and_other_accounts_survive() {
        let store = demo().await;
        let accounts = store.list_accounts(None).await.unwrap();
        let a = &accounts[0];
        let preview = store.preview_account_removal(&a.id).await.unwrap();
        sqlx::query("INSERT INTO accounting_overrides(id,target_kind,target_id,version,payload,created_at) VALUES('changed','leg','absent',1,'{}',0)").execute(&store.pool).await.unwrap();
        assert!(
            store
                .remove_account(&a.id, &preview.revision)
                .await
                .is_err()
        );
        assert_eq!(
            store.list_accounts(None).await.unwrap().len(),
            accounts.len()
        );
        let preview = store.preview_account_removal(&a.id).await.unwrap();
        store
            .remove_account(&a.id, &preview.revision)
            .await
            .unwrap();
        store.replay_accounting().await.unwrap();
        assert_eq!(
            store.list_accounts(None).await.unwrap().len(),
            accounts.len() - 1
        );
        assert!(
            sqlx::query("PRAGMA foreign_key_check")
                .fetch_all(&store.pool)
                .await
                .unwrap()
                .is_empty()
        );
        for other in accounts.iter().skip(1) {
            assert!(store.account(&other.id).await.is_ok());
        }
    }
    #[tokio::test]
    async fn cache_cleanup_and_rescan_preserve_history_prices_decisions_and_lots() {
        let store = demo().await;
        store.replay_accounting().await.unwrap();
        let counts = |store: Store| async move {
            let mut counts = Vec::new();
            for table in [
                "activity_legs",
                "prices",
                "accounting_overrides",
                "lots",
                "provider_usage",
            ] {
                counts.push(
                    sqlx::QueryBuilder::<sqlx::Sqlite>::new("SELECT COUNT(*) FROM ")
                        .push(table)
                        .build_query_scalar::<i64>()
                        .fetch_one(&store.pool)
                        .await
                        .unwrap(),
                );
            }
            counts
        };
        let before = counts(store.clone()).await;
        let account = store.list_accounts(None).await.unwrap().remove(0);
        store.clear_caches().await.unwrap();
        store
            .prepare_rescan(std::slice::from_ref(&account.id))
            .await
            .unwrap();
        assert_eq!(counts(store.clone()).await, before);
        assert!(store.prepare_rescan(&["missing".into()]).await.is_err());
        assert_eq!(counts(store.clone()).await, before);
        assert!(store.prepare_rescan(&[]).await.is_err());
    }
    #[tokio::test]
    async fn quota_updates_preserve_usage_and_reject_zero_or_above_default() {
        let store = demo().await;
        for provider in ["blockscout", "drpc"] {
            assert_eq!(
                store.provider_quota(provider).await.unwrap().daily_credits,
                80000
            );
        }
        let mut quota = store.provider_quota("alchemy").await.unwrap();
        quota.daily_requests = 100;
        store.set_provider_quota("alchemy", &quota).await.unwrap();
        assert_eq!(
            store
                .provider_quota("alchemy")
                .await
                .unwrap()
                .daily_requests,
            100
        );
        quota.daily_requests = 0;
        assert!(store.set_provider_quota("alchemy", &quota).await.is_err());
        quota.daily_requests = 5001;
        assert!(store.set_provider_quota("alchemy", &quota).await.is_err());
        assert_eq!(
            store
                .provider_quota("alchemy")
                .await
                .unwrap()
                .daily_requests,
            100
        );
    }
    #[tokio::test]
    async fn only_empty_wallets_can_be_removed_and_groups_survive() {
        let store = demo().await;
        let wallet = store.list_wallets().await.unwrap().remove(0);
        assert!(store.remove_empty_wallet(&wallet.id).await.is_err());
        let empty = store.create_wallet("Empty").await.unwrap();
        let group = store.create_group("Keep").await.unwrap();
        store
            .set_group_wallets(&group.id, std::slice::from_ref(&empty.id))
            .await
            .unwrap();
        store.remove_empty_wallet(&empty.id).await.unwrap();
        let group = store
            .list_groups()
            .await
            .unwrap()
            .into_iter()
            .find(|g| g.id == group.id)
            .unwrap();
        assert!(group.wallet_ids.is_empty());
    }
}

#[cfg(test)]
mod rollback_tests {
    use super::*;
    use crate::{ProfileKind, Scope};
    use portfolio_core::{clock::FixedClock, network::NetworkId};
    use std::sync::Arc;
    #[tokio::test]
    async fn reorg_replay_removes_affected_value_and_preserves_reviewed_decisions() {
        let store = Store::open_in_memory(ProfileKind::Demo, Arc::new(FixedClock(1790000000)))
            .await
            .unwrap();
        store.seed_demo().await.unwrap();
        store.replay_accounting().await.unwrap();
        let row=sqlx::query("SELECT l.id,t.network_id,t.canonical_tx_id,l.account_id FROM activity_legs l JOIN chain_transactions t ON t.id=l.transaction_id WHERE t.status IN ('confirmed','final') LIMIT 1").fetch_one(&store.pool).await.unwrap();
        let id: String = row.get("id");
        sqlx::query("INSERT INTO accounting_overrides(id,target_kind,target_id,version,payload,created_at) VALUES('decision','leg',?,999,'{\"note\":\"reviewed\"}',0)").bind(&id).execute(&store.pool).await.unwrap();
        let network = NetworkId::parse(&row.get::<String, _>("network_id")).unwrap();
        let hash: String = row.get("canonical_tx_id");
        let account: String = row.get("account_id");
        store
            .invalidate_confirmation(network, &hash, false)
            .await
            .unwrap();
        let replay = store.replay_accounting().await.unwrap();
        assert!(replay.orphaned_overrides > 0);
        let exists: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM accounting_overrides WHERE id='decision'")
                .fetch_one(&store.pool)
                .await
                .unwrap();
        assert_eq!(exists, 1);
        let notes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM leg_accounting WHERE leg_id=?")
            .bind(&id)
            .fetch_one(&store.pool)
            .await
            .unwrap();
        assert_eq!(notes, 0);
        store.resume_after_rollback(&account).await.unwrap();
        let statuses = store.account_coverage(&account).await.unwrap();
        assert!(
            statuses
                .iter()
                .all(|s| s.category == "finality" || s.coverage == crate::Coverage::Loading)
        );
        assert!(
            store
                .get_chart(&Scope::All, crate::ChartRange::Month)
                .await
                .is_ok()
        );
    }
}

#[cfg(test)]
mod owner_decision_tests {
    use super::*;
    use crate::ProfileKind;
    use portfolio_core::clock::FixedClock;
    use std::sync::Arc;
    #[tokio::test]
    async fn permanent_removal_deletes_own_opening_and_fee_decisions_only() {
        let store = Store::open_in_memory(ProfileKind::Demo, Arc::new(FixedClock(1790000000)))
            .await
            .unwrap();
        store.seed_demo().await.unwrap();
        let fee=sqlx::query("SELECT id,payer_account_id,asset_id FROM transaction_fees WHERE payer_account_id IS NOT NULL LIMIT 1").fetch_one(&store.pool).await.unwrap();
        let account: String = fee.get("payer_account_id");
        let fee_id: String = fee.get("id");
        let asset: String = fee.get("asset_id");
        let payload=serde_json::json!({"account_id":account,"asset_id":asset,"quantity":"1","basis_usd":"10","basis_kind":"known","acquired_at":1780000000,"cutoff_at":1780000000,"note":"owner opening"}).to_string();
        sqlx::query("INSERT INTO accounting_overrides(id,target_kind,target_id,version,payload,created_at) VALUES('own-opening','lot','own-opening',999,?,0)").bind(payload).execute(&store.pool).await.unwrap();
        sqlx::query("INSERT INTO accounting_overrides(id,target_kind,target_id,version,payload,created_at) VALUES('own-fee','leg',?,999,'{\"note\":\"fee review\"}',0)").bind(fee_id).execute(&store.pool).await.unwrap();
        let preview = store.preview_account_removal(&account).await.unwrap();
        assert!(preview.decisions >= 2);
        assert!(preview.fees > 0);
        store
            .remove_account(&account, &preview.revision)
            .await
            .unwrap();
        let left: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM accounting_overrides WHERE id IN ('own-opening','own-fee')",
        )
        .fetch_one(&store.pool)
        .await
        .unwrap();
        assert_eq!(left, 0);
        store.replay_accounting().await.unwrap();
    }
    #[tokio::test]
    async fn asset_custom_window_retains_exact_boundaries_and_rejects_invalid_dates() {
        let store = Store::open_in_memory(ProfileKind::Demo, Arc::new(FixedClock(1790000000)))
            .await
            .unwrap();
        store.seed_demo().await.unwrap();
        let asset = store
            .list_holdings(&crate::Scope::All)
            .await
            .unwrap()
            .remove(0)
            .asset_id;
        let (start, end) = (1788000000, 1789900000);
        let chart = store
            .asset_chart_window(
                &crate::Scope::All,
                &asset,
                crate::ChartRange::Month,
                Some((start, end)),
            )
            .await
            .unwrap();
        assert_eq!(chart.holdings.points.first().unwrap().t, start);
        assert_eq!(chart.holdings.points.last().unwrap().t, end);
        assert_eq!(chart.price.first().unwrap().t, start);
        assert_eq!(chart.price.last().unwrap().t, end);
        assert!(chart.price.len() <= 1001);
        assert!(chart.holdings.points.len() <= 1001);
        assert!(
            store
                .asset_chart_window(
                    &crate::Scope::All,
                    &asset,
                    crate::ChartRange::Month,
                    Some((end, start))
                )
                .await
                .is_err()
        );
    }
}
