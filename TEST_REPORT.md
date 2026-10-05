# Verification report — 2026-10-05

This report distinguishes current local verification from imported historical evidence. The original report is preserved in [docs/reports/IMPORTED_TEST_REPORT.md](docs/reports/IMPORTED_TEST_REPORT.md).

The implementation covers accounting/replay fixes, recovery and exports, operational UI and Windows CI automation. Full release acceptance is **BLOCKED** by the live Zerion quota response and the remaining native scenarios listed below.

## Build identity

Local verification ran on the working tree based on `c6e8f3404564acb1f0b235bc073b40af7ee81489`. The implementation's Linux and native Windows checks passed at `071aa21b93a40842a96741a6db93350fed471a1d`; subsequent report-only commits preserve that code. The final PR's checks identify the exact tested head. Debian 13 x86_64; Rust/Cargo 1.97.0, Node 24.19.0, npm 11.9.0; SQLx 0.9.0 and Tauri 2.12.1 are locked in Cargo.lock. Schema version 6; accounting engine version 3. Local Rust verification uses the debug profile. CI uses Node 22 and the same Rust toolchain.

## Local checks

| Command / scope                                 | Result  | Evidence                                                                                                                   |
| ----------------------------------------------- | ------- | -------------------------------------------------------------------------------------------------------------------------- |
| `bash scripts/cloud-setup.sh`                   | PASS    | Complete warm setup script ran with exit 0; package extraction and Rust installation were also executed during first setup |
| `npm run check`                                 | PASS    | TypeScript, ESLint, Prettier, rustfmt and Clippy with warnings denied                                                      |
| `npm run test:offline`                          | PASS    | 128 deterministic Rust tests plus 8 opt-out live-test functions; 20 Vitest tests, including the recovery regression below  |
| `cargo test -p portfolio-store --test recovery` | PASS    | 3 tests, including populated-profile restore with exact CSV comparison, basis/audit, groups and asset preferences          |
| `npm run build`                                 | PASS    | Typechecked production frontend built successfully                                                                         |
| `npm run gen:bindings`                          | PASS    | Rust-generated bindings updated; current generated files are included                                                      |
| `node scripts/verify-release.mjs --source-only` | PASS    | Scoped command inventory, production CSP, packaged frontend, resource/sentinel exclusion                                   |
| `npm run test:e2e:windows` on this Linux host   | BLOCKED | Actual Windows runner required; browser tests are not counted as native tests                                              |

Accounting regressions verify chain position before a same-second disposal, identity retention under provider leg reordering, non-recycled IDs and orphan decisions, and refusal to carry basis merely because unrelated payments co-occur. Replay holds the write gate from evidence loading through commit and computes outside the async executor. Recovery uses VACUUM INTO, validates integrity/foreign keys and inserts/deletes tables in dependency order; a populated destination is covered.

Controlled provider tests cover cancellation/resume without duplicates, authoritative confirmed BTC reorg rollback, and shared credential-budget authentication stopping. UI tests cover full-paste validation before wallet creation and status-filtered activity. Date-boundary, hidden-vs-excluded accounting and atomic batch behavior also have Rust regressions.

## Real API reads

The first bounded run selected Esplora, Zerion, LCW, DefiLlama, TronGrid and TonAPI. Credentials were configured and never printed. Its sanitized reports are in [docs/reports/live-2026-10-05-initial](docs/reports/live-2026-10-05-initial/LIVE_REPORT.md).

| Scope                       | Result  | Observed behavior                                                                                                                               |
| --------------------------- | ------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| Esplora/BTC                 | PASS    | Balance/statistics, two distinct confirmed pages, known transaction effects and mempool                                                         |
| Live Coin Watch             | PASS    | Authenticated credits, mapped USD batch, 24h ratios, history ordering, absent code handling                                                     |
| DefiLlama                   | PASS    | Current/historical contract identity, absent token, daily token/native chart                                                                    |
| TronGrid                    | PASS    | Native and TRC-20 pagination, known token transfer and actual fee receipts                                                                      |
| TonAPI                      | PASS    | TON/Jetton identity, two event pages, event/transaction evidence and known receipt                                                              |
| Full-network engine         | PASS    | Ethereum, Base, Arbitrum, Optimism, Polygon, BNB, Solana, TRON and TON persisted and replayed; immediate resync did not duplicate history       |
| BTC/Ethereum vertical slice | PASS    | Real holdings, bounded history, prices, accounting and resync                                                                                   |
| Standalone Zerion suite     | BLOCKED | Initial Ethereum checks passed, then HTTP 429. A later isolated follow-up received HTTP 429 on positions. Further local Zerion requests stopped |

The original live runner limited each suite separately. This change fixes that defect with one shared request budget/pacing per provider across all suites, removes duplicate chain probes in combined runs, and preserves partial reports on aborted requests. Initial reports retain their original counter semantics; they do not prove the revised per-run ceiling. The [first PR CI run](https://github.com/kurasis/CoinControl/actions/runs/37339714732) at `4fa9cdb1702713b92550fc4aca85aeb10cba17d0` verified the shared ceiling: Zerion stopped after HTTP 429 with 3 requests; other providers used 5–33 requests. Five suites passed; Zerion-dependent network and vertical suites could not complete. [Sanitized CI evidence](docs/reports/ci-live-2026-10-05-4fa9cdb/LIVE_REPORT.md) records those failures. Later request suppression now preserves the original authentication/throttling error rather than reporting a spent local budget. The final implementation's [live CI evidence](docs/reports/live-2026-10-05-071aa21/LIVE_REPORT.md) at `071aa21b93a40842a96741a6db93350fed471a1d` again passed five independent suites; the three Zerion-dependent suites could not finish because of HTTP 429. Every provider stayed below the shared ceiling, and later failures retained the throttling cause. Successful whole-suite live verification still requires available Zerion quota.

## Windows and release gates

The workflow now builds an isolated native target, pins tauri-driver and matches the Edge driver to WebView2. It explicitly configures a separate browser directory and local automation port only in the `native-e2e` feature; production builds exclude both overrides. Implemented native scenarios cover clean launch, real SQLite demo, account navigation, manual basis/audit across restart, isolated OS credential entry/persistence/removal, language/theme and privacy. Evidence and screenshots are uploaded as `windows-native-evidence`. Production installer checks and release inspection upload `windows-release-evidence`.

The [Windows CI run](https://github.com/kurasis/CoinControl/actions/runs/37344132830) at `071aa21b93a40842a96741a6db93350fed471a1d` passed **12 executed checks**. [Native evidence](docs/reports/windows-2026-10-05-071aa21/NATIVE_REPORT.json) covers real IPC/SQLite, basis/audit after process termination and restart, Windows Credential Manager persistence/removal without returning keys to the renderer, credential exclusion from SQLite/WAL, account navigation, locale/theme and privacy. Four uploaded screenshots were inspected. Earlier startup failures in WebDriver automation were corrected before this passing run. The same CI run passed Rust tests on Windows, NSIS generation, silent installation and actual production launch, database preservation on reinstall/uninstall, and the complete production release inspection. [Installer evidence](docs/reports/windows-2026-10-05-071aa21/INSTALLER_REPORT.json) and [release hashes/inspection](docs/reports/windows-2026-10-05-071aa21/RELEASE_REPORT.json) are preserved. The native report's installer item is blocked within that separate native job; the production job independently executes the installer checks. No local Windows execution is claimed. Remaining full native gates are **BLOCKED**: small live synchronization through the UI, native backup/file-dialog restoration and offline reconnect. The 50-account/100,000-leg performance and full screen/scaling matrices have not been executed and remain **BLOCKED**. Installer tests check file preservation on reinstall/uninstall; they do not alone prove every populated-data upgrade scenario.

## Practical limits

Confirmed reorg probing is currently bounded to the recent BTC tail. Other chains rely on their existing indexer coverage and overlap behavior. Exact transaction index is used only where the source reports it. Automatic owned transfers require direct payment evidence; ambiguous cases need manual pairing. Backups are local checksummed database images with a 128 MiB limit and matching profile kind; source credentials remain external to SQLite.
