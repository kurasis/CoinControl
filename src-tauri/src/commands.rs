//! Typed IPC commands. This is the complete frontend-reachable surface: there
//! is no generic URL fetch, RPC, SQL, shell, or key readback command.

use portfolio_core::accounting::ACCOUNTING_ENGINE_VERSION;
use portfolio_core::address::{NormalizedAddress, normalize_address};
use portfolio_core::clock::{Clock, SystemClock, utc_day};
use portfolio_core::network::{NetworkFamily, NetworkId};
use portfolio_providers::capabilities::{NetworkCapability, network_capabilities};
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
    let mut guard = state.store.write().await;
    if guard.profile() == profile {
        return Ok(profile);
    }
    let next = open_profile(&state.profiles_dir, profile).await?;
    let previous = std::mem::replace(&mut *guard, next);
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
    Ok(state.store().await.update_settings(&settings).await?)
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
pub fn list_network_capabilities() -> Vec<NetworkCapability> {
    network_capabilities()
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
) -> CommandResult<ChartSeries> {
    Ok(state.store().await.get_chart(&scope, range).await?)
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
                status.last_error = u.last_error.clone();
            }
            status
        })
        .collect())
}

/// Stores a key. The response reports only where it was stored, never the key.
#[tauri::command]
pub fn save_provider_key(
    state: State<'_, AppState>,
    provider: String,
    key: String,
) -> CommandResult<ProviderStatus> {
    let spec = providers::find(&provider)
        .ok_or_else(|| CommandError::new("not_found", "Unknown data source."))?;
    let storage = state
        .secrets
        .save(spec.id, Secret::new(key))
        .map_err(|m| CommandError::new("secret_store", m))?;
    Ok(ProviderStatus::from_spec(spec, Some(storage)))
}

#[tauri::command]
pub fn remove_provider_key(
    state: State<'_, AppState>,
    provider: String,
) -> CommandResult<ProviderStatus> {
    let spec = providers::find(&provider)
        .ok_or_else(|| CommandError::new("not_found", "Unknown data source."))?;
    state
        .secrets
        .remove(spec.id)
        .map_err(|m| CommandError::new("secret_store", m))?;
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
