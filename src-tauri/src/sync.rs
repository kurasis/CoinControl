//! Background synchronization: builds provider adapters from stored keys,
//! runs account sweeps and price refreshes on the configured schedule, and
//! notifies the UI when cached data changed.
//!
//! Only the real profile synchronizes. The demo profile never contacts a
//! provider, so demo data cannot be mistaken for, or mixed with, real data.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::Duration;

use portfolio_core::clock::{Clock, SystemClock, utc_day};
use portfolio_providers::defillama::{self, DefiLlama};
use portfolio_providers::esplora::{self, Esplora};
use portfolio_providers::http::Budget;
use portfolio_providers::livecoinwatch::{self, LiveCoinWatch};
use portfolio_providers::mirrors::Reserve;
use portfolio_providers::tonapi::{self, TonApi};
use portfolio_providers::trongrid::{self, TronGrid};
use portfolio_providers::zerion::{self, Zerion};
use portfolio_providers::{
    AccountSyncReport, PriceHistoryReport, PriceReport, Providers, SyncEngine, SyncOptions,
};
use portfolio_providers::{
    alchemy::{self, Alchemy},
    helius::{self, Helius},
};
use portfolio_store::ReplayReport;
use portfolio_store::{ProfileKind, Store};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use zeroize::Zeroizing;

use crate::AppState;
use crate::error::{CommandError, CommandResult};
use crate::secrets::SecretStore;

/// Event the UI listens to; it refetches cached queries when received.
pub const DATA_CHANGED_EVENT: &str = "portfolio-data-changed";

/// Local daily soft limits (API_PROVIDERS.md §4). Provider limits still apply.
const ZERION_DAILY_BUDGET: u32 = 1_600;
const LCW_DAILY_BUDGET: u32 = 9_000;
/// TronGrid publishes no fixed free quota; stay far below typical plan limits.
const TRONGRID_DAILY_BUDGET: u32 = 5_000;
/// TonAPI's free key allows about one request per second.
const TONAPI_DAILY_BUDGET: u32 = 5_000;
const HELIUS_DAILY_CREDITS: u32 = 20_000;
const ALCHEMY_DAILY_CU: u32 = 150_000;
const TICK: Duration = Duration::from_secs(15);

/// Scheduler bookkeeping shared by the background loop and commands.
#[derive(Default)]
pub struct SyncState {
    pub(crate) run_lock: tokio::sync::Mutex<()>,
    pub(crate) cancelled: Arc<AtomicBool>,
    pub(crate) network_log: Arc<portfolio_providers::network_log::NetworkLog>,
    details: Arc<std::sync::Mutex<SyncDetails>>,
    last_sweep_at: AtomicI64,
    last_prices_at: AtomicI64,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SyncSummary {
    pub accounts: Vec<AccountSyncReport>,
    pub prices: PriceReport,
    pub price_history: PriceHistoryReport,
    /// `None` when nothing changed since the last accounting replay.
    pub accounting: Option<ReplayReport>,
    pub accounting_error: Option<String>,
}

/// A provider key from the OS credential store. Debug builds also accept the
/// development variables from `.env.example`; release builds never read them.
fn credential(secrets: &dyn SecretStore, provider: &str, env: &str) -> Option<Zeroizing<String>> {
    if let Some(secret) = secrets.get(provider) {
        return Some(Zeroizing::new(secret.expose().to_owned()));
    }
    if cfg!(debug_assertions) {
        return std::env::var(env)
            .ok()
            .filter(|v| !v.trim().is_empty())
            .map(Zeroizing::new);
    }
    None
}

pub fn network_capabilities(
    secrets: &dyn SecretStore,
) -> Vec<portfolio_providers::capabilities::NetworkCapability> {
    portfolio_providers::capabilities::configured_capabilities(
        credential(secrets, alchemy::PROVIDER, "ALCHEMY_API_KEY").is_some(),
        credential(secrets, helius::PROVIDER, "HELIUS_API_KEY").is_some(),
    )
}

async fn remaining_today(store: &Store, provider: &str, limit: u32) -> u32 {
    let used = store
        .provider_usage(&utc_day(SystemClock.now()))
        .await
        .ok()
        .and_then(|u| u.into_iter().find(|p| p.provider == provider))
        .map_or(0, |p| p.requests);
    limit.saturating_sub(used)
}

async fn build_providers(
    store: &Store,
    secrets: &dyn SecretStore,
    log: Arc<portfolio_providers::network_log::NetworkLog>,
) -> Providers {
    // Native live acceptance uses public BTC reads only, with one persisted
    // ceiling across restarts/sweeps. Production has its normal daily budgets.
    #[cfg(feature = "native-e2e")]
    let public_budget = |provider| remaining_today(store, provider, 50);
    let mut providers = Providers {
        esplora: Esplora::new(esplora::DEFAULT_BASE, {
            #[cfg(feature = "native-e2e")]
            {
                Budget::limited(public_budget(esplora::PROVIDER).await)
            }
            #[cfg(not(feature = "native-e2e"))]
            {
                Budget::unlimited()
            }
        })
        .ok(),
        defillama: DefiLlama::new(defillama::DEFAULT_BASE, {
            #[cfg(feature = "native-e2e")]
            {
                Budget::limited(public_budget(defillama::PROVIDER).await)
            }
            #[cfg(not(feature = "native-e2e"))]
            {
                Budget::unlimited()
            }
        })
        .ok(),
        ..Providers::default()
    };
    providers.mempool = Esplora::with_provider(
        "mempool",
        "https://mempool.space/api/",
        Budget::limited(
            remaining_today(
                store,
                "mempool",
                if cfg!(feature = "native-e2e") {
                    50
                } else {
                    5_000
                },
            )
            .await,
        ),
        Esplora::default_config(),
    )
    .ok();
    for (id, var) in [
        ("publicnode", ""),
        ("blockscout", "BLOCKSCOUT_API_KEY"),
        ("etherscan", "ETHERSCAN_API_KEY"),
        ("drpc", "DRPC_API_KEY"),
        ("chainstack", "CHAINSTACK_API_KEY"),
        ("toncenter", "TONCENTER_API_KEY"),
    ] {
        let key = credential(secrets, id, var);
        if !matches!(id, "publicnode" | "toncenter") && key.is_none() {
            continue;
        }
        let day = utc_day(SystemClock.now());
        let daily = remaining_today(
            store,
            id,
            if cfg!(feature = "native-e2e") {
                50
            } else {
                1_000
            },
        )
        .await;
        let (monthly_requests, _) = store
            .provider_usage_month(&day[..7], id)
            .await
            .unwrap_or((u32::MAX, u32::MAX));
        let request_limit = if matches!(id, "chainstack" | "drpc") {
            daily.min(30_000u32.saturating_sub(monthly_requests))
        } else {
            daily
        };
        let budget = if id == "blockscout" {
            let used = match store.provider_usage(&day).await {
                Ok(rows) => rows
                    .into_iter()
                    .find(|u| u.provider == id)
                    .map_or(0, |u| u.credits),
                Err(_) => u32::MAX,
            };
            Budget::limited_with_credits(request_limit, 80_000u32.saturating_sub(used))
        } else {
            Budget::limited(request_limit)
        };
        if let Ok(p) = Reserve::new(id, key.as_deref().map_or("", |k| k.as_str()), budget) {
            providers.reserves.push(p);
        }
    }
    // Prefer keyed reserves before the public node, which is the final option.
    providers.reserves.sort_by_key(|p| match p.provider() {
        "blockscout" => 0,
        "etherscan" => 1,
        "toncenter" => 2,
        "drpc" => 3,
        "chainstack" => 4,
        _ => 5,
    });
    if let Some(key) = credential(secrets, zerion::PROVIDER, "ZERION_API_KEY") {
        let budget = remaining_today(store, zerion::PROVIDER, ZERION_DAILY_BUDGET).await;
        providers.zerion = Zerion::new(zerion::DEFAULT_BASE, &key, Budget::limited(budget)).ok();
    }
    for (id, var, limit) in [
        (helius::PROVIDER, "HELIUS_API_KEY", HELIUS_DAILY_CREDITS),
        (alchemy::PROVIDER, "ALCHEMY_API_KEY", ALCHEMY_DAILY_CU),
    ] {
        if let Some(key) = credential(secrets, id, var) {
            let used = store
                .provider_usage(&utc_day(SystemClock.now()))
                .await
                .ok()
                .and_then(|u| u.into_iter().find(|u| u.provider == id))
                .map_or(0, |u| u.credits);
            let budget = Budget::limited_with_credits(
                remaining_today(store, id, 5_000).await,
                limit.saturating_sub(used),
            );
            if id == helius::PROVIDER {
                providers.helius = Helius::new(&key, budget).ok();
            } else {
                providers.alchemy = Alchemy::new(&key, budget).ok();
            }
        }
    }
    if let Some(key) = credential(secrets, trongrid::PROVIDER, "TRONGRID_API_KEY") {
        let budget = remaining_today(store, trongrid::PROVIDER, TRONGRID_DAILY_BUDGET).await;
        providers.trongrid =
            TronGrid::new(trongrid::DEFAULT_BASE, &key, Budget::limited(budget)).ok();
    }
    // TonAPI works anonymously at a stricter rate; a free key is recommended.
    let ton_key = credential(secrets, tonapi::PROVIDER, "TONAPI_API_KEY");
    let budget = remaining_today(store, tonapi::PROVIDER, TONAPI_DAILY_BUDGET).await;
    providers.tonapi = TonApi::new(
        tonapi::DEFAULT_BASE,
        ton_key.as_deref().map_or("", |k| k.as_str()),
        Budget::limited(budget),
    )
    .ok();
    if let Some(key) = credential(secrets, livecoinwatch::PROVIDER, "LIVECOINWATCH_API_KEY") {
        let budget = remaining_today(store, livecoinwatch::PROVIDER, LCW_DAILY_BUDGET).await;
        providers.livecoinwatch =
            LiveCoinWatch::new(livecoinwatch::DEFAULT_BASE, &key, Budget::limited(budget)).ok();
    }
    providers.set_network_log(log);
    providers
}

fn notify(app: &AppHandle) {
    if let Err(e) = app.emit(DATA_CHANGED_EVENT, ()) {
        tracing::warn!(error = %e, "cannot notify the UI");
    }
}

/// Runs one synchronization (all accounts, or one) followed by a price refresh.
pub async fn run(app: &AppHandle, account_id: Option<&str>) -> CommandResult<SyncSummary> {
    run_kind(app, account_id, true).await
}
async fn run_kind(
    app: &AppHandle,
    account_id: Option<&str>,
    retry_transient: bool,
) -> CommandResult<SyncSummary> {
    let state = app.state::<AppState>();
    let _guard = state
        .sync
        .run_lock
        .try_lock()
        .map_err(|_| CommandError::new("sync_busy", "Synchronization is already running."))?;
    let store = state.store().await;
    if store.profile() != ProfileKind::Real {
        return Err(CommandError::new(
            "demo_profile",
            "The demo portfolio never synchronizes with data providers.",
        ));
    }
    state.sync.cancelled.store(false, Ordering::Relaxed);
    let mut run_status = RunStatus::begin(&state.sync, "preparing");
    let total = if account_id.is_some() {
        1
    } else {
        store
            .list_accounts(None)
            .await?
            .iter()
            .filter(|a| !a.archived)
            .count() as u32
    };
    state
        .sync
        .details
        .lock()
        .expect("sync details")
        .total_accounts = total;
    let details = state.sync.details.clone();
    let engine = SyncEngine::new(
        store.clone(),
        build_providers(
            &store,
            state.secrets.as_ref(),
            state.sync.network_log.clone(),
        )
        .await,
        SyncOptions::default(),
    )
    .with_cancellation(state.sync.cancelled.clone())
    .with_transient_retry(retry_transient)
    .with_progress(Arc::new(move |account, pages, done| {
        let mut p = details.lock().expect("sync details");
        p.active_account = Some(account.to_owned());
        p.pages_fetched += pages;
        if done {
            p.completed_accounts += 1;
        }
    }));
    state.sync.details.lock().expect("sync details").phase = "accounts".into();
    let accounts = match account_id {
        Some(id) => {
            let account = store.account(id).await?;
            vec![engine.sync_account(&account).await]
        }
        None => {
            let reports = engine.sync_all().await;
            state
                .sync
                .last_sweep_at
                .store(SystemClock.now(), Ordering::Relaxed);
            reports
        }
    };
    notify(app);
    state.sync.details.lock().expect("sync details").phase = "prices".into();
    let prices = engine.refresh_prices().await;
    state
        .sync
        .last_prices_at
        .store(SystemClock.now(), Ordering::Relaxed);
    notify(app);
    state.sync.details.lock().expect("sync details").phase = "price_history".into();
    let price_history = engine.refresh_price_history().await;
    state.sync.details.lock().expect("sync details").phase = "accounting".into();
    let (accounting, accounting_error) = match store.replay_if_dirty().await {
        Ok(r) => (r, None),
        Err(e) => {
            tracing::error!(error = %e, "accounting replay failed");
            (None, Some(e.to_string()))
        }
    };
    notify(app);
    for report in &accounts {
        if let Some(error) = &report.error {
            tracing::warn!(account = %report.account_id, %error, "synchronization incomplete");
        }
    }
    let summary = SyncSummary {
        accounts,
        prices,
        price_history,
        accounting,
        accounting_error,
    };
    run_status.finish(
        summary_outcome(&summary, state.sync.cancelled.load(Ordering::Relaxed)),
        summary
            .accounts
            .iter()
            .filter(|a| a.error.is_some())
            .count()
            + summary.prices.errors.len()
            + summary.price_history.errors.len()
            + usize::from(summary.accounting_error.is_some()),
    );
    Ok(summary)
}

/// Refreshes prices only (between sweeps).
async fn refresh_prices(app: &AppHandle) {
    let state = app.state::<AppState>();
    let Ok(_guard) = state.sync.run_lock.try_lock() else {
        return; // a sweep is running and refreshes prices itself
    };
    let store = state.store().await;
    if store.profile() != ProfileKind::Real {
        return;
    }
    state.sync.cancelled.store(false, Ordering::Relaxed);
    let mut run_status = RunStatus::begin(&state.sync, "prices");
    let engine = SyncEngine::new(
        store.clone(),
        build_providers(
            &store,
            state.secrets.as_ref(),
            state.sync.network_log.clone(),
        )
        .await,
        SyncOptions::default(),
    )
    .with_cancellation(state.sync.cancelled.clone());
    let report = engine.refresh_prices().await;
    state
        .sync
        .last_prices_at
        .store(SystemClock.now(), Ordering::Relaxed);
    if report.priced > 0 {
        notify(app);
    }
    run_status.finish(
        if state.sync.cancelled.load(Ordering::Relaxed) {
            "cancelled"
        } else if !report.errors.is_empty() {
            "errors"
        } else if !report.unpriced.is_empty() {
            "partial"
        } else {
            "completed"
        },
        report.errors.len(),
    );
}

/// Starts the background schedule. The first sweep runs shortly after launch;
/// it is incremental, so an up-to-date portfolio costs a few requests.
pub fn spawn_scheduler(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(3)).await;
        loop {
            tick(&app).await;
            tokio::time::sleep(TICK).await;
        }
    });
}

async fn tick(app: &AppHandle) {
    let state = app.state::<AppState>();
    let store = state.store().await;
    if store.profile() != ProfileKind::Real {
        return;
    }
    let Ok(settings) = store.get_settings().await else {
        return;
    };
    let Ok(accounts) = store.list_accounts(None).await else {
        return;
    };
    if accounts.iter().all(|a| a.archived) {
        return;
    }
    let now = SystemClock.now();
    let sweep_due = now - state.sync.last_sweep_at.load(Ordering::Relaxed)
        >= i64::from(settings.sweep_interval_minutes.max(5)) * 60;
    let prices_due = now - state.sync.last_prices_at.load(Ordering::Relaxed)
        >= i64::from(settings.price_refresh_seconds.max(30));
    if sweep_due {
        if let Err(e) = run_kind(app, None, false).await {
            tracing::warn!(error = %e.message, "scheduled sweep skipped");
        }
    } else if prices_due {
        refresh_prices(app).await;
    }
}

/// Synchronizes a newly added account in the background.
pub fn spawn_account_sync(app: AppHandle, account_id: String) {
    tauri::async_runtime::spawn(async move {
        if let Err(e) = run_kind(&app, Some(&account_id), false).await {
            tracing::warn!(error = %e.message, "initial account sync skipped");
        }
    });
}

pub fn shared() -> Arc<SyncState> {
    Arc::new(SyncState::default())
}

#[derive(Debug, Clone, Default, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SyncDetails {
    pub active_account: Option<String>,
    pub completed_accounts: u32,
    pub total_accounts: u32,
    pub pages_fetched: u32,
    pub phase: String,
    pub kind: String,
    pub outcome: String,
    pub error_count: u32,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
}

/// Keeps final results visible for background runs and guarantees an error
/// state when an early return or panic unwinds a run.
struct RunStatus<'a> {
    state: &'a SyncState,
    finished: bool,
}
impl<'a> RunStatus<'a> {
    fn begin(state: &'a SyncState, phase: &str) -> Self {
        *state.details.lock().expect("sync details") = SyncDetails {
            phase: phase.into(),
            kind: if phase == "prices" {
                "prices"
            } else {
                "accounts"
            }
            .into(),
            outcome: "running".into(),
            started_at: Some(SystemClock.now()),
            ..SyncDetails::default()
        };
        Self {
            state,
            finished: false,
        }
    }
    fn finish(&mut self, outcome: &str, errors: usize) {
        let mut details = self.state.details.lock().expect("sync details");
        details.phase = "idle".into();
        details.active_account = None;
        details.outcome = outcome.into();
        details.error_count = u32::try_from(errors).unwrap_or(u32::MAX);
        details.finished_at = Some(SystemClock.now());
        self.finished = true;
    }
}
impl Drop for RunStatus<'_> {
    fn drop(&mut self) {
        if !self.finished {
            self.finish("failed", 1);
        }
    }
}
fn summary_outcome(summary: &SyncSummary, cancelled: bool) -> &'static str {
    if cancelled {
        "cancelled"
    } else if summary.accounting_error.is_some() {
        "failed"
    } else if summary.accounts.iter().any(|a| a.error.is_some())
        || !summary.prices.errors.is_empty()
        || !summary.price_history.errors.is_empty()
    {
        "errors"
    } else if summary
        .accounts
        .iter()
        .any(|a| a.coverage != portfolio_store::Coverage::Complete)
        || !summary.prices.unpriced.is_empty()
        || summary.price_history.pending_assets > 0
        || !summary.price_history.unavailable.is_empty()
    {
        "partial"
    } else {
        "completed"
    }
}

pub(crate) fn probe_guard(state: &SyncState) -> CommandResult<tokio::sync::MutexGuard<'_, ()>> {
    state
        .run_lock
        .try_lock()
        .map_err(|_| CommandError::new("sync_busy", "Synchronization is already running."))
}
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SyncProgress {
    pub running: bool,
    pub cancel_requested: bool,
    pub details: SyncDetails,
}
pub fn progress(state: &SyncState) -> SyncProgress {
    let details = state.details.lock().expect("sync details").clone();
    SyncProgress {
        running: details.outcome == "running",
        cancel_requested: state.cancelled.load(Ordering::Relaxed),
        details,
    }
}

/// One small read against the selected adapter; never starts a history import.
pub async fn test_provider(
    state: &SyncState,
    store: &Store,
    secrets: &dyn SecretStore,
    id: &str,
) -> CommandResult<()> {
    let _guard = probe_guard(state)?;
    let p = build_providers(store, secrets, state.network_log.clone()).await;
    let missing = || CommandError::new("missing_key", "Configure this provider's key first.");
    let reserve = p.reserves.iter().find(|p| p.provider() == id);
    let outcome = match id {
        "blockscout" | "etherscan" | "drpc" | "publicnode" | "chainstack" | "toncenter" => {
            let api = reserve.ok_or_else(missing)?;
            let (network, address) = match id {
                "chainstack" => (
                    portfolio_core::network::NetworkId::Solana,
                    "Vote111111111111111111111111111111111111111",
                ),
                "toncenter" => (
                    portfolio_core::network::NetworkId::Ton,
                    "0:0000000000000000000000000000000000000000000000000000000000000000",
                ),
                _ => (
                    portfolio_core::network::NetworkId::Ethereum,
                    "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045",
                ),
            };
            api.snapshot(network, address, &[]).await.map(|_| ())
        }
        "mempool" => p
            .mempool
            .as_ref()
            .ok_or_else(missing)?
            .tip_height()
            .await
            .map(|_| ()),
        "helius" => p
            .helius
            .as_ref()
            .ok_or_else(missing)?
            .slot()
            .await
            .map(|_| ()),
        "alchemy" => {
            p.alchemy
                .as_ref()
                .ok_or_else(missing)?
                .check_chain(portfolio_core::network::NetworkId::Ethereum)
                .await
        }
        "livecoinwatch" => p
            .livecoinwatch
            .as_ref()
            .ok_or_else(missing)?
            .credits()
            .await
            .map(|_| ()),
        "esplora" => p
            .esplora
            .as_ref()
            .ok_or_else(missing)?
            .tip_height()
            .await
            .map(|_| ()),
        "zerion" => p
            .zerion
            .as_ref()
            .ok_or_else(missing)?
            .transactions(
                portfolio_core::network::NetworkId::Ethereum,
                "0x1db3439a222c519ab44bb1144fc28167b4fa6ee6",
                None,
                1,
                portfolio_providers::zerion::Window::default(),
            )
            .await
            .map(|_| ()),
        "defillama" => p
            .defillama
            .as_ref()
            .ok_or_else(missing)?
            .current(&["coingecko:bitcoin".into()])
            .await
            .map(|_| ()),
        "trongrid" => p
            .trongrid
            .as_ref()
            .ok_or_else(missing)?
            .account("TT2T17KZhoDu47i2E4FWxfG79zdkEWkU9N")
            .await
            .map(|_| ()),
        "tonapi" => p
            .tonapi
            .as_ref()
            .ok_or_else(missing)?
            .holdings("0:0000000000000000000000000000000000000000000000000000000000000000")
            .await
            .map(|_| ()),
        _ => {
            return Err(CommandError::new(
                "unsupported",
                "Connection check is unavailable for this provider.",
            ));
        }
    };
    let engine = SyncEngine::new(store.clone(), p, SyncOptions::default());
    engine.flush_usage().await?;
    outcome.map_err(|e| CommandError::new("provider", e.to_string()))
}

#[cfg(test)]
mod progress_tests {
    use super::*;

    fn summary() -> SyncSummary {
        SyncSummary {
            accounts: vec![],
            prices: PriceReport::default(),
            price_history: PriceHistoryReport::default(),
            accounting: None,
            accounting_error: None,
        }
    }
    #[test]
    fn results_distinguish_partial_errors_cancellation_and_early_failure() {
        let mut s = summary();
        assert_eq!(summary_outcome(&s, false), "completed");
        s.price_history.pending_assets = 2;
        assert_eq!(summary_outcome(&s, false), "partial");
        s.prices.errors.push("rate limited".into());
        assert_eq!(summary_outcome(&s, false), "errors");
        s.accounting_error = Some("replay failed".into());
        assert_eq!(summary_outcome(&s, false), "failed");
        assert_eq!(summary_outcome(&s, true), "cancelled");
        let state = SyncState::default();
        {
            let _run = RunStatus::begin(&state, "accounts");
            assert!(progress(&state).running);
        }
        let ended = progress(&state);
        assert!(!ended.running);
        assert_eq!(ended.details.outcome, "failed");
        assert!(ended.details.finished_at.is_some());
    }
    #[tokio::test]
    async fn connection_probe_never_waits_on_an_already_held_run_lock() {
        struct EmptySecrets;
        impl SecretStore for EmptySecrets {
            fn save(
                &self,
                _: &str,
                _: crate::secrets::Secret,
            ) -> Result<crate::secrets::KeyStorage, String> {
                unreachable!()
            }
            fn remove(&self, _: &str) -> Result<(), String> {
                unreachable!()
            }
            fn status(&self, _: &str) -> Option<crate::secrets::KeyStorage> {
                None
            }
            fn get(&self, _: &str) -> Option<crate::secrets::Secret> {
                None
            }
        }
        let state = SyncState::default();
        let guard = probe_guard(&state).unwrap();
        assert!(probe_guard(&state).is_err());
        drop(guard);
        assert!(probe_guard(&state).is_ok());
        let store = Store::open_in_memory(
            ProfileKind::Test,
            Arc::new(portfolio_core::clock::FixedClock(1)),
        )
        .await
        .unwrap();
        // Unknown provider never makes a network call. The actual probe must
        // return immediately instead of acquiring its held run lock again.
        let result = tokio::time::timeout(
            Duration::from_secs(1),
            test_provider(&state, &store, &EmptySecrets, "unknown"),
        )
        .await
        .unwrap();
        assert_eq!(result.unwrap_err().code, "unsupported");
    }
}
