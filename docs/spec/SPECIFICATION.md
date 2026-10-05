# Portfolio Desk — Implementation Specification

Version 1.0 · 2026-10-04 · Personal use · Windows first

## 1. Product objective

Build a local desktop application that answers:

1. What cryptocurrencies do my tracked addresses hold?
2. What are those holdings worth in USD now?
3. How have prices and portfolio values changed?
4. What happened on each address, across all available history?
5. What profit or loss can be calculated reliably, accounting for purchases, sales, fees, and transfers between my own addresses?

Use Tauri 2, React, TypeScript, Rust, and SQLite. The visual reference is Ledger Live desktop, adapted to an information-only product. Detailed visual requirements are in [DESIGN.md](DESIGN.md).

## 2. Non-negotiable boundaries

- Read public blockchain and market data only. Never generate/import seed phrases or private keys, sign messages/transactions, broadcast transactions, approve tokens, or connect a signing wallet.
- No Send, Swap, Buy, Sell, Stake, or hardware-device management action. A historical transaction may have one of those labels; the application cannot execute it.
- HTTP POST is allowed for read-only market/JSON-RPC requests. Enforce a semantic allowlist of RPC methods and endpoints, not a blanket POST ban.
- No required cloud backend, account, subscription, webhook receiver, or cloud synchronization.
- Keep addresses, groups, accounting decisions, history, and caches locally. Selected providers necessarily see the public addresses queried; explain this once during address/provider setup.
- API credentials identify the user's provider account. They are not wallet keys and must be kept separate from portfolio data.
- Default to recurring free API plans. No automatic purchases, paid fallbacks, x402/MPP payments, or quota evasion through multiple accounts.

## 3. Release scope

### 3.1 Platforms

| Target | Requirement |
|---|---|
| Windows 11 x64 | Required first release: native Tauri application, installer, real Windows tests |
| Windows ARM64 | Architecture must not preclude it; separate build/testing is later work |
| macOS | Shared core and frontend must remain portable; packaging and platform verification are a later milestone |
| iOS | Keep mobile-compatible core and responsive screens; shipping/signing and device validation are later work |
| Linux | May run core tests; not a required end-user release |

Windows 10 is not an initial acceptance target. Do not infer support merely because a dependency can run there. Windows uses WebView2; Apple platforms use system WebKit. iOS development requires a Mac and Xcode; it is not a Windows cross-compilation deliverable. See [SOURCES.md](SOURCES.md), T1–T3.

### 3.2 Required first-release networks

| Network | Canonical network identity | Required fungible assets |
|---|---|---|
| Bitcoin mainnet | Bitcoin mainnet namespace | BTC |
| Ethereum mainnet | EVM chain ID 1 | ETH and discovered ERC-20 assets |
| Base | EVM chain ID 8453 | Native ETH and discovered ERC-20 assets |
| Arbitrum One | EVM chain ID 42161 | Native ETH and discovered ERC-20 assets |
| Optimism | EVM chain ID 10 | Native ETH and discovered ERC-20 assets |
| Polygon PoS | EVM chain ID 137 | Current native asset metadata and discovered ERC-20 assets |
| BNB Smart Chain | EVM chain ID 56 | BNB and discovered BEP-20/ERC-20-compatible assets |
| Solana mainnet | Solana mainnet namespace | SOL, SPL fungible tokens; Token-2022 where data is available |
| TRON mainnet | TRON mainnet namespace | TRX and TRC-20 assets, including USDT |
| TON mainnet | TON mainnet namespace | TON and Jettons |

This is a finite release matrix, not a promise to parse every blockchain or smart contract. Other Zerion-supported EVM networks may be enabled only after passing the same adapter checks. Testnets are separate development configurations and never enter a mainnet portfolio total.

For each network, support current balances, paginated address activity, incremental updates, native fees where attributable, token metadata, and honest coverage reporting. Complex or unsupported activity remains visible with an explanation. Do not label it fully interpreted merely because its raw record was downloaded.

### 3.3 Included and deferred capabilities

Required: portfolio/asset/account/group views, price and value charts, address import, grouping, transaction details, synchronization controls, local accounting, missing-basis editing, generic CSV basis import, CSV export, backup/restore, provider settings, English/Russian UI, dark/light/system themes, and privacy mode.

Deferred: exchange trading APIs, automatic exchange CSV dialects, NFTs and collectibles valuation, complete DeFi position accounting, lending liabilities, automatic bridge attribution, derivatives, tax forms, push notifications, WebSocket streaming, cloud sync, mobile distribution, address-name services, Bitcoin extended public key/descriptor discovery.

Deferred assets/activity must not silently distort the supported portfolio: show a scoped notice when known DeFi/NFT/locked assets are excluded. Do not include a broad provider portfolio total while displaying only simple wallet balances.

## 4. Domain terminology and identity

- **Portfolio:** all active tracked accounts in the local profile.
- **Wallet:** a user-named container, such as “Cold storage,” which can contain several chain-specific accounts. It stores no signing material.
- **Account:** one normalized address on one network. A BTC account in v1 is exactly one address, not the entire HD wallet.
- **Group:** a named, optional collection of wallets. A wallet may belong to several groups. Groups are filters, not additional balances.
- **Holding:** an account's quantity of a canonical on-chain asset at an observation point.
- **Asset:** native asset or contract/mint/master address identified together with its network.
- **Display asset:** an optional verified grouping of compatible asset representations for the UI. It never replaces chain-specific accounting identity.
- **Activity:** a user-facing operation composed of one or more chain transactions and normalized legs.

Identity rules:

1. Uniqueness is `(network_id, canonical_address)` for accounts and `(network_id, asset_kind, canonical_asset_identifier)` for assets. Do not identify tokens by symbol.
2. One account belongs to one wallet container. Adding an existing account offers navigation or an explicit move; it never creates a duplicate economic balance.
3. Group totals use the union of account IDs. Never sum group totals to compute the portfolio when groups overlap.
4. Preserve EVM checksummed display but compare canonical address bytes. Solana addresses remain case-sensitive. Normalize TON friendly/raw representations to workchain plus account ID while validating network flags. Normalize TRON Base58Check/hex representations correctly. Validate Bitcoin address checksum and network using a library.
5. EVM addresses do not identify a network: require the user to select networks, with optional supported-network discovery and review.
6. ERC-20 USDT, TRC-20 USDT, and other representations remain separate assets. Cross-network display aggregation is opt-in and uses per-representation prices. Wrapped, bridged, staking, and receipt tokens are not automatically interchangeable with native assets.
7. Zero-balance historical accounts/assets remain queryable. Hiding an asset visually does not delete its transactions or cost basis.

Keep ownership knowledge separate from view inclusion: an archived account can still be a known owned counterparty. Historical wallet/group charts use the **current selected account set over the requested historical interval**. Editing group membership changes that historical scope; it does not create an on-chain cash flow. Historical membership replay is deferred, and the scope tooltip must explain this rule.

## 5. Required user flows

### 5.1 First launch

Show a short welcome explaining public-address tracking, local storage, and provider queries. Offer “Add address,” “Configure data sources,” and an explicitly labeled demo mode. The app must also open with no keys and display useful setup instructions.

Select UI language from the OS where supported; otherwise English. Use USD as the fixed v1 reporting currency. Read the display timezone from the OS and allow a settings override. Persist timestamps in UTC.

### 5.2 Add and manage wallets/accounts

Create a wallet name, choose network(s), enter public address, validate locally, show the canonical address, then start background synchronization. Batch paste/import public addresses with per-row validation and a preview; do not query invalid rows.

Show “An address is only part of a Bitcoin HD wallet” for BTC entry. Users can add multiple known receive/change addresses to one wallet. Do not pretend to discover all BTC addresses from one address.

Support rename, move account to another wallet, archive/unarchive, group assignment, and explicit permanent local removal. Archiving excludes an account from current portfolio views while preserving history and transfer lineage. Before removal, show affected accounts and accounting dependencies. Group deletion never deletes wallets or accounts.

### 5.3 Portfolio home

- Total valued holdings in USD; indicate incomplete price/balance coverage beside the total.
- Portfolio value chart with 24H, 7D, 1M, 3M, 1Y, ALL, and custom range controls.
- Summary fields: unrealized P&L USD and %, period gain USD, period return %, and last successful synchronization. Do not use ambiguous “Profit” for different metrics.
- Asset table with icon/name/symbol, network indication, current USD unit price, balance in asset units, total USD value, and **24h market price change %**. Value descending by default; sorting/search/filtering supported.
- Optional row sparkline, allocation %, and unrealized return columns. The required columns must remain visible at the target desktop size.
- Clearly scoped view: all wallets, a wallet, a group, or selected accounts. Overlapping selections are deduplicated.
- Recent activity preview with “View all.”

A missing quote means `—`, not $0. A failed balance fetch keeps the previous observation with a stale marker; it does not erase the holding. If nothing has a valid valuation, show “Value unavailable,” not a misleading $0.00.

Spam exclusion is an explicit valuation policy distinct from merely collapsing/hiding a table row. Unverified assets may be quarantined from the valued total with an excluded-item count and a review action. Do not permanently delete them, silently omit a verified holding, or trust an implausible quote from an unsolicited token.

### 5.4 Asset detail

Show asset/network identity and explorer link, current price, market change, owned quantity, owned USD value, remaining cost basis, unrealized P&L, realized P&L, and coverage. Provide **Price** and **Your holdings value** chart tabs. These are different series.

Display holdings split by wallet/account/network and an activity table filtered to that asset. Cross-network aggregation retains drill-down rows; no single unit price is shown for representations with materially different prices.

### 5.5 Wallet, account, and group detail

Show value, chart, holdings, period performance, lifetime accounting breakdown, and transaction list for the selected scope. Wallet views expand to accounts; groups expand to wallets. An individual account exposes its address, network, explorer, sync progress, earliest covered date/block, and completeness by activity category.

### 5.6 Activity and transaction details

Columns: timestamp, operation type, status, asset movement, fee, wallet/account, network, historical USD value if available. Sort stably by chain order and timestamp; filter by date, network, wallet/group, asset, type, status, and unresolved items.

Detail drawer: transaction identifier, chain position, involved addresses, all asset legs, fee payer/fee, explorer link, decoding provider, data timestamps, classification, price/basis provenance, and errors. Display one collapsed activity with expandable legs for a swap; do not hide multi-asset transactions behind one transfer amount.

Show failed calls and approvals when the source exposes them even if they move no tokens. Failed transactions must not apply their intended token movements. Actually paid fees may still apply.

### 5.7 Correct incomplete accounting

Provide “Review missing data” filters and editing for acquisition basis/date, unknown incoming transfers, unknown outgoing transfers, disposal proceeds, transfer pairing, reward classification, and historical price overrides. Preserve the original observation and a versioned user override. Show a recalculation preview before applying multi-row imports or bulk changes.

Generic CSV import annotates existing events or creates explicit opening lots; it must not invent a second balance for an existing on-chain receipt. Its schema and matching rules are in [ACCOUNTING.md](ACCOUNTING.md).

### 5.8 Settings and operations

Data Sources: key entry, replace/remove, small connection test, supported features, key status, configured quota, usage estimate, next reset if known, last error, and provider documentation link. Never claim local usage estimates include requests made by other applications.

General: language, theme, timezone, refresh intervals, privacy mode, data location display, diagnostics export. Data: backup, restore, CSV export, cache cleanup, rescan selected accounts. Show progress and support cancellation at safe checkpoints.

Do not show implementation vocabulary such as “RPC cursor” in normal portfolio screens. Technical details belong in diagnostics and source settings.

## 6. Architecture

### 6.1 Component responsibilities

| Component | Responsibility |
|---|---|
| React + TypeScript | Accessible UI, routing, view state, formatting, charts, typed IPC calls |
| Tauri 2 shell | Native window, lifecycle, dialogs, narrowly scoped OS integration |
| Rust core | All provider HTTP/RPC, validation, synchronization, normalization, valuations, accounting, persistence, secrets access |
| SQLite | Local transactional store, migrations, indexes, cached observations, audit history |
| OS credential adapter | Persist user-entered API credentials outside SQLite and the renderer bundle |

Suggested implementation choices: Vite, React Router, TanStack Query for IPC-backed query caching, TanStack Table/Virtual for large lists, an accessible component primitive library, and Apache ECharts with modular imports for time series. Keep styling driven by the tokens in DESIGN.md. These supporting libraries may be changed for a concrete compatibility reason; the approved core stack may not.

Rust: Tokio, reqwest with TLS, serde, SQLx with SQLite and migrations, arbitrary-precision integer/decimal arithmetic, structured logging with redaction, and small provider adapter modules. Verify current crate versions/MSRV and Apple-target support before pinning them. Do not hard-code speculative “latest” versions into this specification.

SQLx is the single database access layer. Do not expose arbitrary SQL or a second frontend SQL plugin. Export TypeScript DTOs from Rust or maintain a checked schema so types cannot drift unnoticed.

### 6.2 Core interfaces

Define `MarketDataProvider`, `AccountDataProvider`, `HistoricalPriceProvider`, `SecretStore`, `Clock`, and `HttpTransport` abstractions. Providers advertise capabilities **per network and endpoint**, including balance/history/fees/internal transfers, pagination, granularity, earliest available data, plan requirements, and historical charts.

Use typed commands such as `list_holdings`, `get_portfolio_summary`, `get_chart`, `list_activity`, `add_account`, `start_sync`, `cancel_sync`, `update_basis`, `test_provider`, `save_provider_key`, `export_backup`. These names are a contract outline, not a requirement to expose every internal function.

Do not expose `fetch_any_url`, arbitrary RPC, arbitrary SQL, arbitrary shell, or a command that returns stored keys. Never let an API response supply an unrestricted credential-bearing next URL.

Long jobs return a job ID and publish bounded progress events. Cancellation persists the last committed checkpoint. Ordinary DB/HTTP work must not block the WebView or a Tauri command dispatcher thread.

### 6.3 Packaging and future platforms

Keep pure domain logic in a Rust library independent of Tauri and desktop APIs. Put credential storage, filesystem pickers, and lifecycle behind platform adapters. Do not assume continuous background execution on iOS; later mobile synchronization runs primarily on foreground/resume and permitted system opportunities.

Use application data directories from Tauri APIs. No database next to the executable and no hard-coded Windows drive paths. Build with lockfiles and a documented Rust toolchain/Node version.

Deliver a per-user Windows NSIS installer with an appropriate WebView2 runtime setup strategy. Test the supported online-bootstrap installation path; an offline installer can be a separate optional artifact. Do not require administrator rights for normal use. If signing credentials are unavailable, label the build unsigned and report it; do not claim signed distribution. Automatic application updates are deferred.

## 7. Data model and precision

Minimum logical tables/entities:

| Entity | Required fields / invariant |
|---|---|
| `wallets` | ID, label, archived flag, creation time |
| `accounts` | ID, wallet ID, network, canonical/display address, archived flag; unique network/address |
| `groups`, `group_wallets` | Many-to-many membership; no duplicate relationship |
| `assets` | Network, kind, contract/mint/master ID, decimals, name/symbol, metadata provenance |
| `asset_mappings` | Provider ID, optional display grouping, confidence, manual verification, effective version |
| `balance_observations` | Account, asset, exact quantity, observed time, chain height/hash or slot if known, provider, status |
| `chain_transactions` | Network + canonical transaction identity, order, time, finality/status, sanitized source references |
| `activity_legs` | Stable leg identity, account, asset, signed raw quantity, direction/type, source evidence |
| `transaction_fees` | Transaction, payer, fee asset/quantity, attribution quality; charged once per economic scope |
| `sync_checkpoints` | Provider/network/account/category, backfill cursor, forward cursor, boundary, coverage, retry state |
| `prices` | Asset/provider, exact USD price, requested and observed timestamps, granularity, quality |
| `accounting_overrides` | Versioned classification/basis/price edits, link to immutable evidence |
| `lots`, `lot_consumptions` | Account/asset, acquisition lineage, quantity/basis remaining, transfer lineage, method version |
| `portfolio_snapshots` | Scope-independent account/asset observations for charts, quality, valuation version |
| `import_batches` | File hash, mapping, row IDs, review/commit status, duplicate detection |
| `provider_usage`, `settings`, `schema_migrations` | No plaintext secrets |

Normalize transaction identities carefully. EVM logs, native movements and trace paths, BTC output/input references, Solana instruction paths/signatures, and TON account transaction logical-time/hash identities are not interchangeable. A transaction hash alone is insufficient to deduplicate every token movement. Retain source IDs alongside canonical IDs; conflicting evidence becomes a reviewable conflict rather than duplicated money.

Store integer token quantities as canonical decimal text or an exact binary representation, not SQLite REAL and not an overflowing signed 64-bit value. Support 256-bit on-chain integers. Store decimal prices/basis as exact decimal strings with explicit arithmetic rules. JavaScript DTOs use strings for quantities, prices, and monetary results; JS numbers are permitted only for chart coordinates and presentation after bounded conversion. Never calculate accounting in floating point. SQLite REAL is approximate [SOURCES.md, D1](SOURCES.md).

Use arbitrary-precision decimal calculations with at least 50 significant digits for intermediate ratios. Preserve original quantities and source precision. Round USD only for display/export formatting, normally half-even to cents; keep full precision in calculations. Allocate split-lot residuals deterministically so children sum exactly to the parent. No per-leg rounding before aggregation.

Enable foreign keys, WAL with sensible checkpointing, a busy timeout, and transactional migrations. Serialize writes; allow bounded read concurrency. Record schema and accounting-engine versions. Index account/network/time, asset/time, transaction identities, and unresolved status. Use keyset pagination for large activity lists.

## 8. Synchronization

### 8.1 Initial synchronization

1. Validate account locally and resolve enabled provider capabilities.
2. Fetch current balances first for fast initial usefulness; label history/accounting as loading.
3. Capture a chain/provider upper boundary where possible.
4. Backfill all available pages for each required activity category. Store raw sanitized evidence and normalized results transactionally with its checkpoint.
5. Detect repeated cursors, gaps, provider caps, and unavailable historical ranges. A stopped page budget is “paused,” not “complete.”
6. Run a forward overlap pass to catch activity arriving during backfill.
7. Reconcile normalized holdings against a compatible balance observation. Differences become explicit unresolved amounts, never fabricated purchases.
8. Resolve prices and accounting asynchronously. Balance availability does not imply complete P&L.

### 8.2 Incremental synchronization

Persist both backfill and forward progress independently. Re-fetch a configurable recent overlap, deduplicate idempotently, update pending/final status, and account for chain reorganization. Use network-specific finality policy and store the boundary used; do not impose one universal confirmation count on every chain.

A reorg invalidates affected legs, lots, summaries, and derived historical points, followed by deterministic replay from a prior safe checkpoint. Preserve user overrides and flag any whose underlying event disappeared.

Do not mark a holding zero because it is missing from a partial/paginated response. Replace a full holding set only after the relevant balance scan completes successfully. Reconciliation must compare compatible heights/slots where supported; otherwise record observational skew and retry before declaring a discrepancy.

### 8.3 Scheduling and errors

All components share a provider/account quota scheduler; the UI cannot bypass it. Batch prices, coalesce identical requests, reuse metadata, and avoid one HTTP call per row. Priority: explicit user request, active view, foreground balance updates, background history, optional enrichment.

Defaults and budgets are in API_PROVIDERS.md. Pause regular polling while the app is closed; do not install a service. Minimized apps reduce polling. On resume perform one coalesced catch-up, not a burst of overdue jobs.

Honor `Retry-After` and provider rate-limit information. Retry transient failures with exponential backoff/jitter and a bounded attempt count. Authentication/entitlement failures need configuration changes, not an endless retry loop. Classify HTTP errors and provider errors inside HTTP 200 responses. An indexed “not ready” response becomes a pending job. Stop safely on exhausted quota and resume after the known reset or a user retry.

### 8.4 Completeness model

Keep independent status dimensions:

- Current balance: fresh / stale / missing / conflicted.
- History: loading / paused / available-range complete / partial / unsupported.
- Decoding: interpreted / partially interpreted / raw-only.
- Price: current / stale / missing / manually supplied / low-confidence.
- Basis: known / estimated / partially known / unknown.

“Available-range complete” means pagination was exhausted for documented provider categories between explicit boundaries. It does not prove all possible blockchain activity has been indexed. The UI must expose missing categories and earliest coverage without overwhelming the main table.

## 9. Chart requirements

Asset market chart = historical price. Holdings value chart = historical quantity times historical price. Never multiply today's quantity by old prices and call it historical portfolio value.

Use one shared UTC grid per requested range. Reconstruct quantities from supported activity only when history and opening state suffice; otherwise use stored observations and show “History available since …”. A provider's wallet chart is optional separate-source enrichment with its own scope/coverage, not an interchangeable replacement for local group history.

Source series carry timestamps, interval, quality, and scope. Mark gaps instead of connecting across long missing intervals or using current prices to fill the past. Do not extrapolate before the earliest valid data. A coarse historical point used within the configured tolerance is labeled estimated; do not present daily prices as tick-accurate execution prices.

Separate the original acquisition timestamp from the timestamp inventory entered a tracked account. An exchange purchase dated before an on-chain deposit supplies basis lineage; it must not make the deposit address appear to have held those coins before arrival.

Cache market series by asset/source/range/resolution. Persist account-level observations and aggregate deduplicated accounts at view time. Use bounded chart point counts (target <= 1,000 displayed points) while preserving exact source/ledger data. Snapshots taken while the app is open cannot by themselves recreate closed-app periods.

## 10. Local security, privacy, and resilience

Store keys using Windows Credential Manager/DPAPI through a tested Rust adapter; use Keychain equivalents when Apple targets are implemented. If persistent secure storage is unavailable, offer session-only keys and report the limitation; never silently write plaintext keys to SQLite. Credential entry briefly exists in renderer memory, then goes to Rust and is cleared. Readback commands return configured/not-configured status only.

Test secrets come from process environment or an explicitly loaded `.env.test.local`; production does not scan arbitrary project directories for dotenv files. No `VITE_*` secret variables. Mask raw keys, Basic-auth encodings, URL query/path credentials, headers, and serialized errors in all logs. Use fake credentials in masking tests.

Restrict Tauri permissions and custom IPC command access explicitly. Tauri's documented defaults do not automatically restrict every registered custom command [SOURCES.md, T4](SOURCES.md). The Rust HTTP client enforces host/method rules itself; Tauri plugin scopes do not automatically constrain reqwest.

Render untrusted labels as text. No remote executable UI, arbitrary HTML, or token-provided scripts. Cache sanitized/raster token icons with size/content-type limits and a fallback monogram. Validate explorer URL hosts and open them externally. Never forward keys to a redirected or provider-supplied foreign host.

Production CSP allows bundled assets and only the local resources needed by the app. Do not loosen it to make a development tool work. No analytics or telemetry by default. Privacy mode masks visible balances/addresses and excludes sensitive values from application screenshots made through an app export feature, if provided.

Keep test/demo profiles separate from the real profile. Release builds must exclude test HTTP overrides, arbitrary fixture injection, embedded automation servers, and debug-only commands.

## 11. Import, export, backup, and recovery

CSV exports include UTC timestamps, network/asset identities, exact quantities, valuation source, basis/coverage state, and explicit metric labels. Escape spreadsheet-formula-like text fields and preserve numeric strings. Never export keys.

Backups contain a consistent SQLite snapshot, version manifest, optional sanitized cached evidence, and checksums. Use SQLite backup facilities or another consistent snapshot mechanism; do not copy only a live database file while ignoring WAL [SOURCES.md, D2](SOURCES.md). Keys are excluded and must be re-entered after moving machines.

Restore validates archive paths, size limits, checksums, schema compatibility, and database integrity before replacing data. Create a safety snapshot of the current profile, close writers, and replace atomically. Reject unsupported future schemas; never guess a downgrade. Interrupted import/restore/migration must leave a recoverable previous state.

Cache cleanup never deletes user overrides or acquired lot lineage. A rescan rebuilds imported observations and derived results while retaining reviewed decisions where their evidence still matches.

## 12. Performance targets

Targets are acceptance goals measured on a reported Windows test machine, not guarantees derived from framework marketing.

- Local cached first useful screen within 2 seconds after app process startup on a typical SSD-equipped development PC.
- Target dataset: 50 accounts, 500 asset identities, 100,000 normalized activity legs. Cached view queries p95 < 300 ms excluding rendering; report actual dataset and machine.
- Virtualize long tables; avoid rendering the entire history. Sorting/filtering large histories happens in Rust/SQLite.
- Do not freeze the UI during imports, replay, backup, or synchronization. Cancellation feedback within 1 second; the in-flight request may finish before checkpointed cancellation.
- Keep default network use within provider budgets for the worked example in API_PROVIDERS.md. Report actual request/credit consumption.
- Opening a cached page, toggling theme, changing a sort order, or regrouping wallets must not restart full blockchain history downloads.

## 13. Delivery stages and acceptance

| Stage | Deliverable and exit condition |
|---|---|
| A. Foundation | Tauri app, typed IPC, DB migrations, exact domain types, isolated demo profile, source configuration, design shell |
| B. First vertical slice | BTC + Ethereum + price source: add address, current holdings, actual history pagination, persistence, real API tests |
| C. Accounting and history | FIFO lots, transfer pairing, fees, basis corrections/CSV, correct charts, deterministic cases and reconciliation |
| D. Required networks | Base, Arbitrum, Optimism, Polygon, BNB, Solana, TRON, TON; per-network capability/coverage report and live evidence |
| E. Product completion | Groups, languages/themes, backup/restore/export, quota handling, accessibility, Windows installer, final screenshots/report |

The first full release requires A–E. A provider outage or missing credential may block a validation but must not be disguised as completed coverage. Do not implement every optional fallback before completing the required user flows.

Definition of done:

- Required screens and networks meet the documented acceptance matrix.
- No economic double counting from groups, own transfers, fees, provider fallback, or multi-leg activities.
- Unknown data stays unknown, with a path for user correction.
- Mandatory checks in TESTING.md pass or have an explicitly blocking status; the release is not declared fully verified while a required gate is blocked.
- A clean Windows installation can add keys/addresses, synchronize, close/reopen, export/restore, and reproduce the tested values.
- Deliver reproducible commands, sanitized evidence, known limitations, and no credentials/test servers in release artifacts.
