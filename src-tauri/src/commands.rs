//! Typed IPC commands. This is the complete frontend-reachable surface: there
//! is no generic URL fetch, RPC, SQL, shell, or key readback command.

use portfolio_core::accounting::ACCOUNTING_ENGINE_VERSION;
use portfolio_core::address::{NormalizedAddress, normalize_address};
use portfolio_core::clock::{Clock, SystemClock, utc_day};
use portfolio_core::network::{NetworkFamily, NetworkId};
use portfolio_providers::capabilities::NetworkCapability;
use std::collections::BTreeMap;

use portfolio_store::{
    Account, AccountSyncStatus, ActivityFilter, ActivityPage, AssetChart, AssetDetail, ChartRange,
    ChartSeries, Group, HoldingRow, ImportPreview, ImportResult, LegDetail, LegOverride,
    PortfolioSummary, ProfileKind, ReplayReport, ReviewList, Scope, Settings, Wallet,
};
use serde::Serialize;
use tauri::State;

use crate::error::{CommandError, CommandResult};
use crate::providers::{self, ProviderStatus};
use crate::secrets::Secret;
use crate::sync::{self, SyncSummary};
use crate::{AppState, open_profile};

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AppInfo {
    pub product_name: String,
    pub version: String,
    pub profile: ProfileKind,
    pub schema_version: i64,
    pub accounting_engine_version: u32,
    pub data_directory: String,
}

#[tauri::command]
pub async fn app_info(app: tauri::AppHandle, state: State<'_, AppState>) -> CommandResult<AppInfo> {
    let store = state.store().await;
    Ok(AppInfo {
        product_name: app.package_info().name.clone(),
        version: app.package_info().version.to_string(),
        profile: store.profile(),
        schema_version: store.schema_version().await?,
        accounting_engine_version: ACCOUNTING_ENGINE_VERSION,
        data_directory: state.profiles_dir.display().to_string(),
    })
}

/// Switches between the real portfolio and the separate demo database.
#[tauri::command]
pub async fn switch_profile(
    state: State<'_, AppState>,
    profile: ProfileKind,
) -> CommandResult<ProfileKind> {
    if profile == ProfileKind::Test {
        return Err(CommandError::new(
            "invalid_input",
            "The test profile is not available in the app.",
        ));
    }
    let _sync_guard = state.sync.run_lock.lock().await;
    let mut guard = state.store.write().await;
    if guard.profile() == profile {
        return Ok(profile);
    }
    let next = open_profile(&state.profiles_dir, profile).await?;
    let previous = std::mem::replace(&mut *guard, next);
    state.sync.network_log.clear();
    state
        .sync
        .network_log
        .set_enabled(guard.get_settings().await?.network_console_enabled);
    previous.close().await;
    Ok(profile)
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> CommandResult<Settings> {
    Ok(state.store().await.get_settings().await?)
}

#[tauri::command]
pub async fn update_settings(
    state: State<'_, AppState>,
    settings: Settings,
) -> CommandResult<Settings> {
    let saved = state.store().await.update_settings(&settings).await?;
    state
        .sync
        .network_log
        .set_enabled(saved.network_console_enabled);
    Ok(saved)
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct NetworkInfo {
    pub id: NetworkId,
    pub name: String,
    pub family: NetworkFamily,
    pub native_symbol: String,
    pub evm_chain_id: Option<u32>,
}

#[tauri::command]
pub fn list_networks() -> Vec<NetworkInfo> {
    NetworkId::ALL
        .iter()
        .map(|n| NetworkInfo {
            id: *n,
            name: n.display_name().into(),
            family: n.family(),
            native_symbol: n.native_symbol().into(),
            evm_chain_id: n.evm_chain_id().and_then(|id| u32::try_from(id).ok()),
        })
        .collect()
}

/// What this build reads for each required network (capability report).
#[tauri::command]
pub fn list_network_capabilities(state: State<'_, AppState>) -> Vec<NetworkCapability> {
    sync::network_capabilities(state.secrets.as_ref())
}

/// Local-only validation; never contacts a provider.
#[tauri::command]
pub fn validate_address(network: NetworkId, address: String) -> CommandResult<NormalizedAddress> {
    Ok(normalize_address(network, &address)?)
}

#[tauri::command]
pub async fn create_wallet(state: State<'_, AppState>, label: String) -> CommandResult<Wallet> {
    Ok(state.store().await.create_wallet(&label).await?)
}

#[tauri::command]
pub async fn list_wallets(state: State<'_, AppState>) -> CommandResult<Vec<Wallet>> {
    Ok(state.store().await.list_wallets().await?)
}

#[tauri::command]
pub async fn rename_wallet(
    state: State<'_, AppState>,
    id: String,
    label: String,
) -> CommandResult<()> {
    Ok(state.store().await.rename_wallet(&id, &label).await?)
}

/// Adds an address and starts its first synchronization in the background.
#[tauri::command]
pub async fn add_account(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    wallet_id: String,
    network: NetworkId,
    address: String,
    label: Option<String>,
) -> CommandResult<Account> {
    let store = state.store().await;
    let account = store
        .add_account(&wallet_id, network, &address, label.as_deref())
        .await?;
    if store.profile() == ProfileKind::Real {
        sync::spawn_account_sync(app, account.id.clone());
    }
    Ok(account)
}

#[tauri::command]
pub async fn add_accounts(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    wallet_id: String,
    network: NetworkId,
    addresses: Vec<String>,
) -> CommandResult<Vec<Account>> {
    let store = state.store().await;
    let accounts = store
        .add_accounts(&wallet_id, network, &addresses, None)
        .await?;
    if store.profile() == ProfileKind::Real {
        tauri::async_runtime::spawn(async move {
            let _ = sync::run(&app, None).await;
        });
    }
    Ok(accounts)
}

#[tauri::command]
pub async fn list_asset_policies(
    state: State<'_, AppState>,
) -> CommandResult<Vec<portfolio_store::AssetPolicy>> {
    Ok(state.store().await.list_asset_policies().await?)
}
#[tauri::command]
pub async fn set_asset_policy(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    asset_id: String,
    hidden: bool,
    exclude_override: Option<bool>,
) -> CommandResult<()> {
    let store = state.store().await;
    store
        .set_asset_policy(&asset_id, hidden, exclude_override)
        .await?;
    store.replay_accounting().await?;
    use tauri::Emitter;
    app.emit(sync::DATA_CHANGED_EVENT, ())
        .map_err(|_| CommandError::new("event", "Could not refresh asset policy."))?;
    Ok(())
}
#[tauri::command]
pub async fn list_accounts(
    state: State<'_, AppState>,
    wallet_id: Option<String>,
) -> CommandResult<Vec<Account>> {
    Ok(state
        .store()
        .await
        .list_accounts(wallet_id.as_deref())
        .await?)
}

#[tauri::command]
pub async fn set_account_archived(
    state: State<'_, AppState>,
    id: String,
    archived: bool,
) -> CommandResult<()> {
    Ok(state
        .store()
        .await
        .set_account_archived(&id, archived)
        .await?)
}

#[tauri::command]
pub async fn move_account(
    state: State<'_, AppState>,
    id: String,
    wallet_id: String,
) -> CommandResult<()> {
    Ok(state.store().await.move_account(&id, &wallet_id).await?)
}

#[tauri::command]
pub async fn create_group(state: State<'_, AppState>, label: String) -> CommandResult<Group> {
    Ok(state.store().await.create_group(&label).await?)
}

#[tauri::command]
pub async fn list_groups(state: State<'_, AppState>) -> CommandResult<Vec<Group>> {
    Ok(state.store().await.list_groups().await?)
}

#[tauri::command]
pub async fn set_group_wallets(
    state: State<'_, AppState>,
    group_id: String,
    wallet_ids: Vec<String>,
) -> CommandResult<()> {
    Ok(state
        .store()
        .await
        .set_group_wallets(&group_id, &wallet_ids)
        .await?)
}

#[tauri::command]
pub async fn delete_group(state: State<'_, AppState>, group_id: String) -> CommandResult<()> {
    Ok(state.store().await.delete_group(&group_id).await?)
}

#[tauri::command]
pub async fn list_holdings(
    state: State<'_, AppState>,
    scope: Scope,
) -> CommandResult<Vec<HoldingRow>> {
    Ok(state.store().await.list_holdings(&scope).await?)
}

#[tauri::command]
pub async fn get_portfolio_summary(
    state: State<'_, AppState>,
    scope: Scope,
) -> CommandResult<PortfolioSummary> {
    Ok(state.store().await.portfolio_summary(&scope).await?)
}

#[tauri::command]
pub async fn get_chart(
    state: State<'_, AppState>,
    scope: Scope,
    range: ChartRange,
    start: Option<i64>,
    end: Option<i64>,
) -> CommandResult<ChartSeries> {
    let window = match (start, end) {
        (None, None) => None,
        (Some(a), Some(b)) => Some((a, b)),
        _ => {
            return Err(CommandError::new(
                "invalid",
                "both chart dates are required",
            ));
        }
    };
    Ok(state
        .store()
        .await
        .get_chart_window(&scope, range, window)
        .await?)
}

#[tauri::command]
pub async fn list_activity(
    state: State<'_, AppState>,
    scope: Scope,
    cursor: Option<String>,
    limit: Option<u32>,
    filter: Option<ActivityFilter>,
) -> CommandResult<ActivityPage> {
    Ok(state
        .store()
        .await
        .list_activity(
            &scope,
            &filter.unwrap_or_default(),
            cursor.as_deref(),
            limit.unwrap_or(50),
        )
        .await?)
}

#[tauri::command]
pub async fn get_asset_detail(
    state: State<'_, AppState>,
    scope: Scope,
    asset_id: String,
) -> CommandResult<AssetDetail> {
    Ok(state.store().await.asset_detail(&scope, &asset_id).await?)
}

#[tauri::command]
pub async fn get_asset_chart(
    state: State<'_, AppState>,
    scope: Scope,
    asset_id: String,
    range: ChartRange,
) -> CommandResult<AssetChart> {
    Ok(state
        .store()
        .await
        .asset_chart(&scope, &asset_id, range)
        .await?)
}

#[tauri::command]
pub async fn list_review_items(
    state: State<'_, AppState>,
    scope: Scope,
    limit: Option<u32>,
) -> CommandResult<ReviewList> {
    Ok(state.store().await.list_review_items(&scope, limit).await?)
}

#[tauri::command]
pub async fn get_leg_detail(
    state: State<'_, AppState>,
    leg_id: String,
) -> CommandResult<LegDetail> {
    Ok(state.store().await.leg_detail(&leg_id).await?)
}

/// Saves a new version of the user's decision for one movement and replays
/// accounting. The original observation is never modified.
#[tauri::command]
pub async fn update_basis(
    state: State<'_, AppState>,
    leg_id: String,
    decision: LegOverride,
) -> CommandResult<ReplayReport> {
    Ok(state
        .store()
        .await
        .save_leg_override(&leg_id, &decision)
        .await?)
}

/// Parses a CSV basis file (sent as text by the file picker) into a preview.
#[tauri::command]
pub async fn preview_basis_import(
    state: State<'_, AppState>,
    file_name: String,
    content: String,
    mapping: Option<BTreeMap<String, String>>,
) -> CommandResult<ImportPreview> {
    Ok(state
        .store()
        .await
        .preview_basis_import(&file_name, &content, mapping)
        .await?)
}

#[tauri::command]
pub async fn commit_basis_import(
    state: State<'_, AppState>,
    batch_id: String,
) -> CommandResult<ImportResult> {
    Ok(state.store().await.commit_basis_import(&batch_id).await?)
}

#[tauri::command]
pub async fn discard_basis_import(
    state: State<'_, AppState>,
    batch_id: String,
) -> CommandResult<()> {
    Ok(state.store().await.discard_basis_import(&batch_id).await?)
}

/// Rebuilds derived accounting from stored evidence and decisions.
#[tauri::command]
pub async fn replay_accounting(state: State<'_, AppState>) -> CommandResult<ReplayReport> {
    Ok(state.store().await.replay_accounting().await?)
}

#[tauri::command]
pub async fn list_providers(state: State<'_, AppState>) -> CommandResult<Vec<ProviderStatus>> {
    let usage = state
        .store()
        .await
        .provider_usage(&utc_day(SystemClock.now()))
        .await?;
    Ok(providers::PROVIDERS
        .iter()
        .map(|spec| {
            let mut status = ProviderStatus::from_spec(spec, state.secrets.status(spec.id));
            if let Some(u) = usage.iter().find(|u| u.provider == spec.id) {
                status.requests_today = u.requests;
                status.estimated_credits_today = u.credits;
                status.last_error = u.last_error.clone();
            }
            status
        })
        .collect())
}

/// Stores a key. The response reports only where it was stored, never the key.
#[tauri::command]
pub async fn save_provider_key(
    state: State<'_, AppState>,
    provider: String,
    key: String,
) -> CommandResult<ProviderStatus> {
    let spec = providers::find(&provider)
        .ok_or_else(|| CommandError::new("not_found", "Unknown data source."))?;
    if matches!(
        spec.id,
        "chainstack" | "blockscout" | "etherscan" | "drpc" | "toncenter"
    ) {
        portfolio_providers::mirrors::Reserve::new(
            spec.id,
            &key,
            portfolio_providers::http::Budget::limited(0),
        )
        .map_err(|e| CommandError::new("provider", e.to_string()))?;
    }
    let storage = state
        .secrets
        .save(spec.id, Secret::new(key))
        .map_err(|m| CommandError::new("secret_store", m))?;
    state.store().await.clear_provider_cooldown(spec.id).await?;
    Ok(ProviderStatus::from_spec(spec, Some(storage)))
}

#[tauri::command]
pub async fn remove_provider_key(
    state: State<'_, AppState>,
    provider: String,
) -> CommandResult<ProviderStatus> {
    let spec = providers::find(&provider)
        .ok_or_else(|| CommandError::new("not_found", "Unknown data source."))?;
    state
        .secrets
        .remove(spec.id)
        .map_err(|m| CommandError::new("secret_store", m))?;
    state.store().await.clear_provider_cooldown(spec.id).await?;
    Ok(ProviderStatus::from_spec(spec, None))
}

/// Synchronizes one account (or every active account) now, then refreshes prices.
#[tauri::command]
pub async fn sync_now(
    app: tauri::AppHandle,
    account_id: Option<String>,
) -> CommandResult<SyncSummary> {
    sync::run(&app, account_id.as_deref()).await
}

#[tauri::command]
pub async fn list_sync_status(state: State<'_, AppState>) -> CommandResult<Vec<AccountSyncStatus>> {
    Ok(state.store().await.sync_status().await?)
}

#[tauri::command]
pub fn cancel_sync(state: State<'_, AppState>) {
    state
        .sync
        .cancelled
        .store(true, std::sync::atomic::Ordering::Relaxed);
}
#[tauri::command]
pub fn get_sync_progress(state: State<'_, AppState>) -> sync::SyncProgress {
    sync::progress(&state.sync)
}

#[tauri::command]
pub fn get_network_log(
    state: State<'_, AppState>,
) -> Vec<portfolio_providers::network_log::NetworkRequest> {
    state.sync.network_log.entries()
}

#[tauri::command]
pub fn clear_network_log(state: State<'_, AppState>) {
    state.sync.network_log.clear();
}

#[tauri::command]
pub async fn test_provider(state: State<'_, AppState>, provider: String) -> CommandResult<()> {
    // The probe acquires the shared run lock exactly once.
    sync::test_provider(
        &state.sync,
        &state.store().await,
        state.secrets.as_ref(),
        &provider,
    )
    .await
}

async fn save_text(app: tauri::AppHandle, name: &str, text: String) -> CommandResult<bool> {
    use tauri_plugin_dialog::DialogExt;
    let name = name.to_owned();
    let selected = tauri::async_runtime::spawn_blocking(move || {
        app.dialog().file().set_file_name(name).blocking_save_file()
    })
    .await
    .map_err(|_| CommandError::new("dialog", "File dialog failed."))?;
    let Some(selected) = selected else {
        return Ok(false);
    };
    let path = selected
        .into_path()
        .map_err(|_| CommandError::new("file", "Choose a local file."))?;
    tauri::async_runtime::spawn_blocking(move || std::fs::write(path, text))
        .await
        .map_err(|_| CommandError::new("file", "Export worker failed."))?
        .map_err(|_| CommandError::new("file", "Could not save the selected file."))?;
    Ok(true)
}

#[tauri::command]
pub async fn export_backup(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<bool> {
    let text = state.store().await.export_backup().await?;
    save_text(app, "CoinControl.ccbackup", text).await
}

#[tauri::command]
pub async fn inspect_backup(state: State<'_, AppState>, content: String) -> CommandResult<String> {
    Ok(state.store().await.inspect_backup(&content).await?)
}

#[tauri::command]
pub async fn restore_backup(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    content: String,
) -> CommandResult<()> {
    let _sync = state.sync.run_lock.lock().await;
    let store = state.store().await;
    let backups = state.profiles_dir.join("safety-backups");
    std::fs::create_dir_all(&backups)
        .map_err(|_| CommandError::new("file", "Could not create safety backup directory."))?;
    let path = backups.join(format!(
        "{}-{}-{}.ccbackup",
        store.profile().as_str(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        std::process::id()
    ));
    store.restore_backup(&content, &path).await?;
    state.sync.network_log.clear();
    state
        .sync
        .network_log
        .set_enabled(store.get_settings().await?.network_console_enabled);
    store.replay_accounting().await?;
    use tauri::Emitter;
    app.emit(sync::DATA_CHANGED_EVENT, ())
        .map_err(|_| CommandError::new("event", "Could not refresh restored views."))?;
    Ok(())
}

#[tauri::command]
pub async fn export_csv(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    kind: String,
) -> CommandResult<bool> {
    let store = state.store().await;
    store.replay_if_dirty().await?;
    let text = store.export_csv(&kind).await?;
    save_text(app, &format!("CoinControl-{kind}.csv"), text).await
}
