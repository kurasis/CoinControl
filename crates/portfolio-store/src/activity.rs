//! Activity list with keyset pagination (SPECIFICATION.md §5.6, §7).

use portfolio_core::decimal::{parse_raw_amount, raw_to_quantity, to_canonical};
use portfolio_core::network::NetworkId;
use sqlx::{QueryBuilder, Row, Sqlite};

use crate::{
    ActivityFilter, ActivityLeg, ActivityPage, ActivityRow, Result, Scope, Store, StoreError,
};

const MAX_PAGE: u32 = 200;

fn parse_cursor(cursor: &str) -> Result<(i64, String, String)> {
    let mut parts = cursor.splitn(3, '|');
    let (Some(t), Some(tx), Some(acct)) = (parts.next(), parts.next(), parts.next()) else {
        return Err(StoreError::Invalid("malformed activity cursor".into()));
    };
    let t = t
        .parse()
        .map_err(|_| StoreError::Invalid("malformed activity cursor".into()))?;
    Ok((t, tx.to_owned(), acct.to_owned()))
}

impl Store {
    /// Activity for the scope, newest first, one row per (transaction, account).
    ///
    /// Rows come from `account_transactions`, so a transaction that only paid a
    /// fee (an approval, a failed call) is listed even without asset legs.
    pub async fn list_activity(
        &self,
        scope: &Scope,
        filter: &ActivityFilter,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<ActivityPage> {
        let accounts = self.resolve_scope(scope).await?;
        let limit = limit.clamp(1, MAX_PAGE);
        if accounts.is_empty() {
            return Ok(ActivityPage {
                rows: Vec::new(),
                next_cursor: None,
            });
        }
        let parsed = cursor.map(parse_cursor).transpose()?;

        // Keyset pagination over (occurred_at DESC, transaction id DESC, account DESC).
        let mut query = QueryBuilder::<Sqlite>::new(
            "SELECT t.id AS tx_id, t.network_id, t.occurred_at, t.status, x.account_id,
                    x.operation, x.decoding
             FROM account_transactions x
             JOIN chain_transactions t ON t.id = x.transaction_id
             WHERE x.account_id IN (",
        );
        let mut ids = query.separated(", ");
        for id in &accounts {
            ids.push_bind(id);
        }
        ids.push_unseparated(")");
        if let Some(account) = &filter.account_id {
            query.push(" AND x.account_id = ").push_bind(account);
        }
        if let Some(network) = filter.network {
            query
                .push(" AND t.network_id = ")
                .push_bind(network.as_str());
        }
        if let Some(operation) = &filter.operation {
            query.push(" AND x.operation = ").push_bind(operation);
        }
        if let Some(status) = &filter.status {
            query.push(" AND t.status = ").push_bind(status);
        }
        if let Some(start) = filter.start {
            query.push(" AND t.occurred_at >= ").push_bind(start);
        }
        if let Some(end) = filter.end {
            query.push(" AND t.occurred_at <= ").push_bind(end);
        }
        if filter.start.zip(filter.end).is_some_and(|(a, b)| a > b) {
            return Err(StoreError::Invalid(
                "activity date range is reversed".into(),
            ));
        }
        if let Some(asset) = &filter.asset_id {
            query
                .push(
                    " AND (EXISTS (SELECT 1 FROM activity_legs l WHERE l.transaction_id = x.transaction_id
                       AND l.account_id = x.account_id AND l.asset_id = ",
                )
                .push_bind(asset)
                .push(
                    ") OR EXISTS (SELECT 1 FROM transaction_fees f WHERE f.transaction_id = x.transaction_id
                       AND f.payer_account_id = x.account_id AND f.asset_id = ",
                )
                .push_bind(asset)
                .push("))");
        }
        if filter.unresolved_only {
            query.push(
                " AND (x.decoding != 'interpreted' OR EXISTS (SELECT 1 FROM leg_accounting la
                   WHERE la.transaction_id = x.transaction_id AND la.account_id = x.account_id
                   AND la.review IS NOT NULL))",
            );
        }
        if let Some((t, tx, acct)) = &parsed {
            query
                .push(" AND (t.occurred_at, t.id, x.account_id) < (")
                .push_bind(*t)
                .push(", ")
                .push_bind(tx)
                .push(", ")
                .push_bind(acct)
                .push(")");
        }
        query
            .push(" ORDER BY t.occurred_at DESC, t.id DESC, x.account_id DESC LIMIT ")
            .push_bind(i64::from(limit) + 1);
        let rows = query.build().fetch_all(&self.pool).await?;

        let has_more = rows.len() > limit as usize;
        let mut out = Vec::with_capacity(rows.len().min(limit as usize));
        for row in rows.iter().take(limit as usize) {
            let network: String = row.get("network_id");
            let mut entry = ActivityRow {
                transaction_id: row.get("tx_id"),
                account_id: row.get("account_id"),
                network: NetworkId::parse(&network)?,
                occurred_at: row.get("occurred_at"),
                operation: row.get("operation"),
                status: row.get("status"),
                legs: Vec::new(),
                fee_quantity: None,
                fee_symbol: None,
                fee_value_usd: None,
                unresolved: row.get::<String, _>("decoding") != "interpreted",
            };
            let legs = sqlx::query(
                "SELECT l.id, l.asset_id, l.signed_raw_quantity, l.direction, l.unresolved, a.symbol, a.decimals,
                        la.treatment, la.value_usd, la.value_estimated, la.review
                 FROM activity_legs l JOIN assets a ON a.id = l.asset_id
                 LEFT JOIN leg_accounting la ON la.leg_id = l.id
                 WHERE l.transaction_id = ? AND l.account_id = ? ORDER BY l.id",
            )
            .bind(&entry.transaction_id)
            .bind(&entry.account_id)
            .fetch_all(&self.pool)
            .await?;
            for leg in &legs {
                let decimals = u32::try_from(leg.get::<i64, _>("decimals"))
                    .map_err(|_| StoreError::Corrupt("asset decimals".into()))?;
                let raw = parse_raw_amount(&leg.get::<String, _>("signed_raw_quantity"))?;
                let treatment: Option<String> = leg.get("treatment");
                let review: Option<String> = leg.get("review");
                // Before the first replay the provider's own hint applies;
                // afterwards the accounting interpretation decides.
                entry.unresolved |= match &treatment {
                    Some(_) => review.is_some(),
                    None => leg.get::<i64, _>("unresolved") != 0,
                };
                entry.legs.push(ActivityLeg {
                    leg_id: leg.get("id"),
                    asset_id: leg.get("asset_id"),
                    symbol: leg.get("symbol"),
                    signed_quantity: to_canonical(&raw_to_quantity(&raw, decimals)),
                    direction: leg.get("direction"),
                    treatment,
                    value_usd: leg.get("value_usd"),
                    value_estimated: leg.get::<Option<i64>, _>("value_estimated").unwrap_or(0) != 0,
                    review,
                });
            }
            let fee = sqlx::query(
                "SELECT f.raw_quantity, a.symbol, a.decimals, la.value_usd FROM transaction_fees f
                 JOIN assets a ON a.id = f.asset_id
                 LEFT JOIN leg_accounting la ON la.leg_id = f.id
                 WHERE f.transaction_id = ? AND f.payer_account_id = ?",
            )
            .bind(&entry.transaction_id)
            .bind(&entry.account_id)
            .fetch_optional(&self.pool)
            .await?;
            if let Some(fee) = fee {
                let decimals = u32::try_from(fee.get::<i64, _>("decimals"))
                    .map_err(|_| StoreError::Corrupt("asset decimals".into()))?;
                let raw = parse_raw_amount(&fee.get::<String, _>("raw_quantity"))?;
                entry.fee_quantity = Some(to_canonical(&raw_to_quantity(&raw, decimals)));
                entry.fee_symbol = fee.get("symbol");
                entry.fee_value_usd = fee.get("value_usd");
            }
            out.push(entry);
        }
        let next_cursor = has_more.then(|| {
            let last = out.last().expect("non-empty when has_more");
            format!(
                "{}|{}|{}",
                last.occurred_at, last.transaction_id, last.account_id
            )
        });
        Ok(ActivityPage {
            rows: out,
            next_cursor,
        })
    }
}
