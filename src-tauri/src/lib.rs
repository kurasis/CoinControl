//! Portfolio Desk desktop shell: window lifecycle, typed IPC commands, and
//! platform adapters. Domain logic lives in `portfolio-core`; persistence in
//! `portfolio-store`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use portfolio_core::clock::SystemClock;
use portfolio_store::{ProfileKind, Store};
use tauri::Manager;
use tokio::sync::RwLock;

mod commands;
mod error;
mod providers;
mod secrets;
mod sync;
mod window_geometry;

use secrets::{OsSecretStore, SecretStore};

pub struct AppState {
    store: RwLock<Store>,
    profiles_dir: PathBuf,
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
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,sqlx=warn".into()),
        )
        .init();

    let context = tauri::generate_context!();
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

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::ScaleFactorChanged { .. }) {
                window_geometry::fit_main(window, true);
            } else if matches!(event, tauri::WindowEvent::Resized(_)) {
                // Native size/minimum changes are queued; constrain the final decorated bounds.
                window_geometry::fit_main(window, false);
            }
        })
        .setup(|app| {
            window_geometry::fit_main(app, true);
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
            app.manage(AppState {
                store: RwLock::new(store),
                profiles_dir,
                secrets: Arc::new(OsSecretStore::default()),
                sync: sync::shared(),
            });
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
            commands::get_asset_chart,
            commands::list_review_items,
            commands::get_leg_detail,
            commands::update_basis,
            commands::preview_basis_import,
            commands::commit_basis_import,
            commands::discard_basis_import,
            commands::replay_accounting,
            commands::list_providers,
            commands::save_provider_key,
            commands::remove_provider_key,
            commands::sync_now,
            commands::list_sync_status,
            commands::cancel_sync,
            commands::get_sync_progress,
            commands::test_provider,
            commands::export_backup,
            commands::inspect_backup,
            commands::restore_backup,
            commands::export_csv,
        ])
        .run(context)
        .expect("error while running Portfolio Desk");
}
