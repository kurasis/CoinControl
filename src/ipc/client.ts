// Typed wrappers for the Rust IPC commands. This is the only way the UI
// reaches data: there is no direct provider call from the renderer.
import { invoke } from "@tauri-apps/api/core";
import type { Account } from "./bindings/Account";
import type { AccountSyncStatus } from "./bindings/AccountSyncStatus";
import type { ActivityFilter } from "./bindings/ActivityFilter";
import type { ActivityPage } from "./bindings/ActivityPage";
import type { AppInfo } from "./bindings/AppInfo";
import type { AssetChart } from "./bindings/AssetChart";
import type { AssetDetail } from "./bindings/AssetDetail";
import type { ChartRange } from "./bindings/ChartRange";
import type { ChartSeries } from "./bindings/ChartSeries";
import type { CommandError } from "./bindings/CommandError";
import type { Group } from "./bindings/Group";
import type { HoldingRow } from "./bindings/HoldingRow";
import type { ImportPreview } from "./bindings/ImportPreview";
import type { ImportResult } from "./bindings/ImportResult";
import type { LegDetail } from "./bindings/LegDetail";
import type { LegOverride } from "./bindings/LegOverride";
import type { NetworkCapability } from "./bindings/NetworkCapability";
import type { NetworkId } from "./bindings/NetworkId";
import type { NetworkInfo } from "./bindings/NetworkInfo";
import type { NormalizedAddress } from "./bindings/NormalizedAddress";
import type { PortfolioSummary } from "./bindings/PortfolioSummary";
import type { ProfileKind } from "./bindings/ProfileKind";
import type { ProviderStatus } from "./bindings/ProviderStatus";
import type { ReplayReport } from "./bindings/ReplayReport";
import type { ReviewList } from "./bindings/ReviewList";
import type { Scope } from "./bindings/Scope";
import type { Settings } from "./bindings/Settings";
import type { SyncSummary } from "./bindings/SyncSummary";
import type { Wallet } from "./bindings/Wallet";

export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** True when running the browser preview with mock data (development and tests only). */
export function isBrowserPreview(): boolean {
  return !isTauri() && (import.meta.env.DEV || import.meta.env.MODE === "test");
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (isTauri()) {
    return invoke<T>(cmd, args);
  }
  // The mock is tree-shaken out of production builds.
  if (import.meta.env.DEV || import.meta.env.MODE === "test") {
    const { mockInvoke } = await import("./mock");
    return mockInvoke(cmd, args) as Promise<T>;
  }
  throw {
    code: "unavailable",
    message: "Portfolio Desk must run as the desktop application.",
    detail: null,
  };
}

export function isCommandError(e: unknown): e is CommandError {
  return typeof e === "object" && e !== null && "code" in e && "message" in e;
}

export const api = {
  appInfo: () => call<AppInfo>("app_info"),
  switchProfile: (profile: ProfileKind) => call<ProfileKind>("switch_profile", { profile }),
  getSettings: () => call<Settings>("get_settings"),
  updateSettings: (settings: Settings) => call<Settings>("update_settings", { settings }),
  listNetworks: () => call<NetworkInfo[]>("list_networks"),
  listNetworkCapabilities: () => call<NetworkCapability[]>("list_network_capabilities"),
  validateAddress: (network: NetworkId, address: string) =>
    call<NormalizedAddress>("validate_address", { network, address }),
  createWallet: (label: string) => call<Wallet>("create_wallet", { label }),
  listWallets: () => call<Wallet[]>("list_wallets"),
  renameWallet: (id: string, label: string) => call<void>("rename_wallet", { id, label }),
  addAccount: (walletId: string, network: NetworkId, address: string, label?: string) =>
    call<Account>("add_account", { walletId, network, address, label: label ?? null }),
  listAccounts: (walletId?: string) =>
    call<Account[]>("list_accounts", { walletId: walletId ?? null }),
  setAccountArchived: (id: string, archived: boolean) =>
    call<void>("set_account_archived", { id, archived }),
  moveAccount: (id: string, walletId: string) => call<void>("move_account", { id, walletId }),
  createGroup: (label: string) => call<Group>("create_group", { label }),
  listGroups: () => call<Group[]>("list_groups"),
  setGroupWallets: (groupId: string, walletIds: string[]) =>
    call<void>("set_group_wallets", { groupId, walletIds }),
  deleteGroup: (groupId: string) => call<void>("delete_group", { groupId }),
  listHoldings: (scope: Scope) => call<HoldingRow[]>("list_holdings", { scope }),
  portfolioSummary: (scope: Scope) => call<PortfolioSummary>("get_portfolio_summary", { scope }),
  chart: (scope: Scope, range: ChartRange) => call<ChartSeries>("get_chart", { scope, range }),
  listActivity: (
    scope: Scope,
    cursor?: string | null,
    limit?: number,
    filter?: Partial<ActivityFilter>,
  ) =>
    call<ActivityPage>("list_activity", {
      scope,
      cursor: cursor ?? null,
      limit: limit ?? null,
      filter: filter
        ? { asset_id: filter.asset_id ?? null, unresolved_only: filter.unresolved_only ?? false }
        : null,
    }),
  assetDetail: (scope: Scope, assetId: string) =>
    call<AssetDetail>("get_asset_detail", { scope, assetId }),
  assetChart: (scope: Scope, assetId: string, range: ChartRange) =>
    call<AssetChart>("get_asset_chart", { scope, assetId, range }),
  listReviewItems: (scope: Scope, limit?: number) =>
    call<ReviewList>("list_review_items", { scope, limit: limit ?? null }),
  legDetail: (legId: string) => call<LegDetail>("get_leg_detail", { legId }),
  updateBasis: (legId: string, decision: LegOverride) =>
    call<ReplayReport>("update_basis", { legId, decision }),
  previewBasisImport: (fileName: string, content: string, mapping?: Record<string, string>) =>
    call<ImportPreview>("preview_basis_import", { fileName, content, mapping: mapping ?? null }),
  commitBasisImport: (batchId: string) => call<ImportResult>("commit_basis_import", { batchId }),
  discardBasisImport: (batchId: string) => call<void>("discard_basis_import", { batchId }),
  replayAccounting: () => call<ReplayReport>("replay_accounting"),
  listProviders: () => call<ProviderStatus[]>("list_providers"),
  saveProviderKey: (provider: string, key: string) =>
    call<ProviderStatus>("save_provider_key", { provider, key }),
  removeProviderKey: (provider: string) =>
    call<ProviderStatus>("remove_provider_key", { provider }),
  syncNow: (accountId?: string) => call<SyncSummary>("sync_now", { accountId: accountId ?? null }),
  listSyncStatus: () => call<AccountSyncStatus[]>("list_sync_status"),
};

/** Emitted by the Rust side whenever synchronized data changed. */
export const DATA_CHANGED_EVENT = "portfolio-data-changed";

export type { Scope, ChartRange, NetworkId, LegOverride };
