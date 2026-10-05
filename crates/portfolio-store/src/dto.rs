//! Data transfer objects shared with the frontend over IPC.
//!
//! Quantities, prices and USD amounts are exact decimal strings. TypeScript
//! definitions are generated from these types (`npm run gen:bindings`) so the
//! two sides cannot drift unnoticed.

use portfolio_core::accounting::{BasisKind, UnavailableReason};
use portfolio_core::network::NetworkId;
use serde::{Deserialize, Serialize};

#[cfg(feature = "ts")]
use ts_rs::TS;

/// Which local database is open. Demo data never shares a file with real data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum ProfileKind {
    Real,
    Demo,
    Test,
}

impl ProfileKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ProfileKind::Real => "real",
            ProfileKind::Demo => "demo",
            ProfileKind::Test => "test",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct Wallet {
    pub id: String,
    pub label: String,
    pub archived: bool,
    pub created_at: i64,
    pub account_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct Account {
    pub id: String,
    pub wallet_id: String,
    pub network: NetworkId,
    pub canonical_address: String,
    pub display_address: String,
    pub label: Option<String>,
    pub archived: bool,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct Group {
    pub id: String,
    pub label: String,
    pub wallet_ids: Vec<String>,
}

/// The accounts a view covers. Overlapping selections are deduplicated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Scope {
    All,
    Wallet { id: String },
    Group { id: String },
    Accounts { ids: Vec<String> },
}

/// Freshness of the latest balance observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum BalanceStatus {
    Fresh,
    Stale,
    Missing,
    Conflicted,
}

/// One chain-specific asset held within the selected scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct HoldingRow {
    pub asset_id: String,
    pub network: NetworkId,
    pub symbol: Option<String>,
    pub name: Option<String>,
    pub verification: String,
    pub quantity: String,
    /// `None` when no usable quote exists. Never rendered as $0.
    pub price_usd: Option<String>,
    pub value_usd: Option<String>,
    pub change_24h_percent: Option<String>,
    pub price_observed_at: Option<i64>,
    pub allocation_percent: Option<String>,
    pub balance_status: BalanceStatus,
    pub observed_at: i64,
    /// Remaining cost basis when known for the whole position.
    pub basis_usd: Option<String>,
    pub unrealized_pnl_usd: Option<String>,
    pub unrealized_return_percent: Option<String>,
    pub basis_coverage: BasisCoverage,
}

/// How much of a position's acquisition basis is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum BasisCoverage {
    Known,
    /// Known, but at least one lot uses an estimated basis.
    Estimated,
    Partial,
    Unknown,
}

/// A USD sum whose components may be partially unknown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct PartialUsd {
    /// Sum of the components that are known.
    pub known_usd: String,
    /// False when at least one component is missing; the total is then not shown as complete.
    pub complete: bool,
}

/// Cost-basis accounting for a scope (ACCOUNTING.md §4–§6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct AccountingSummary {
    pub unrealized_pnl_usd: Option<String>,
    pub unrealized_return_percent: Option<String>,
    pub unrealized_reason: Option<UnavailableReason>,
    /// Remaining basis when every lot's basis is known.
    pub remaining_basis_usd: Option<String>,
    /// P&L of the lots whose basis is known, valued at their own quantity only.
    pub known_subset_value_usd: String,
    pub known_subset_basis_usd: String,
    pub known_subset_pnl_usd: String,
    pub known_subset_return_percent: Option<String>,
    /// Share of the valued holdings whose basis is known, in percent.
    pub basis_coverage_percent: Option<String>,
    pub has_estimated_basis: bool,
    pub realized: PartialUsd,
    pub income: PartialUsd,
    pub expenses: PartialUsd,
    /// Unrealized + realized + income - expenses, only when every part is complete.
    pub total_accounted_pnl_usd: Option<String>,
    pub fee_charges: u32,
    /// Movements waiting for a user decision (basis, classification, proceeds, price).
    pub review_count: u32,
    /// Inventory gaps, balance mismatches and other reconciliation findings.
    pub reconciliation_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct PortfolioSummary {
    /// Sum of valued holdings; `None` means "Value unavailable", not zero.
    pub total_value_usd: Option<String>,
    pub holding_count: u32,
    pub unpriced_count: u32,
    pub stale_count: u32,
    pub excluded_spam_count: u32,
    pub unrealized_pnl_usd: Option<String>,
    pub unrealized_return_percent: Option<String>,
    pub unrealized_reason: Option<UnavailableReason>,
    pub last_successful_sync_at: Option<i64>,
    pub account_count: u32,
    pub accounting: AccountingSummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum ChartRange {
    #[serde(rename = "24h")]
    Day,
    #[serde(rename = "7d")]
    Week,
    #[serde(rename = "1m")]
    Month,
    #[serde(rename = "3m")]
    Quarter,
    #[serde(rename = "1y")]
    Year,
    All,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct ChartPoint {
    pub t: i64,
    /// `None` marks a gap; the chart must not connect across it.
    pub value_usd: Option<String>,
    /// Daily or otherwise coarse prices were used for this point.
    pub estimated: bool,
    /// Some held assets had no usable price; the value covers a subset.
    pub partial: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct ChartSeries {
    pub range: ChartRange,
    pub interval_seconds: i64,
    pub points: Vec<ChartPoint>,
    /// Earliest time with usable data in this scope, if any.
    pub history_available_since: Option<i64>,
    /// Flow-adjusted performance over the displayed interval.
    pub performance: Option<PeriodPerformanceDto>,
}

/// Modified Dietz performance between the first and last valued chart points.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct PeriodPerformanceDto {
    pub start: i64,
    pub end: i64,
    pub beginning_value_usd: String,
    pub ending_value_usd: String,
    /// Net external flows (inflows positive) that are known.
    pub net_flows_usd: String,
    pub flow_count: u32,
    pub gain_usd: Option<String>,
    pub return_percent: Option<String>,
    pub reason: Option<UnavailableReason>,
    /// Endpoint values or flows use daily (estimated) prices.
    pub estimated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct ActivityLeg {
    pub leg_id: String,
    pub asset_id: String,
    pub symbol: Option<String>,
    pub signed_quantity: String,
    pub direction: String,
    /// How the accounting replay interpreted this movement.
    pub treatment: Option<String>,
    /// Historical USD value at the time of the movement, if a quote exists.
    pub value_usd: Option<String>,
    pub value_estimated: bool,
    pub review: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct ActivityRow {
    pub transaction_id: String,
    pub account_id: String,
    pub network: NetworkId,
    pub occurred_at: i64,
    pub operation: String,
    pub status: String,
    pub legs: Vec<ActivityLeg>,
    pub fee_quantity: Option<String>,
    pub fee_symbol: Option<String>,
    pub fee_value_usd: Option<String>,
    pub unresolved: bool,
}

/// Optional activity filters (SPECIFICATION.md §5.6).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct ActivityFilter {
    #[serde(default)]
    pub asset_id: Option<String>,
    /// Only movements that need review or are not fully decoded.
    #[serde(default)]
    pub unresolved_only: bool,
    #[serde(default)]
    pub account_id: Option<String>,
    #[serde(default)]
    pub network: Option<NetworkId>,
    #[serde(default)]
    pub operation: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub start: Option<i64>,
    #[serde(default)]
    pub end: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct ActivityPage {
    pub rows: Vec<ActivityRow>,
    /// Opaque keyset cursor for the next page.
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum ThemePreference {
    Dark,
    Light,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct Settings {
    /// `None` follows the operating system language.
    pub language: Option<String>,
    pub theme: ThemePreference,
    /// IANA timezone override; `None` follows the operating system.
    pub timezone: Option<String>,
    pub privacy_mode: bool,
    pub price_refresh_seconds: u32,
    pub sweep_interval_minutes: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            language: None,
            theme: ThemePreference::Dark,
            timezone: None,
            privacy_mode: false,
            price_refresh_seconds: 60,
            sweep_interval_minutes: 60,
        }
    }
}

/// One remaining (or historical) FIFO lot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct LotRow {
    pub id: String,
    pub account_id: String,
    pub quantity: String,
    pub remaining_quantity: String,
    pub basis_usd: Option<String>,
    pub remaining_basis_usd: Option<String>,
    pub basis_kind: BasisKind,
    /// Original acquisition time (kept through own transfers).
    pub acquired_at: i64,
    /// When the inventory entered this account.
    pub arrived_at: i64,
    pub parent_lot_id: Option<String>,
}

/// An asset's position in one account of the scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct AssetAccountRow {
    pub account_id: String,
    pub wallet_id: String,
    pub quantity: String,
    pub value_usd: Option<String>,
    pub basis_usd: Option<String>,
    pub unrealized_pnl_usd: Option<String>,
    pub basis_coverage: BasisCoverage,
    pub balance_status: BalanceStatus,
}

/// Asset detail for a scope (SPECIFICATION.md §5.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct AssetDetail {
    pub asset_id: String,
    pub network: NetworkId,
    /// Contract, mint or master address; `None` for the native asset.
    pub contract: Option<String>,
    pub symbol: Option<String>,
    pub name: Option<String>,
    pub decimals: u32,
    pub verification: String,
    pub explorer_url: Option<String>,
    pub price_usd: Option<String>,
    pub price_observed_at: Option<i64>,
    pub change_24h_percent: Option<String>,
    pub quantity: String,
    pub value_usd: Option<String>,
    pub remaining_basis_usd: Option<String>,
    pub unrealized_pnl_usd: Option<String>,
    pub unrealized_return_percent: Option<String>,
    pub unrealized_reason: Option<UnavailableReason>,
    pub known_subset_pnl_usd: String,
    pub basis_coverage_quantity_percent: Option<String>,
    pub has_estimated_basis: bool,
    pub realized: PartialUsd,
    pub income: PartialUsd,
    pub expenses: PartialUsd,
    pub accounts: Vec<AssetAccountRow>,
    pub lots: Vec<LotRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct PricePoint {
    pub t: i64,
    /// `None` marks a gap: no quote within tolerance.
    pub price_usd: Option<String>,
    pub estimated: bool,
}

/// Market price and holdings value of one asset: two different series.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct AssetChart {
    pub price: Vec<PricePoint>,
    pub holdings: ChartSeries,
}

/// A movement that needs a user decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct ReviewItem {
    pub leg_id: String,
    pub transaction_id: String,
    pub account_id: String,
    pub network: NetworkId,
    pub occurred_at: i64,
    pub asset_id: String,
    pub symbol: Option<String>,
    pub quantity: String,
    pub treatment: String,
    /// `unknown_basis`, `unclassified_outgoing`, `missing_proceeds`,
    /// `missing_price` or `missing_trade_value`.
    pub reason: String,
    pub value_usd: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct ReconciliationRow {
    pub id: String,
    /// `inventory_gap`, `balance_mismatch`, `opening_overlap`,
    /// `orphaned_override` or `invalid_pairing`.
    pub kind: String,
    pub account_id: Option<String>,
    pub asset_id: Option<String>,
    pub symbol: Option<String>,
    pub quantity: Option<String>,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct ReviewList {
    pub items: Vec<ReviewItem>,
    /// Items in scope; `items` may be truncated to the requested limit.
    pub total: u32,
    pub reconciliation: Vec<ReconciliationRow>,
}

/// One saved version of a user decision (audit trail).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct OverrideVersion {
    pub version: i64,
    pub created_at: i64,
    /// `manual` or `csv:<batch>`.
    pub source: String,
    pub payload: crate::LegOverride,
    pub orphaned: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct PairCandidate {
    pub leg_id: String,
    pub account_id: String,
    pub network: NetworkId,
    pub occurred_at: i64,
    pub quantity: String,
    pub transaction_id: String,
}

/// Everything known about one movement, for the detail drawer and editor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct LegDetail {
    pub leg_id: String,
    pub transaction_id: String,
    pub tx_hash: String,
    pub tx_status: String,
    pub block_height: Option<i64>,
    pub provider: String,
    pub explorer_url: Option<String>,
    pub account_id: String,
    pub network: NetworkId,
    pub occurred_at: i64,
    pub asset_id: String,
    pub symbol: Option<String>,
    pub quantity: String,
    pub direction: String,
    pub operation: String,
    pub treatment: Option<String>,
    pub counterparty_account_id: Option<String>,
    pub value_usd: Option<String>,
    pub value_estimated: bool,
    pub basis_usd: Option<String>,
    pub basis_kind: Option<BasisKind>,
    pub proceeds_usd: Option<String>,
    pub review: Option<String>,
    pub current: Option<crate::LegOverride>,
    pub history: Vec<OverrideVersion>,
    pub pair_candidates: Vec<PairCandidate>,
}

/// Visibility is independent from accounting inclusion. None follows provider spam classification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct AssetPolicy {
    pub asset_id: String,
    pub symbol: Option<String>,
    pub name: Option<String>,
    pub verification: String,
    pub hidden: bool,
    pub exclude_override: Option<bool>,
}
