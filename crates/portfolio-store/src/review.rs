//! "Review missing data": movements that need a decision, and the versioned
//! user overrides that answer them (SPECIFICATION.md §5.7, ACCOUNTING.md §7, §10).
//!
//! Overrides never modify evidence. Each save appends a new version; the
//! latest version applies, the earlier ones remain as an audit trail.

use std::collections::BTreeSet;

use bigdecimal::Zero;
use portfolio_core::accounting::BasisKind;
use portfolio_core::decimal::{Dec, parse_dec, parse_raw_amount, raw_to_quantity, to_canonical};
use portfolio_core::network::NetworkId;
use sqlx::{Row, Sqlite};

use crate::accounting::{LegOverride, parse_basis_kind};
use crate::{
    LegDetail, OverrideVersion, PairCandidate, ReconciliationRow, ReplayReport, Result, ReviewItem,
    ReviewList, Scope, Store, StoreError,
};

/// How far apart an outgoing and an incoming leg may be to be offered as a pair.
const PAIR_WINDOW_SECONDS: i64 = 14 * 86_400;
const MAX_REVIEW_ROWS: u32 = 500;

/// Loaded facts about one leg needed to validate a decision.
pub(crate) struct LegFacts {
    pub account_id: String,
    pub asset_id: String,
    /// Signed asset units.
    pub quantity: Dec,
    pub occurred_at: i64,
}

fn invalid(msg: impl Into<String>) -> StoreError {
    StoreError::Invalid(msg.into())
}

/// Checks a decision against the leg it targets. `other` resolves a paired leg.
pub(crate) fn validate_override(
    leg_id: &str,
    leg: &LegFacts,
    o: &LegOverride,
    pair: Option<&LegFacts>,
) -> Result<()> {
    let incoming = leg.quantity > Dec::zero();
    if let Some(c) = o.classification
        && c.is_incoming() != incoming
    {
        return Err(invalid(if incoming {
            "an incoming movement can be a deposit or a reward"
        } else {
            "an outgoing movement can be a sale, payment, withdrawal, gift or transfer to an untracked own address"
        }));
    }
    for text in [&o.proceeds_usd, &o.price_usd].into_iter().flatten() {
        let v =
            parse_dec(text).map_err(|_| invalid(format!("{text:?} is not an exact decimal")))?;
        if v < Dec::zero() {
            return Err(invalid("amounts cannot be negative"));
        }
    }
    if let Some(lots) = &o.basis_lots {
        if !incoming {
            return Err(invalid("acquisition lots belong to an incoming movement"));
        }
        let mut sum = Dec::zero();
        for lot in lots {
            let q = parse_dec(&lot.quantity)
                .map_err(|_| invalid(format!("{:?} is not an exact decimal", lot.quantity)))?;
            if q <= Dec::zero() {
                return Err(invalid("lot quantities must be positive"));
            }
            if let Some(b) = &lot.basis_usd {
                let b =
                    parse_dec(b).map_err(|_| invalid(format!("{b:?} is not an exact decimal")))?;
                if b < Dec::zero() {
                    return Err(invalid("basis cannot be negative"));
                }
            }
            if lot.basis_usd.is_none() && lot.basis_kind != BasisKind::Unknown {
                return Err(invalid("a lot without a basis amount has unknown basis"));
            }
            if lot.acquired_at > leg.occurred_at {
                return Err(invalid(
                    "a lot cannot be acquired after it arrived in the account",
                ));
            }
            sum += q;
        }
        // Validated against the receipt's original quantity, not the current balance.
        if sum > leg.quantity {
            return Err(invalid(format!(
                "lots total {} but the receipt is {}",
                to_canonical(&sum),
                to_canonical(&leg.quantity)
            )));
        }
    }
    if let Some(pair_id) = &o.pair_with {
        let Some(other) = pair else {
            return Err(invalid("the paired movement does not exist"));
        };
        if incoming {
            return Err(invalid(
                "pair an outgoing movement with the receipt it became",
            ));
        }
        if pair_id == leg_id || other.account_id == leg.account_id {
            return Err(invalid("a transfer pairs two different owned accounts"));
        }
        if other.asset_id != leg.asset_id {
            return Err(invalid(
                "only the same chain-specific asset can be paired; bridges need a supported decoder",
            ));
        }
        if other.quantity <= Dec::zero() || other.quantity > leg.quantity.abs() {
            return Err(invalid(
                "the receipt must be incoming and not larger than what was sent",
            ));
        }
    }
    Ok(())
}

impl Store {
    pub(crate) async fn leg_facts(&self, leg_id: &str) -> Result<Option<LegFacts>> {
        let row = sqlx::query(
            "SELECT l.account_id, l.asset_id, l.signed_raw_quantity, a.decimals, t.occurred_at
             FROM activity_legs l JOIN assets a ON a.id = l.asset_id
             JOIN chain_transactions t ON t.id = l.transaction_id WHERE l.id = ?",
        )
        .bind(leg_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| {
            let decimals = u32::try_from(r.get::<i64, _>("decimals")).unwrap_or(0);
            Ok(LegFacts {
                account_id: r.get("account_id"),
                asset_id: r.get("asset_id"),
                quantity: raw_to_quantity(
                    &parse_raw_amount(&r.get::<String, _>("signed_raw_quantity"))?,
                    decimals,
                ),
                occurred_at: r.get("occurred_at"),
            })
        })
        .transpose()
    }

    /// Movements in scope that need a decision, plus reconciliation findings.
    pub async fn list_review_items(&self, scope: &Scope, limit: Option<u32>) -> Result<ReviewList> {
        let accounts = self.resolve_scope(scope).await?;
        let limit = limit.unwrap_or(MAX_REVIEW_ROWS).clamp(1, MAX_REVIEW_ROWS) as usize;
        let rows = sqlx::query(
            "SELECT la.leg_id, la.transaction_id, la.account_id, la.asset_id, la.occurred_at,
                    la.quantity, la.treatment, la.review, la.value_usd, t.network_id, a.symbol
             FROM leg_accounting la
             JOIN chain_transactions t ON t.id = la.transaction_id
             JOIN assets a ON a.id = la.asset_id
             WHERE la.review IS NOT NULL
             ORDER BY la.occurred_at DESC, la.leg_id",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut items = Vec::new();
        let mut total = 0u32;
        for r in rows {
            if !accounts.contains(&r.get::<String, _>("account_id")) {
                continue;
            }
            total += 1;
            if items.len() >= limit {
                continue;
            }
            items.push(ReviewItem {
                leg_id: r.get("leg_id"),
                transaction_id: r.get("transaction_id"),
                account_id: r.get("account_id"),
                network: NetworkId::parse(&r.get::<String, _>("network_id"))?,
                occurred_at: r.get("occurred_at"),
                asset_id: r.get("asset_id"),
                symbol: r.get("symbol"),
                quantity: r.get("quantity"),
                treatment: r.get("treatment"),
                reason: r.get("review"),
                value_usd: r.get("value_usd"),
            });
        }
        let recon = sqlx::query(
            "SELECT r.id, r.kind, r.account_id, r.asset_id, r.quantity, r.detail, a.symbol
             FROM reconciliation_items r LEFT JOIN assets a ON a.id = r.asset_id
             ORDER BY r.kind, r.account_id, r.asset_id, r.id",
        )
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .filter(|r| {
            r.get::<Option<String>, _>("account_id")
                .is_none_or(|a| accounts.contains(&a))
        })
        .map(|r| ReconciliationRow {
            id: r.get("id"),
            kind: r.get("kind"),
            account_id: r.get("account_id"),
            asset_id: r.get("asset_id"),
            symbol: r.get("symbol"),
            quantity: r.get("quantity"),
            detail: r.get("detail"),
        })
        .collect();
        Ok(ReviewList {
            items,
            total,
            reconciliation: recon,
        })
    }

    /// Everything known about a leg (or a fee), its decision history and
    /// candidate own-account receipts it could be paired with.
    pub async fn leg_detail(&self, leg_id: &str) -> Result<LegDetail> {
        let row = sqlx::query(
            "SELECT l.id, l.transaction_id, l.account_id, l.asset_id, l.signed_raw_quantity AS raw,
                    l.direction, a.symbol, a.decimals, t.network_id, t.canonical_tx_id, t.status,
                    t.block_height, t.occurred_at, t.source_provider,
                    (SELECT x.operation FROM account_transactions x
                     WHERE x.transaction_id = l.transaction_id AND x.account_id = l.account_id) AS operation
             FROM activity_legs l JOIN assets a ON a.id = l.asset_id
             JOIN chain_transactions t ON t.id = l.transaction_id WHERE l.id = ?1
             UNION ALL
             SELECT f.id, f.transaction_id, f.payer_account_id, f.asset_id, '-' || f.raw_quantity,
                    'out', a.symbol, a.decimals, t.network_id, t.canonical_tx_id, t.status,
                    t.block_height, t.occurred_at, t.source_provider, 'fee'
             FROM transaction_fees f JOIN assets a ON a.id = f.asset_id
             JOIN chain_transactions t ON t.id = f.transaction_id WHERE f.id = ?1",
        )
        .bind(leg_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StoreError::NotFound("movement"))?;
        let network = NetworkId::parse(&row.get::<String, _>("network_id"))?;
        let decimals = u32::try_from(row.get::<i64, _>("decimals")).unwrap_or(0);
        let quantity = raw_to_quantity(&parse_raw_amount(&row.get::<String, _>("raw"))?, decimals);
        let tx_hash: String = row.get("canonical_tx_id");
        let account_id: Option<String> = row.get("account_id");
        let account_id = account_id.unwrap_or_default();
        let asset_id: String = row.get("asset_id");
        let occurred_at: i64 = row.get("occurred_at");

        let note = sqlx::query(
            "SELECT treatment, counterparty_account_id, value_usd, value_estimated, basis_usd,
                    basis_kind, proceeds_usd, review FROM leg_accounting WHERE leg_id = ?",
        )
        .bind(leg_id)
        .fetch_optional(&self.pool)
        .await?;

        let history = self.override_history("leg", leg_id).await?;
        let current = history
            .last()
            .map(|v| v.payload.clone())
            .filter(|p| !p.is_empty());

        let mut pair_candidates = Vec::new();
        if quantity < Dec::zero() {
            let rows = sqlx::query(
                "SELECT l.id, l.account_id, l.signed_raw_quantity, l.transaction_id, t.occurred_at, t.network_id
                 FROM activity_legs l JOIN chain_transactions t ON t.id = l.transaction_id
                 LEFT JOIN leg_accounting la ON la.leg_id = l.id
                 WHERE l.asset_id = ? AND l.account_id != ? AND l.direction = 'in'
                   AND t.status IN ('confirmed', 'final')
                   AND t.occurred_at BETWEEN ? AND ?
                   AND (la.treatment IS NULL OR la.treatment NOT IN ('own_transfer_in', 'netted', 'before_opening'))
                 ORDER BY ABS(t.occurred_at - ?), l.id LIMIT 20",
            )
            .bind(&asset_id)
            .bind(&account_id)
            .bind(occurred_at - PAIR_WINDOW_SECONDS)
            .bind(occurred_at + PAIR_WINDOW_SECONDS)
            .bind(occurred_at)
            .fetch_all(&self.pool)
            .await?;
            for r in rows {
                let q = raw_to_quantity(
                    &parse_raw_amount(&r.get::<String, _>("signed_raw_quantity"))?,
                    decimals,
                );
                if q <= Dec::zero() || q > quantity.abs() {
                    continue;
                }
                pair_candidates.push(PairCandidate {
                    leg_id: r.get("id"),
                    account_id: r.get("account_id"),
                    network: NetworkId::parse(&r.get::<String, _>("network_id"))?,
                    occurred_at: r.get("occurred_at"),
                    quantity: to_canonical(&q),
                    transaction_id: r.get("transaction_id"),
                });
            }
        }

        let provider: String = row.get("source_provider");
        Ok(LegDetail {
            leg_id: leg_id.to_owned(),
            transaction_id: row.get("transaction_id"),
            // Synthetic demo transactions do not exist on any explorer.
            explorer_url: if provider == "demo" {
                None
            } else {
                network.explorer_tx_url(&tx_hash)
            },
            tx_hash,
            tx_status: row.get("status"),
            block_height: row.get("block_height"),
            provider,
            account_id,
            network,
            occurred_at,
            asset_id,
            symbol: row.get("symbol"),
            quantity: to_canonical(&quantity),
            direction: row.get("direction"),
            operation: row
                .get::<Option<String>, _>("operation")
                .unwrap_or_default(),
            treatment: note.as_ref().map(|n| n.get("treatment")),
            counterparty_account_id: note.as_ref().and_then(|n| n.get("counterparty_account_id")),
            value_usd: note.as_ref().and_then(|n| n.get("value_usd")),
            value_estimated: note
                .as_ref()
                .is_some_and(|n| n.get::<i64, _>("value_estimated") != 0),
            basis_usd: note.as_ref().and_then(|n| n.get("basis_usd")),
            basis_kind: note
                .as_ref()
                .and_then(|n| n.get::<Option<String>, _>("basis_kind"))
                .map(|k| parse_basis_kind(&k)),
            proceeds_usd: note.as_ref().and_then(|n| n.get("proceeds_usd")),
            review: note.as_ref().and_then(|n| n.get("review")),
            current,
            history,
            pair_candidates,
        })
    }

    async fn override_history(&self, kind: &str, target: &str) -> Result<Vec<OverrideVersion>> {
        sqlx::query(
            "SELECT version, created_at, evidence_ref, payload, orphaned FROM accounting_overrides
             WHERE target_kind = ? AND target_id = ? ORDER BY version",
        )
        .bind(kind)
        .bind(target)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|r| {
            Ok(OverrideVersion {
                version: r.get("version"),
                created_at: r.get("created_at"),
                source: r
                    .get::<Option<String>, _>("evidence_ref")
                    .unwrap_or_else(|| "manual".into()),
                payload: serde_json::from_str(&r.get::<String, _>("payload"))
                    .map_err(|e| StoreError::Corrupt(format!("override payload: {e}")))?,
                orphaned: r.get::<i64, _>("orphaned") != 0,
            })
        })
        .collect()
    }

    /// Saves a new version of the decision for a leg and replays accounting.
    /// An empty override reverts the leg to its default interpretation.
    pub async fn save_leg_override(
        &self,
        leg_id: &str,
        payload: &LegOverride,
    ) -> Result<ReplayReport> {
        let leg = self
            .leg_facts(leg_id)
            .await?
            .ok_or(StoreError::NotFound("movement"))?;
        let pair = match &payload.pair_with {
            Some(id) => self.leg_facts(id).await?,
            None => None,
        };
        validate_override(leg_id, &leg, payload, pair.as_ref())?;
        if let Some(pair_id) = &payload.pair_with {
            // A receipt can only be the destination of one pairing.
            let (current, _) = self.current_overrides().await?;
            if current
                .iter()
                .any(|(k, o)| k != leg_id && o.pair_with.as_deref() == Some(pair_id.as_str()))
            {
                return Err(invalid(
                    "that receipt is already paired with another movement",
                ));
            }
        }
        {
            let _guard = self.write_lock.lock().await;
            let mut tx = self.pool.begin().await?;
            insert_override(&mut tx, "leg", leg_id, payload, "manual", self.now()).await?;
            tx.commit().await?;
        }
        self.replay_accounting().await
    }

    /// Distinct accounts that have movements needing review (for badges).
    pub async fn accounts_needing_review(&self) -> Result<BTreeSet<String>> {
        let rows: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT account_id FROM leg_accounting WHERE review IS NOT NULL",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().collect())
    }
}

/// Appends the next version of an override inside an open transaction.
pub(crate) async fn insert_override<T: serde::Serialize>(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    kind: &str,
    target: &str,
    payload: &T,
    source: &str,
    now: i64,
) -> Result<i64> {
    let version: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(version), 0) + 1 FROM accounting_overrides WHERE target_kind = ? AND target_id = ?",
    )
    .bind(kind)
    .bind(target)
    .fetch_one(&mut **tx)
    .await?;
    let json = serde_json::to_string(payload).map_err(|e| invalid(e.to_string()))?;
    sqlx::query(
        "INSERT INTO accounting_overrides (id, target_kind, target_id, version, payload, evidence_ref, orphaned, created_at)
         VALUES (?, ?, ?, ?, ?, ?, 0, ?)",
    )
    .bind(format!("{kind}:{target}:{version}"))
    .bind(kind)
    .bind(target)
    .bind(version)
    .bind(json)
    .bind(source)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "INSERT INTO app_meta (key, value) VALUES ('accounting_dirty', '1')
         ON CONFLICT(key) DO UPDATE SET value = '1'",
    )
    .execute(&mut **tx)
    .await?;
    Ok(version)
}
