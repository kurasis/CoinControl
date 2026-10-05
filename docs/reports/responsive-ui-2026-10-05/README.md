# Responsive UI and keyboard acceptance — 2026-10-05

Application/automation source: [`7b94c67e944c085d938f8a22d5d78bf4de7b6bfa`](https://github.com/kurasis/CoinControl/commit/7b94c67e944c085d938f8a22d5d78bf4de7b6bfa), version **0.1.2**. Subsequent evidence commits change documentation only. This report separates executed Linux checks from new Windows assertions awaiting execution. Schema 7 and accounting engine 3 are unchanged.

## Changes

The window fits the monitor's physical work area, including caption/borders, taskbar, scale factor and negative monitor origins, at startup and OS DPI changes. Its normal logical minimum shrinks only when necessary. Narrow navigation retains accessible names, a visible demo indicator and scrollable Settings access. Headers, chart badges, settings and form layouts wrap. Page navigation resets the main scroll position; opening/closing details keeps the list position.

Asset, activity, review and CSV-preview tables virtualize measured rows above 100 records. Logical table row counts/indices remain available; a focused row stays mounted across scrolling and data reordering. Import and movement panels contain Tab/Shift+Tab, use the latest close handler, restore their opener and respect pending atomic import commits. Applying/recalculation progress is visible.

## Executed local checks

[LOCAL_VERIFICATION.json](LOCAL_VERIFICATION.json) records passing TypeScript, ESLint, Prettier, rustfmt, Clippy with warnings denied, native-e2e-feature Clippy, frontend build, unchanged generated IPC bindings and PowerShell 7.6.6 syntax parsing. Offline tests: **133 deterministic Rust tests / 27 Vitest tests**; eight opt-out live functions do no network work. New regressions cover high-DPI work-area geometry, negative monitor coordinates, bounded 10,000-row rendering, final-row access, focus across data reordering/filtering, modal keyboard behavior/latest callback and navigation/detail scroll behavior.

[SOURCE_RELEASE_INSPECTION.json](SOURCE_RELEASE_INSPECTION.json) is production source/frontend inspection only. It verifies CSP, command scope, excluded secrets/test infrastructure and built resources; it does not inspect or execute Windows PE/NSIS artifacts. PowerShell parsing does not execute Windows UI Automation. Local host: Debian 13/Linux 6.18.44, Intel Xeon Platinum 8573C, five logical CPUs, 35,743,784,960 bytes RAM, Rust 1.97.0 and Node 24.19.0.

## Executed browser layout checks

[BROWSER_LAYOUT_REPORT.json](BROWSER_LAYOUT_REPORT.json) records Chromium **153.0.8010.12** on Linux against the existing mock demo IPC. The run starts from the exact clean source commit above. **24 combinations** (six CSS sizes × EN/RU × dark/light) passed **264 page checks and 48 panel checks**. Sizes are 1440×900, 1280×800, 1024×720, 800×480, 672×440 and 504×340. Each case checks portfolio, wallets, activity, review, four settings pages, account, asset and group; horizontal columns, toolbar actions and sidebar Settings remain reachable. Both panels pass actual browser Tab/Shift+Tab and Escape/focus-return assertions.

A separate dev-only fixture renders **10,000 rows with variable heights**, reaches the last row with keyboard End, preserves a focused row during scrolling and retains fewer than 80 DOM rows. The final mounted-row count is recorded in the report. Visibility permits one CSS pixel of rounding: DOM rectangles retain fractional coordinates while scrollHeight/clientHeight are integers.

All **14 original browser screenshots** in [screenshots](screenshots) were inspected: 11 narrow RU/dark pages, two panels and the long-table endpoint. These are viewport captures of browser/mock UI; scrolling exposes additional content. They are not Windows/native screenshots or OS DPI evidence. Reproduce with `npx playwright install chromium` and `npm run test:layout:browser`; use a writable `PLAYWRIGHT_BROWSERS_PATH` in the cloud sandbox. The helper starts/stops its own localhost:4175 dev server and blocks external browser requests.

## Windows acceptance remains unexecuted

Windows automation implements the 11-page/two-panel matrix in three actual HWND logical sizes × EN/RU/dark/light. OS scale acceptance uses the Windows Display Settings UI for 125/150/200%, verifies application HWND DPI and WebView2 pixel ratio, checks physical outer bounds against the monitor work area and restores the original setting. Missing UI controls, unavailable scale options or required logoff are explicitly BLOCKED. Browser zoom is not used. Hosted Windows Server does not establish physical Windows 11 monitor coverage.

Production-native load automation checks all 500 holdings and 200 loaded activity records with bounded mounted rows and last-row access, alongside the preceding startup/import/replay assertions. The populated 0.1.0 upgrade now derives its target version from configuration. **None of these new Windows assertions or the 0.1.2 installer have executed at capture.**

[Preceding main CI 37368503959](https://github.com/kurasis/CoinControl/actions/runs/37368503959) failed to acquire hosted runners: six jobs have no runner/steps; the dependent production-native job is skipped. [The original check annotations](HOSTED_RUNNER_ANNOTATIONS.json) say “The job was not acquired by Runner of type hosted even after multiple attempts”. This allocation failure is not an application test failure. New delivery CI runs the checks when a runner becomes available.

Remaining release gates are executed production-native startup/useful-screen timing, large-profile native rendering/import responsiveness, Windows 0.1.2 installer/upgrade and OS scaling, physical Windows 11 coverage, full production ingestion beyond normalized SQL fixtures/one basis decision, and available Zerion quota (last known live runs return HTTP 429). [Earlier Store import evidence](../native-load-2026-10-05/README.md) and [historical Windows evidence](../upgrade-and-native-2026-10-05/README.md) retain their original scopes/sources.
