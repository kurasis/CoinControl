// Every custom IPC command is declared here. Tauri then generates an
// `allow-<command>` permission for each, and only the commands granted in
// `capabilities/main.json` can be invoked by the main window.
const COMMANDS: &[&str] = &[
    "app_info",
    "switch_profile",
    "get_settings",
    "update_settings",
    "list_networks",
    "validate_address",
    "create_wallet",
    "list_wallets",
    "rename_wallet",
    "add_account",
    "list_accounts",
    "set_account_archived",
    "move_account",
    "create_group",
    "list_groups",
    "set_group_wallets",
    "delete_group",
    "list_holdings",
    "get_portfolio_summary",
    "get_chart",
    "list_activity",
    "get_asset_detail",
    "get_asset_chart",
    "list_review_items",
    "get_leg_detail",
    "update_basis",
    "preview_basis_import",
    "commit_basis_import",
    "discard_basis_import",
    "replay_accounting",
    "list_providers",
    "save_provider_key",
    "remove_provider_key",
    "sync_now",
    "list_sync_status",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to run tauri-build");
}
