# Release acceptance follow-up — 2026-10-05

Application and automation source: `34af7256cd8cd3225cd6c82d39d4fad94b1a5a56`.
GitHub tested PR merge: `054029a0289688d315ceb999c34a035012a1f031`.
[Executed CI run](https://github.com/kurasis/CoinControl/actions/runs/37351607605).
Schema 7, accounting engine 3. Production installer is unsigned.

| Executed scope                                                           | Result  | Evidence                                                                                     |
| ------------------------------------------------------------------------ | ------- | -------------------------------------------------------------------------------------------- |
| Linux static checks, offline tests, generated DTOs                       | PASS    | CI: 130 deterministic Rust tests, 20 Vitest tests; 8 live functions opt out without requests |
| Windows Rust tests, NSIS build and production launch/reinstall/uninstall | PASS    | [Installer report](INSTALLER_REPORT.json), six checks                                        |
| Production permission/CSP/secret/test-infrastructure/artifact inspection | PASS    | [Release report](RELEASE_REPORT.json), 61 checks and artifact SHA256                         |
| Real Windows native scenarios                                            | PASS    | [Native report](NATIVE_REPORT.json), 16 checks; installer scope runs separately              |
| Linux and Windows 50-account / 500-asset / 100000-leg store load         | PASS    | [Linux](PERFORMANCE_LINUX.json), [Windows](PERFORMANCE_WINDOWS.json)                         |
| Five independent provider suites                                         | PASS    | [Live report](live/LIVE_REPORT.md)                                                           |
| Zerion and dependent network/vertical suites                             | BLOCKED | HTTP 429; the shared ledger records three Zerion requests including retries                  |

The native run used Windows 10.0.26100, Node 22.23.3, tauri-driver 2.0.5 (the executed workflow install pin), and Edge WebDriver matching WebView2 153.0.4234.48. The driver's `--version` probe emitted no stdout, so the JSON does not invent a returned version. All ten screenshots in [screenshots](screenshots/) were inspected: demo, audit after restart, Russian settings, light/privacy, export, restored source configuration, live BTC wallet/detail, offline cache and reconnect. The captured viewport is 1028 × 749; this is not the full required resolution/scaling matrix.

Recovery saved a backup and holdings/activity/lots/decisions CSV through real Windows Save dialogs. Windows Shell's filename ValuePattern was unavailable on this hosted image; the helper focused only the application-owned Save HWND and used the normal filename accelerator/clipboard/Enter path. The helper supplies a destination only; the application produces every file. Restore uses WebDriver's normal file-input selection and the real inspect/restore IPC. A fresh installation seeds its matching demo profile before restore; this is not an empty, unseeded demo destination. All four restored CSV files and group memberships exactly matched the edited source. The dummy credential was configured during export, absent from the SQLite archive, and removed from the isolated OS service before the fresh installation. No real API key was sent to WebDriver or embedded in native artifacts.

The separately opted-in public BTC path imported 22 transactions, showed progress, accepted cancellation in 13 ms, restarted/resumed, and navigated Portfolio → Asset → Account → account-filtered Activity detail. An outbound Windows firewall rule for the tested executable caused a real failed balance read. Cached quantities persisted, the UI showed a stale balance, and a reconnect refreshed the account without extra transactions. Remaining price backfill was canceled after the failed balance read to conserve the test budget. The firewall rule was removed in `finally`. Esplora used 13 requests and DefiLlama 16; each stayed below 50.

| Store metric (release, offline synthetic fixture) | Linux            | Windows        |
| ------------------------------------------------- | ---------------- | -------------- |
| Reopen + first portfolio summary                  | 453 ms           | 599 ms         |
| Full accounting replay                            | 19.84 s          | 16.94 s        |
| Activity p50 / p95 (25 calls, uncached SQL)       | 11.29 / 12.22 ms | 5.30 / 6.85 ms |
| Cold holdings query                               | See JSON         | 531 ms         |
| Cold chart query                                  | See JSON         | 315 ms         |
| Peak process memory                               | 831 MiB          | 388 MiB        |

The [initial Windows measurement](baseline-5c28f4f/PERFORMANCE_WINDOWS.json) failed with activity p95 563 ms. The global time index and broad-scope query now avoid sorting the entire history for one page. The workload checks exact order and no overlap for two consecutive pages. Summary/holdings bursts include memory-cache hits; cold reads remain in `first_ms`, `max_ms` and store-open timing. Reported p95 is not a claim that every cold query meets 300 ms. The bounded cache expires after 30 seconds (one second for time-dependent charts) and is invalidated by any SQLite commit through a dedicated connection's `data_version`. Regression tests include Store clones, a second writer, rollback and restore. The fixture is loaded directly as normalized SQL; production ingestion is measured separately on 1000 idempotent overlap transactions, not a full 100000-row CSV import. Native rendering startup, cancellation on the large dataset, and peak native-app memory have not been measured by this store harness.

Full release acceptance remains **BLOCKED**: Zerion quota prevents current complete EVM/Solana live verification; the full screen/scaling matrix, cold native startup target, large-dataset cancellation/rendering, every populated-data installer upgrade case, and the remaining native multi-account/group workflows need separate acceptance evidence. Reinstall/uninstall evidence checks file preservation on the profile created by production first launch; it does not claim a populated-data version-to-version migration.

The subsequent delivery adds an explicit profile-ready wait in native automation and curated reports/screenshots. Rust/frontend application code and production configuration remain the tested implementation above.
