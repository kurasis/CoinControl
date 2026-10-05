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
use portfolio_providers::tonapi::{self, TonApi};
use portfolio_providers::trongrid::{self, TronGrid};
use portfolio_providers::zerion::{self, Zerion};
use portfolio_providers::{
    AccountSyncReport, PriceHistoryReport, PriceReport, Providers, SyncEngine, SyncOptions,
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
const TICK: Duration = Duration::from_secs(15);

/// Scheduler bookkeeping shared by the background loop and commands.
#[derive(Default)]
pub struct SyncState {
    pub(crate) run_lock: tokio::sync::Mutex<()>,
    pub(crate) cancelled: Arc<AtomicBool>,
    pub(crate) running: AtomicBool,
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

async fn remaining_today(store: &Store, provider: &str, limit: u32) -> u32 {
    let used = store
        .provider_usage(&utc_day(SystemClock.now()))
        .await
        .ok()
        .and_then(|u| u.into_iter().find(|p| p.provider == provider))
        .map_or(0, |p| p.requests);
    limit.saturating_sub(used)
}

async fn build_providers(store: &Store, secrets: &dyn SecretStore) -> Providers {
    let mut providers = Providers {
        esplora: Esplora::new(esplora::DEFAULT_BASE, Budget::unlimited()).ok(),
        defillama: DefiLlama::new(defillama::DEFAULT_BASE, Budget::unlimited()).ok(),
        ..Providers::default()
    };
    if let Some(key) = credential(secrets, zerion::PROVIDER, "ZERION_API_KEY") {
        let budget = remaining_today(store, zerion::PROVIDER, ZERION_DAILY_BUDGET).await;
        providers.zerion = Zerion::new(zerion::DEFAULT_BASE, &key, Budget::limited(budget)).ok();
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
    providers
}

fn notify(app: &AppHandle) {
    if let Err(e) = app.emit(DATA_CHANGED_EVENT, ()) {
        tracing::warn!(error = %e, "cannot notify the UI");
    }
}

/// Runs one synchronization (all accounts, or one) followed by a price refresh.
pub async fn run(app: &AppHandle, account_id: Option<&str>) -> CommandResult<SyncSummary> {
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
    state.sync.running.store(true, Ordering::Relaxed);
    struct RunningGuard<'a>(&'a AtomicBool);
    impl Drop for RunningGuard<'_> {
        fn drop(&mut self) {
            self.0.store(false, Ordering::Relaxed);
        }
    }
    let _running = RunningGuard(&state.sync.running);
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
    *state.sync.details.lock().expect("sync details") = SyncDetails {
        total_accounts: total,
        phase: "accounts".into(),
        ..SyncDetails::default()
    };
    let details = state.sync.details.clone();
    let engine = SyncEngine::new(
        store.clone(),
        build_providers(&store, state.secrets.as_ref()).await,
        SyncOptions::default(),
    )
    .with_cancellation(state.sync.cancelled.clone())
    .with_progress(Arc::new(move |account, pages, done| {
        let mut p = details.lock().expect("sync details");
        p.active_account = Some(account.to_owned());
        p.pages_fetched += pages;
        if done {
            p.completed_accounts += 1;
        }
    }));
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
    Ok(SyncSummary {
        accounts,
        prices,
        price_history,
        accounting,
        accounting_error,
    })
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
    let engine = SyncEngine::new(
        store.clone(),
        build_providers(&store, state.secrets.as_ref()).await,
        SyncOptions::default(),
    );
    let report = engine.refresh_prices().await;
    state
        .sync
        .last_prices_at
        .store(SystemClock.now(), Ordering::Relaxed);
    if report.priced > 0 {
        notify(app);
    }
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
        if let Err(e) = run(app, None).await {
            tracing::warn!(error = %e.message, "scheduled sweep skipped");
        }
    } else if prices_due {
        refresh_prices(app).await;
    }
}

/// Synchronizes a newly added account in the background.
pub fn spawn_account_sync(app: AppHandle, account_id: String) {
    tauri::async_runtime::spawn(async move {
        if let Err(e) = run(&app, Some(&account_id)).await {
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
}
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SyncProgress {
    pub running: bool,
    pub cancel_requested: bool,
    pub details: SyncDetails,
}
pub fn progress(state: &SyncState) -> SyncProgress {
    SyncProgress {
        running: state.running.load(Ordering::Relaxed),
        cancel_requested: state.cancelled.load(Ordering::Relaxed),
        details: state.details.lock().expect("sync details").clone(),
    }
}

/// One small read against the selected adapter; never starts a history import.
pub async fn test_provider(
    store: &Store,
    secrets: &dyn SecretStore,
    id: &str,
) -> CommandResult<()> {
    let p = build_providers(store, secrets).await;
    let missing = || CommandError::new("missing_key", "Configure this provider's key first.");
    let outcome = match id {
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
