# Populated upgrade and native acceptance — 2026-10-05

Application version **0.1.1**, schema **7**, accounting engine **3**. This stage verifies populated production upgrades and adds deterministic owned-account/group and actual native viewport acceptance. Full release acceptance remains **BLOCKED** by Zerion HTTP 429 and the unexecuted scopes below.

## Build identity and executed checks

The latest [CI run 37359707392](https://github.com/kurasis/CoinControl/actions/runs/37359707392) tested source `8daaa7996ebce230f9e32f35aec4b1b2a4c517d9`, merge `4839295372909328c338cbbe9033b655170055df`:

| Scope                                                         | Result                     | Evidence                                                                                               |
| ------------------------------------------------------------- | -------------------------- | ------------------------------------------------------------------------------------------------------ |
| Windows native UI, real Rust IPC and SQLite                   | PASS, 19 checks            | [NATIVE_REPORT.json](NATIVE_REPORT.json), 40 inspected original screenshots                            |
| Populated production installer upgrade, restart and uninstall | PASS, 10 checks            | [INSTALLER_REPORT.json](INSTALLER_REPORT.json), five upgrade snapshots                                 |
| Windows production inspection                                 | PASS, 61 checks            | [RELEASE_REPORT.json](RELEASE_REPORT.json), hashes of the unsigned EXE and NSIS installer              |
| Windows release-mode store load                               | PASS                       | [PERFORMANCE_WINDOWS.json](PERFORMANCE_WINDOWS.json), exact tested merge above                         |
| Independent live suites                                       | PASS, 5 suites             | [live report](live/LIVE_REPORT.md): Esplora, Live Coin Watch, DefiLlama, TronGrid, TonAPI              |
| Zerion-dependent live suites                                  | FAIL, external blocker     | Zerion, combined networks, vertical slice; HTTP 429, shared Zerion ledger 3 requests including retries |
| Latest Linux checks and load                                  | CANCELLED before execution | No completed result is claimed for this run                                                            |

Linux checks/offline tests/generated bindings and Linux load passed in [run 37358447838](https://github.com/kurasis/CoinControl/actions/runs/37358447838), source `88d6a98f6c8d86d04beae67d88a159a22c8ed3d5`, tested merge `3040d40b83a6abf1f3bc0d81ced7705cd9602d23`. [PERFORMANCE_LINUX.json](PERFORMANCE_LINUX.json) belongs to that earlier run. The only subsequent code change is a 250 ms wait before native screenshots to let WebView2 paint; application, Rust tests, installer automation and Linux checks are unchanged. The final delivery also corrects the installer report description to derive its fingerprint count; assertions and application code are unchanged.

Local Debian 13 verification passed `npm run check`, `npm run test:offline` (**131 deterministic Rust tests**, **20 Vitest tests**), `npm run build` and `npm run verify:release -- --source-only`. Eight opt-out live Rust functions return without network work in the offline command; they are not counted as executed live tests. The new Rust integration test independently compares production Store results with hand-calculated fixture totals. Script changes also passed ESLint, Prettier and Node syntax checks.

Windows jobs ran on hosted **Windows Server 2025**, x64, OS `10.0.26100`, Node `22.23.3`, matching WebView2/Edge WebDriver `153.0.4234.48`; the workflow pins tauri-driver `2.0.5`. This is not evidence from a physical Windows 11 workstation. The native job uses a separate `native-e2e` build and isolated profiles/credential entries. The installer job launches actual installed production builds without that feature; production inspection checks its exclusion.

## Production 0.1.0 → 0.1.1 upgrade

The baseline is the actual production NSIS artifact from main `f0603fde8a7f4e13d60509ee2c188ffe2486d136`, [run 37345501830](https://github.com/kurasis/CoinControl/actions/runs/37345501830), version `0.1.0`, schema 6. SHA-256 is verified before installation: `00b4dc04679d3de435a6c91d494ef1960361a999a01a63b80978915bd621b7c5`. Re-running this scenario requires that pinned artifact to remain available; GitHub artifact retention is 90 days. An expired baseline must be explicitly replaced with a verified production baseline, rather than silently skipped or fabricated.

The old installed app creates an empty profile, closes, and an external Node test utility seeds controlled offline evidence. The old app then opens and replays that evidence. Installing production 0.1.1 migrates the populated database to schema 7. All **19 selected source/derived table fingerprints** and exact account/asset/raw-quantity balances match before upgrade, after upgrade, after another process restart and after normal uninstall. SQLite integrity remains `ok`, accounting is ready, and the test retains two accounts, two overlapping groups/three memberships, history, lots, two audit versions and one fee. The uninstall check reads the retained database after uninstall, including any WAL-backed evidence; checking the main file alone is not used as proof.

- [Empty old-app profile](upgrade-empty-baseline.json)
- [Populated baseline](upgrade-before.json)
- [After upgrade](upgrade-after.json)
- [After restart](upgrade-reopened.json)
- [After uninstall](upgrade-uninstalled.json)

The raw installer report incorrectly labels the count as 20; the four snapshots each contain 19 fingerprints, all equal. Its original machine content is preserved, and the report script now derives the count from the actual snapshot. Fingerprints cover selected stable tables. Mutable price/checkpoint state and the migration ledger are not asserted to be identical. The fixture is synthetic and small; it does not establish upgrade safety for every possible user database.

## Owned transfers and overlapping groups

The native UI creates two wallets/accounts using the actual address validation, then the application closes while the external utility inserts deterministic chain evidence. An app-scoped firewall rule prevents querying these synthetic addresses. The application reopens and creates overlapping groups through normal UI controls. No fixture command or synthetic evidence utility is bundled into the application.

Two ETH arrive externally with a final audited acquisition cost of $4000. One ETH moves from A to B with direct counterparties on both sides; A pays one 0.01 ETH fee. Final quantities are A **0.99 ETH**, B **1 ETH**; at $3000/ETH the combined portfolio and both-account group equal **$5970**, A-only equals **$2970**, and the applicable fee is **$30 once**. UI scope totals and fee count survive restart. The external [OWNED_FIXTURE_REPORT.json](OWNED_FIXTURE_REPORT.json) confirms two own-transfer legs and one fee charge; the Rust integration separately asserts basis/audit, summary and group results through production Store APIs.

## Actual native viewport matrix and screenshots

The runner uses `SetWindowPos` on the actual Tauri HWND, records native window DPI and measures WebView2 `innerWidth`/`innerHeight`. It executes **1440×900**, **1280×800**, **1024×720** CSS client sizes × **EN/RU** × **dark/light**: 12 combinations. Portfolio main-content overflow checks pass and all six asset/eight activity columns are reachable using their own horizontal scrolling. Captures wait for the compositor; narrow tables have separate left/right evidence instead of implying every column fits simultaneously.

The host reports **DPI 96 / devicePixelRatio 1**, screen **1024×768**, available work area **1024×720**. Larger requested windows extend beyond that work area. These are real native client sizes, not browser viewport mocks, but they do not prove physical monitor coverage or Windows display scaling at 125%, 150% or 200%. Automated overflow assertions apply to the portfolio/table scope; they do not establish every page or overlay layout.

All **40 original PNGs** in [screenshots/](screenshots/) were inspected: 12 portfolio overview combinations, 12 left/right table views, and 16 first-launch, owned-group, audit, missing-basis, provider setup, settings, privacy, export/restore and public-BTC/offline/reconnect captures. Representative evidence:

| View                    | Screenshots                                                                                                                                                         |
| ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Portfolio, RU dark      | [1440×900](screenshots/viewport-1440x900-ru-dark.png), [1280×800](screenshots/viewport-1280x800-ru-dark.png), [1024×720](screenshots/viewport-1024x720-ru-dark.png) |
| Narrow assets           | [Left](screenshots/viewport-1024x720-ru-dark-assets-left.png), [right](screenshots/viewport-1024x720-ru-dark-assets-right.png)                                      |
| Narrow activity         | [Left](screenshots/viewport-1024x720-ru-dark-activity-left.png), [right](screenshots/viewport-1024x720-ru-dark-activity-right.png)                                  |
| Owned groups            | [Both](screenshots/owned-both-group.png), [A only](screenshots/owned-single-group.png)                                                                              |
| Missing basis and audit | [Review](screenshots/missing-basis-review.png), [Reopened audit](screenshots/audit-reopened.png)                                                                    |
| Connectivity            | [Cached offline](screenshots/offline-cached-bitcoin.png), [Reconnected](screenshots/reconnected-bitcoin.png)                                                        |

## Recovery, real API reads and load limits

Retained native acceptance verifies basis/audit across process restart, isolated Windows Credential Manager persistence/removal without echoing the dummy key, credential exclusion from SQLite/WAL and backup, and backup plus four CSV exports through actual Save dialogs. Restore into a fresh matching demo installation preserves exact holdings/history/lots/audit exports and group membership; the destination demo is initially seeded by the application.

Public BTC native synchronization imports **22 transactions**, acknowledges cancellation in **18 ms**, resumes after restart, retains cached quantities/stale status with outbound traffic blocked by the OS, and refreshes without duplicates after reconnect. Native usage is **Esplora 13 / DefiLlama 16 requests**. Actual private provider keys are excluded from the native child's environment. The separate live job uses configured credentials and preserves five passing suites and three Zerion-dependent failures; suite-local zero request counts on an aborted suite do not override the shared [usage ledger](live/usage.json).

Both load reports use release-mode Store APIs and a synthetic **50-account / 500-asset / 100,000-leg** SQL fixture, plus 1,000 real ingestion overlaps. All measured query p95 values meet the <300 ms target. Activity p95: **Windows 13.60 ms**, **Linux 9.62 ms**. Windows store reopen/first summary is **1024.51 ms**, replay **30.78 s**, peak process memory **384.88 MiB**; Linux is **597.93 ms**, **14.72 s**, **599.53 MiB**. Cold Windows holdings **922.73 ms** and chart **396.66 ms** remain visible; cached p95 is not a guarantee for every cold query. Fixture loading is normalized SQL, not a 100,000-record production provider import.

Remaining release gates: available Zerion quota; actual Windows display scaling **125/150/200%** with a full page/overlay matrix and Windows 11 workstation coverage; cold native process-to-useful-screen ≤2 seconds; native rendering/cancellation on the large dataset and full production import acceptance. Small public-BTC cancellation and Store startup measurements do not establish those large/native requirements.

Only sanitized machine reports and original screenshots are committed here. Raw application logs, credential values, SQLite profiles, backup archives and installers remain outside the repository.
