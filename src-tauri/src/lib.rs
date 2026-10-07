//! Portfolio Desk desktop shell: window lifecycle, typed IPC commands, and
//! platform adapters. Domain logic lives in `portfolio-core`; persistence in
//! `portfolio-store`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use portfolio_core::clock::SystemClock;
use portfolio_store::{ProfileKind, Scope, Store};
use tauri::Manager;
use tokio::sync::RwLock;

mod commands;
mod error;
mod jobs;
mod providers;
mod schedule;
mod secrets;
mod sync;
mod window_geometry;

use secrets::{OsSecretStore, SecretStore};

pub struct AppState {
    store: Arc<RwLock<Store>>,
    profiles_dir: PathBuf,
    icons: portfolio_providers::icons::IconCache,
    secrets: Arc<dyn SecretStore>,
    sync: Arc<sync::SyncState>,
}

impl AppState {
    /// A cheap handle to the currently open profile.
    pub async fn store(&self) -> Store {
        self.store.read().await.clone()
    }
}

pub fn profile_path(dir: &Path, kind: ProfileKind) -> PathBuf {
    dir.join(format!("{}.sqlite", kind.as_str()))
}

pub async fn open_profile(
    dir: &Path,
    kind: ProfileKind,
) -> Result<Store, portfolio_store::StoreError> {
    let store = Store::open(&profile_path(dir, kind), kind, Arc::new(SystemClock)).await?;
    if kind == ProfileKind::Demo {
        store.seed_demo().await?;
    }
    // Bring derived accounting up to date (first start after an upgrade, or
    // a sync interrupted before its replay).
    store.replay_if_dirty().await?;
    Ok(store)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let startup = std::time::Instant::now();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,sqlx=warn".into()),
        )
        .init();

    let context = tauri::generate_context!();
    tracing::info!(
        elapsed_ms = startup.elapsed().as_millis(),
        "Startup context ready"
    );
    #[cfg(feature = "native-e2e")]
    let context = {
        let mut context = context;
        if let Some(root) = std::env::var_os("COINCONTROL_E2E_DATA_DIR") {
            for window in &mut context.config_mut().app.windows {
                window.data_directory = Some(PathBuf::from(&root).join("webview"));
                if let Some(port) = std::env::var("COINCONTROL_E2E_DEBUG_PORT")
                    .ok()
                    .and_then(|value| value.parse::<u16>().ok())
                    .filter(|port| *port > 0)
                {
                    window.additional_browser_args = Some(format!(
                        "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --remote-debugging-port={port}"
                    ));
                }
            }
        }
        context
    };

    // Initialize local state and its cached summary while WebView2 creates the
    // main window, instead of putting both cold operations on the critical path.
    let main_window_config = context
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == "main")
        .cloned()
        .expect("main window configuration");
    let mut context = context;
    for window in &mut context.config_mut().app.windows {
        if window.label == "main" {
            window.create = false;
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::ScaleFactorChanged { .. }) {
                window_geometry::fit_after_scale(window);
            } else if matches!(event, tauri::WindowEvent::Resized(_)) {
                // Native size/minimum changes are queued; constrain the final decorated bounds.
                window_geometry::fit_main(window, false);
            }
        })
        .setup(move |app| {
            let data_dir = app.path().app_data_dir()?;
            #[cfg(feature = "native-e2e")]
            let data_dir = std::env::var_os("COINCONTROL_E2E_DATA_DIR")
                .map(PathBuf::from)
                .unwrap_or(data_dir);
            let profiles_dir = data_dir.join("profiles");
            std::fs::create_dir_all(&profiles_dir)?;
            let store =
                tauri::async_runtime::block_on(open_profile(&profiles_dir, ProfileKind::Real))
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            tracing::info!(
                elapsed_ms = startup.elapsed().as_millis(),
                "Startup local profile ready"
            );
            let sync_state = sync::shared();
            sync_state.network_log.set_enabled(
                tauri::async_runtime::block_on(store.get_settings())?.network_console_enabled,
            );
            let store = Arc::new(RwLock::new(store));
            // Gate initial IPC reads until this one calculation completes. An
            // owned guard lets the UI thread continue creating/painting its
            // window and prevents duplicate cold summary calculations.
            let priming_store = tauri::async_runtime::block_on(store.clone().write_owned());
            tauri::async_runtime::spawn(async move {
                if priming_store.portfolio_summary(&Scope::All).await.is_ok() {
                    tracing::info!(
                        elapsed_ms = startup.elapsed().as_millis(),
                        "Startup cached summary ready"
                    );
                } else {
                    tracing::warn!("Startup summary prewarm unavailable");
                }
                // Dropping the guard releases queued IPC; errors remain visible
                // through the normal summary command rather than fabricated data.
            });
            app.manage(AppState {
                store,
                profiles_dir,
                secrets: Arc::new(OsSecretStore::default()),
                icons: portfolio_providers::icons::IconCache::new(
                    data_dir.join("cache/token-icons"),
                    sync_state.network_log.clone(),
                ),
                sync: sync_state,
            });
            tauri::WebviewWindowBuilder::from_config(app, &main_window_config)?.build()?;
            tracing::info!(
                elapsed_ms = startup.elapsed().as_millis(),
                "Startup native WebView ready"
            );
            window_geometry::fit_main(app, true);
            sync::spawn_scheduler(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::switch_profile,
            commands::get_settings,
            commands::update_settings,
            commands::list_networks,
            commands::list_network_capabilities,
            commands::validate_address,
            commands::create_wallet,
            commands::list_wallets,
            commands::rename_wallet,
            commands::add_accounts,
            commands::list_asset_policies,
            commands::set_asset_policy,
            commands::add_account,
            commands::list_accounts,
            commands::set_account_archived,
            commands::move_account,
            commands::create_group,
            commands::list_groups,
            commands::set_group_wallets,
            commands::delete_group,
            commands::list_holdings,
            commands::get_portfolio_summary,
            commands::get_chart,
            commands::list_activity,
            commands::get_asset_detail,
            commands::get_asset_icon,
            commands::get_asset_chart,
            commands::list_review_items,
            commands::get_leg_detail,
            commands::update_basis,
            commands::preview_basis_import,
            commands::commit_basis_import,
            commands::discard_basis_import,
            commands::replay_accounting,
            commands::preview_account_removal,
            commands::remove_account,
            commands::clear_caches,
            commands::rescan_accounts,
            commands::start_rescan,
            commands::start_sync,
            commands::get_sync_job,
            commands::cancel_sync_job,
            commands::get_provider_quota,
            commands::set_provider_quota,
            commands::account_explorer,
            commands::export_diagnostics,
            commands::set_sync_scope,
            commands::account_coverage,
            commands::remove_empty_wallet,
            commands::list_providers,
            commands::save_provider_key,
            commands::remove_provider_key,
            commands::sync_now,
            commands::list_sync_status,
            commands::cancel_sync,
            commands::get_sync_progress,
            commands::get_network_log,
            commands::clear_network_log,
            commands::test_provider,
            commands::export_backup,
            commands::inspect_backup,
            commands::restore_backup,
            commands::export_csv,
        ])
        .run(context)
        .expect("error while running Portfolio Desk");
}
