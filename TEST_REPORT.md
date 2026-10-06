# Verification report — 2026-10-06

Development version **0.1.3** is delivered on main. Exact tested code: [8830f1827a002f52a47eba14713d4fdfc0edbc64](https://github.com/kurasis/CoinControl/commit/8830f1827a002f52a47eba14713d4fdfc0edbc64), [CI 37430180430](https://github.com/kurasis/CoinControl/actions/runs/37430180430): **six jobs passed; only the live job failed because of Zerion HTTP 429**. This source includes chart accessibility, pagination/cached-query fixes, settled native window bounds, full identity wrapping, verified Evergreen setup and parallel startup cache priming. **Full release acceptance remains BLOCKED** by Zerion live validation and physical Windows 11 verification. Evidence commits change documentation only.

## Executed verification

| Scope                                                    | Result                                                                                               |
| -------------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| Local/CI check, offline tests, production frontend build | PASS; 133 deterministic Rust / 33 frontend tests; eight live opt-out functions                       |
| Chromium/mock IPC                                        | PASS; 24 combinations, 264 pages, 48 panels, 96 chart tables; visible scrollbars and full identities |
| Windows native app                                       | 24 PASS; 165 pages, 30 panels, 60 chart tables; installer is checked in its independent job          |
| Real Windows display scale                               | PASS at 125/150/200% (DPI 120/144/192), settled physical bounds and original settings restored       |
| Installed production startup, 100000-leg UI/CSV          | 12 PASS / 0 FAIL; strict 2000 ms startup gates retained                                              |
| Populated production upgrade/restart/uninstall           | 10 PASS; 0.1.0/schema 6 → 0.1.3/schema 7                                                             |
| Production EXE/NSIS inspection                           | 64 PASS; installed payload provenance, no native-e2e/credentials                                     |
| Release-mode Store load                                  | PASS on Windows and Linux; 50 accounts, 500 assets, 100000 legs, cached p95 <300 ms                  |
| Live API                                                 | Five independent suites PASS; three Zerion-dependent suites FAIL with HTTP 429                       |
| Physical Windows 11                                      | BLOCKED; hosted Windows Server does not establish this gate                                          |

Commands: `npm run check`, `npm run test:offline`, `npm run build`, `npm run test:layout:browser`, `npm run test:performance`, `npm run test:e2e:windows`, installed production load workflow, and `npm run verify:release`. Local PowerShell parsing verifies syntax only; actual registry/DPI/runtime behavior is exercised in Windows CI. Local native-feature Clippy also passed. [Sanitized native/runtime/installer/load/live evidence and inspected images](docs/reports/native-ci-2026-10-06/README.md); [browser evidence](docs/reports/accessible-charts-2026-10-06/README.md).

## Product and regression coverage

Local profile/cache priming overlaps native window creation. Initial IPC reads share an owned store gate, then receive the true cached summary; errors release the gate and retain the normal error path. Background scheduler timing is unchanged. [Renderer-only production report](docs/reports/native-ci-2026-10-06/PREVIOUS_RENDERER_NATIVE_LOAD_REPORT.json), source f72ff6d / CI 37427041726, still measured 2459.1ms for the first screen before this parallel initialization; its 11 passing checks and startup failure are preserved.

Chart canvas code loads independently, reducing the initial JS from 1011.78 to 516.39 KB. Observations remain available and privacy-responsive while the renderer loads; shared browser/native checks additionally require an actual canvas. The prior 427131e/main CI 37425103567 first screen took 2042.8 ms, failing its unchanged 2000 ms gate; [prior production report](docs/reports/native-ci-2026-10-06/PREVIOUS_NATIVE_LOAD_REPORT.json) preserves that failure. Startup markers and all gates stay unchanged.

Chart observations use localized dates and exact-string USD, distinguish zero/missing/partial/estimated data, paginate 50 rows and respond immediately to privacy. Public market prices remain visible. Reduced-motion changes update live chart options. Keyboard disclosure/pagination and bounded scrolling are verified separately in components, Chromium and native WebView2.

Background invalidation no longer cancels pending infinite pagination; a real query-observer regression reproduces the old aborted request, then checks 200 unique rows and refreshed completed pages. Scoped review counts execute SQL rather than loading up to 100000 IDs. Portfolio summaries load before heavier chart/holding work, with truthful pending UI.

Native acceptance includes owned-account/group accounting, basis audit across restart, Windows Credential Manager without renderer/SQLite credential exposure, backup/four CSV exports/fresh restore, privacy and public BTC cancellation/offline/reconnect. The supported 1600×1200 physical desktop exposes real 200%; all required 12 size/language/theme combinations pass. Full token identifiers, account addresses and data paths wrap without truncation.

## Production startup and remaining failures

First normal-network process: **1,815.1 ms**. Three subsequent offline processes: **1,272.6 / 1,212.0 / 1,262.7 ms**. All four launches retain the **≤2000 ms** requirement. First normal and first offline browser folders are fresh; the normal folder is preserved separately and subsequent offline folders retained. OS/runtime caches are not reset. Useful screen means balance/navigation after two frames, not completion of every chart/row. Browser attachment time is excluded. Exact production counts/timings and sanitized context/WebView/profile phases are preserved with source provenance.

All executed production checks passed.

Pending CSV cancellation took **25 ms**, maximum measured rendering frame gap was **203.2 ms**, and one imported basis row committed/replayed all **100000 legs in 32908 ms**. Exact import/history counts survived a process restart. These are synthetic portfolio/native UI checks, not 100000 downloaded transactions or CSV rows.

Evergreen CI installation checks official distribution and Microsoft Authenticode before execution. Elevated production automation uses supported per-app HKLM settings because Runtime 150+ ignores environment/HKCU; original values are restored. Production does not include native-e2e or a test IPC bridge.

Zerion rate limiting blocks its standalone, vertical-slice and required-network suites. The shared budget suppresses further calls after three Zerion requests; no keys, raw response bodies or full authenticated URLs are published. Five independent providers pass. Available Zerion quota and actual Windows 11 validation are required before full release acceptance.

## Evidence history and practical limits

Earlier source-specific failures/passes remain in [CI recovery](docs/reports/ci-recovery-2026-10-06/README.md), [0.1.2 responsive UI](docs/reports/responsive-ui-2026-10-05/README.md), [native upgrade acceptance](docs/reports/upgrade-and-native-2026-10-05/README.md), [release follow-up](docs/reports/release-acceptance-2026-10-05/README.md), and the [imported report](docs/reports/IMPORTED_TEST_REPORT.md). The current results supersede earlier scopes only where actually executed.

The large dataset uses synthetic normalized SQL evidence and native UI/IPC, not 100000 production-provider downloads. Store p95 uses 25 repeated calls with bounded caching; first/max cold query times remain visible. Confirmed reorg probing is bounded to the recent BTC tail; other networks depend on indexer coverage/overlap. Automatic owned transfers require direct payment evidence; ambiguous cases require manual decisions. Backups are checksummed local profile-kind-matched database images with a 128 MiB limit. No full release or physical Windows 11 pass is claimed.
