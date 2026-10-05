//! Account synchronization and price refresh (SPECIFICATION.md §6).
//!
//! History is downloaded newest first. A forward pass stops at the first page
//! that overlaps already stored history; older history continues from a
//! persisted backfill cursor on later runs, a bounded number of pages per run,
//! so large wallets import over several sweeps without exhausting free quotas.
//! Every write is idempotent, so a repeated or interrupted run never duplicates
//! records.

use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::sync::Mutex;

use bigdecimal::Zero;
use num_bigint::BigInt;
use portfolio_core::clock::utc_day;
use portfolio_core::decimal::{parse_dec, to_canonical};
use portfolio_core::network::NetworkId;
use portfolio_store::ingest::{AssetSpec, Checkpoint, Coverage, PriceSpec, TxSpec, TxStatus};
use portfolio_store::prices::{DailyPrice, PriceHistoryNeed};
use portfolio_store::{Account, Store, StoreError};
use serde::Serialize;

use crate::defillama::{self, DefiLlama};
use crate::error::ProviderError;
use crate::esplora::{self, Esplora};
use crate::http::HttpClient;
use crate::livecoinwatch::{self, LiveCoinWatch};
use crate::tonapi::{self, TonApi};
use crate::trongrid::{self, TronGrid};
use crate::zerion::{self, Positions, Window, Zerion};

const HISTORY: &str = "history";
/// Second history category of TRON accounts (TRC-20 transfer events).
const HISTORY_TRC20: &str = "history:trc20";
/// TRC-20 contracts whose metadata may be looked up per account and run.
const MAX_TOKEN_LOOKUPS: usize = 10;
const DAY_SECONDS: i64 = 86_400;
/// A token without a quote is asked for again after this long.
const QUOTE_MISS_RETRY_SECONDS: i64 = DAY_SECONDS;

/// Configured adapters. `None` means not configured (for example no API key).
#[derive(Default)]
pub struct Providers {
    pub esplora: Option<Esplora>,
    pub zerion: Option<Zerion>,
    pub livecoinwatch: Option<LiveCoinWatch>,
    pub defillama: Option<DefiLlama>,
    pub trongrid: Option<TronGrid>,
    pub tonapi: Option<TonApi>,
}

impl Providers {
    fn clients(&self) -> Vec<&HttpClient> {
        let mut out = Vec::new();
        if let Some(p) = &self.esplora {
            out.push(p.http());
        }
        if let Some(p) = &self.zerion {
            out.push(p.http());
        }
        if let Some(p) = &self.livecoinwatch {
            out.push(p.http());
        }
        if let Some(p) = &self.defillama {
            out.push(p.http());
        }
        if let Some(p) = &self.trongrid {
            out.push(p.http());
        }
        if let Some(p) = &self.tonapi {
            out.push(p.http());
        }
        out
    }
}

#[derive(Debug, Clone)]
pub struct SyncOptions {
    /// History pages per account per run (forward and backfill together).
    pub max_history_pages: u32,
    pub zerion_page_size: u32,
    /// Stored pending transactions re-checked per account per run.
    pub max_pending_checks: u32,
    /// Historical price requests per run (each covers up to 500 days of one asset).
    pub max_price_history_requests: u32,
}

impl Default for SyncOptions {
    fn default() -> Self {
        SyncOptions {
            max_history_pages: 8,
            zerion_page_size: zerion::MAX_PAGE_SIZE,
            max_pending_checks: 5,
            max_price_history_requests: 12,
        }
    }
}

/// Result of synchronizing one account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AccountSyncReport {
    pub account_id: String,
    pub network: NetworkId,
    pub provider: Option<String>,
    pub balance_refreshed: bool,
    pub new_transactions: u32,
    pub pages_fetched: u32,
    pub coverage: Coverage,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PriceReport {
    pub priced: u32,
    /// Asset IDs with no usable quote (shown as "price unavailable").
    pub unpriced: Vec<String>,
    pub errors: Vec<String>,
}

/// Result of downloading daily historical prices.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PriceHistoryReport {
    pub requests: u32,
    pub points: u32,
    /// Assets that still need older or newer daily history after this run.
    pub pending_assets: u32,
    /// Assets the provider has no series for (shown as unpriced in the past).
    pub unavailable: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug)]
pub enum SyncError {
    Provider(ProviderError),
    Store(StoreError),
}

impl std::fmt::Display for SyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncError::Provider(e) => e.fmt(f),
            SyncError::Store(e) => write!(f, "local storage: {e}"),
        }
    }
}

impl From<ProviderError> for SyncError {
    fn from(e: ProviderError) -> Self {
        SyncError::Provider(e)
    }
}

impl From<StoreError> for SyncError {
    fn from(e: StoreError) -> Self {
        SyncError::Store(e)
    }
}

/// A page of normalized history and its continuation.
pub struct HistoryPage {
    pub txs: Vec<TxSpec>,
    pub next: Option<String>,
    pub indexing: bool,
}

#[derive(Debug, Default)]
struct HistoryOutcome {
    new_transactions: u32,
    pages: u32,
    coverage: Coverage,
    confirmed_seen: BTreeSet<String>,
}

pub struct SyncEngine {
    store: Store,
    providers: Providers,
    options: SyncOptions,
    /// Providers stopped for the rest of this run (auth failure, budget, throttling).
    stopped: Mutex<BTreeMap<&'static str, String>>,
    btc_tip: tokio::sync::Mutex<Option<i64>>,
}

impl SyncEngine {
    pub fn new(store: Store, providers: Providers, options: SyncOptions) -> Self {
        SyncEngine {
            store,
            providers,
            options,
            stopped: Mutex::new(BTreeMap::new()),
            btc_tip: tokio::sync::Mutex::new(None),
        }
    }

    pub fn providers(&self) -> &Providers {
        &self.providers
    }

    /// Which provider serves an account's network in this build.
    pub fn provider_for(network: NetworkId) -> Option<&'static str> {
        Some(match network {
            NetworkId::Bitcoin => esplora::PROVIDER,
            NetworkId::Ethereum
            | NetworkId::Base
            | NetworkId::Arbitrum
            | NetworkId::Optimism
            | NetworkId::Polygon
            | NetworkId::Bsc
            | NetworkId::Solana => zerion::PROVIDER,
            NetworkId::Tron => trongrid::PROVIDER,
            NetworkId::Ton => tonapi::PROVIDER,
        })
    }

    fn check_stopped(&self, provider: &'static str) -> Result<(), ProviderError> {
        match self.stopped.lock().expect("stopped lock").get(provider) {
            Some(_) => Err(ProviderError::BudgetExhausted { provider }),
            None => Ok(()),
        }
    }

    fn observe(&self, provider: &'static str, error: &SyncError) {
        if let SyncError::Provider(e) = error
            && e.stops_provider()
        {
            self.stopped
                .lock()
                .expect("stopped lock")
                .insert(provider, e.to_string());
        }
    }

    /// Persists locally counted requests for every adapter.
    pub async fn flush_usage(&self) -> Result<(), StoreError> {
        let day = utc_day(self.store_now());
        for client in self.providers.clients() {
            let usage = client.take_usage();
            if usage.requests > 0 {
                self.store
                    .add_provider_usage(
                        client.provider(),
                        &day,
                        usage.requests,
                        usage.last_error.as_deref(),
                    )
                    .await?;
            }
        }
        Ok(())
    }

    fn store_now(&self) -> i64 {
        self.store.now()
    }

    /// Synchronizes every active account; archived accounts are skipped.
    pub async fn sync_all(&self) -> Vec<AccountSyncReport> {
        let accounts = match self.store.list_accounts(None).await {
            Ok(a) => a,
            Err(e) => {
                tracing::error!(error = %e, "cannot list accounts");
                return Vec::new();
            }
        };
        let mut reports = Vec::new();
        for account in accounts.into_iter().filter(|a| !a.archived) {
            reports.push(self.sync_account(&account).await);
        }
        reports
    }

    pub async fn sync_account(&self, account: &Account) -> AccountSyncReport {
        let provider = Self::provider_for(account.network);
        let mut report = AccountSyncReport {
            account_id: account.id.clone(),
            network: account.network,
            provider: provider.map(str::to_owned),
            balance_refreshed: false,
            new_transactions: 0,
            pages_fetched: 0,
            coverage: Coverage::Unsupported,
            error: None,
        };
        let Some(provider) = provider else {
            report.error = Some(format!(
                "{} synchronization is not available in this build yet",
                account.network.display_name()
            ));
            return report;
        };

        let mut checkpoint = match self.store.checkpoint(&account.id, provider, HISTORY).await {
            Ok(c) => c,
            Err(e) => {
                report.error = Some(SyncError::from(e).to_string());
                return report;
            }
        };
        let now = self.store_now();
        checkpoint.state.last_attempt_at = Some(now);

        let result = match account.network {
            NetworkId::Bitcoin => {
                self.sync_bitcoin(account, &mut checkpoint, &mut report)
                    .await
            }
            NetworkId::Tron => self.sync_tron(account, &mut checkpoint, &mut report).await,
            NetworkId::Ton => self.sync_ton(account, &mut checkpoint, &mut report).await,
            _ => {
                self.sync_zerion(account, &mut checkpoint, &mut report)
                    .await
            }
        };
        match result {
            Ok(()) => {
                checkpoint.state.last_success_at = Some(now);
                checkpoint.state.last_error = None;
            }
            Err(e) => {
                self.observe(provider, &e);
                if !report.balance_refreshed {
                    let _ = self.store.mark_balances_stale(&account.id).await;
                }
                checkpoint.state.last_error = Some(e.to_string());
                report.error = Some(e.to_string());
            }
        }
        checkpoint.earliest_covered_at = self
            .store
            .earliest_account_transaction(&account.id)
            .await
            .unwrap_or(None);
        report.coverage = checkpoint.coverage;
        if let Err(e) = self
            .store
            .save_checkpoint(&account.id, provider, HISTORY, &checkpoint)
            .await
        {
            report.error.get_or_insert_with(|| e.to_string());
        }
        if let Err(e) = self.flush_usage().await {
            tracing::warn!(error = %e, "cannot record provider usage");
        }
        report
    }

    // ------------------------------------------------------------ history core

    /// Forward pass, then backfill, within `max_history_pages`.
    async fn sync_history<F, Fut>(
        &self,
        account: &Account,
        checkpoint: &mut Checkpoint,
        fingerprint: &str,
        mut fetch: F,
    ) -> Result<HistoryOutcome, SyncError>
    where
        F: FnMut(Option<String>) -> Fut,
        Fut: Future<Output = Result<HistoryPage, ProviderError>>,
    {
        if checkpoint.boundary.as_deref() != Some(fingerprint) {
            // A cursor is only valid with the exact query that produced it.
            checkpoint.backfill_cursor = None;
            checkpoint.boundary = Some(fingerprint.to_owned());
            checkpoint.state.completed_once = false;
            checkpoint.coverage = Coverage::Loading;
        }
        let mut outcome = HistoryOutcome {
            coverage: checkpoint.coverage,
            ..HistoryOutcome::default()
        };
        let first_run = checkpoint.backfill_cursor.is_none() && !checkpoint.state.completed_once;
        let total = self.options.max_history_pages.max(1);
        // While older history is pending, the forward pass gets half the pages so
        // the backfill always progresses too.
        let forward_budget = if checkpoint.backfill_cursor.is_some() {
            (total / 2).max(1)
        } else {
            total
        };
        let mut pages_left = total;

        // Forward pass from the newest record.
        let mut cursor: Option<String> = None;
        let mut reached_known = false;
        let mut reached_end = false;
        let mut forward_pages = 0;
        while forward_pages < forward_budget {
            forward_pages += 1;
            pages_left -= 1;
            let page = fetch(cursor.clone()).await?;
            outcome.pages += 1;
            if page.indexing {
                checkpoint.coverage = Coverage::Loading;
                outcome.coverage = Coverage::Loading;
                return Ok(outcome);
            }
            let known = self.ingest_page(account, &page.txs, &mut outcome).await?;
            if known > 0 && !first_run {
                reached_known = true;
                break;
            }
            match page.next {
                None => {
                    reached_end = true;
                    break;
                }
                Some(next) => {
                    if cursor.as_deref() == Some(next.as_str()) {
                        return Err(ProviderError::RepeatedCursor {
                            provider: "sync",
                            endpoint: "history",
                        }
                        .into());
                    }
                    cursor = Some(next);
                }
            }
        }
        if reached_end {
            checkpoint.backfill_cursor = None;
            checkpoint.state.completed_once = true;
        } else if !reached_known {
            // Budget ran out before reaching stored history (first import, or a
            // gap after a long pause): continue downward from here next time.
            if checkpoint.backfill_cursor.is_some() && !first_run {
                // An older gap is still pending; walk down through everything
                // from the newer one rather than tracking two cursors.
                checkpoint.state.completed_once = false;
            }
            checkpoint.backfill_cursor = cursor;
        }

        // Backfill older history from the persisted cursor.
        while pages_left > 0 {
            let Some(from) = checkpoint.backfill_cursor.clone() else {
                break;
            };
            pages_left -= 1;
            let page = fetch(Some(from.clone())).await?;
            outcome.pages += 1;
            let known = self.ingest_page(account, &page.txs, &mut outcome).await?;
            let all_known = !page.txs.is_empty() && known == page.txs.len();
            match page.next {
                None => {
                    checkpoint.backfill_cursor = None;
                    checkpoint.state.completed_once = true;
                }
                Some(_) if all_known && checkpoint.state.completed_once => {
                    // The gap is closed: everything below was stored before.
                    checkpoint.backfill_cursor = None;
                }
                Some(next) if next == from => {
                    return Err(ProviderError::RepeatedCursor {
                        provider: "sync",
                        endpoint: "history",
                    }
                    .into());
                }
                Some(next) => checkpoint.backfill_cursor = Some(next),
            }
        }

        checkpoint.coverage =
            if checkpoint.backfill_cursor.is_none() && checkpoint.state.completed_once {
                Coverage::Complete
            } else {
                Coverage::Loading
            };
        outcome.coverage = checkpoint.coverage;
        Ok(outcome)
    }

    /// Ingests a page; returns how many of its records were already known.
    /// A component record (`TxSpec::part`) is known only when that component
    /// was stored, not merely its transaction.
    async fn ingest_page(
        &self,
        account: &Account,
        txs: &[TxSpec],
        outcome: &mut HistoryOutcome,
    ) -> Result<usize, SyncError> {
        let hashes: Vec<String> = txs
            .iter()
            .filter(|t| t.part.is_none())
            .map(|t| t.hash.clone())
            .collect();
        let known = self
            .store
            .known_account_transactions(&account.id, account.network, &hashes)
            .await?;
        let parts: Vec<(String, String)> = txs
            .iter()
            .filter_map(|t| t.part.clone().map(|p| (t.hash.clone(), p)))
            .collect();
        let known_parts = self
            .store
            .known_account_parts(&account.id, account.network, &parts)
            .await?;
        let is_known = |t: &TxSpec| match &t.part {
            None => known.contains(&t.hash),
            Some(p) => known_parts.contains(&(t.hash.clone(), p.clone())),
        };
        let known_count = txs.iter().filter(|t| is_known(t)).count();
        for tx in txs {
            if self.store.ingest_transaction(&account.id, tx).await? {
                outcome.new_transactions += 1;
            }
            if tx.status != TxStatus::Pending {
                outcome.confirmed_seen.insert(tx.hash.clone());
            }
        }
        Ok(known_count)
    }

    /// Records a complete set of current holdings: listed assets get their
    /// quantity and previously held assets missing from the list become zero.
    async fn record_holdings(
        &self,
        account: &Account,
        provider: &'static str,
        holdings: &[(AssetSpec, BigInt)],
        height: Option<i64>,
    ) -> Result<(), SyncError> {
        let mut seen = BTreeSet::new();
        for (asset, raw) in holdings {
            seen.insert(asset.id());
            self.store
                .record_balance(&account.id, asset, raw, height, "fresh")
                .await?;
        }
        for asset_id in self.store.nonzero_balance_assets(&account.id).await? {
            if !seen.contains(&asset_id) {
                self.store
                    .record_zero_balance(&account.id, &asset_id, provider)
                    .await?;
            }
        }
        Ok(())
    }

    /// Re-reads stored pending records that this run did not see confirmed;
    /// `lookup` returns the current record or `None` when it disappeared.
    async fn recheck_pending<F, Fut>(
        &self,
        account: &Account,
        confirmed_seen: &BTreeSet<String>,
        mut lookup: F,
    ) -> Result<(), SyncError>
    where
        F: FnMut(String) -> Fut,
        Fut: Future<Output = Result<Option<TxSpec>, ProviderError>>,
    {
        let stale: Vec<String> = self
            .store
            .pending_account_transactions(&account.id)
            .await?
            .into_iter()
            .filter(|h| !confirmed_seen.contains(h))
            .take(self.options.max_pending_checks as usize)
            .collect();
        for hash in stale {
            match lookup(hash.clone()).await? {
                Some(spec) => {
                    self.store.ingest_transaction(&account.id, &spec).await?;
                }
                None => {
                    self.store
                        .mark_transaction_dropped(account.network, &hash)
                        .await?
                }
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------ Bitcoin

    async fn sync_bitcoin(
        &self,
        account: &Account,
        checkpoint: &mut Checkpoint,
        report: &mut AccountSyncReport,
    ) -> Result<(), SyncError> {
        self.check_stopped(esplora::PROVIDER)?;
        let esplora = self
            .providers
            .esplora
            .as_ref()
            .ok_or(ProviderError::MissingKey {
                provider: esplora::PROVIDER,
            })?;
        let address = account.canonical_address.as_str();
        let owned: BTreeSet<String> = self
            .store
            .accounts_on_network(NetworkId::Bitcoin)
            .await?
            .into_iter()
            .map(|a| a.canonical_address)
            .collect();

        // Current confirmed balance at a known chain height.
        let tip = {
            let mut tip = self.btc_tip.lock().await;
            match *tip {
                Some(h) => h,
                None => {
                    let h = esplora.tip_height().await?;
                    *tip = Some(h);
                    h
                }
            }
        };
        let info = esplora.address(address).await?;
        let balance = info.confirmed_balance()?;
        self.store
            .record_balance(
                &account.id,
                &AssetSpec::native(NetworkId::Bitcoin, esplora::PROVIDER),
                &BigInt::from(balance),
                Some(tip),
                "fresh",
            )
            .await?;
        report.balance_refreshed = true;

        let now = self.store_now();
        let outcome = self
            .sync_history(account, checkpoint, "esplora;v1", |cursor| {
                let owned = &owned;
                async move {
                    let txs = esplora.chain_txs(address, cursor.as_deref()).await?;
                    let next = (txs.len() == esplora::CHAIN_PAGE_SIZE)
                        .then(|| txs.last().map(|t| t.txid.clone()))
                        .flatten();
                    Ok(HistoryPage {
                        txs: txs
                            .iter()
                            .filter_map(|t| esplora::tx_for_account(t, address, owned, now))
                            .collect(),
                        next,
                        indexing: false,
                    })
                }
            })
            .await?;
        report.pages_fetched = outcome.pages;
        report.new_transactions = outcome.new_transactions;

        // Unconfirmed activity, kept separate from the confirmed ledger.
        let mempool = esplora.mempool_txs(address).await?;
        checkpoint.state.pending_incomplete = mempool.len() >= esplora::MEMPOOL_CAP;
        let mut in_mempool = BTreeSet::new();
        for tx in &mempool {
            if let Some(spec) = esplora::tx_for_account(tx, address, &owned, now) {
                in_mempool.insert(spec.hash.clone());
                if self.store.ingest_transaction(&account.id, &spec).await? {
                    report.new_transactions += 1;
                }
            }
        }
        // Previously pending transactions that are neither in the mempool nor in
        // the confirmed pages just read: confirmed later, or dropped/replaced.
        let stale: Vec<String> = self
            .store
            .pending_account_transactions(&account.id)
            .await?
            .into_iter()
            .filter(|h| !in_mempool.contains(h) && !outcome.confirmed_seen.contains(h))
            .take(self.options.max_pending_checks as usize)
            .collect();
        for hash in stale {
            match esplora.tx(&hash).await? {
                Some(tx) => {
                    if let Some(spec) = esplora::tx_for_account(&tx, address, &owned, now) {
                        self.store.ingest_transaction(&account.id, &spec).await?;
                    }
                }
                None => {
                    self.store
                        .mark_transaction_dropped(NetworkId::Bitcoin, &hash)
                        .await?
                }
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------ EVM (Zerion)

    async fn sync_zerion(
        &self,
        account: &Account,
        checkpoint: &mut Checkpoint,
        report: &mut AccountSyncReport,
    ) -> Result<(), SyncError> {
        self.check_stopped(zerion::PROVIDER)?;
        let zerion = self
            .providers
            .zerion
            .as_ref()
            .ok_or(ProviderError::MissingKey {
                provider: zerion::PROVIDER,
            })?;
        let network = account.network;
        let address = account.canonical_address.as_str();

        match zerion.positions(network, address).await? {
            Positions::Indexing => {
                checkpoint.coverage = Coverage::Loading;
                return Ok(());
            }
            Positions::Ready(positions) => {
                let mut seen = BTreeSet::new();
                for p in &positions {
                    seen.insert(p.asset.id());
                    self.store
                        .record_balance(&account.id, &p.asset, &p.raw, p.block, "fresh")
                        .await?;
                }
                // Simple positions omit zero balances: an asset that disappeared
                // from the list is now zero, not "unknown".
                for asset_id in self.store.nonzero_balance_assets(&account.id).await? {
                    if !seen.contains(&asset_id) {
                        self.store
                            .record_zero_balance(&account.id, &asset_id, zerion::PROVIDER)
                            .await?;
                    }
                }
                if !seen.iter().any(|id| id.ends_with(":native")) {
                    // A wallet with no native balance still has a known zero.
                    let native = AssetSpec::native(network, zerion::PROVIDER);
                    self.store
                        .record_balance(&account.id, &native, &BigInt::from(0), None, "fresh")
                        .await?;
                }
                report.balance_refreshed = true;
            }
        }

        let page_size = self.options.zerion_page_size;
        let fingerprint = Zerion::fingerprint(network, page_size);
        let outcome = self
            .sync_history(account, checkpoint, &fingerprint, |cursor| async move {
                let page = zerion
                    .transactions(
                        network,
                        address,
                        cursor.as_deref(),
                        page_size,
                        Window::default(),
                    )
                    .await?;
                Ok(HistoryPage {
                    txs: page.txs,
                    next: page.next_cursor,
                    indexing: page.indexing,
                })
            })
            .await?;
        report.pages_fetched = outcome.pages;
        report.new_transactions = outcome.new_transactions;
        Ok(())
    }

    // ------------------------------------------------------------ TRON

    async fn sync_tron(
        &self,
        account: &Account,
        checkpoint: &mut Checkpoint,
        report: &mut AccountSyncReport,
    ) -> Result<(), SyncError> {
        self.check_stopped(trongrid::PROVIDER)?;
        let api = self
            .providers
            .trongrid
            .as_ref()
            .ok_or(ProviderError::MissingKey {
                provider: trongrid::PROVIDER,
            })?;
        let address = account.canonical_address.as_str();

        // Current holdings: liquid plus staked TRX, and TRC-20 balances. The
        // account route lists contracts and raw amounts only; decimals come
        // from stored metadata or one transfer record of that token.
        let current = api.account(address).await?;
        let mut lookups = MAX_TOKEN_LOOKUPS;
        let unknown = self
            .record_tron_holdings(account, api, &current, &mut lookups)
            .await?;
        report.balance_refreshed = true;

        // Native activity (main view, with fees).
        let size = trongrid::PAGE_SIZE;
        let native = self
            .sync_history(
                account,
                checkpoint,
                &TronGrid::fingerprint("native", size),
                |cursor| async move {
                    let page = api.transactions(address, cursor.as_deref(), size).await?;
                    Ok(HistoryPage {
                        txs: page
                            .items
                            .iter()
                            .map(|t| trongrid::native_tx_for_account(t, address))
                            .collect::<Result<_, _>>()?,
                        next: page.next,
                        indexing: false,
                    })
                },
            )
            .await?;
        report.pages_fetched = native.pages;
        report.new_transactions = native.new_transactions;

        // TRC-20 transfer events (components of the same transactions).
        let mut tokens = self
            .store
            .checkpoint(&account.id, trongrid::PROVIDER, HISTORY_TRC20)
            .await?;
        tokens.state.last_attempt_at = Some(self.store_now());
        let result = self
            .sync_history(
                account,
                &mut tokens,
                &TronGrid::fingerprint("trc20", size),
                |cursor| async move {
                    let page = api
                        .trc20_transfers(address, cursor.as_deref(), size, None)
                        .await?;
                    Ok(HistoryPage {
                        txs: trongrid::trc20_specs_for_account(&page.items, address),
                        next: page.next,
                        indexing: false,
                    })
                },
            )
            .await;
        match &result {
            Ok(_) => {
                tokens.state.last_success_at = tokens.state.last_attempt_at;
                tokens.state.last_error = None;
            }
            Err(e) => tokens.state.last_error = Some(e.to_string()),
        }
        self.store
            .save_checkpoint(&account.id, trongrid::PROVIDER, HISTORY_TRC20, &tokens)
            .await?;
        let trc20 = result?;
        report.pages_fetched += trc20.pages;
        report.new_transactions += trc20.new_transactions;
        if unknown > 0 {
            // The imported transfers taught token metadata: complete the
            // holdings without another account request.
            let mut none = 0;
            self.record_tron_holdings(account, api, &current, &mut none)
                .await?;
        }
        // Complete only when both categories are complete.
        if tokens.coverage != Coverage::Complete {
            checkpoint.coverage = tokens.coverage;
        }
        Ok(())
    }

    /// Records TRX and every TRC-20 balance whose metadata is known (looking
    /// up at most `lookups` unknown tokens). Returns how many balances are
    /// still missing metadata; while any are, no token is zeroed.
    async fn record_tron_holdings(
        &self,
        account: &Account,
        api: &TronGrid,
        current: &trongrid::TronAccount,
        lookups: &mut usize,
    ) -> Result<u32, SyncError> {
        let address = account.canonical_address.as_str();
        let mut holdings = vec![(
            AssetSpec::native(NetworkId::Tron, trongrid::PROVIDER),
            current.total_trx(),
        )];
        let mut unknown = 0u32;
        for (contract, raw) in &current.trc20 {
            let asset_id = portfolio_store::demo::asset_id(NetworkId::Tron, Some(contract));
            let mut asset = self.store.asset_spec(&asset_id, trongrid::PROVIDER).await?;
            if asset.is_none() && *lookups > 0 {
                *lookups -= 1;
                let page = api
                    .trc20_transfers(address, None, 1, Some(contract))
                    .await?;
                asset = page
                    .items
                    .iter()
                    .find(|t| t.token_info.address == *contract)
                    .map(|t| t.token_info.asset());
            }
            match asset {
                Some(asset) => holdings.push((asset, raw.clone())),
                // Without decimals the quantity cannot be stated; it stays
                // missing (never zero) until metadata is known.
                None => unknown += 1,
            }
        }
        if unknown == 0 {
            self.record_holdings(account, trongrid::PROVIDER, &holdings, None)
                .await?;
        } else {
            for (asset, raw) in &holdings {
                self.store
                    .record_balance(&account.id, asset, raw, None, "fresh")
                    .await?;
            }
        }
        Ok(unknown)
    }

    // ------------------------------------------------------------ TON

    async fn sync_ton(
        &self,
        account: &Account,
        checkpoint: &mut Checkpoint,
        report: &mut AccountSyncReport,
    ) -> Result<(), SyncError> {
        self.check_stopped(tonapi::PROVIDER)?;
        let api = self
            .providers
            .tonapi
            .as_ref()
            .ok_or(ProviderError::MissingKey {
                provider: tonapi::PROVIDER,
            })?;
        let address = account.canonical_address.as_str();

        let current = api.holdings(address).await?;
        let mut holdings = vec![(
            AssetSpec::native(NetworkId::Ton, tonapi::PROVIDER),
            current.nanoton.clone(),
        )];
        holdings.extend(current.jettons.iter().cloned());
        self.record_holdings(account, tonapi::PROVIDER, &holdings, None)
            .await?;
        report.balance_refreshed = true;

        let size = tonapi::PAGE_SIZE;
        let outcome = self
            .sync_history(
                account,
                checkpoint,
                &TonApi::fingerprint(size),
                |cursor| async move {
                    let page = api.events(address, cursor.as_deref(), size).await?;
                    Ok(HistoryPage {
                        txs: page
                            .events
                            .iter()
                            .map(|e| tonapi::event_for_account(e, address))
                            .collect(),
                        next: page.next,
                        indexing: false,
                    })
                },
            )
            .await?;
        report.pages_fetched = outcome.pages;
        report.new_transactions = outcome.new_transactions;

        // Traces still running when they were read are re-read until final.
        self.recheck_pending(account, &outcome.confirmed_seen, |hash| async move {
            Ok(api
                .event(address, &hash)
                .await?
                .map(|e| tonapi::event_for_account(&e, address)))
        })
        .await
    }

    // ------------------------------------------------------------ prices

    /// Refreshes quotes for every held, non-spam asset.
    ///
    /// Native assets use Live Coin Watch with curated codes; without an LCW key
    /// they fall back to DefiLlama's verified CoinGecko identities. Tokens are
    /// priced only by contract identity through DefiLlama. An asset with no
    /// quote stays unpriced; it is never valued at zero.
    pub async fn refresh_prices(&self) -> PriceReport {
        let mut report = PriceReport::default();
        let held = match self.store.held_assets().await {
            Ok(h) => h,
            Err(e) => {
                report.errors.push(e.to_string());
                return report;
            }
        };
        let now = self.store_now();
        let mut priced: BTreeSet<String> = BTreeSet::new();

        // Native assets via Live Coin Watch.
        let natives: Vec<_> = held
            .iter()
            .filter(|a| a.contract.is_none())
            .filter_map(|a| livecoinwatch::native_code(a.network).map(|c| (a, c)))
            .collect();
        if let Some(lcw) = &self.providers.livecoinwatch
            && !natives.is_empty()
            && self.check_stopped(livecoinwatch::PROVIDER).is_ok()
        {
            let mut codes: Vec<&str> = natives.iter().map(|(_, c)| *c).collect();
            codes.sort_unstable();
            codes.dedup();
            match lcw.quotes(&codes).await {
                Ok(quotes) => {
                    for (asset, code) in &natives {
                        let Some(q) = quotes.iter().find(|q| q.code == *code) else {
                            continue;
                        };
                        let spec = PriceSpec {
                            asset_id: asset.asset_id.clone(),
                            provider: livecoinwatch::PROVIDER,
                            price_usd: to_canonical(&q.rate_usd),
                            requested_at: now,
                            observed_at: now,
                            granularity: "tick",
                            quality: "current",
                            change_24h_percent: q.change_24h_percent.as_ref().map(to_canonical),
                        };
                        match self
                            .save_price(&spec, livecoinwatch::PROVIDER, code, "verified", true)
                            .await
                        {
                            Ok(()) => {
                                priced.insert(asset.asset_id.clone());
                            }
                            Err(e) => report.errors.push(e.to_string()),
                        }
                    }
                }
                Err(e) => {
                    self.observe(livecoinwatch::PROVIDER, &SyncError::Provider(e.clone()));
                    report.errors.push(e.to_string());
                }
            }
        }

        // Tokens by contract, plus natives LCW could not price. Tokens the
        // service had no quote for within the last day are not asked again.
        let misses = self
            .store
            .recent_quote_misses(defillama::PROVIDER, now - QUOTE_MISS_RETRY_SECONDS)
            .await
            .unwrap_or_default();
        let mut wanted: Vec<(String, String)> = Vec::new(); // (asset_id, coin id)
        for asset in &held {
            if priced.contains(&asset.asset_id)
                || (asset.contract.is_some() && misses.contains(&asset.asset_id))
            {
                continue;
            }
            let coin = match &asset.contract {
                Some(contract) => defillama::coin_id(asset.network, contract),
                None => native_coingecko_id(asset.network).map(|id| format!("coingecko:{id}")),
            };
            if let Some(coin) = coin {
                wanted.push((asset.asset_id.clone(), coin));
            }
        }
        if let Some(llama) = &self.providers.defillama
            && !wanted.is_empty()
            && self.check_stopped(defillama::PROVIDER).is_ok()
        {
            let ids: Vec<String> = wanted.iter().map(|(_, c)| c.clone()).collect();
            match llama.current(&ids).await {
                Ok(quotes) => {
                    let missed: Vec<(String, String)> = wanted
                        .iter()
                        .filter(|(_, coin)| !quotes.contains_key(coin))
                        .cloned()
                        .collect();
                    let quoted: Vec<String> = wanted
                        .iter()
                        .filter(|(_, coin)| quotes.contains_key(coin))
                        .map(|(id, _)| id.clone())
                        .collect();
                    if let Err(e) = self
                        .store
                        .record_quote_results(defillama::PROVIDER, &missed, &quoted)
                        .await
                    {
                        report.errors.push(e.to_string());
                    }
                    let min_conf = parse_dec(defillama::MIN_CONFIDENCE).expect("constant");
                    for (asset_id, coin) in &wanted {
                        let Some(q) = quotes.get(coin) else {
                            continue;
                        };
                        if q.price_usd < bigdecimal::BigDecimal::zero() {
                            continue;
                        }
                        let quality = if q.confidence.as_ref().is_some_and(|c| *c < min_conf) {
                            "low_confidence"
                        } else if now - q.timestamp > 3_600 {
                            "stale"
                        } else {
                            "current"
                        };
                        let spec = PriceSpec {
                            asset_id: asset_id.clone(),
                            provider: defillama::PROVIDER,
                            price_usd: to_canonical(&q.price_usd),
                            requested_at: now,
                            observed_at: q.timestamp.min(now),
                            granularity: "tick",
                            quality,
                            change_24h_percent: None,
                        };
                        let confidence = if coin.starts_with("coingecko:") {
                            "verified"
                        } else {
                            "high"
                        };
                        match self
                            .save_price(
                                &spec,
                                defillama::PROVIDER,
                                coin,
                                confidence,
                                coin.starts_with("coingecko:"),
                            )
                            .await
                        {
                            Ok(()) if quality != "low_confidence" => {
                                priced.insert(asset_id.clone());
                            }
                            Ok(()) => {}
                            Err(e) => report.errors.push(e.to_string()),
                        }
                    }
                }
                Err(e) => {
                    self.observe(defillama::PROVIDER, &SyncError::Provider(e.clone()));
                    report.errors.push(e.to_string());
                }
            }
        }
        if self.providers.livecoinwatch.is_none() && !natives.is_empty() {
            report
                .errors
                .push("livecoinwatch: no API key configured; using fallback quotes".into());
        }

        report.priced = u32::try_from(priced.len()).unwrap_or(u32::MAX);
        report.unpriced = held
            .iter()
            .filter(|a| !priced.contains(&a.asset_id))
            .map(|a| a.asset_id.clone())
            .collect();
        if let Err(e) = self.flush_usage().await {
            report.errors.push(e.to_string());
        }
        report
    }

    /// Downloads missing daily USD history for every asset with activity or a
    /// balance, newest range first, within `max_price_history_requests`.
    ///
    /// Tokens are identified by contract and natives by verified CoinGecko
    /// identities through DefiLlama's free chart route; nothing is matched by
    /// ticker. Coverage is remembered, so a run with nothing new costs nothing.
    pub async fn refresh_price_history(&self) -> PriceHistoryReport {
        let mut report = PriceHistoryReport::default();
        if self.providers.defillama.is_none() {
            return report;
        }
        let needs = match self.store.price_history_needs(defillama::PROVIDER).await {
            Ok(n) => n,
            Err(e) => {
                report.errors.push(e.to_string());
                return report;
            }
        };
        let now = self.store_now();
        let today = now - now.rem_euclid(DAY_SECONDS);
        for need in needs {
            let coin = match &need.contract {
                Some(contract) => defillama::coin_id(need.network, contract),
                None => native_coingecko_id(need.network).map(|id| format!("coingecko:{id}")),
            };
            let Some(coin) = coin else { continue };
            if need.retry_after.is_some_and(|t| t > now) {
                continue;
            }
            self.fetch_price_history(need, &coin, today, &mut report)
                .await;
        }
        if let Err(e) = self.flush_usage().await {
            report.errors.push(e.to_string());
        }
        report
    }

    async fn fetch_price_history(
        &self,
        mut need: PriceHistoryNeed,
        coin: &str,
        today: i64,
        report: &mut PriceHistoryReport,
    ) {
        let Some(llama) = &self.providers.defillama else {
            return;
        };
        let min_conf = parse_dec(defillama::MIN_CONFIDENCE).expect("constant");
        while let Some((from, to)) = need_window(&need, today) {
            if report.requests >= self.options.max_price_history_requests
                || self.check_stopped(defillama::PROVIDER).is_err()
            {
                report.pending_assets += 1;
                return;
            }
            report.requests += 1;
            let span = (to - from) / DAY_SECONDS + 1;
            match llama.daily_chart(coin, from, span).await {
                Ok(Some(series)) => {
                    let low = series.confidence.as_ref().is_some_and(|c| *c < min_conf);
                    let points: Vec<DailyPrice> = series
                        .points
                        .iter()
                        .map(|(at, price)| DailyPrice {
                            at: *at,
                            price_usd: to_canonical(price),
                            low_confidence: low,
                        })
                        .collect();
                    report.points += u32::try_from(points.len()).unwrap_or(u32::MAX);
                    if let Err(e) = self
                        .store
                        .store_price_history(
                            &need.asset_id,
                            defillama::PROVIDER,
                            coin,
                            from,
                            to,
                            &points,
                        )
                        .await
                    {
                        report.errors.push(e.to_string());
                        return;
                    }
                    need.covered_from = Some(need.covered_from.map_or(from, |c| c.min(from)));
                    need.covered_to = Some(need.covered_to.map_or(to, |c| c.max(to)));
                }
                Ok(None) => {
                    report.unavailable.push(need.asset_id.clone());
                    if let Err(e) = self
                        .store
                        .note_price_history_missing(
                            &need.asset_id,
                            defillama::PROVIDER,
                            coin,
                            "no historical series",
                        )
                        .await
                    {
                        report.errors.push(e.to_string());
                    }
                    return;
                }
                Err(e) => {
                    self.observe(defillama::PROVIDER, &SyncError::Provider(e.clone()));
                    report.errors.push(e.to_string());
                    if !e.stops_provider() {
                        let _ = self
                            .store
                            .note_price_history_missing(
                                &need.asset_id,
                                defillama::PROVIDER,
                                coin,
                                &e.to_string(),
                            )
                            .await;
                    }
                    return;
                }
            }
        }
    }

    async fn save_price(
        &self,
        spec: &PriceSpec,
        provider: &str,
        provider_asset_id: &str,
        confidence: &str,
        manual: bool,
    ) -> Result<(), StoreError> {
        self.store
            .upsert_asset_mapping(
                &spec.asset_id,
                provider,
                provider_asset_id,
                confidence,
                manual,
            )
            .await?;
        self.store.insert_price(spec).await
    }
}

/// The next daily window `[from, to]` still missing for an asset: first the
/// recent tail (up to today), then older history back to `need_from`, each at
/// most [`defillama::MAX_CHART_POINTS`] days.
fn need_window(need: &PriceHistoryNeed, today: i64) -> Option<(i64, i64)> {
    let max = (defillama::MAX_CHART_POINTS - 1) * DAY_SECONDS;
    match (need.covered_from, need.covered_to) {
        (Some(from), Some(to)) => {
            if to < today {
                let start = to;
                Some((start, (start + max).min(today)))
            } else if from > need.need_from {
                let end = from;
                Some(((end - max).max(need.need_from), end))
            } else {
                None
            }
        }
        _ => Some(((today - max).max(need.need_from), today)),
    }
}

/// Verified CoinGecko identities of native assets (used through DefiLlama).
fn native_coingecko_id(network: NetworkId) -> Option<&'static str> {
    Some(match network {
        NetworkId::Bitcoin => "bitcoin",
        NetworkId::Ethereum | NetworkId::Base | NetworkId::Arbitrum | NetworkId::Optimism => {
            "ethereum"
        }
        NetworkId::Polygon => "polygon-ecosystem-token",
        NetworkId::Bsc => "binancecoin",
        NetworkId::Solana => "solana",
        NetworkId::Tron => "tron",
        NetworkId::Ton => "the-open-network",
    })
}
