//! Wallets, accounts, groups, and scope resolution (SPECIFICATION.md §4, §5.2).

use std::collections::BTreeSet;

use portfolio_core::address::normalize_address;
use portfolio_core::network::NetworkId;
use sqlx::Row;

use crate::{Account, Group, Result, Scope, Store, StoreError, Wallet};

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn clean_label(label: &str) -> Result<String> {
    let trimmed = label.trim();
    if trimmed.is_empty() {
        return Err(StoreError::Invalid("name must not be empty".into()));
    }
    if trimmed.chars().count() > 80 {
        return Err(StoreError::Invalid(
            "name must be at most 80 characters".into(),
        ));
    }
    Ok(trimmed.to_owned())
}

fn account_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<Account> {
    let network: String = row.get("network_id");
    Ok(Account {
        id: row.get("id"),
        wallet_id: row.get("wallet_id"),
        network: NetworkId::parse(&network)?,
        canonical_address: row.get("canonical_address"),
        display_address: row.get("display_address"),
        label: row.get("label"),
        archived: row.get::<i64, _>("archived") != 0,
        created_at: row.get("created_at"),
    })
}

impl Store {
    pub async fn create_wallet(&self, label: &str) -> Result<Wallet> {
        let label = clean_label(label)?;
        let id = new_id();
        let now = self.now();
        let _guard = self.write_lock.lock().await;
        sqlx::query("INSERT INTO wallets (id, label, archived, created_at) VALUES (?, ?, 0, ?)")
            .bind(&id)
            .bind(&label)
            .bind(now)
            .execute(&self.pool)
            .await?;
        Ok(Wallet {
            id,
            label,
            archived: false,
            created_at: now,
            account_count: 0,
        })
    }

    pub async fn list_wallets(&self) -> Result<Vec<Wallet>> {
        let rows = sqlx::query(
            "SELECT w.id, w.label, w.archived, w.created_at,
                    (SELECT COUNT(*) FROM accounts a WHERE a.wallet_id = w.id) AS account_count
             FROM wallets w ORDER BY w.created_at, w.id",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| Wallet {
                id: r.get("id"),
                label: r.get("label"),
                archived: r.get::<i64, _>("archived") != 0,
                created_at: r.get("created_at"),
                account_count: u32::try_from(r.get::<i64, _>("account_count")).unwrap_or(0),
            })
            .collect())
    }

    pub async fn rename_wallet(&self, id: &str, label: &str) -> Result<()> {
        let label = clean_label(label)?;
        let _guard = self.write_lock.lock().await;
        let done = sqlx::query("UPDATE wallets SET label = ? WHERE id = ?")
            .bind(label)
            .bind(id)
            .execute(&self.pool)
            .await?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound("wallet"));
        }
        Ok(())
    }

    /// Adds one public address on one network to a wallet.
    ///
    /// The address is validated and normalized locally first. An address that is
    /// already tracked is never duplicated; the caller receives the existing
    /// account so the UI can offer navigation or an explicit move.
    pub async fn add_account(
        &self,
        wallet_id: &str,
        network: NetworkId,
        address: &str,
        label: Option<&str>,
    ) -> Result<Account> {
        let normalized = normalize_address(network, address)?;
        let label = label.map(clean_label).transpose()?;
        let _guard = self.write_lock.lock().await;
        let mut tx = self.pool.begin().await?;

        let wallet_exists: Option<String> =
            sqlx::query_scalar("SELECT id FROM wallets WHERE id = ?")
                .bind(wallet_id)
                .fetch_optional(&mut *tx)
                .await?;
        if wallet_exists.is_none() {
            return Err(StoreError::NotFound("wallet"));
        }

        let existing = sqlx::query(
            "SELECT a.id, w.label FROM accounts a JOIN wallets w ON w.id = a.wallet_id
             WHERE a.network_id = ? AND a.canonical_address = ?",
        )
        .bind(network.as_str())
        .bind(&normalized.canonical)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(row) = existing {
            return Err(StoreError::AccountExists {
                account_id: row.get(0),
                wallet_label: row.get(1),
            });
        }

        let account = Account {
            id: new_id(),
            wallet_id: wallet_id.to_owned(),
            network,
            canonical_address: normalized.canonical,
            display_address: normalized.display,
            label,
            archived: false,
            created_at: self.now(),
        };
        sqlx::query(
            "INSERT INTO accounts (id, wallet_id, network_id, canonical_address, display_address, label, archived, created_at)
             VALUES (?, ?, ?, ?, ?, ?, 0, ?)",
        )
        .bind(&account.id)
        .bind(&account.wallet_id)
        .bind(network.as_str())
        .bind(&account.canonical_address)
        .bind(&account.display_address)
        .bind(&account.label)
        .bind(account.created_at)
        .execute(&mut *tx)
        .await?;
        crate::ingest::mark_dirty_in(&mut tx).await?;
        tx.commit().await?;
        Ok(account)
    }

    pub async fn list_accounts(&self, wallet_id: Option<&str>) -> Result<Vec<Account>> {
        let rows = sqlx::query(
            "SELECT * FROM accounts WHERE (?1 IS NULL OR wallet_id = ?1)
             ORDER BY created_at, id",
        )
        .bind(wallet_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(account_from_row).collect()
    }

    /// Archiving hides an account from current views but keeps its history and
    /// its role as a known owned counterparty.
    pub async fn set_account_archived(&self, id: &str, archived: bool) -> Result<()> {
        let _guard = self.write_lock.lock().await;
        let done = sqlx::query("UPDATE accounts SET archived = ? WHERE id = ?")
            .bind(i64::from(archived))
            .bind(id)
            .execute(&self.pool)
            .await?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound("account"));
        }
        Ok(())
    }

    /// Moves an account to another wallet; never creates a second balance.
    pub async fn move_account(&self, id: &str, wallet_id: &str) -> Result<()> {
        let _guard = self.write_lock.lock().await;
        let done = sqlx::query(
            "UPDATE accounts SET wallet_id = ?1 WHERE id = ?2 AND EXISTS (SELECT 1 FROM wallets WHERE id = ?1)",
        )
        .bind(wallet_id)
        .bind(id)
        .execute(&self.pool)
        .await?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound("account or wallet"));
        }
        Ok(())
    }

    pub async fn create_group(&self, label: &str) -> Result<Group> {
        let label = clean_label(label)?;
        let id = new_id();
        let _guard = self.write_lock.lock().await;
        sqlx::query("INSERT INTO groups (id, label, created_at) VALUES (?, ?, ?)")
            .bind(&id)
            .bind(&label)
            .bind(self.now())
            .execute(&self.pool)
            .await?;
        Ok(Group {
            id,
            label,
            wallet_ids: Vec::new(),
        })
    }

    /// Replaces a group's wallet membership. Membership is a view filter only.
    pub async fn set_group_wallets(&self, group_id: &str, wallet_ids: &[String]) -> Result<()> {
        let _guard = self.write_lock.lock().await;
        let mut tx = self.pool.begin().await?;
        let exists: Option<String> = sqlx::query_scalar("SELECT id FROM groups WHERE id = ?")
            .bind(group_id)
            .fetch_optional(&mut *tx)
            .await?;
        if exists.is_none() {
            return Err(StoreError::NotFound("group"));
        }
        sqlx::query("DELETE FROM group_wallets WHERE group_id = ?")
            .bind(group_id)
            .execute(&mut *tx)
            .await?;
        let unique: BTreeSet<&String> = wallet_ids.iter().collect();
        for wallet_id in unique {
            sqlx::query("INSERT INTO group_wallets (group_id, wallet_id) VALUES (?, ?)")
                .bind(group_id)
                .bind(wallet_id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Deletes a group. Wallets and accounts are never deleted with it.
    pub async fn delete_group(&self, group_id: &str) -> Result<()> {
        let _guard = self.write_lock.lock().await;
        sqlx::query("DELETE FROM groups WHERE id = ?")
            .bind(group_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn list_groups(&self) -> Result<Vec<Group>> {
        let groups = sqlx::query("SELECT id, label FROM groups ORDER BY created_at, id")
            .fetch_all(&self.pool)
            .await?;
        let members =
            sqlx::query("SELECT group_id, wallet_id FROM group_wallets ORDER BY wallet_id")
                .fetch_all(&self.pool)
                .await?;
        Ok(groups
            .iter()
            .map(|g| {
                let id: String = g.get("id");
                Group {
                    wallet_ids: members
                        .iter()
                        .filter(|m| m.get::<String, _>("group_id") == id)
                        .map(|m| m.get("wallet_id"))
                        .collect(),
                    label: g.get("label"),
                    id,
                }
            })
            .collect())
    }

    /// Resolves a scope to the deduplicated set of active account IDs.
    pub async fn resolve_scope(&self, scope: &Scope) -> Result<BTreeSet<String>> {
        let ids: Vec<String> = match scope {
            Scope::All => {
                sqlx::query_scalar(
                    "SELECT a.id FROM accounts a JOIN wallets w ON w.id = a.wallet_id
                     WHERE a.archived = 0 AND w.archived = 0",
                )
                .fetch_all(&self.pool)
                .await?
            }
            Scope::Wallet { id } => {
                sqlx::query_scalar("SELECT id FROM accounts WHERE wallet_id = ? AND archived = 0")
                    .bind(id)
                    .fetch_all(&self.pool)
                    .await?
            }
            Scope::Group { id } => sqlx::query_scalar(
                "SELECT a.id FROM accounts a JOIN group_wallets gw ON gw.wallet_id = a.wallet_id
                     JOIN wallets w ON w.id = a.wallet_id
                     WHERE gw.group_id = ? AND a.archived = 0 AND w.archived = 0",
            )
            .bind(id)
            .fetch_all(&self.pool)
            .await?,
            Scope::Accounts { ids } => ids.clone(),
        };
        Ok(ids.into_iter().collect())
    }
}
