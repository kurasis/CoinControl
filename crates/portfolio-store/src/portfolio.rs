//! Holdings, portfolio summary, asset detail and value charts
//! (SPECIFICATION.md §5.3–§5.4, §9; ACCOUNTING.md §4, §8).

use std::collections::{BTreeMap, BTreeSet};

use bigdecimal::Zero;
use portfolio_core::accounting::{
    BasisKind, ChainOrder, ExternalFlow, Lot, PositionSummary, UnavailableReason, modified_dietz,
    summarize_position, total_accounted_pnl,
};
use portfolio_core::decimal::{
    Dec, div, parse_dec, parse_raw_amount, raw_to_quantity, to_canonical,
};
use portfolio_core::network::NetworkId;
use sqlx::Row;

use crate::accounting::{DAY, HOUR, PriceBook, parse_basis_kind};
use crate::{
    AccountingSummary, AssetAccountRow, AssetChart, AssetDetail, BalanceStatus, BasisCoverage,
    ChartPoint, ChartRange, ChartSeries, HoldingRow, LotRow, PartialUsd, PeriodPerformanceDto,
    PortfolioSummary, PricePoint, Result, Scope, Store, StoreError,
};

/// `(account_id, asset_id)`.
type AccountAsset = (String, String);

/// Upper bound on displayed chart points (SPECIFICATION.md §9).
const MAX_CHART_POINTS: i64 = 1_000;

pub(crate) struct AssetMeta {
    pub network: NetworkId,
    pub kind: String,
    pub contract: String,
    pub symbol: Option<String>,
    pub name: Option<String>,
    pub decimals: u32,
    pub verification: String,
}

struct LatestPrice {
    price: Dec,
    observed_at: i64,
    change_24h: Option<String>,
}

/// Latest balance observation of one account for one asset.
struct Observation {
    account: String,
    quantity: Dec,
    status: BalanceStatus,
    observed_at: i64,
}

/// A held asset in scope with the lots that explain it.
struct Position {
    row: HoldingRow,
    price: Option<Dec>,
    summary: PositionSummary,
}

fn status_rank(s: BalanceStatus) -> u8 {
    match s {
        BalanceStatus::Fresh => 0,
        BalanceStatus::Stale => 1,
        BalanceStatus::Missing => 2,
        BalanceStatus::Conflicted => 3,
    }
}

fn parse_status(s: &str) -> BalanceStatus {
    match s {
        "fresh" => BalanceStatus::Fresh,
        "stale" => BalanceStatus::Stale,
        "missing" => BalanceStatus::Missing,
        _ => BalanceStatus::Conflicted,
    }
}

fn canon(d: &Dec) -> String {
    to_canonical(d)
}

/// Percentages are display values: four decimals are plenty.
fn pct(d: &Dec) -> String {
    to_canonical(&d.with_scale_round(4, bigdecimal::RoundingMode::HalfEven))
}

fn percent(numerator: &Dec, denominator: &Dec) -> Option<Dec> {
    div(&(numerator * Dec::from(100)), denominator)
}

/// Lots that explain an observed quantity. When the replayed inventory and
/// the observed balance disagree, the unexplained part has unknown basis; when
/// the ledger holds more than observed, which lots left is unknown, so the
/// whole position is treated as unknown basis. Never fabricates a basis.
fn explaining_lots(account: &str, asset: &str, held: &Dec, lots: &[Lot]) -> Vec<Lot> {
    if held.is_zero() {
        return Vec::new();
    }
    let unknown = |quantity: Dec| Lot {
        id: 0,
        account: account.to_owned(),
        asset: asset.to_owned(),
        quantity,
        basis_usd: None,
        basis_kind: BasisKind::Unknown,
        acquired: ChainOrder { time: 0, seq: 0 },
        arrived: ChainOrder { time: 0, seq: 0 },
        source_event: String::new(),
        parent: None,
    };
    let in_lots: Dec = lots.iter().map(|l| l.quantity.clone()).sum();
    if in_lots == *held {
        lots.to_vec()
    } else if in_lots < *held {
        let mut out = lots.to_vec();
        out.push(unknown(held - &in_lots));
        out
    } else {
        vec![unknown(held.clone())]
    }
}

fn coverage_of(summary: &PositionSummary) -> BasisCoverage {
    let known = &summary.known_subset.quantity;
    if summary.quantity.is_zero() || *known == summary.quantity {
        if summary.has_estimated_basis {
            BasisCoverage::Estimated
        } else {
            BasisCoverage::Known
        }
    } else if known.is_zero() {
        BasisCoverage::Unknown
    } else {
        BasisCoverage::Partial
    }
}

fn partial(sum: &portfolio_core::accounting::PartialSum) -> PartialUsd {
    PartialUsd {
        known_usd: canon(&sum.known),
        complete: sum.complete,
    }
}

impl Store {
    pub(crate) async fn asset_meta(&self) -> Result<BTreeMap<String, AssetMeta>> {
        let rows = sqlx::query(
            "SELECT id, network_id, asset_kind, canonical_identifier, symbol, name, decimals, verification FROM assets",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|r| {
                let network: String = r.get("network_id");
                Ok((
                    r.get("id"),
                    AssetMeta {
                        network: NetworkId::parse(&network)?,
                        kind: r.get("asset_kind"),
                        contract: r.get("canonical_identifier"),
                        symbol: r.get("symbol"),
                        name: r.get("name"),
                        decimals: u32::try_from(r.get::<i64, _>("decimals"))
                            .map_err(|_| StoreError::Corrupt("asset decimals".into()))?,
                        verification: r.get("verification"),
                    },
                ))
            })
            .collect()
    }

    async fn latest_prices(&self) -> Result<BTreeMap<String, LatestPrice>> {
        let rows = sqlx::query(
            "SELECT p.asset_id, p.price_usd, p.observed_at, p.change_24h_percent FROM prices p
             WHERE p.quality != 'low_confidence' AND p.id = (
                 SELECT p2.id FROM prices p2 WHERE p2.asset_id = p.asset_id AND p2.quality != 'low_confidence'
                 ORDER BY p2.observed_at DESC, p2.id DESC LIMIT 1)",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|r| {
                Ok((
                    r.get("asset_id"),
                    LatestPrice {
                        price: parse_dec(&r.get::<String, _>("price_usd"))?,
                        observed_at: r.get("observed_at"),
                        change_24h: r.get("change_24h_percent"),
                    },
                ))
            })
            .collect()
    }

    /// Latest observation per (account, asset) within `accounts`, by asset.
    async fn observations(
        &self,
        accounts: &BTreeSet<String>,
        assets: &BTreeMap<String, AssetMeta>,
    ) -> Result<BTreeMap<String, Vec<Observation>>> {
        let rows = sqlx::query(
            "SELECT b.account_id, b.asset_id, b.raw_quantity, b.observed_at, b.status
             FROM balance_observations b
             WHERE b.id = (SELECT b2.id FROM balance_observations b2
                           WHERE b2.account_id = b.account_id AND b2.asset_id = b.asset_id
                           ORDER BY b2.observed_at DESC, b2.id DESC LIMIT 1)
             ORDER BY b.asset_id, b.account_id",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut out: BTreeMap<String, Vec<Observation>> = BTreeMap::new();
        for row in &rows {
            let account: String = row.get("account_id");
            if !accounts.contains(&account) {
                continue;
            }
            let asset_id: String = row.get("asset_id");
            let meta = assets
                .get(&asset_id)
                .ok_or_else(|| StoreError::Corrupt(format!("unknown asset {asset_id}")))?;
            let raw = parse_raw_amount(&row.get::<String, _>("raw_quantity"))?;
            out.entry(asset_id).or_default().push(Observation {
                account,
                quantity: raw_to_quantity(&raw, meta.decimals),
                status: parse_status(&row.get::<String, _>("status")),
                observed_at: row.get("observed_at"),
            });
        }
        Ok(out)
    }

    /// Current holdings in scope, aggregated per chain-specific asset.
    pub async fn list_holdings(&self, scope: &Scope) -> Result<Vec<HoldingRow>> {
        let accounts = self.resolve_scope(scope).await?;
        let (positions, _) = self.positions(&accounts).await?;
        Ok(positions.into_iter().map(|p| p.row).collect())
    }

    async fn positions(&self, accounts: &BTreeSet<String>) -> Result<(Vec<Position>, u32)> {
        let assets = self.asset_meta().await?;
        let prices = self.latest_prices().await?;
        let observations = self.observations(accounts, &assets).await?;
        let lots = self.scope_lots(accounts).await?;

        let mut excluded_spam = 0u32;
        let mut positions = Vec::new();
        for (asset_id, obs) in observations {
            let quantity: Dec = obs.iter().map(|o| o.quantity.clone()).sum();
            if quantity.is_zero() {
                continue;
            }
            let meta = &assets[&asset_id];
            if meta.verification == "spam" {
                excluded_spam += 1;
                continue;
            }
            let mut status = BalanceStatus::Fresh;
            let mut observed_at = i64::MAX;
            let mut explaining = Vec::new();
            for o in &obs {
                if status_rank(o.status) > status_rank(status) {
                    status = o.status;
                }
                observed_at = observed_at.min(o.observed_at);
                let account_lots = lots
                    .get(&(o.account.clone(), asset_id.clone()))
                    .map(Vec::as_slice)
                    .unwrap_or(&[]);
                explaining.extend(explaining_lots(
                    &o.account,
                    &asset_id,
                    &o.quantity,
                    account_lots,
                ));
            }
            let price = prices.get(&asset_id);
            let summary = summarize_position(&explaining, price.map(|p| &p.price));
            let value = price.map(|p| &quantity * &p.price);
            positions.push(Position {
                price: price.map(|p| p.price.clone()),
                row: HoldingRow {
                    asset_id: asset_id.clone(),
                    network: meta.network,
                    symbol: meta.symbol.clone(),
                    name: meta.name.clone(),
                    verification: meta.verification.clone(),
                    quantity: canon(&quantity),
                    price_usd: price.map(|p| canon(&p.price)),
                    value_usd: value.as_ref().map(canon),
                    change_24h_percent: price.and_then(|p| p.change_24h.clone()),
                    price_observed_at: price.map(|p| p.observed_at),
                    allocation_percent: None,
                    balance_status: status,
                    observed_at,
                    basis_usd: summary.basis_usd.as_ref().map(canon),
                    unrealized_pnl_usd: summary.unrealized_usd.as_ref().map(canon),
                    unrealized_return_percent: summary.unrealized_percent.as_ref().map(pct),
                    basis_coverage: coverage_of(&summary),
                },
                summary,
            });
        }

        let total: Dec = positions
            .iter()
            .filter_map(|p| p.summary.value_usd.clone())
            .sum();
        for p in &mut positions {
            p.row.allocation_percent = p
                .summary
                .value_usd
                .as_ref()
                .and_then(|v| percent(v, &total))
                .map(|x| canon(&x));
        }
        // Value descending; unpriced rows after priced ones; deterministic tie-break.
        positions.sort_by(|a, b| match (&a.summary.value_usd, &b.summary.value_usd) {
            (Some(x), Some(y)) => y.cmp(x).then_with(|| a.row.asset_id.cmp(&b.row.asset_id)),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.row.asset_id.cmp(&b.row.asset_id),
        });
        Ok((positions, excluded_spam))
    }

    pub async fn portfolio_summary(&self, scope: &Scope) -> Result<PortfolioSummary> {
        let accounts = self.resolve_scope(scope).await?;
        let (positions, excluded_spam) = self.positions(&accounts).await?;
        let valued: Vec<Dec> = positions
            .iter()
            .filter_map(|p| p.summary.value_usd.clone())
            .collect();
        let total = (!valued.is_empty()).then(|| valued.iter().cloned().sum::<Dec>());
        let last_sync: Option<i64> = sqlx::query_scalar(
            "SELECT MAX(observed_at) FROM balance_observations WHERE status = 'fresh'",
        )
        .fetch_one(&self.pool)
        .await?;
        let accounting = self.accounting_summary(&accounts, &positions).await?;
        Ok(PortfolioSummary {
            total_value_usd: total.as_ref().map(canon),
            holding_count: u32::try_from(positions.len()).unwrap_or(u32::MAX),
            unpriced_count: u32::try_from(positions.iter().filter(|p| p.price.is_none()).count())
                .unwrap_or(u32::MAX),
            stale_count: u32::try_from(
                positions
                    .iter()
                    .filter(|p| p.row.balance_status != BalanceStatus::Fresh)
                    .count(),
            )
            .unwrap_or(u32::MAX),
            excluded_spam_count: excluded_spam,
            unrealized_pnl_usd: accounting.unrealized_pnl_usd.clone(),
            unrealized_return_percent: accounting.unrealized_return_percent.clone(),
            unrealized_reason: accounting.unrealized_reason,
            last_successful_sync_at: last_sync,
            account_count: u32::try_from(accounts.len()).unwrap_or(u32::MAX),
            accounting,
        })
    }

    async fn accounting_summary(
        &self,
        accounts: &BTreeSet<String>,
        positions: &[Position],
    ) -> Result<AccountingSummary> {
        let mut value = Dec::zero();
        let mut basis = Dec::zero();
        let mut known_value = Dec::zero();
        let mut known_basis = Dec::zero();
        let mut all_priced = true;
        let mut all_known = true;
        let mut estimated = false;
        for p in positions {
            let s = &p.summary;
            estimated |= s.has_estimated_basis;
            match &s.value_usd {
                Some(v) => value += v,
                None => all_priced = false,
            }
            match &s.basis_usd {
                Some(b) => basis += b,
                None => all_known = false,
            }
            // The known subset is valued at its own quantity only, and only when priced.
            if let Some(v) = &s.known_subset.value_usd {
                known_value += v;
                known_basis += &s.known_subset.basis_usd;
            }
        }
        let unrealized =
            (all_priced && all_known && !positions.is_empty()).then(|| &value - &basis);
        let reason = if positions.is_empty() {
            None
        } else if !all_priced {
            Some(UnavailableReason::MissingPrice)
        } else if !all_known {
            Some(UnavailableReason::MissingBasis)
        } else if basis.is_zero() {
            Some(UnavailableReason::ZeroBasis)
        } else {
            None
        };
        let unrealized_percent = unrealized.as_ref().and_then(|u| percent(u, &basis));
        let known_pnl = &known_value - &known_basis;
        let totals = self.scope_totals(accounts, None).await?;
        let (review, recon) = self.review_counts(accounts).await?;
        Ok(AccountingSummary {
            total_accounted_pnl_usd: total_accounted_pnl(unrealized.as_ref(), &totals)
                .as_ref()
                .map(canon),
            unrealized_pnl_usd: unrealized.as_ref().map(canon),
            unrealized_return_percent: unrealized_percent.as_ref().map(pct),
            unrealized_reason: reason,
            remaining_basis_usd: all_known.then(|| canon(&basis)),
            known_subset_return_percent: percent(&known_pnl, &known_basis).map(|x| pct(&x)),
            known_subset_value_usd: canon(&known_value),
            known_subset_basis_usd: canon(&known_basis),
            known_subset_pnl_usd: canon(&known_pnl),
            basis_coverage_percent: percent(&known_value, &value).map(|x| pct(&x)),
            has_estimated_basis: estimated,
            realized: partial(&totals.realized_usd),
            income: partial(&totals.income_usd),
            expenses: partial(&totals.expense_usd),
            fee_charges: totals.fee_charges,
            review_count: review,
            reconciliation_count: recon,
        })
    }

    async fn review_counts(&self, accounts: &BTreeSet<String>) -> Result<(u32, u32)> {
        let review: Vec<String> =
            sqlx::query_scalar("SELECT account_id FROM leg_accounting WHERE review IS NOT NULL")
                .fetch_all(&self.pool)
                .await?;
        let recon: Vec<Option<String>> =
            sqlx::query_scalar("SELECT account_id FROM reconciliation_items")
                .fetch_all(&self.pool)
                .await?;
        let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        Ok((
            count(review.iter().filter(|a| accounts.contains(*a)).count()),
            count(
                recon
                    .iter()
                    .filter(|a| a.as_ref().is_none_or(|a| accounts.contains(a)))
                    .count(),
            ),
        ))
    }

    /// Asset detail for a scope: identity, price, position, basis and P&L,
    /// per-account split and remaining lots.
    pub async fn asset_detail(&self, scope: &Scope, asset_id: &str) -> Result<AssetDetail> {
        let accounts = self.resolve_scope(scope).await?;
        let assets = self.asset_meta().await?;
        let meta = assets.get(asset_id).ok_or(StoreError::NotFound("asset"))?;
        let price = self.latest_prices().await?.remove(asset_id);
        let observations = self
            .observations(&accounts, &assets)
            .await?
            .remove(asset_id)
            .unwrap_or_default();
        let lots = self.scope_lots(&accounts).await?;
        let wallets: BTreeMap<String, String> = self
            .list_accounts(None)
            .await?
            .into_iter()
            .map(|a| (a.id, a.wallet_id))
            .collect();

        let mut all_lots = Vec::new();
        let mut rows = Vec::new();
        for o in &observations {
            if o.quantity.is_zero() {
                continue;
            }
            let account_lots = lots
                .get(&(o.account.clone(), asset_id.to_owned()))
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let explaining = explaining_lots(&o.account, asset_id, &o.quantity, account_lots);
            let s = summarize_position(&explaining, price.as_ref().map(|p| &p.price));
            rows.push(AssetAccountRow {
                account_id: o.account.clone(),
                wallet_id: wallets.get(&o.account).cloned().unwrap_or_default(),
                quantity: canon(&o.quantity),
                value_usd: s.value_usd.as_ref().map(canon),
                basis_usd: s.basis_usd.as_ref().map(canon),
                unrealized_pnl_usd: s.unrealized_usd.as_ref().map(canon),
                basis_coverage: coverage_of(&s),
                balance_status: o.status,
            });
            all_lots.extend(explaining);
        }
        let s = summarize_position(&all_lots, price.as_ref().map(|p| &p.price));
        let totals = self.scope_totals(&accounts, Some(asset_id)).await?;

        let lot_rows = sqlx::query(
            "SELECT id, account_id, quantity, remaining_quantity, basis_usd, remaining_basis_usd,
                    basis_kind, acquired_at, arrived_at, parent_lot_id
             FROM lots WHERE asset_id = ? AND remaining_quantity != '0'
             ORDER BY acquired_at, CAST(substr(id, 2) AS INTEGER)",
        )
        .bind(asset_id)
        .fetch_all(&self.pool)
        .await?;
        let lots_out = lot_rows
            .iter()
            .filter(|r| accounts.contains(&r.get::<String, _>("account_id")))
            .map(|r| LotRow {
                id: r.get("id"),
                account_id: r.get("account_id"),
                quantity: r.get("quantity"),
                remaining_quantity: r.get("remaining_quantity"),
                basis_usd: r.get("basis_usd"),
                remaining_basis_usd: r.get("remaining_basis_usd"),
                basis_kind: parse_basis_kind(&r.get::<String, _>("basis_kind")),
                acquired_at: r.get("acquired_at"),
                arrived_at: r.get("arrived_at"),
                parent_lot_id: r.get("parent_lot_id"),
            })
            .collect();
        let contract = (meta.kind == "token").then(|| meta.contract.clone());
        Ok(AssetDetail {
            asset_id: asset_id.to_owned(),
            network: meta.network,
            explorer_url: contract
                .as_deref()
                .and_then(|c| meta.network.explorer_address_url(c)),
            contract,
            symbol: meta.symbol.clone(),
            name: meta.name.clone(),
            decimals: meta.decimals,
            verification: meta.verification.clone(),
            price_usd: price.as_ref().map(|p| canon(&p.price)),
            price_observed_at: price.as_ref().map(|p| p.observed_at),
            change_24h_percent: price.as_ref().and_then(|p| p.change_24h.clone()),
            quantity: canon(&s.quantity),
            value_usd: s.value_usd.as_ref().map(canon),
            remaining_basis_usd: s.basis_usd.as_ref().map(canon),
            unrealized_pnl_usd: s.unrealized_usd.as_ref().map(canon),
            unrealized_return_percent: s.unrealized_percent.as_ref().map(pct),
            unrealized_reason: if s.quantity.is_zero() { None } else { s.reason },
            known_subset_pnl_usd: s
                .known_subset
                .unrealized_usd
                .as_ref()
                .map_or_else(|| "0".to_owned(), canon),
            basis_coverage_quantity_percent: s.basis_coverage_quantity_percent.as_ref().map(pct),
            has_estimated_basis: s.has_estimated_basis,
            realized: partial(&totals.realized_usd),
            income: partial(&totals.income_usd),
            expenses: partial(&totals.expense_usd),
            accounts: rows,
            lots: lots_out,
        })
    }

    /// Historical holdings value of the scope: historical quantity times
    /// historical price, with the period performance of the interval.
    pub async fn get_chart(&self, scope: &Scope, range: ChartRange) -> Result<ChartSeries> {
        let accounts = self.resolve_scope(scope).await?;
        let mut series = self.holdings_series(&accounts, None, range).await?;
        series.performance = self.period_performance(&accounts, &series).await?;
        Ok(series)
    }

    /// Price and holdings-value series of one asset within the scope.
    pub async fn asset_chart(
        &self,
        scope: &Scope,
        asset_id: &str,
        range: ChartRange,
    ) -> Result<AssetChart> {
        let accounts = self.resolve_scope(scope).await?;
        let holdings = self
            .holdings_series(&accounts, Some(asset_id), range)
            .await?;
        let now = self.now();
        let (interval, start) = grid_params(range, now, None);
        let book = self
            .price_book(&BTreeSet::from([asset_id.to_owned()]), start - DAY)
            .await?;
        // The market series is independent of when this portfolio held the asset.
        let earliest_price: Option<i64> = sqlx::query_scalar(
            "SELECT MIN(observed_at) FROM prices WHERE asset_id = ? AND quality != 'low_confidence'",
        )
        .bind(asset_id)
        .fetch_one(&self.pool)
        .await?;
        let (interval, start) = if range == ChartRange::All {
            grid_params(range, now, earliest_price)
        } else {
            (interval, start)
        };
        let price = grid(start, interval, now)
            .into_iter()
            .map(|t| {
                let quote = book.at(asset_id, t);
                PricePoint {
                    t,
                    estimated: quote.as_ref().is_some_and(|(_, e)| *e),
                    price_usd: quote.map(|(p, _)| canon(&p)),
                }
            })
            .collect();
        Ok(AssetChart { price, holdings })
    }

    async fn price_book(&self, assets: &BTreeSet<String>, since: i64) -> Result<PriceBook> {
        let mut book = PriceBook::default();
        let rows = sqlx::query(
            "SELECT asset_id, price_usd, observed_at, granularity FROM prices
             WHERE observed_at >= ? AND quality != 'low_confidence' ORDER BY observed_at, id",
        )
        .bind(since)
        .fetch_all(&self.pool)
        .await?;
        for row in &rows {
            let asset: String = row.get("asset_id");
            if assets.contains(&asset) {
                book.push(
                    &asset,
                    row.get("observed_at"),
                    row.get("price_usd"),
                    &row.get::<String, _>("granularity"),
                );
            }
        }
        book.finish();
        Ok(book)
    }

    /// Quantity history per (account, asset): stored snapshots where they exist
    /// (demo), otherwise a reconstruction from supported activity anchored at
    /// the latest balance observation, otherwise the observations themselves.
    /// `None` quantities mark points where the reconstruction is inconsistent.
    async fn quantity_history(
        &self,
        accounts: &BTreeSet<String>,
        asset_filter: Option<&str>,
        assets: &BTreeMap<String, AssetMeta>,
    ) -> Result<BTreeMap<(String, String), Vec<(i64, Option<Dec>)>>> {
        let keep = |account: &str, asset: &str| {
            accounts.contains(account)
                && asset_filter.is_none_or(|a| a == asset)
                && assets.get(asset).is_some_and(|m| m.verification != "spam")
        };
        let qty = |asset: &str, raw: &str| -> Result<Dec> {
            let decimals = assets.get(asset).map_or(0, |m| m.decimals);
            Ok(raw_to_quantity(&parse_raw_amount(raw)?, decimals))
        };

        let mut snapshots: BTreeMap<AccountAsset, Vec<(i64, Option<Dec>)>> = BTreeMap::new();
        for row in sqlx::query(
            "SELECT account_id, asset_id, at, raw_quantity FROM portfolio_snapshots ORDER BY at",
        )
        .fetch_all(&self.pool)
        .await?
        {
            let (account, asset): (String, String) = (row.get("account_id"), row.get("asset_id"));
            if keep(&account, &asset) {
                let q = qty(&asset, &row.get::<String, _>("raw_quantity"))?;
                snapshots
                    .entry((account, asset))
                    .or_default()
                    .push((row.get("at"), Some(q)));
            }
        }

        let mut observations: BTreeMap<(String, String), Vec<(i64, Dec)>> = BTreeMap::new();
        let mut account_last_obs: BTreeMap<String, i64> = BTreeMap::new();
        for row in sqlx::query(
            "SELECT account_id, asset_id, observed_at, raw_quantity FROM balance_observations
             WHERE status IN ('fresh', 'stale') ORDER BY observed_at, id",
        )
        .fetch_all(&self.pool)
        .await?
        {
            let (account, asset): (String, String) = (row.get("account_id"), row.get("asset_id"));
            if !accounts.contains(&account) {
                continue;
            }
            let at: i64 = row.get("observed_at");
            let e = account_last_obs.entry(account.clone()).or_insert(at);
            *e = (*e).max(at);
            if keep(&account, &asset) {
                let q = qty(&asset, &row.get::<String, _>("raw_quantity"))?;
                observations
                    .entry((account, asset))
                    .or_default()
                    .push((at, q));
            }
        }

        // Signed movements: legs of settled transactions and actually paid fees.
        let mut deltas: BTreeMap<(String, String), Vec<(i64, Dec)>> = BTreeMap::new();
        for row in sqlx::query(
            "SELECT l.account_id, l.asset_id, l.signed_raw_quantity AS raw, t.occurred_at
             FROM activity_legs l JOIN chain_transactions t ON t.id = l.transaction_id
             WHERE t.status IN ('confirmed', 'final')
             UNION ALL
             SELECT f.payer_account_id, f.asset_id, '-' || f.raw_quantity, t.occurred_at
             FROM transaction_fees f JOIN chain_transactions t ON t.id = f.transaction_id
             WHERE t.status IN ('confirmed', 'final', 'failed') AND f.payer_account_id IS NOT NULL
               AND f.attribution != 'sponsored'",
        )
        .fetch_all(&self.pool)
        .await?
        {
            let account: Option<String> = row.get(0);
            let Some(account) = account else { continue };
            let asset: String = row.get(1);
            if keep(&account, &asset) {
                let q = qty(&asset, &row.get::<String, _>(2))?;
                deltas
                    .entry((account, asset))
                    .or_default()
                    .push((row.get(3), q));
            }
        }

        let mut out = BTreeMap::new();
        let keys: BTreeSet<(String, String)> = snapshots
            .keys()
            .chain(observations.keys())
            .chain(deltas.keys())
            .cloned()
            .collect();
        for key in keys {
            if let Some(mut snaps) = snapshots.remove(&key) {
                if let Some(obs) = observations.get(&key) {
                    snaps.extend(obs.iter().map(|(t, q)| (*t, Some(q.clone()))));
                }
                snaps.sort_by_key(|(t, _)| *t);
                out.insert(key, snaps);
                continue;
            }
            let obs = observations.get(&key);
            let Some(mut moves) = deltas.remove(&key) else {
                if let Some(obs) = obs {
                    out.insert(
                        key,
                        obs.iter().map(|(t, q)| (*t, Some(q.clone()))).collect(),
                    );
                }
                continue;
            };
            // Anchor: the latest observation, or zero at the account's latest
            // complete balance scan when the asset is no longer reported.
            let anchor = match obs.and_then(|o| o.last()) {
                Some((t, q)) => Some((*t, q.clone())),
                None => account_last_obs.get(&key.0).map(|t| (*t, Dec::zero())),
            };
            let Some((anchor_t, anchor_q)) = anchor else {
                continue;
            };
            moves.sort_by_key(|(t, _)| *t);
            moves.retain(|(t, _)| *t <= anchor_t);
            let mut points: Vec<(i64, Option<Dec>)> = Vec::with_capacity(moves.len() + 1);
            let mut q = anchor_q.clone();
            let mut after = Vec::with_capacity(moves.len());
            for (t, delta) in moves.iter().rev() {
                after.push((*t, q.clone()));
                q -= delta;
            }
            after.reverse();
            for (t, q) in after {
                points.push((t, (q >= Dec::zero()).then_some(q)));
            }
            points.push((anchor_t, Some(anchor_q)));
            // Later observations (if any arrived after the anchor) continue the series.
            out.insert(key, points);
        }
        Ok(out)
    }

    async fn holdings_series(
        &self,
        accounts: &BTreeSet<String>,
        asset_filter: Option<&str>,
        range: ChartRange,
    ) -> Result<ChartSeries> {
        let assets = self.asset_meta().await?;
        let now = self.now();
        let quantities = self
            .quantity_history(accounts, asset_filter, &assets)
            .await?;
        let earliest = quantities
            .values()
            .filter_map(|s| s.first().map(|p| p.0))
            .min();
        let (interval, start) = grid_params(range, now, earliest);
        let held: BTreeSet<String> = quantities.keys().map(|(_, a)| a.clone()).collect();
        let book = self.price_book(&held, start - DAY).await?;

        let points = grid(start, interval, now)
            .into_iter()
            .map(|t| {
                let mut total = Dec::zero();
                let mut any_quantity = false;
                let mut partial = false;
                let mut estimated = false;
                let mut priced_any = false;
                for ((_, asset), series) in &quantities {
                    let idx = series.partition_point(|(at, _)| *at <= t);
                    if idx == 0 {
                        continue; // no data yet: do not extrapolate backwards
                    }
                    any_quantity = true;
                    let Some(qty) = &series[idx - 1].1 else {
                        partial = true;
                        continue;
                    };
                    if qty.is_zero() {
                        continue;
                    }
                    match book.at(asset, t) {
                        Some((price, est)) => {
                            total += qty * price;
                            priced_any = true;
                            estimated |= est;
                        }
                        None => partial = true,
                    }
                }
                ChartPoint {
                    t,
                    // A point where nothing held could be priced is a gap, not $0.
                    value_usd: (any_quantity && (priced_any || !partial)).then(|| canon(&total)),
                    estimated,
                    partial,
                }
            })
            .collect();

        Ok(ChartSeries {
            range,
            interval_seconds: interval,
            points,
            history_available_since: earliest,
            performance: None,
        })
    }

    /// Modified Dietz between the first and last valued points of a series
    /// (ACCOUNTING.md §8). Flows crossing the scope boundary come from the
    /// accounting replay; own transfers inside the scope cancel.
    async fn period_performance(
        &self,
        accounts: &BTreeSet<String>,
        series: &ChartSeries,
    ) -> Result<Option<PeriodPerformanceDto>> {
        let valued: Vec<&ChartPoint> = series
            .points
            .iter()
            .filter(|p| p.value_usd.is_some())
            .collect();
        let (Some(first), Some(last)) = (valued.first(), valued.last()) else {
            return Ok(None);
        };
        let v0 = parse_dec(first.value_usd.as_deref().unwrap_or("0"))?;
        let v1 = parse_dec(last.value_usd.as_deref().unwrap_or("0"))?;
        let rows = sqlx::query(
            "SELECT from_account_id, to_account_id, value_usd, classified, at FROM accounting_flows
             WHERE at > ? AND at <= ? ORDER BY at, event_id",
        )
        .bind(first.t)
        .bind(last.t)
        .fetch_all(&self.pool)
        .await?;
        let mut flows = Vec::new();
        let mut complete = true;
        let mut net = Dec::zero();
        for r in rows {
            let from: Option<String> = r.get("from_account_id");
            let to: Option<String> = r.get("to_account_id");
            let from_in = from.as_ref().is_some_and(|a| accounts.contains(a));
            let to_in = to.as_ref().is_some_and(|a| accounts.contains(a));
            if from_in == to_in {
                continue; // inside the scope (cancels) or entirely outside it
            }
            let value: Option<String> = r.get("value_usd");
            complete &= r.get::<i64, _>("classified") != 0;
            match value {
                Some(v) => {
                    let v = parse_dec(&v)?;
                    let signed = if to_in { v } else { -v };
                    net += &signed;
                    flows.push(ExternalFlow {
                        time: r.get("at"),
                        amount_usd: signed,
                    });
                }
                None => complete = false,
            }
        }
        let mut out = PeriodPerformanceDto {
            start: first.t,
            end: last.t,
            beginning_value_usd: canon(&v0),
            ending_value_usd: canon(&v1),
            net_flows_usd: canon(&net),
            flow_count: u32::try_from(flows.len()).unwrap_or(u32::MAX),
            gain_usd: None,
            return_percent: None,
            reason: None,
            estimated: first.estimated || last.estimated,
        };
        if first.partial || last.partial {
            out.reason = Some(UnavailableReason::MissingPrice);
        } else if !complete {
            out.reason = Some(UnavailableReason::IncompleteClassification);
        } else {
            let perf = modified_dietz(first.t, last.t, &v0, &v1, &flows);
            out.gain_usd = Some(canon(&perf.gain_usd));
            out.return_percent = perf.return_percent.as_ref().map(pct);
            out.reason = perf.reason;
        }
        Ok(Some(out))
    }
}

/// Interval and start of the shared UTC grid for a range.
fn grid_params(range: ChartRange, now: i64, earliest: Option<i64>) -> (i64, i64) {
    match range {
        ChartRange::Day => (HOUR, now - DAY),
        ChartRange::Week => (4 * HOUR, now - 7 * DAY),
        ChartRange::Month => (DAY, now - 30 * DAY),
        ChartRange::Quarter => (DAY, now - 90 * DAY),
        ChartRange::Year => (DAY, now - 365 * DAY),
        ChartRange::All => {
            let first = earliest.unwrap_or(now);
            let span = (now - first).max(DAY);
            let mut interval = DAY;
            while span / interval > MAX_CHART_POINTS {
                interval *= 2;
            }
            // One step earlier so the first data point is inside the grid.
            (interval, first - interval)
        }
    }
}

fn grid(start: i64, interval: i64, now: i64) -> Vec<i64> {
    let aligned_start = start - start.rem_euclid(interval) + interval;
    let mut out: Vec<i64> = (0..)
        .map(|i| aligned_start + i * interval)
        .take_while(|t| *t < now)
        .collect();
    out.push(now);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Dec {
        parse_dec(s).unwrap()
    }

    fn lot(q: &str, basis: Option<&str>) -> Lot {
        Lot {
            id: 1,
            account: "a".into(),
            asset: "x".into(),
            quantity: d(q),
            basis_usd: basis.map(d),
            basis_kind: if basis.is_some() {
                BasisKind::Known
            } else {
                BasisKind::Unknown
            },
            acquired: ChainOrder { time: 1, seq: 0 },
            arrived: ChainOrder { time: 1, seq: 0 },
            source_event: "e".into(),
            parent: None,
        }
    }

    #[test]
    fn unexplained_balance_has_unknown_basis_and_excess_lots_are_not_trusted() {
        let lots = vec![lot("1", Some("100"))];
        let more = explaining_lots("a", "x", &d("1.5"), &lots);
        assert_eq!(more.len(), 2);
        assert_eq!(more[1].quantity, d("0.5"));
        assert!(more[1].basis_usd.is_none());
        let less = explaining_lots("a", "x", &d("0.5"), &lots);
        assert_eq!(less.len(), 1);
        assert!(less[0].basis_usd.is_none());
        assert_eq!(explaining_lots("a", "x", &d("1"), &lots), lots);
        assert!(explaining_lots("a", "x", &d("0"), &lots).is_empty());
    }

    #[test]
    fn grid_ends_at_now_and_is_bounded() {
        let now = 10 * DAY + 123;
        let g = grid(now - 3 * DAY, DAY, now);
        assert_eq!(*g.last().unwrap(), now);
        assert!(g.windows(2).all(|w| w[0] < w[1]));
        let (interval, _) = grid_params(ChartRange::All, now, Some(now - 5_000 * DAY));
        assert!(5_000 * DAY / interval <= MAX_CHART_POINTS);
    }
}
