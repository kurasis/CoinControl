# Native CI and delivery evidence — 0.1.3

Source [8830f1827a002f52a47eba14713d4fdfc0edbc64](https://github.com/kurasis/CoinControl/commit/8830f1827a002f52a47eba14713d4fdfc0edbc64), [main CI 37430180430](https://github.com/kurasis/CoinControl/actions/runs/37430180430). Reports belong to this exact source. Six jobs passed; only the live job failed because of Zerion HTTP 429. Development changes were merged automatically through PRs [#6](https://github.com/kurasis/CoinControl/pull/6), [#7](https://github.com/kurasis/CoinControl/pull/7), [#8](https://github.com/kurasis/CoinControl/pull/8), [#9](https://github.com/kurasis/CoinControl/pull/9) and [#10](https://github.com/kurasis/CoinControl/pull/10). **Full release acceptance remains BLOCKED** by Zerion live validation and physical Windows 11 verification.

| Scope                                               | Executed result                                                 | Evidence                                                               |
| --------------------------------------------------- | --------------------------------------------------------------- | ---------------------------------------------------------------------- |
| Windows native IPC/SQLite                           | 24 PASS; installer checked separately                           | [NATIVE_REPORT.json](NATIVE_REPORT.json)                               |
| Native page/panel/chart table layout                | 165 pages / 30 panels / 60 chart tables                         | Same native report                                                     |
| Actual OS display scale                             | 125/150/200%, DPI 120/144/192; physical bounds and restoration  | Same native report                                                     |
| Installed production startup/UI/import              | 12 PASS / 0 FAIL                                                | [NATIVE_LOAD_REPORT.json](NATIVE_LOAD_REPORT.json)                     |
| Populated 0.1.0 → 0.1.3 upgrade, restart, uninstall | 10 PASS                                                         | [INSTALLER_REPORT.json](INSTALLER_REPORT.json), upgrade snapshots      |
| Production artifact inspection                      | 64 PASS                                                         | [RELEASE_REPORT.json](RELEASE_REPORT.json)                             |
| Store cache load, both operating systems            | PASS, dataset 50 accounts / 500 assets / 100000 legs            | STORE_LOAD JSON reports                                                |
| Browser/mock IPC matrix                             | 24 combinations / 264 pages / 48 panels / 96 chart tables       | [Separate browser evidence](../accessible-charts-2026-10-06/README.md) |
| Live providers                                      | Five suites PASS; three Zerion-dependent suites FAIL (HTTP 429) | [live/LIVE_REPORT.md](live/LIVE_REPORT.md)                             |

The hosted adapter rejected larger wide modes; **1600×1200 physical pixels** worked and exposed 200% in Windows Settings. Actual HWND DPI and settled physical bounds were measured. The 12 combinations of 1440×900, 1280×800 and 1024×720 CSS client sizes, EN/RU and dark/light passed. At 200%, the 512×360 CSS viewport still exposes all eleven routes, full identity text, both keyboard panels and chart disclosures. Resolution and scale returned to their original values. This is hosted Windows Server 2022, not physical Windows 11.

Native tests exercised owned accounts/overlapping groups, exact transfer/fee accounting, OS credentials without exposing values, private chart observations, four CSV exports and backup/restore, and public BTC cancellation/restart/offline/reconnect. Native and browser privacy each masked 31 private observations while preserving 50 public market prices. The installer uses the production build without native-e2e; its installed EXE hash must match the actual application payload extracted from the inspected NSIS installer.

## Startup and runtime

The local profile opens first; its read-only All-summary priming then runs alongside manual creation of the main window from its original configuration. An owned store guard coalesces initial IPC reads until the true cached summary is ready; errors release readers and remain visible through the normal command. The UI thread continues creating/painting the window. Scheduler timing, startup markers and every 2000ms gate are unchanged. [Previous renderer-only report](PREVIOUS_RENDERER_NATIVE_LOAD_REPORT.json), source f72ff6d315741b4862673ae4a53292273e7f5e72 / [CI 37427041726](https://github.com/kurasis/CoinControl/actions/runs/37427041726), preserves its 2459.1ms first-startup failure; 11 other production checks passed.

Chart canvas code loads in a separate local chunk: initial JS 1011.78 → 516.39 KB ([bundle-size evidence](BUNDLE_SIZE_REPORT.json)). The table remains eager, keyboard-usable and privacy-responsive while the canvas loads. Shared layout assertions require an actual canvas, so a loading skeleton cannot count as a rendered chart. [Previous production report](PREVIOUS_NATIVE_LOAD_REPORT.json), source 427131ee62c9ab70e2ea741d2536836b2570745d / [CI 37425103567](https://github.com/kurasis/CoinControl/actions/runs/37425103567), preserves its 2042.8 ms first-launch failure and 11 passing load checks; current results below belong to the new source.

Evergreen was refreshed through the official Microsoft x64 installer with a valid Microsoft Authenticode signature before execution. Native and production runtime reports retain before/after versions and the binary hash. Runtime 150+ intentionally ignores environment/HKCU overrides in elevated processes ([Microsoft explanation](https://github.com/MicrosoftEdge/WebView2Feedback/issues/5640#issuecomment-4923662109)); disposable production CI uses supported per-app HKLM settings and restores every original value. No debug override is built into production.

First normal-network process: **1,815.1 ms**. Three subsequent offline processes: **1,272.6 / 1,212.0 / 1,262.7 ms**. Each process has the same **≤2000 ms** target. The first normal-network folder is preserved separately; the first offline folder is fresh, subsequent offline folders are retained. OS/runtime caches are not reset. Browser attachment time is excluded; useful screen means balance/navigation after two animation frames, not every chart and row. [STARTUP_PHASES.json](STARTUP_PHASES.json) contains only sanitized phase durations, tied to the report's launchLog fields.

All executed production checks passed.

The first profile opened at 16 ms; its cached summary was ready at 1100 ms, before the native WebView at 1284 ms. The renderer's useful frame followed at 1815.1 ms. Three further offline processes passed their unchanged gate. Pending preview cancellation took **25 ms**, maximum measured frame gap **203.2 ms**, and one basis row committed/replayed all **100000 legs in 32908 ms**. Exact data survived process restart.

Production load uses synthetic normalized SQL evidence and actual native UI/IPC. Store p95 measurements involve 25 calls with bounded read caching; they do not guarantee every cold query is below 300 ms. Cold/max timings stay visible in the original JSON.

## Inspected images and regression controls

- [Native 200% account](native-os-scale-200-account.png): complete account address stays within the view.
- [Native 200% Data settings](native-os-scale-200-data-settings.png): narrow settings navigation and wrapped data path.
- [Native 200% chart observations](native-os-scale-200-portfolio-chart-data.png): focused keyboard disclosure and scrollable exact-value table.
- [Installed production activity](production-activity-200-windowed.png): the 100000-leg synthetic profile reaches row 200 with fewer than 80 mounted rows, as verified in the production report.
- [Token identity before](policy-identity-before-browser.png) / [after](policy-identity-after-browser.png), [POLICY_LENGTH_REGRESSION.json](POLICY_LENGTH_REGRESSION.json): reverting the wrapping style produces 524px of content in 448px; correction fits 448px and preserves every character.
- [Account address before](account-address-before-browser.png) / [after](account-address-after-browser.png), [ADDRESS_LENGTH_REGRESSION.json](ADDRESS_LENGTH_REGRESSION.json): full TON address with a real 15px scrollbar produces 533px in 433px; correction fits 433px and preserves every character.

Negative controls are Chromium/mock IPC at 512×360 CSS pixels on explicitly recorded working-tree bases; they are not native DPI tests. The native images/reports above are from the stated main CI. Original artifact values are preserved, with JSON/Markdown formatting normalized; raw process command lines and unsanitized logs are excluded.

## Remaining release gates

Zerion HTTP 429 blocks its standalone, vertical-slice and required-network suites. The shared request ledger stops at three Zerion requests; keys and full URLs are excluded. Other providers remain below the shared 50-request ceiling. Do not replace these failures with mock success or repeatedly retry while quota is unavailable.

Physical Windows 11 acceptance still requires an actual suitable machine. Current startup checks passed; the earlier failed measurements remain preserved above. This delivery is development version 0.1.3; no full release is declared verified.
