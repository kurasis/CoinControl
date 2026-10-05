# CoinControl

A personal, read-only cryptocurrency portfolio tracker for Windows, built with
Tauri 2, React, TypeScript, Rust, and SQLite. The working product name inside
the application is **Portfolio Desk**.

The authoritative product specification lives in [`docs/spec`](docs/spec/README.md).
Start with its README, which lists the reading order.

## Status

The project follows the delivery stages in
[SPECIFICATION.md §13](docs/spec/SPECIFICATION.md#13-delivery-stages-and-acceptance).

| Stage                     | State                                                                                         |
| ------------------------- | --------------------------------------------------------------------------------------------- |
| A. Foundation             | Implemented. See [TEST_REPORT.md](TEST_REPORT.md).                                            |
| B. First vertical slice   | Implemented: Bitcoin, Ethereum, prices, live API tests.                                       |
| C. Accounting and history | Implemented: FIFO replay, pairing, review, CSV, history charts.                               |
| D. Required networks      | Implemented: all 10 networks sync; see the coverage table.                                    |
| E. Product completion     | Recovery and operational UI implemented; native/release acceptance tracked in TEST_REPORT.md. |

Addresses on all ten required networks (Bitcoin, Ethereum, Base, Arbitrum,
Optimism, Polygon, BNB Chain, Solana, TRON, TON) synchronize real balances and
history, held assets are priced in USD, and every movement is replayed into FIFO
cost-basis lots with realized/unrealized P&L, fees and flow-adjusted period
returns. The separate
**demo profile** uses synthetic data, is labeled as such, and never contacts a
provider.

## Synchronization (stage B)

| Network / data      | Source                                   | Key                     |
| ------------------- | ---------------------------------------- | ----------------------- |
| Bitcoin             | Blockstream Esplora (public)             | none                    |
| Ethereum            | Zerion (simple positions + transactions) | `ZERION_API_KEY`        |
| Native asset prices | Live Coin Watch (curated native codes)   | `LIVECOINWATCH_API_KEY` |
| Token prices        | DefiLlama by contract identity           | none                    |

- A new address synchronizes right after it is added. A background sweep runs
  at startup and then every `sweep_interval_minutes` (default 60); prices
  refresh every `price_refresh_seconds` (default 60). **Wallets → Sync now**
  runs a sweep immediately.
- History downloads newest first. Each run reads at most 8 pages per account;
  older history continues from a saved cursor on later runs, so a large wallet
  imports over several sweeps without exhausting free quotas. The Wallets
  screen shows "Loading history" until the import completes.
- All writes are idempotent: repeated pages, overlapping runs and restarts
  never duplicate transactions. Pending Bitcoin transactions are kept apart and
  marked `reorged` if they disappear.
- A failed refresh keeps the last balances as **stale**, never zero. An asset
  with no usable quote is **unpriced**, never $0. Tokens are never priced by
  ticker; without a Live Coin Watch key, native BTC/ETH fall back to DefiLlama's
  CoinGecko identities.
- Local request budgets: Zerion 1,600/day, Live Coin Watch 9,000/day, public
  services at most one request per second. Requests are counted per day and
  shown on each data source card.
- Debug builds also read provider keys from the environment variables above for
  development. Release builds read keys only from the OS credential store.

## Required networks (stage D)

| Network                                      | Source                                     | Key                         |
| -------------------------------------------- | ------------------------------------------ | --------------------------- |
| Base, Arbitrum, Optimism, Polygon, BNB Chain | Zerion, filtered to the chain              | `ZERION_API_KEY`            |
| Solana                                       | Zerion (SOL and SPL by exact mint)         | `ZERION_API_KEY`            |
| TRON                                         | TronGrid (TRX, staking, TRC-20)            | `TRONGRID_API_KEY`          |
| TON                                          | TonAPI (account events, Jettons by master) | `TONAPI_API_KEY` (optional) |

**Settings → Networks** lists, per network, the provider, which history
categories are imported, and every known limitation (for example: TRON staked
TRX counts toward the holding, TRC-10 tokens are not tracked; TON records one
event per trace). The full table with live evidence is in
[docs/NETWORK_COVERAGE.md](docs/NETWORK_COVERAGE.md). Local request budgets:
TronGrid and TonAPI 5,000/day each.

## Accounting and history (stage C)

The rules are in [ACCOUNTING.md](docs/spec/ACCOUNTING.md); this is how the
build applies them.

- **Deterministic replay.** Lots, consumptions, realized/income/expense totals,
  external flows and review items are derived data. They are rebuilt in full
  from synchronized history plus user decisions whenever something changed (a
  sync, a decision, an import, new historical prices, or an accounting engine
  upgrade) and never edited in place. Replaying twice gives identical results.
- **FIFO per account and asset.** Lots keep their original acquisition date
  through own transfers. A receipt from outside the tracked accounts has
  **unknown** basis until the user decides; it is never treated as zero.
- **Transfer pairing.** An outgoing and an incoming movement of the same asset
  between two tracked accounts in one transaction are paired automatically;
  other pairs (for example an exchange hop) can be paired by hand. Pairing moves
  basis and lineage; a received amount below the sent amount is expensed as a
  transfer cost.
- **Fees** are charged once, to the payer, at their historical value, and
  consume FIFO lots of the fee asset.
- **Review missing data** (sidebar → Review) lists receipts with unknown basis,
  unclassified outgoing movements, sales without proceeds and movements without
  a historical price, plus reconciliation findings (inventory gaps, balance
  differences on complete history, opening-balance overlaps, decisions whose
  movement disappeared). Each decision is stored as a new version; the original
  observation is kept and the history is shown in the movement drawer.
- **Partial results stay partial.** Unrealized P&L is shown in full only when
  every remaining lot has a basis; otherwise the known-basis subset and its
  coverage are shown. Realized, income and expenses are marked partial when a
  component is missing, and the period return (Modified Dietz) is `—` with a
  reason when prices or classifications are missing.
- **Historical prices.** Daily USD history comes from DefiLlama
  (`coins.llama.fi/chart/{coin}?start&span&period=1d`, at most 500 points per
  request) for assets that have a market quote. Downloaded ranges are
  remembered, so each day is fetched once; a sync makes at most 12 history
  requests and continues on the next sweep.
- **Charts** use historical quantities (reconstructed from history and balance
  observations) times the price within tolerance (tick/minute 5 minutes, hour
  1 hour, day 24 hours, marked estimated). There is no extrapolation before the
  earliest data. The asset page shows **Price** and **Your holdings value** as
  separate series.

### Cost basis CSV

**Review → Import cost basis CSV** previews every row, the column mapping and
the before/after recalculation; nothing is written until **Apply**, which
commits all rows in one transaction. Columns (header names are matched
automatically and can be remapped):

```text
external_row_id,network_id,account_address,transaction_id,leg_id,
asset_identifier,quantity,acquired_at_utc,total_basis_usd,
basis_kind,classification,note,opening_cutoff_utc
```

- Required: `network_id`, `account_address`, `quantity`. Amounts are exact
  decimals; a blank cost means unknown, `0` is an explicit zero. Timestamps are
  ISO 8601 with a timezone.
- A row annotates an existing receipt (matched by account, transaction, asset
  and optionally `leg_id`); several rows may split one receipt into lots with
  earlier acquisition dates, up to its original quantity. `classification`
  (`deposit`, `reward`, `withdrawal`, `gift`, `own_untracked`, `sale`,
  `payment`) classifies the matched movement.
- `classification=opening` with `opening_cutoff_utc` creates an explicit opening
  lot; movements of that account and asset before the cutoff are then ignored
  and listed for reconciliation.
- Rows are identified by `external_row_id` (or a content hash). Rows and files
  that were already imported are detected and skipped. Limits: 5 MB, 10,000
  rows.

## Requirements

- Node.js 22 (see `.nvmrc`) and npm 10
- Rust 1.97 (pinned in `rust-toolchain.toml`; `rustup` installs it automatically)
- Windows 11: WebView2 (preinstalled on Windows 11) and the MSVC build tools
- Linux (development only): the
  [Tauri system dependencies](https://tauri.app/start/prerequisites/#linux),
  e.g. `libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev librsvg2-dev`

## Commands

```sh
npm ci                    # install frontend dependencies
npm run tauri dev         # run the desktop app with hot reload
npm run dev               # browser preview with a mock IPC backend (UI work only)
npm run check             # tsc, ESLint, Prettier, rustfmt, Clippy (-D warnings)
npm run test:offline      # all Rust tests + Vitest; no network
npm run test:live         # opt-in live provider tests (see below)
npm run test:performance  # release-mode synthetic 50-account / 100000-leg load
npm run build:windows     # unsigned NSIS installer (run on Windows)
npm run gen:bindings      # regenerate src/ipc/bindings from the Rust DTOs
```

`npm run test:e2e:windows` drives a real Windows binary; `npm run verify:release`
inspects production resources and the installer. Their Windows scopes require a
Windows runner; portable inspection is available with `--source-only`.

The browser preview (`npm run dev`) uses an in-memory mock of the IPC layer so
screens can be developed without the Rust backend. It is excluded from
production builds and is never a substitute for native testing.

## Entering your own API keys

Portfolio Desk never ships with keys. Each user supplies their own:

1. Open **Settings → Data sources**.
2. Pick a provider, follow its **Get a free key** link, and paste the key.
3. Press **Save**. The key goes to the operating system credential store
   (Windows Credential Manager). If the OS store is unavailable, the key is kept
   in memory for the current session only and the card says so. Keys are never
   written to SQLite, logs, or exported files, and the app has no command that
   reads a key back.
4. **Remove** deletes the key from the credential store.

Sources that need no key (Blockstream Esplora, DefiLlama) show "No key needed".
TonAPI works without a key at a slower rate. Optional sources that this build
does not use (Helius, Alchemy, Ankr) say so on their cards.

### Keys for live tests

Live tests read keys only from environment variables or an untracked
`.env.test.local` (template: [`docs/spec/.env.example`](docs/spec/.env.example)).
Never use `VITE_`-prefixed variables for keys: Vite embeds them in the client
bundle.

```sh
cp docs/spec/.env.example .env.test.local   # fill in only the keys you have
RUN_LIVE_API_TESTS=1 LIVE_TEST_PROVIDERS=esplora,zerion,livecoinwatch,defillama,trongrid,tonapi npm run test:live
```

The script prints only whether each key is configured, never its value. It
refuses to run when a selected provider has no key, caps every provider at
`LIVE_TEST_MAX_REQUESTS_PER_PROVIDER` requests (default 50, retries included),
and writes a sanitized report to `target/live-report/LIVE_REPORT.md`. Targets
are public addresses and transactions listed in
[`tests/live/public-targets.json`](tests/live/public-targets.json). CI runs the
same suite in the `live-api` job with the repository secrets
`ZERION_API_KEY` and `LIVECOINWATCH_API_KEY`; TronGrid and TonAPI join the run
when the `TRONGRID_API_KEY` and `TONAPI_API_KEY` secrets are set.

## Architecture

```
crates/portfolio-core    Pure domain logic: exact decimals, network IDs, address
                         normalization (EVM, Bitcoin, Solana, TRON, TON), FIFO
                         ledger with lot lineage, valuation, Modified Dietz
                         return, explorer links. No I/O.
crates/portfolio-store   SQLite via SQLx: migrations, profile isolation
                         (real / demo / test), wallets, groups, holdings,
                         accounting replay, review decisions, CSV import,
                         historical prices, charts, activity paging, settings,
                         demo seed, and the idempotent sync write path.
crates/portfolio-providers
                         Read-only provider adapters (Esplora, Zerion, Live Coin
                         Watch, DefiLlama), shared HTTP transport (pacing,
                         budgets, retries, size caps, redacted errors), and the
                         sync engine (forward pass + resumable backfill).
src-tauri                Tauri 2 shell: typed IPC commands, capability allowlist,
                         CSP, OS credential storage, provider catalog,
                         background sync scheduler.
src                      React 19 + TypeScript UI: portfolio, asset detail,
                         wallets, activity, review and CSV import, groups,
                         settings; EN/RU; dark/light/system themes.
scripts                  check / test / bindings scripts used by npm and CI.
```

Key decisions:

- **Exact arithmetic.** Amounts are `BigDecimal` in Rust and decimal strings
  across IPC. Floats are denied by Clippy (`float_arithmetic = "deny"`).
  The UI formats decimal strings with `Intl.NumberFormat` without converting them
  to `number`.
- **Typed, minimal IPC.** Every command is listed in `src-tauri/build.rs` and
  granted individually in `src-tauri/capabilities/main.json`. There is no
  generic SQL, URL fetch, RPC, or shell command. TypeScript types are generated
  from Rust with ts-rs (`src/ipc/bindings`), and CI fails if they drift.
- **Separate demo database.** Each profile is its own SQLite file
  (`profiles/real.sqlite`, `profiles/demo.sqlite`) stamped with its kind; a
  database is refused if opened as the wrong profile or if it was written by a
  newer schema.
- **Unknown stays unknown.** Missing prices or cost basis are reported as
  unavailable with a reason; charts show gaps instead of zeros, and partial
  totals are labeled as partial.

## Repository layout

| Path                         | Contents                                         |
| ---------------------------- | ------------------------------------------------ |
| `docs/spec/`                 | Product specification and acceptance fixtures    |
| `docs/ENVIRONMENT_REPORT.md` | Development environment report                   |
| `TEST_REPORT.md`             | Verification evidence with PASS/BLOCKED statuses |
| `.github/workflows/ci.yml`   | Linux checks/tests and Windows installer build   |

## Recovery and portfolio controls

Settings → Data saves a `.ccbackup` file with a versioned manifest and SHA-256 checksum. The SQLite image includes committed WAL data, accounts, groups, source evidence, settings and versioned decisions. Provider keys stay in the OS credential store and are not exported. Restore accepts the same profile kind, checks the actual schema and database integrity, and saves the current database under `profiles/safety-backups` before replacing rows in one transaction. The database image limit is 128 MiB.

CSV exports cover current active-account holdings and activity, FIFO lots and the complete decision audit. Quantities and USD amounts remain decimal strings; timestamps are Unix seconds in UTC. Exports use the desktop file picker. User-controlled text is escaped against spreadsheet formula interpretation.

Token controls in Settings → Data let you hide an asset from the holdings list independently of its inclusion in valuation/accounting. Provider spam classification is the default, and an explicit inclusion decision survives later provider metadata refreshes. Original observations and activity remain available.

Address entry accepts up to 50 lines on one explicitly selected network. All lines are normalized before the database transaction; a duplicate or invalid row prevents the batch from being added. Wallet and asset views link to individual account pages. Activity supports account, asset, network, type, status and UTC date filters; portfolio charts support custom UTC endpoints and compute period performance over the same interval.

Synchronization shows account/page progress and can stop after the current bounded request/page. A first import and older backfill save safe cursors; a resumed sweep reuses stored evidence without duplication. BTC synchronization rechecks up to five transactions from the last six blocks and invalidates an old confirmation only after an authoritative transaction lookup, never after a timeout or an omitted history page. Other indexer sources retain their documented coverage limitations.

Own-transfer accounting requires matching sender/recipient evidence in both account views. A multi-party transaction or a source without payment-edge evidence stays available for manual pairing. Same-second transaction order uses numeric block position where reported; absent positions use a deterministic fallback without claiming an unavailable chain index.

## Windows automation

`npm run test:e2e:windows` uses tauri-driver 2.0.5 and the Edge WebDriver matching the runner's installed WebView2 runtime. Build its isolated test target with:

```sh
npm run build
cargo build -p portfolio-desk --features native-e2e --target-dir target/native-e2e
npm run test:e2e:windows -- --smoke-only
```

The deterministic suite drives the real UI/IPC/database, checks a basis edit and isolated Windows credential storage across restart, and exports a backup plus four CSV files through the actual Windows Save dialogs. It restores the matching demo archive into a fresh installation (whose demo destination is initially seeded by the app), compares all four exports and overlapping group membership, and checks that the OS credential is absent from the archive and restored installation. `--smoke-only` permits the separate live gate to remain blocked; it does not skip executed recovery checks. Production builds omit the feature and its data-directory and automation-port overrides.

The separate small live path needs no private keys or Zerion quota. It uses the public BTC manifest target, cancels and resumes synchronization across restart, then blocks the application executable's outbound traffic with an application-scoped Windows firewall rule. It checks cached quantities and stale indicators, removes the rule in cleanup, reconnects and checks that history was not duplicated. Stop remaining price backfill after the failed offline balance read to conserve the shared 50-request public-provider budget. The Windows runner needs permission to manage this isolated firewall rule. Run in PowerShell:

```powershell
$env:RUN_LIVE_API_TESTS = "1"
npm run test:e2e:windows -- --live-btc
```

The separate production job tests an upgrade from the pinned, SHA-256-verified `0.1.0` NSIS artifact (main `f0603fd`, CI `37345501830`) to `0.1.1`. The old installed app creates schema 6; an external offline fixture utility fills the closed database with two synthetic owned accounts, an internal transfer, one fee, two audit versions and overlapping groups. Both installed versions open and replay that data. The check compares exact source/derived table fingerprints and balance quantities after migration to schema 7, another restart and normal uninstall. The baseline CI artifact must remain available; expiry is an explicit prerequisite failure, never a silently substituted baseline. `npm run verify:release` checks command permissions, CSP, excluded credentials/test infrastructure, PE artifacts, installer contents and checksums. `--source-only` performs the portable configuration/frontend subset and does not claim installer verification.

Native acceptance adds the two synthetic addresses and overlapping groups through the actual UI; only deterministic chain evidence is seeded externally while the app is closed. Expected values are independently checked against the production Store APIs: 1.99 ETH at $3000 gives $5970, with one $30 fee. The viewport matrix resizes the actual Windows HWND client area to 1440×900, 1280×800 and 1024×720 CSS pixels, captures EN/RU and dark/light combinations, and checks that all six asset and eight activity columns remain reachable by scrolling. It records WebView2 pixel ratio and Windows window DPI. This does not substitute for changing Windows display scaling to 125%, 150% and 200%; that release gate remains separate. The hosted monitor can be smaller than a requested test window, so the report also records available screen size.

## Local load measurement

`npm run test:performance` creates an isolated synthetic SQLite profile with 50 accounts, 500 token identities and 100,000 normalized legs. No addresses are queried. It measures fixture loading, full accounting replay, 1,000 idempotent ingestion overlaps, cached store reopening/first summary, 25 calls per view (summary, holdings, account holdings, paginated activity and chart), and peak process memory. Reports include p50/p95, first/max query latency, release/debug mode, commit and machine details; p95 >= 300 ms fails the command. CI executes the same workload on Linux and Windows.

Repeated summary/holdings queries use a bounded process-local DTO cache for up to 30 seconds; charts expire after one second so their end time follows the clock. Every SQLite commit invalidates entries through a dedicated connection's `data_version`, including commits by another Store or process. Only unchanged database versions are cached after computation. Account queries restrict lots in SQL. Broad activity scopes use a global time index so the first page does not sort all history; small/selective scopes retain account-first lookup. Two deterministic pages are checked for exact order and no overlap. Cold reads remain visible in the report; memory-cache p95 and store-open timing are not claims about cold native rendering or a full 100,000-row CSV import. In-memory test stores do not use this cache.

## Cloud development

`bash scripts/cloud-setup.sh` installs the pinned Rust toolchain and Node dependencies. In the Debian cloud sandbox, system libraries are extracted under `/workspace/.system`, keeping the host package database untouched. Start later shells with `source /workspace/coincontrol-env.sh`. The setup and its warm rerun were tested; restoring a newly published cloud snapshot has not yet been independently verified.
