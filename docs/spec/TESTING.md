# Mandatory Verification and Release Evidence

## 1. Completion means demonstrated behavior

The developer must implement and execute meaningful tests of balances, accounting, persistence, API adapters, and the actual Windows application. Do not count generated tests, successful compilation, screenshots of a browser mock, or a response with HTTP 200 as sufficient proof.

Use `PASS`, `FAIL`, `BLOCKED`, and `SKIPPED_NOT_IN_SCOPE` consistently. A required missing key, unavailable Windows runner, unavailable network, missing entitlement, or failed provider endpoint blocks the affected acceptance gate. Continue all independent work; report the limitation precisely.

This handoff itself contains no API credentials and claims no executed application tests.

## 2. Credential injection

Preferred: environment secrets injected into the process running Rust integration tests. Local alternative: copy `.env.example` to `.env.test.local` and fill it privately. The developer must explicitly implement loading that exact file for test commands; Rust does not automatically inherit values merely because Vite loaded a dotenv file.

In CI, configure repository/environment secrets and map them into the correct job's process environment. GitHub Actions secrets do not automatically become available to a separate coding agent. Restricted/forked workflows may not receive secrets. Source: S1 in SOURCES.md.

Rules:

- Print only whether each named credential is configured, never its value, prefix, suffix, Base64 form, environment dump, or full endpoint URL.
- Do not put keys in prompts, source, screenshots, fixture recordings, test reports, archives, installer resources, or VITE-prefixed variables. Vite embeds the latter in client code (S2).
- Keys remain usable only in the trusted process's environment/secret adapter. A process that can execute with a secret can access it; do not describe environment variables as making a secret inaccessible to that process.
- Prefer separate development keys where supported. Do not rotate a working key just to run tests.
- Release uses runtime OS credential storage, not bundled test dotenv files.

Implement project commands with the following behavior; these are required interfaces for the future repository, not commands already available in this archive:

| Command | Required behavior |
|---|---|
| `npm run check` | Typecheck/lint plus Rust format/lint checks with clearly reported subcommands |
| `npm run test:offline` | Deterministic domain, DB, adapter fixtures, and frontend component checks; no external network |
| `npm run test:live` | Explicitly selected providers; real read requests; per-provider result/usage report; required missing keys cause nonzero exit |
| `npm run test:e2e:windows` | Launch and drive a real Tauri binary on Windows; report native/browser mode accurately |
| `npm run build:windows` | Build the release application/installer on a supported environment |
| `npm run verify:release` | Check release contents, secret exclusions, production capability configuration, and expected installer artifacts |

Require `RUN_LIVE_API_TESTS=1` and a selected provider list for live requests. That explicit test invocation authorizes bounded read-only queries to those configured providers and public test addresses; do not ask for confirmation before every page. Regular offline/unit tests must never spend API quota.

## 3. Layer A — offline correctness

### Accounting and identity

Implement every case in `fixtures/accounting_cases.json` and additional edge cases:

- FIFO partial sales, lot splitting, fees in another asset, fee-asset realization, zero/unknown basis.
- Own transfers across two accounts and overlapping wallet groups; whole-portfolio and group-scope performance differ correctly.
- Duplicate receipts/provider observations, reimports, opening-lot overlap, and deterministic replay after a user edit.
- Quantities above JavaScript's safe-integer range, 256-bit integers, high-decimal tokens, tiny balances, and rounding residue conservation.
- Same symbol on different contracts/networks; EVM address casing; Solana case sensitivity; TON friendly/raw address equivalence.
- Missing current price, missing historical price, and low-confidence quote; none becomes zero.
- Modified Dietz deposit/withdrawal timing, interval endpoints, zero duration, nonpositive denominator, incomplete classifications.
- LCW ratio-to-percent conversion: 1.05 -> 5%, 0.8 -> -20%, 1 -> 0%.

Expected answers must be calculated independently of the function under test. Do not generate expectations by calling that same function.

### Chain normalization

| Family | Minimum offline fixtures |
|---|---|
| EVM | Native transfer; ERC-20 log; internal movement; swap with several legs; approval with fee; reverted transaction; receipt with extra fee fields; sponsored fee payer |
| BTC | Incoming payment; outgoing with owned change; multiple owned inputs; same tx seen at two accounts; pending confirmation; replaced/reorged tx; mixed-owner fee ambiguity |
| Solana | SOL transfer; SPL transfer via token account; closed historical token account; inner instruction; failed transaction fee; rent refund; wrapped SOL; partially supported Token-2022 behavior |
| TRON | TRX transfer; TRC-20 transfer; paginated history; failed contract call; actual energy/bandwidth fee; empty/partial account response |
| TON | TON transfer; Jetton master identity; multiple messages; bounced/refunded flow; unfinished trace later completed; event/transaction duplicate evidence |

For adapters, maintain sanitized, small fixtures from documented schemas or permitted real responses. Test unknown/new optional fields. Contract changes should cause a meaningful schema/coverage error, not silently discard the whole account or fabricate zeros.

### Reliability and persistence

Use controlled HTTP responses to test 401/403, 429 with and without `Retry-After`, exhausted quota, 5xx, timeout, invalid JSON, provider error inside HTTP 200, too-large response, and repeated cursor. No live quota-exhaustion tests.

Test interrupted backfill and resume, new activity during backfill, cursor expiration and overlap restart, reorg rollback, source switch, delayed indexer response, out-of-order events, and discrepancy with a balance snapshot.

Verify migration rollback/recovery, database reopening, concurrent reader/writer behavior, backups during writes, restore integrity, invalid archive paths, unsupported future schema, duplicate CSV import, invalid decimals, and an import canceled before commit.

## 4. Layer B — real API integration

Use public, non-sensitive test addresses and historical transaction IDs, chosen from current official examples or independently verified public explorers. Commit a test-target manifest with network, address, expected categories, known historical transaction IDs, source URL, and verification date. Do not fill the repository with the user's real portfolio addresses.

Use a small deterministic historical range where possible. Current balances/prices change: assert identity, units, sane schema, timestamp/coverage semantics, and relationships rather than a hard-coded live price. For meaningful balance reconciliation, compare compatible chain boundaries or report and tolerate only documented observation skew.

| Provider | Mandatory live checks when enabled |
|---|---|
| Live Coin Watch | Authenticated USD quote batch for multiple mapped assets; 24h ratio normalization; one historical series with ascending timestamps; missing code handling |
| Zerion | Positions plus history; at least two distinct pages for a chosen active address; correct chain filtering; known historical activity found; no duplicate replay; basic EVM and Solana coverage |
| DefiLlama | Documented free current/historical route resolves a verified token identity; quote timestamp and missing-token handling |
| Esplora | BTC balance/statistics; >= 2 confirmed-history pages using last txid; compute known tx input/output effects |
| TronGrid | Key access; native and TRC-20 history; pagination continuity; known transfer and fee-bearing receipt normalization |
| TonAPI | Account + Jetton holdings + events; pagination; link one event to transaction/trace without double counting |
| Helius, if required by chosen Solana path | Verify actual free entitlement for each method used; raw/parsed data for one historical transaction and relevant token-account activity |
| Alchemy or Ankr, if used | Actual enabled chain/method entitlement, pagination where relevant, receipt/balance cross-check; documented method cost tracking |
| CoinLore, if shipped | Mapped quote and missing-ID behavior; no ticker-only matching |

Also exercise at least one known non-empty account per required network in SPECIFICATION.md. One successful Ethereum query does not prove BNB, Polygon, Base, Optimism, or Arbitrum support. Do not probe an enormous exchange wallet's entire history for a smoke test.

Default live-test limits: at most 50 outbound requests per selected provider per run, counting retries; additionally impose a provider-credit ceiling and stop on the first exhausted limit. Suggested ceilings: 5,000 Helius credits, 10,000 Alchemy CU, and 50,000 Ankr credits. Use stricter values if the account requires them. Authentication failures stop that provider's suite immediately.

A deliberate deep-history test can use a separately declared larger budget after the small smoke suite passes. Never keep retrying until a free monthly allocation disappears. No blockchain signing, sending, faucets, contract writes, or funded test wallets are required.

Save a sanitized report: provider, plan/entitlement observed, network, endpoint/method name without credentials, HTTP/provider status, response schema checks, record/page counts, time range, latency, request/estimated credit consumption, and result. Quota remaining is provider-reported if available, otherwise explicitly estimated.

## 5. Layer C — real native Windows tests

Use a Windows runner or the user's permitted local Windows environment. Tauri's current guidance supports WebdriverIO with `@wdio/tauri-service`; the external `tauri-driver` route works on Windows with an appropriate Edge WebDriver. Embedded automation is another documented option but introduces test-only plugins. Choose and pin a verified setup. Sources: T2–T3.

Browser/component tests may mock IPC for fast rendering checks. Native end-to-end tests must exercise the actual Tauri boundary and Rust/database behavior. Never report the browser-only mode as a native Windows test.

If using an embedded driver or mock transport, build an explicitly separate test target/profile. Release artifacts must not include its server, command execution bridge, arbitrary endpoint override, or mock fixture injection. Perform a separate smoke test of the actual release installer/build.

Required native scenarios:

1. Clean install, first launch, and setup with no keys; no crash or fake balances.
2. Add public addresses, show sync progress, receive data, and navigate Portfolio -> Asset -> Account -> Activity detail.
3. Add two owned accounts, assign overlapping groups, confirm no duplicate balance or fee.
4. Enter/correct basis; see deterministic updated P&L and a preserved audit record after restart.
5. Save a key through Settings, restart, and confirm provider connection without key readback/leakage.
6. Toggle English/Russian, dark/light, privacy mode; verify all required columns remain usable at target resolutions/scaling.
7. Go offline/reconnect; keep cached data and stale indicators; no zeros replacing failed data.
8. Cancel an import/sync; reopen; resume from a safe checkpoint without duplicate history.
9. Export/backup, restore into a fresh test profile, and verify balances, overrides, groups, and history. Keys must be absent from backup.
10. Verify installer upgrade preserves data; normal uninstall must not silently destroy the portfolio without a clear opt-in deletion choice.

Use offline controlled fixtures for deterministic edge-state UI checks, plus a separate small live path through the real app. If the environment cannot automate secret entry securely, set test credentials through the designated secure adapter/process and report which manual settings check remains.

## 6. Release inspection and performance

- Scan built assets/resources, repository, logs, reports, screenshots, and archive manifest for injected **dummy sentinel secrets**. Use a test transport to trigger errors containing URL/path/header credentials and prove redaction. Never use real secrets as printed search patterns.
- Verify source keys cannot be returned by IPC, no generic RPC/URL/SQL/shell bridge exists, and no broadcast method is reachable through production commands.
- Validate CSP, external-link allowlist, app data location, secret-store failure behavior, token label/icon sanitization, and disabled test infrastructure in the release configuration.
- Exercise the 50-account / 100,000-leg local dataset. Record startup, query p50/p95, import/replay time, peak memory, and cancellation response, with hardware/build mode. Network time is reported separately.
- Check the daily request budget from API_PROVIDERS.md using a fake clock/scheduler. Do not run an unnecessary 24-hour live soak merely to reproduce arithmetic.
- Capture the screen matrix in DESIGN.md and inspect each image. Keep personal keys and real user addresses out of screenshots/reports.

## 7. Evidence to deliver

Produce `TEST_REPORT.md` with:

| Item | Required detail |
|---|---|
| Build identity | Commit, lockfile/toolchain versions, build date, debug/release mode |
| Environment | OS/architecture, Windows/WebView2 versions for native checks, relevant test tool versions |
| Commands | Exact reproducible commands, exit codes, result counts |
| Offline tests | Calculation/identity, each chain normalizer, DB/recovery, scheduler, UI states |
| Live integration matrix | Provider + network + methods actually called, timestamp, scope, usage, result |
| Windows evidence | Installer tested, native path used, scenarios and screenshots |
| Gaps | Explicit `BLOCKED`/`FAIL`, reason, affected feature, next concrete step |
| Artifact inspection | Secret/test-server exclusion results, installer path/checksum |

Optional providers not shipped can be `SKIPPED_NOT_IN_SCOPE`. Missing verification for a required shipped provider/network is `BLOCKED`, not optional. “All tests pass” is forbidden when the only real-API tests were skipped.
