# Accessible chart observations (0.1.3)

Browser source [8830f1827a002f52a47eba14713d4fdfc0edbc64](https://github.com/kurasis/CoinControl/commit/8830f1827a002f52a47eba14713d4fdfc0edbc64), clean at capture, [main CI 37430180430](https://github.com/kurasis/CoinControl/actions/runs/37430180430). Application code stayed unchanged during capture; report files are updated afterward. This is Chromium/browser mock IPC evidence.

Every portfolio/account/group and asset chart provides a native HTML disclosure with a semantic table: localized dates/time, exact-string USD formatting, explicit missing observations and estimated/partial coverage. Pages contain at most 50 rows. Privacy masks private values immediately while public market prices remain visible. Changed ranges reset disclosure/page identity. ECharts respects reduced motion and subsequent preference changes.

Local and CI check/offline/build passed: **133 deterministic Rust / 33 frontend tests**, plus eight live opt-out functions. Component regressions cover exact large decimals, zero versus missing data, dynamic privacy/public prices, pagination and Russian labels. The InfiniteQueryObserver regression checks that a pending next page survives background invalidation and all completed pages refresh. Actual Enter/Space disclosure actions were tested in Chromium and native WebView2.

[BROWSER_LAYOUT_REPORT.json](BROWSER_LAYOUT_REPORT.json) records **24 scenarios, 264 pages, 48 panels and 96 chart-table checks**, privacy with 31 masked observations / 50 public prices, and a 10000-row variable-height table reaching its last row with bounded DOM/focus. Browser fixtures include full native-length token identity and raw TON address, with Chromium's hide-scrollbars default removed so real scrollbars consume width. The two narrow Russian/dark images were inspected: [portfolio](portfolio-chart-data-narrow-browser.png) and [public market prices](market-chart-data-narrow-browser.png).

[Separate native evidence](../native-ci-2026-10-06/README.md) executes real IPC/SQLite, actual OS scaling and production startup/load. Browser checks do not establish Windows DPI, production performance or physical Windows 11 acceptance.
