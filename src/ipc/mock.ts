// Browser-preview IPC mock for development and component tests only.
// It is never bundled into production builds (see client.ts) and does not
// implement real validation or accounting; the Rust core is authoritative.
import type { AccountingSummary } from "./bindings/AccountingSummary";
import type { Account } from "./bindings/Account";
import type { ActivityFilter } from "./bindings/ActivityFilter";
import type { ActivityRow } from "./bindings/ActivityRow";
import type { AssetDetail } from "./bindings/AssetDetail";
import type { ChartPoint } from "./bindings/ChartPoint";
import type { ChartRange } from "./bindings/ChartRange";
import type { Group } from "./bindings/Group";
import type { HoldingRow } from "./bindings/HoldingRow";
import type { ImportPreview } from "./bindings/ImportPreview";
import type { LegDetail } from "./bindings/LegDetail";
import type { LegOverride } from "./bindings/LegOverride";
import type { OverrideVersion } from "./bindings/OverrideVersion";
import type { PeriodPerformanceDto } from "./bindings/PeriodPerformanceDto";
import type { NetworkCapability } from "./bindings/NetworkCapability";
import type { NetworkId } from "./bindings/NetworkId";
import type { NetworkInfo } from "./bindings/NetworkInfo";
import type { ProfileKind } from "./bindings/ProfileKind";
import type { ProviderStatus } from "./bindings/ProviderStatus";
import type { ReconciliationRow } from "./bindings/ReconciliationRow";
import type { ReviewItem } from "./bindings/ReviewItem";
import type { Settings } from "./bindings/Settings";
import type { Wallet } from "./bindings/Wallet";

const NOW = Math.floor(Date.now() / 1000);
const DAY = 86_400;

const NETWORKS: NetworkInfo[] = [
  ["bitcoin", "Bitcoin", "bitcoin", "BTC", null],
  ["ethereum", "Ethereum", "evm", "ETH", 1],
  ["base", "Base", "evm", "ETH", 8453],
  ["arbitrum", "Arbitrum One", "evm", "ETH", 42161],
  ["optimism", "Optimism", "evm", "ETH", 10],
  ["polygon", "Polygon PoS", "evm", "POL", 137],
  ["bsc", "BNB Smart Chain", "evm", "BNB", 56],
  ["solana", "Solana", "solana", "SOL", null],
  ["tron", "TRON", "tron", "TRX", null],
  ["ton", "TON", "ton", "TON", null],
].map(([id, name, family, native_symbol, evm_chain_id]) => ({
  id: id as NetworkId,
  name: name as string,
  family: family as NetworkInfo["family"],
  native_symbol: native_symbol as string,
  evm_chain_id: evm_chain_id as number | null,
}));

interface State {
  profile: ProfileKind;
  settings: Settings;
  wallets: Wallet[];
  accounts: Account[];
  groups: Group[];
  holdings: HoldingRow[];
  activity: ActivityRow[];
  keys: Record<string, true>;
  reviews: ReviewItem[];
  reconciliation: ReconciliationRow[];
  overrides: Record<string, OverrideVersion[]>;
  imports: Record<string, { rows: number; committed: boolean; sha: string }>;
}

function emptyState(profile: ProfileKind): State {
  return {
    profile,
    settings: {
      language: null,
      theme: "dark",
      timezone: null,
      privacy_mode: false,
      price_refresh_seconds: 60,
      sweep_interval_minutes: 60,
    },
    wallets: [],
    accounts: [],
    groups: [],
    holdings: [],
    activity: [],
    keys: {},
    reviews: [],
    reconciliation: [],
    overrides: {},
    imports: {},
  };
}

function holding(
  asset_id: string,
  network: NetworkId,
  symbol: string,
  name: string,
  quantity: string,
  price: string | null,
  change: string | null,
  basis: string | null = null,
  coverage: HoldingRow["basis_coverage"] = "unknown",
): HoldingRow {
  const value = price === null ? null : (Number(quantity) * Number(price)).toFixed(2);
  const pnl = basis !== null && value !== null ? (Number(value) - Number(basis)).toFixed(2) : null;
  return {
    asset_id,
    network,
    symbol,
    name,
    verification: price === null ? "unverified" : "verified",
    quantity,
    price_usd: price,
    value_usd: value,
    change_24h_percent: change,
    price_observed_at: price === null ? null : NOW - 60,
    allocation_percent: null,
    balance_status: network === "ton" ? "stale" : "fresh",
    observed_at: NOW - 120,
    basis_usd: basis,
    unrealized_pnl_usd: pnl,
    unrealized_return_percent:
      pnl !== null && Number(basis) > 0 ? ((Number(pnl) / Number(basis)) * 100).toFixed(2) : null,
    basis_coverage: coverage,
  };
}

function demoState(): State {
  const s = emptyState("demo");
  s.wallets = [
    {
      id: "w-cold",
      label: "Cold storage",
      archived: false,
      created_at: NOW - 400 * DAY,
      account_count: 2,
    },
    {
      id: "w-daily",
      label: "Daily",
      archived: false,
      created_at: NOW - 400 * DAY,
      account_count: 2,
    },
  ];
  s.accounts = [
    ["a-btc", "w-cold", "bitcoin", "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4"],
    ["a-eth", "w-cold", "ethereum", "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed"],
    ["a-sol", "w-daily", "solana", "11111111111111111111111111111112"],
    [
      "a-ton",
      "w-daily",
      "ton",
      "0:0000000000000000000000000000000000000000000000000000000000000001",
    ],
  ].map(([id, wallet_id, network, address]) => ({
    id: id!,
    wallet_id: wallet_id!,
    network: network as NetworkId,
    canonical_address: address!.toLowerCase(),
    display_address: address!,
    label: null,
    archived: false,
    created_at: NOW - 400 * DAY,
  }));
  s.groups = [{ id: "g-long", label: "Long-term", wallet_ids: ["w-cold"] }];
  s.holdings = [
    holding(
      "bitcoin:native",
      "bitcoin",
      "BTC",
      "Bitcoin",
      "0.75",
      "64210.5",
      "1.84",
      "31500",
      "known",
    ),
    holding(
      "ethereum:native",
      "ethereum",
      "ETH",
      "Ether",
      "4.19",
      "3120.12",
      "-2.31",
      null,
      "partial",
    ),
    holding("solana:native", "solana", "SOL", "Solana", "35", "148.2", "0.42", "4200", "estimated"),
    holding("ton:native", "ton", "TON", "Toncoin", "420", "5.12", "-0.9"),
    holding(
      "ethereum:token:0x0d3e",
      "ethereum",
      "DEMO",
      "Unpriced demo token",
      "1000000",
      null,
      null,
    ),
  ];
  const total = s.holdings.reduce((t, h) => t + (h.value_usd ? Number(h.value_usd) : 0), 0);
  for (const h of s.holdings) {
    h.allocation_percent = h.value_usd ? ((Number(h.value_usd) / total) * 100).toFixed(2) : null;
  }
  s.activity = [
    {
      transaction_id: "t1",
      account_id: "a-eth",
      network: "ethereum",
      occurred_at: NOW - 5 * DAY,
      operation: "send",
      status: "final",
      legs: [
        {
          leg_id: "ethereum:t1:a-eth:0",
          asset_id: "ethereum:native",
          symbol: "ETH",
          signed_quantity: "-0.01",
          direction: "out",
          treatment: "unclassified_out",
          value_usd: "31.40",
          value_estimated: true,
          review: "unclassified_outgoing",
        },
      ],
      fee_quantity: "0.00042",
      fee_symbol: "ETH",
      fee_value_usd: "1.32",
      unresolved: true,
    },
    {
      transaction_id: "t2",
      account_id: "a-btc",
      network: "bitcoin",
      occurred_at: NOW - 120 * DAY,
      operation: "receive",
      status: "final",
      legs: [
        {
          leg_id: "bitcoin:t2:a-btc:0",
          asset_id: "bitcoin:native",
          symbol: "BTC",
          signed_quantity: "0.25",
          direction: "in",
          treatment: "deposit",
          value_usd: "15100.00",
          value_estimated: true,
          review: null,
        },
      ],
      fee_quantity: null,
      fee_symbol: null,
      fee_value_usd: null,
      unresolved: false,
    },
  ];
  s.reviews = [
    {
      leg_id: "ethereum:t1:a-eth:0",
      transaction_id: "t1",
      account_id: "a-eth",
      network: "ethereum",
      occurred_at: NOW - 5 * DAY,
      asset_id: "ethereum:native",
      symbol: "ETH",
      quantity: "-0.01",
      treatment: "unclassified_out",
      reason: "unclassified_outgoing",
      value_usd: "31.40",
    },
    {
      leg_id: "ethereum:t3:a-eth:0",
      transaction_id: "t3",
      account_id: "a-eth",
      network: "ethereum",
      occurred_at: NOW - 200 * DAY,
      asset_id: "ethereum:native",
      symbol: "ETH",
      quantity: "1.2",
      treatment: "deposit",
      reason: "unknown_basis",
      value_usd: "2890.80",
    },
  ];
  s.reconciliation = [
    {
      id: "R1",
      kind: "balance_mismatch",
      account_id: "a-ton",
      asset_id: "ton:native",
      symbol: "TON",
      quantity: "20",
      detail: "the reported balance differs from the complete synchronized history",
    },
  ];
  return s;
}

function accounting(): AccountingSummary {
  const known = current.holdings.filter((h) => h.basis_usd !== null && h.value_usd !== null);
  const kv = known.reduce((t, h) => t + Number(h.value_usd), 0);
  const kb = known.reduce((t, h) => t + Number(h.basis_usd), 0);
  const valued = current.holdings.filter((h) => h.value_usd !== null);
  const total = valued.reduce((t, h) => t + Number(h.value_usd), 0);
  const any = current.holdings.length > 0;
  return {
    unrealized_pnl_usd: null,
    unrealized_return_percent: null,
    unrealized_reason: any ? "missing_basis" : null,
    remaining_basis_usd: null,
    known_subset_value_usd: kv.toFixed(2),
    known_subset_basis_usd: kb.toFixed(2),
    known_subset_pnl_usd: (kv - kb).toFixed(2),
    known_subset_return_percent: kb > 0 ? (((kv - kb) / kb) * 100).toFixed(2) : null,
    basis_coverage_percent: total > 0 ? ((kv / total) * 100).toFixed(2) : null,
    has_estimated_basis: current.holdings.some((h) => h.basis_coverage === "estimated"),
    realized: { known_usd: any ? "1240.18" : "0", complete: !any },
    income: { known_usd: any ? "86.40" : "0", complete: true },
    expenses: { known_usd: any ? "42.77" : "0", complete: true },
    total_accounted_pnl_usd: null,
    fee_charges: current.activity.filter((r) => r.fee_quantity).length,
    review_count: current.reviews.length,
    reconciliation_count: current.reconciliation.length,
  };
}

function performance(points: ChartPoint[]): PeriodPerformanceDto | null {
  const valued = points.filter((p) => p.value_usd !== null);
  const first = valued[0];
  const last = valued[valued.length - 1];
  if (!first || !last || first === last) return null;
  const blocked = current.reviews.some((r) => r.reason === "unclassified_outgoing");
  const gain = Number(last.value_usd) - Number(first.value_usd);
  return {
    start: first.t,
    end: last.t,
    beginning_value_usd: first.value_usd!,
    ending_value_usd: last.value_usd!,
    net_flows_usd: "0",
    flow_count: 0,
    gain_usd: blocked ? null : gain.toFixed(2),
    return_percent: blocked ? null : ((gain / Number(first.value_usd)) * 100).toFixed(2),
    reason: blocked ? "incomplete_classification" : null,
    estimated: true,
  };
}

function legDetail(legId: string): LegDetail {
  const review = current.reviews.find((r) => r.leg_id === legId);
  const row = current.activity.find((r) => r.legs.some((l) => l.leg_id === legId));
  const leg = row?.legs.find((l) => l.leg_id === legId);
  if (!review && !leg) throw err("not_found", "movement not found");
  const history = current.overrides[legId] ?? [];
  const quantity = review?.quantity ?? leg!.signed_quantity;
  const network = review?.network ?? row!.network;
  return {
    leg_id: legId,
    transaction_id: review?.transaction_id ?? row!.transaction_id,
    tx_hash: "0x" + "ab".repeat(32),
    tx_status: "final",
    block_height: 19_000_000,
    provider: network === "bitcoin" ? "esplora" : "zerion",
    explorer_url:
      network === "bitcoin"
        ? `https://mempool.space/tx/${"ab".repeat(32)}`
        : `https://etherscan.io/tx/0x${"ab".repeat(32)}`,
    account_id: review?.account_id ?? row!.account_id,
    network,
    occurred_at: review?.occurred_at ?? row!.occurred_at,
    asset_id: review?.asset_id ?? leg!.asset_id,
    symbol: review?.symbol ?? leg!.symbol,
    quantity: quantity.replace("-", ""),
    direction: quantity.startsWith("-") ? "out" : "in",
    operation: quantity.startsWith("-") ? "send" : "receive",
    treatment: review?.treatment ?? leg?.treatment ?? null,
    counterparty_account_id: null,
    value_usd: review?.value_usd ?? leg?.value_usd ?? null,
    value_estimated: true,
    basis_usd: null,
    basis_kind: quantity.startsWith("-") ? null : "unknown",
    proceeds_usd: null,
    review: review?.reason ?? null,
    current: history[history.length - 1]?.payload ?? null,
    history,
    pair_candidates: quantity.startsWith("-")
      ? [
          {
            leg_id: "solana:t9:a-sol:0",
            account_id: "a-sol",
            network: "solana",
            occurred_at: (review?.occurred_at ?? NOW) + 600,
            quantity: quantity.replace("-", ""),
            transaction_id: "t9",
          },
        ]
      : [],
  };
}

function assetDetail(assetId: string): AssetDetail {
  const h = current.holdings.find((x) => x.asset_id === assetId);
  if (!h) throw err("not_found", "asset not found");
  const accounts = current.accounts.filter((a) => a.network === h.network);
  return {
    asset_id: h.asset_id,
    network: h.network,
    contract: h.asset_id.includes(":token:") ? h.asset_id.split(":").pop()! : null,
    symbol: h.symbol,
    name: h.name,
    decimals: 18,
    verification: h.verification,
    explorer_url: null,
    price_usd: h.price_usd,
    price_observed_at: h.price_observed_at,
    change_24h_percent: h.change_24h_percent,
    quantity: h.quantity,
    value_usd: h.value_usd,
    remaining_basis_usd: h.basis_usd,
    unrealized_pnl_usd: h.unrealized_pnl_usd,
    unrealized_return_percent: h.unrealized_return_percent,
    unrealized_reason: h.basis_usd === null ? "missing_basis" : null,
    known_subset_pnl_usd: h.unrealized_pnl_usd ?? "0",
    basis_coverage_quantity_percent: h.basis_coverage === "unknown" ? "0" : "100",
    has_estimated_basis: h.basis_coverage === "estimated",
    realized: { known_usd: "0", complete: true },
    income: { known_usd: "0", complete: true },
    expenses: { known_usd: "0", complete: true },
    accounts: accounts.map((a) => ({
      account_id: a.id,
      wallet_id: a.wallet_id,
      quantity: h.quantity,
      value_usd: h.value_usd,
      basis_usd: h.basis_usd,
      unrealized_pnl_usd: h.unrealized_pnl_usd,
      basis_coverage: h.basis_coverage,
      balance_status: h.balance_status,
    })),
    lots:
      h.basis_usd === null || !accounts[0]
        ? []
        : [
            {
              id: "L1",
              account_id: accounts[0].id,
              quantity: h.quantity,
              remaining_quantity: h.quantity,
              basis_usd: h.basis_usd,
              remaining_basis_usd: h.basis_usd,
              basis_kind: h.basis_coverage === "estimated" ? "estimated" : "known",
              acquired_at: NOW - 380 * DAY,
              arrived_at: NOW - 380 * DAY,
              parent_lot_id: null,
            },
          ],
  };
}

function importPreview(fileName: string, content: string): ImportPreview {
  const lines = content.split(/\r?\n/).filter((l) => l.trim());
  const columns = (lines[0] ?? "").split(",").map((c) => c.trim());
  const sha = `${content.length}:${lines.length}`;
  const duplicate = Object.values(current.imports).some((b) => b.committed && b.sha === sha);
  const rows = lines.slice(1).map((line, i) => {
    const cells = line.split(",");
    const ok = cells.length === columns.length && cells.every((c) => !c.includes("bad"));
    return {
      row: i + 1,
      external_row_id: cells[0] ?? null,
      status: duplicate ? ("duplicate" as const) : ok ? ("ok" as const) : ("error" as const),
      messages: duplicate ? ["already imported"] : ok ? [] : ["quantity: not an exact decimal"],
      leg_id: null,
      account_id: null,
      asset_id: null,
      quantity: null,
      total_basis_usd: null,
      basis_kind: null,
      acquired_at: null,
      classification: null,
      opening: false,
    };
  });
  const batch_id = `b-${Object.keys(current.imports).length + 1}`;
  const ok = rows.filter((r) => r.status === "ok").length;
  const errors = rows.filter((r) => r.status === "error").length;
  current.imports[batch_id] = { rows: ok, committed: false, sha };
  const summary = (items: number) => ({
    realized_known_usd: "1240.18",
    realized_complete: false,
    remaining_known_basis_usd: "35700",
    unknown_basis_lots: items,
    review_items: items,
    inventory_gaps: 0,
  });
  return {
    batch_id,
    file_name: fileName,
    file_sha256: sha,
    duplicate_file: duplicate,
    columns,
    mapping: {},
    missing_required: [],
    rows,
    ok_count: ok,
    error_count: errors,
    duplicate_count: rows.length - ok - errors,
    can_commit: !duplicate && errors === 0 && ok > 0,
    before: summary(current.reviews.length),
    after: summary(Math.max(0, current.reviews.length - ok)),
  };
}

let real = emptyState("real");
let demo: State | null = null;
let current = real;

function err(code: string, message: string, detail: string | null = null) {
  return { code, message, detail };
}

function chart(range: ChartRange): ChartPoint[] {
  if (current.holdings.length === 0) return [];
  const [interval, count] = {
    "24h": [3600, 24],
    "7d": [4 * 3600, 42],
    "1m": [DAY, 30],
    "3m": [DAY, 90],
    "1y": [DAY, 365],
    all: [DAY, 400],
  }[range] as [number, number];
  const base = current.holdings.reduce((t, h) => t + (h.value_usd ? Number(h.value_usd) : 0), 0);
  return Array.from({ length: count + 1 }, (_, i) => {
    const t = NOW - (count - i) * interval;
    const wave = Math.sin(i / 6) * 0.04 + (i / count - 1) * 0.15;
    return {
      t,
      value_usd: (base * (1 + wave)).toFixed(2),
      estimated: interval >= DAY,
      partial: true,
    };
  });
}

export async function mockInvoke(
  cmd: string,
  args: Record<string, unknown> = {},
): Promise<unknown> {
  await new Promise((r) => setTimeout(r, 30));
  switch (cmd) {
    case "get_sync_progress":
      return {
        running: false,
        cancel_requested: false,
        details: {
          active_account: null,
          completed_accounts: 0,
          total_accounts: 0,
          pages_fetched: 0,
          phase: "idle",
        },
      };
    case "cancel_sync":
      return null;
    case "test_provider":
      return null;
    case "export_backup":
    case "export_csv":
      throw { code: "preview", message: "File export requires the desktop application." };
    case "app_info":
      return {
        product_name: "Portfolio Desk",
        version: "0.1.0",
        profile: current.profile,
        schema_version: 6,
        accounting_engine_version: 3,
        data_directory: "(browser preview: nothing is stored)",
      };
    case "switch_profile":
      if (args.profile === "demo") {
        demo ??= demoState();
        current = demo;
      } else {
        current = real;
      }
      return current.profile;
    case "get_settings":
      return current.settings;
    case "update_settings":
      current.settings = args.settings as Settings;
      real.settings = current.settings;
      return current.settings;
    case "list_networks":
      return NETWORKS;
    case "list_network_capabilities":
      return NETWORK_CAPABILITIES;
    case "validate_address": {
      const address = String(args.address ?? "").trim();
      if (address.length < 26)
        throw err("invalid_input", "invalid address: too short (preview check only)");
      return { network: args.network, canonical: address.toLowerCase(), display: address };
    }
    case "create_wallet": {
      const w: Wallet = {
        id: `w-${current.wallets.length + 1}`,
        label: String(args.label),
        archived: false,
        created_at: NOW,
        account_count: 0,
      };
      current.wallets.push(w);
      return w;
    }
    case "list_wallets":
      return current.wallets.map((w) => ({
        ...w,
        account_count: current.accounts.filter((a) => a.wallet_id === w.id).length,
      }));
    case "add_accounts": {
      const addresses = args.addresses as string[];
      const canonical = addresses.map((a) => a.trim().toLowerCase());
      if (
        new Set(canonical).size !== canonical.length ||
        canonical.some((a) =>
          current.accounts.some((x) => x.network === args.network && x.canonical_address === a),
        )
      )
        throw err("account_exists", "duplicate address");
      return Promise.all(
        addresses.map((address) => mockInvoke("add_account", { ...args, address })),
      );
    }
    case "list_asset_policies":
      return current.holdings.map((h) => ({
        asset_id: h.asset_id,
        symbol: h.symbol,
        name: h.name,
        verification: h.verification,
        hidden: false,
        exclude_override: null,
      }));
    case "set_asset_policy":
      throw err("unavailable", "Token decisions are available in the desktop application.");
    case "add_account": {
      const address = String(args.address).trim();
      const dup = current.accounts.find(
        (a) => a.network === args.network && a.canonical_address === address.toLowerCase(),
      );
      if (dup) throw err("account_exists", "this address is already tracked", dup.id);
      const a: Account = {
        id: `a-${current.accounts.length + 1}`,
        wallet_id: String(args.walletId),
        network: args.network as NetworkId,
        canonical_address: address.toLowerCase(),
        display_address: address,
        label: (args.label as string | null) ?? null,
        archived: false,
        created_at: NOW,
      };
      current.accounts.push(a);
      return a;
    }
    case "list_accounts":
      return current.accounts.filter((a) => !args.walletId || a.wallet_id === args.walletId);
    case "list_groups":
      return current.groups;
    case "create_group": {
      const g: Group = {
        id: `g-${current.groups.length + 1}`,
        label: String(args.label),
        wallet_ids: [],
      };
      current.groups.push(g);
      return g;
    }
    case "set_group_wallets": {
      const g = current.groups.find((x) => x.id === args.groupId);
      if (g) g.wallet_ids = [...new Set(args.walletIds as string[])];
      return null;
    }
    case "delete_group":
      current.groups = current.groups.filter((g) => g.id !== args.groupId);
      return null;
    case "list_holdings":
      return current.holdings;
    case "get_portfolio_summary": {
      const valued = current.holdings.filter((h) => h.value_usd !== null);
      return {
        total_value_usd: valued.length
          ? valued.reduce((t, h) => t + Number(h.value_usd), 0).toFixed(2)
          : null,
        holding_count: current.holdings.length,
        unpriced_count: current.holdings.length - valued.length,
        stale_count: current.holdings.filter((h) => h.balance_status !== "fresh").length,
        excluded_spam_count: current.profile === "demo" ? 1 : 0,
        unrealized_pnl_usd: null,
        unrealized_return_percent: null,
        unrealized_reason: "missing_basis",
        last_successful_sync_at: current.holdings.length ? NOW - 120 : null,
        account_count: current.accounts.length,
        accounting: accounting(),
      };
    }
    case "get_chart": {
      const points = chart(args.range as ChartRange).filter(
        (p) =>
          (args.start == null || p.t >= Number(args.start)) &&
          (args.end == null || p.t <= Number(args.end)),
      );
      return {
        range: args.range,
        interval_seconds: 3600,
        points,
        history_available_since: points[0]?.t ?? null,
        performance: performance(points),
      };
    }
    case "list_activity": {
      const filter = args.filter as ActivityFilter | null | undefined;
      const rows = current.activity.filter(
        (r) =>
          (!filter?.asset_id || r.legs.some((l) => l.asset_id === filter.asset_id)) &&
          (!filter?.unresolved_only || r.unresolved) &&
          (!filter?.account_id || r.account_id === filter.account_id) &&
          (!filter?.network || r.network === filter.network) &&
          (!filter?.operation || r.operation === filter.operation) &&
          (!filter?.status || r.status === filter.status) &&
          (filter?.start == null || r.occurred_at >= filter.start) &&
          (filter?.end == null || r.occurred_at <= filter.end),
      );
      return { rows, next_cursor: null };
    }
    case "get_asset_detail":
      return assetDetail(String(args.assetId));
    case "get_asset_chart": {
      const h = current.holdings.find((x) => x.asset_id === args.assetId);
      const points = chart(args.range as ChartRange);
      const scale = h?.value_usd ? Number(h.value_usd) / Number(points.at(-1)?.value_usd ?? 1) : 0;
      return {
        price: points.map((p) => ({
          t: p.t,
          price_usd:
            h?.price_usd == null
              ? null
              : (
                  (Number(h.price_usd) * Number(p.value_usd)) /
                  Number(points.at(-1)!.value_usd)
                ).toFixed(2),
          estimated: p.estimated,
        })),
        holdings: {
          range: args.range,
          interval_seconds: 3600,
          points: points.map((p) => ({
            ...p,
            value_usd: h?.value_usd == null ? null : (Number(p.value_usd) * scale).toFixed(2),
          })),
          history_available_since: points[0]?.t ?? null,
          performance: null,
        },
      };
    }
    case "list_review_items":
      return {
        items: current.reviews,
        total: current.reviews.length,
        reconciliation: current.reconciliation,
      };
    case "get_leg_detail":
      return legDetail(String(args.legId));
    case "update_basis": {
      const legId = String(args.legId);
      const decision = args.decision as LegOverride;
      const history = (current.overrides[legId] ??= []);
      history.push({
        version: history.length + 1,
        created_at: Math.floor(Date.now() / 1000),
        source: "manual",
        payload: decision,
        orphaned: false,
      });
      const resolved =
        (decision.classification !== null && decision.classification !== "unclassified") ||
        (decision.basis_lots?.length ?? 0) > 0 ||
        decision.basis_from_market ||
        decision.pair_with !== null;
      if (resolved) current.reviews = current.reviews.filter((r) => r.leg_id !== legId);
      return {
        events: 10,
        lots: 4,
        review_items: current.reviews.length,
        reconciliation_items: current.reconciliation.length,
        orphaned_overrides: 0,
      };
    }
    case "preview_basis_import":
      return importPreview(String(args.fileName), String(args.content));
    case "commit_basis_import": {
      const b = current.imports[String(args.batchId)];
      if (!b || b.committed) throw err("invalid_input", "this import is no longer pending");
      b.committed = true;
      current.reviews = current.reviews.slice(Math.min(b.rows, current.reviews.length));
      return {
        batch_id: args.batchId,
        applied_rows: b.rows,
        replay: {
          events: 10,
          lots: 4,
          review_items: current.reviews.length,
          reconciliation_items: current.reconciliation.length,
          orphaned_overrides: 0,
        },
      };
    }
    case "discard_basis_import":
      delete current.imports[String(args.batchId)];
      return null;
    case "replay_accounting":
      return {
        events: 10,
        lots: 4,
        review_items: current.reviews.length,
        reconciliation_items: current.reconciliation.length,
        orphaned_overrides: 0,
      };
    case "list_providers":
      return PROVIDERS.map((p) => ({
        ...p,
        key_storage: current.keys[p.id] ? "os_credential_store" : null,
      }));
    case "save_provider_key": {
      current.keys[String(args.provider)] = true;
      const p = PROVIDERS.find((x) => x.id === args.provider)!;
      return { ...p, key_storage: "os_credential_store" };
    }
    case "remove_provider_key": {
      delete current.keys[String(args.provider)];
      const p = PROVIDERS.find((x) => x.id === args.provider)!;
      return { ...p, key_storage: null };
    }
    case "sync_now":
      if (current.profile !== "real")
        throw err("demo_profile", "The demo portfolio never synchronizes with data providers.");
      return {
        accounts: [],
        prices: { priced: 0, unpriced: [], errors: [] },
        price_history: { requests: 0, points: 0, pending_assets: 0, unavailable: [], errors: [] },
        accounting: null,
        accounting_error: null,
      };
    case "list_sync_status":
      return current.accounts.map((a) => ({
        account_id: a.id,
        provider: NETWORK_CAPABILITIES.find((c) => c.network === a.network)?.provider ?? null,
        coverage: current.profile === "demo" ? "complete" : null,
        last_attempt_at: null,
        last_success_at: current.profile === "demo" ? NOW : null,
        last_error: null,
        earliest_covered_at: null,
        transaction_count: current.activity.filter((r) => r.account_id === a.id).length,
        pending_incomplete: false,
      }));
    default:
      throw err("not_found", `mock has no command ${cmd}`);
  }
}

/** Resets mock state between tests. */
export function resetMock(): void {
  real = emptyState("real");
  demo = null;
  current = real;
}

// Mirrors portfolio_providers::capabilities::network_capabilities().
const EVM_CAPABILITY = {
  provider: "zerion",
  key_required: true,
  balances: "full",
  token_discovery: "full",
  history: ["native", "tokens", "trades", "failed", "internal"],
  fees: "full",
  internal_transfers: "partial",
  limitations: ["zerion_simple_positions", "zerion_interpreted_history"],
  live_verified_on: "2026-10-05",
} as const;
const NETWORK_CAPABILITIES: NetworkCapability[] = [
  {
    network: "bitcoin",
    provider: "esplora",
    key_required: false,
    balances: "full",
    token_discovery: "none",
    history: ["native", "failed", "pending"],
    fees: "full",
    internal_transfers: "none",
    limitations: ["btc_single_address", "btc_pending_cap"],
    live_verified_on: "2026-10-05",
  },
  ...(["ethereum", "base", "arbitrum", "optimism"] as const).map((network) => ({
    ...EVM_CAPABILITY,
    network,
    history: [...EVM_CAPABILITY.history],
    limitations: [...EVM_CAPABILITY.limitations],
  })),
  {
    ...EVM_CAPABILITY,
    network: "polygon",
    history: [...EVM_CAPABILITY.history],
    limitations: [...EVM_CAPABILITY.limitations, "polygon_native_contract"],
  },
  {
    ...EVM_CAPABILITY,
    network: "bsc",
    history: [...EVM_CAPABILITY.history],
    limitations: [...EVM_CAPABILITY.limitations],
  },
  {
    ...EVM_CAPABILITY,
    network: "solana",
    history: ["native", "tokens", "trades", "failed"],
    limitations: ["zerion_simple_positions", "solana_zerion_interpreted", "solana_token_2022"],
  },
  {
    network: "tron",
    provider: "trongrid",
    key_required: true,
    balances: "full",
    token_discovery: "partial",
    history: ["native", "tokens", "failed", "internal"],
    fees: "full",
    internal_transfers: "partial",
    limitations: [
      "tron_staked_included",
      "tron_trc10_unsupported",
      "tron_token_metadata",
      "tron_unverified_tokens",
      "confirmed_only",
    ],
    live_verified_on: "2026-10-05",
  },
  {
    network: "ton",
    provider: "tonapi",
    key_required: false,
    balances: "full",
    token_discovery: "full",
    history: ["native", "tokens", "trades", "failed", "pending"],
    fees: "full",
    internal_transfers: "partial",
    limitations: [
      "ton_events_canonical",
      "ton_contract_calls_partial",
      "ton_nft_excluded",
      "ton_key_recommended",
    ],
    live_verified_on: "2026-10-05",
  },
];

const PROVIDERS: ProviderStatus[] = [
  {
    id: "livecoinwatch",
    name: "Live Coin Watch",
    role: "market_prices",
    key_requirement: "required",
    networks: [],
    docs_url: "https://livecoinwatch.github.io/lcw-api-docs/",
    key_url: "https://www.livecoinwatch.com/tools/api",
    free_allowance: "10,000 requests/day",
    key_storage: null,
    requests_today: 0,
    last_error: null,
    adapter_available: true,
  },
  {
    id: "zerion",
    name: "Zerion",
    role: "account_data",
    key_requirement: "required",
    networks: ["ethereum", "base", "arbitrum", "optimism", "polygon", "bsc", "solana"],
    docs_url: "https://developers.zerion.io/",
    key_url: "https://dashboard.zerion.io/",
    free_allowance: "2,000 requests/day, 3 requests/sec",
    key_storage: null,
    requests_today: 0,
    last_error: null,
    adapter_available: true,
  },
  {
    id: "esplora",
    name: "Blockstream Esplora",
    role: "account_data",
    key_requirement: "not_needed",
    networks: ["bitcoin"],
    docs_url: "https://github.com/Blockstream/esplora/blob/master/API.md",
    key_url: null,
    free_allowance: "Public service, fair use",
    key_storage: null,
    requests_today: 0,
    last_error: null,
    adapter_available: true,
  },
  {
    id: "trongrid",
    name: "TronGrid",
    role: "account_data",
    key_requirement: "required",
    networks: ["tron"],
    docs_url: "https://developers.tron.network/reference/select-network",
    key_url: "https://www.trongrid.io/",
    free_allowance: "Per your TronGrid console",
    key_storage: null,
    requests_today: 0,
    last_error: null,
    adapter_available: true,
  },
  {
    id: "tonapi",
    name: "TonAPI",
    role: "account_data",
    key_requirement: "recommended",
    networks: ["ton"],
    docs_url: "https://docs.tonapi.io/tonapi",
    key_url: "https://tonconsole.com/",
    free_allowance: "1 request/sec with a free key",
    key_storage: null,
    requests_today: 0,
    last_error: null,
    adapter_available: true,
  },
];
