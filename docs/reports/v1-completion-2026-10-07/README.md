# Specification completion — 2026-10-07, 0.1.7

This follow-up implements the remaining confirmed product gaps after the owner's
“доделывай”. Existing public command/DTO contracts remain compatible; new commands
have explicit Tauri ACL entries. No dependency upgrades or schema migration.
Baseline was clean main `927122dc1034596f8cb50fc23dd799b38cffc942`.

## Requirement mapping

| Requirement                                                  | Implementation / verification                                                                                                                                                                                                                                                                                                                                                                                           |
| ------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Wallet/account rename, move and permanent local removal (§4) | `AccountManagement.tsx`, `portfolio-store/maintenance.rs`; explicit dependency preview, evidence revision guard, transactional removal, empty-wallet guard. Other accounts and group ownership survive.                                                                                                                                                                                                                 |
| Account detail, explorer and earliest history                | `AccountPage.tsx`, additive `AccountCoverage`; no raw cursor/error export. Wallet detail has direct navigation.                                                                                                                                                                                                                                                                                                         |
| Retain view settings (DESIGN)                                | Profile-specific in-memory `useViewState`; portfolio/asset dates and ranges, hidden assets, table search/network/sort; frontend regressions including queued-job navigation/cancellation.                                                                                                                                                                                                                               |
| Multi-leg activity (DESIGN)                                  | Collapsible details retain every asset quantity when collapsed; individual review remains available.                                                                                                                                                                                                                                                                                                                    |
| General/source settings (§7)                                 | Existing intervals are now editable; additive provider quotas enforce shared daily/monthly requests and estimated credits. Usage is preserved, UTC local reset labelled separately from unknown provider reset.                                                                                                                                                                                                         |
| Maintenance (§12)                                            | Safe cache cleanup, diagnostics export, rescan with retained evidence and decisions. Real-only rescan and manual synchronization return bounded process-local job ID, progress and cancellation.                                                                                                                                                                                                                        |
| Jobs/dispatcher (§8)                                         | Shared run lock, bounded 32-job history, cancellation before start, safe-checkpoint cancellation, profile-generation guard for queued work.                                                                                                                                                                                                                                                                             |
| Scheduling (§10.3)                                           | Manual priority, active scope before inactive, oldest-attempt order avoids tail starvation, profile-size intervals, minimized reductions and one resume catch-up; fake-clock unit tests.                                                                                                                                                                                                                                |
| Chain finality/reorg (§10.2)                                 | EVM canonical receipt/block and chain-specific finalized tag; Solana finalized signature/slot and canonical block membership; TON event-root/shard membership with masterchain anchors. Missing/incomplete/error responses retain evidence; unsafe unavailable checks expose partial finality. Bitcoin retains its existing authoritative confirmation overlap; TronGrid confirmed/solidified evidence remains partial. |
| Safe accounting rollback                                     | Independent durable rollback flag, cursor rewind, stale observed balances, invalidated derived snapshots and deterministic FIFO replay. User decisions survive disappeared events for review.                                                                                                                                                                                                                           |
| Bounded asset custom windows                                 | Existing command accepts additive optional endpoints; exact boundaries on both series, at most 1001 samples and invalid-date rejection.                                                                                                                                                                                                                                                                                 |
| Windows delivery                                             | Version 0.1.7; all six application/build CI jobs pass on final source. Native management/settings, production startup and upgrade evidence are recorded below; full acceptance still has external gates.                                                                                                                                                                                                                |

The initial browser regression found a Russian wallet row overflowing at 672×440;
its [failure report](INITIAL_BROWSER_LAYOUT_REPORT.json) is retained. Rows now wrap
with the new controls. This browser matrix uses mock IPC and is separate from native
Windows acceptance.

## Local verification

Baseline: static checks, build and offline tests passed with 178 deterministic Rust
and 52 frontend cases. Final: **195 deterministic Rust / 60 frontend cases PASS**;
18 opt-in live functions return without external access in offline mode and are
not counted as live passes. All-feature Clippy (warnings denied), TypeScript,
ESLint, Prettier, rustfmt, generated bindings (71 DTO exports), production frontend
build and source-only release/ACL inspection pass. The browser matrix passes all
24 size/language/theme combinations and its 10000-row bounded-DOM scenario.

Logs and JSON in this directory record these exact scopes. Windows execution is
performed in CI; this Linux workspace cannot establish a native Windows pass.

## Official contracts reviewed

- [Ethereum execution API block schema](https://github.com/ethereum/execution-apis/blob/main/src/eth/block.yaml): finalized tag and canonical block fields.
- [Solana getSignatureStatuses](https://solana.com/docs/rpc/http/getsignaturestatuses), [getBlock](https://solana.com/docs/rpc/http/getblock): finalized commitment, history search and signature membership.
- [TonAPI OpenAPI](https://github.com/tonkeeper/opentonapi/blob/master/api/openapi.yml): mainnet/masterchain head, shard anchors, blockchain transaction, block and transaction membership routes.
- Existing [provider integration](../../PROVIDER_INTEGRATION.md) and [mirror contracts](../../PROVIDER_MIRRORS.md) remain authoritative for holdings/history routing.

Controlled wiremock tests cover matching/forked receipts, wrong mainnet, index absence
with positive block evidence, Solana membership and incomplete TON membership. The
existing bounded live suites now also exercise Ethereum, Solana and TON finality;
actual results must come from their reports. No controlled test is a live proof.

## External acceptance blockers and required configuration

The owner confirmed on 2026-10-07 that Alchemy/Chainstack cannot currently be corrected.
Keep these failures visible; do not retry unchanged authentication-only checks in
isolation to conceal them. Each changed source receives one bounded main verification.

1. **Alchemy:** the previous exact live run returned 403 on Base, Arbitrum, Optimism
   and Polygon. In the Alchemy dashboard enable those four mainnets for the app/key
   or create an app/key entitled to them; update `ALCHEMY_API_KEY` in repository
   Actions secrets and separately the installed application's Settings → Sources.
   Ethereum access alone does not establish these other networks.
2. **Chainstack:** the previous Solana endpoint returned 401. Select a deployed
   **Solana mainnet node**, open **Access and credentials**, and copy its **HTTPS RPC
   endpoint or node RPC authentication token**. A platform management API key is
   not a node credential. Update `CHAINSTACK_API_KEY` in Actions and the installed
   app's source setting. Do not enable a paid plan or overage for this task.
3. **dRPC BNB:** passed on final source `a3512b0`, after intermittent 429 on
   `7b9435f`. It is not a current failed gate, but its earlier limit is preserved.
   If it recurs, check the free BNB service/key allowance in dRPC and wait for its
   reset. Local counts are distinct from global provider usage; reserves/cached
   data continue to work. No paid overage or isolated retry is required.
4. **Windows 11 x64:** hosted Windows Server automation does not establish the
   original Windows 11 installation/UI acceptance. A Windows 11 machine is needed
   for that gate. The unsigned ZIP remains a prerelease while external gates remain.

GitHub Actions secrets are CI-only; production builds never embed or read them.
Never send actual keys in chat. macOS/iOS are migration targets, not tested V1 platforms.

## Delivery provenance

Implementation [PR #27](https://github.com/kurasis/CoinControl/pull/27) is merged.
Application source is `64e29e3b412e55bee29fa1207b57cc061ea2027d`;
[main push CI 37592895467](https://github.com/kurasis/CoinControl/actions/runs/37592895467)
validates exactly that source. The earlier branch run 37592868913 was cancelled
after merge to avoid duplicate live work, but completed partial reads before
cancellation ([actual usage](ci/CANCELLED_PR_LIVE_USAGE.json)); these requests are
additional to the main run and are not presented as a completed acceptance pass.
No unchanged failed run is retried in isolation; each corrected application source
receives its own bounded CI verification with known external failures preserved.

## Initial Windows/live failures and corrections

Initial main CI 37592895467 passed Linux/offline/browser and both load harnesses,
but failed native review navigation and the pinned 0.1.0 fixture launch. Native
account drill-down accidentally changed the global portfolio scope, leaving review
empty while the selector displayed all wallets. Account/wallet detail now supply
only the active polling scope and restore it on leave; the user's portfolio scope
is preserved. The new regression passes corrected behavior and **rejects the old
implementation** ([negative control](SCOPE_NEGATIVE_CONTROL.txt)). Initial native
report and [inspected failure screenshot](ci/initial/NATIVE_FAILURE.png) remain.

The production upgrade fixture failed before updating the new app. Its initializer
now starts in the installation directory and polls the old profile for at most 30 s
before the existing 8 s replay settling wait. This setup readiness is distinct from
the **unchanged 2000 ms current-production startup gate**. No failed check is relabelled
as passed; [initial installer report](ci/initial/INSTALLER_REPORT.json) remains.

Initial live results were 14 passing / 4 failing suites. New Solana and TON finality
checks passed. A new PublicNode check assumed an old historical receipt was available;
its missing result caused a test panic. It now samples an actual current finalized
Ethereum block and verifies that receipt's canonical block through the product
validator, keeping the same Final assertion and bounded shared budget. Known-amount
historical Ethereum checks remain independently enforced by Alchemy/Zerion. The
[initial live report](ci/initial-live/LIVE_REPORT.md) is preserved; corrected-source
results are recorded separately. Alchemy 403, Chainstack 401 and dRPC BNB 429 were
unchanged external failures in the initial run.

## Native test schema correction

Follow-up source `052e4237f762182853a18561759068afb04e9ee5`,
[CI 37594993107](https://github.com/kurasis/CoinControl/actions/runs/37594993107),
passed the upgraded installer and fixed review navigation. New real-IPC wallet rename
passed. The new cache-retention test then failed on an incorrectly named SQL table
(`movement_legs`). The application itself uses `activity_legs`; the test's two queries
are corrected to that exact schema. Both corrected queries compile against **all
seven migrations**, while both old queries are rejected ([query validation](NATIVE_QUERY_VALIDATION.txt)).
The [intermediate native failure](ci/intermediate/NATIVE_REPORT.json) is preserved.
This correction changes only test SQL; production application logic is unchanged.
That run's live checks passed **16 suites** (including Ethereum/Solana/TON finality
and dRPC BNB); only Alchemy's four rejected mainnets and Chainstack Solana failed.
New final-source verification is recorded separately before ZIP publication.

## Production startup correction

Source `052e423` also failed the first normal installed startup: **2383.20 ms**
against the unchanged **2000 ms** target. Its three later offline launches passed;
those warm results do not replace the failed normal gate. The original
[NATIVE_LOAD_REPORT](ci/intermediate/NATIVE_LOAD_REPORT.json) remains unchanged.
Backend logging showed context/profile ready in 3/13 ms, summary at 346 ms,
WebView setup at 1071 ms and renderer useful paint at 1265.8 ms.

Secondary routes now load on navigation. Initial JavaScript totals decrease from
555.41 kB to **481.81 kB** (315.52 kB entry plus 166.29 kB shared formatting chunk),
without counting the shared chunk as removed. Holdings, preview activity and chart
work start after the same data-ready **two-frame** useful-paint mark. Balance,
settings/profile readiness, the mark, process-to-paint measurement and threshold
remain unchanged. A controlled frame test verifies both frames and the once-only
mark. Summary errors still permit the independent queries. No API, accounting or
provider behavior changes.

Local checks, 58 frontend cases, production build/source inspection and all 24
browser layout scenarios pass after this correction. Actual Windows timing and
native management checks must pass on the new source before publication.

### Shared chart observer regression

PR #29 (`76004e0`, CI 37598456811) reduced initial routing work, but additional
screen-level testing found that `PortfolioChart` had a second observer of the same
query. Its older enabled condition started the chart before the useful-paint mark.
Both observers now use the same deferred readiness. The new screen test
**rejects the earlier source** ([negative control](CHART_OBSERVER_NEGATIVE_CONTROL.txt))
and passes the corrected screen ([regression](CHART_OBSERVER_REGRESSION.txt)); it
asserts no chart or holdings IPC before the two ready frames, then one shared chart
request and one holdings request. This is an additional frontend regression, bringing
the total to 59. Live results from that intermediate source again had 16 passing
suites and only Alchemy/Chainstack failures ([exact report](ci/lazy-routes/LIVE_REPORT.md)).
The cancelled schema-only branch run 37596760661 made partial reads
([actual usage](ci/CANCELLED_SCHEMA_PR_LIVE_USAGE.json)); these are additional to
its later main CI and are not presented as task-wide usage below 50/provider.

Intermediate main CI 37598456811 is superseded by the shared-observer correction.
Completed API/offline/load results remain evidence for that exact earlier source;
uncompleted Windows jobs are cancelled, not claimed as passes. The final main CI
will verify all application/build gates again.

## Shared-observer source and completed API/load evidence

The shared-observer fix is merged as [PR #30](https://github.com/kurasis/CoinControl/pull/30).
Exact application source: **`7b9435f70a44ebc2475e1fa3781b3ea45051099b`**.
[Main push CI 37599437253](https://github.com/kurasis/CoinControl/actions/runs/37599437253)
records that exact source. Final live results: **15 passing / 3 failing suites**;
Alchemy's four mainnets return 403, Chainstack Solana returns 401 and dRPC BNB is
rate limited. Its two preceding successes remain historical, not a replacement
for this last failure. Ethereum/Solana/TON finality, other reserves and the
budget-to-reserve route pass ([final live report](ci/observer-before-native-label-fix/live/LIVE_REPORT.md)).
The `npm run test:live` API suite uses at most 50 requests/provider (retries
included, largest 44). Native and earlier changed-source/cancelled branch usage is
additional and preserved, not included in that suite ceiling.

Linux static/offline/build/bindings/browser checks pass. The final offline set is
195 deterministic Rust and 59 frontend cases; the browser matrix uses mock IPC.
Release-mode storage measurements pass on both Linux and Windows: 50 accounts,
500 assets and 100000 movements, with the worst repeated-query p95 on Windows
14.64 ms against 300 ms. Their first cold chart/holdings queries are slower and
remain explicitly recorded; this is not native process-to-screen startup.

## Native background/manual label correction

The `7b9435f` native run passed all new wallet/cache/quota/interval checks, backup,
restore and display/UI scaling. It then timed out waiting for the old `Syncing…`
button label during **background** synchronization, which now intentionally offers
`Sync now (queue)`. The inspected failure screenshot shows valid imported BTC
history (22 transactions), not a failed import. Native automation now requires the
actual spinning account/sidebar status plus an enabled cancellation button. The
spinner and safe-cancellation assertions remain enforced. A component regression
also verifies the queue action remains enabled during background work and cancel
uses the background command, rather than an unrelated job ID. Completion labels
may carry the existing partial outcome suffix; the independent SQL assertion still
requires refreshed balance and no current-source error.

The [native failure report](ci/observer-before-native-label-fix/NATIVE_REPORT.json)
and [inspected screenshot](ci/observer-before-native-label-fix/NATIVE_FAILURE.png)
remain unchanged. Only test expectations change; product behavior is preserved.
All corrected native scenarios will be run on the new main source before publishing.

Production installed startup/load on `7b9435f` passed all **12 checks**. The first
normal fresh-cache useful screen took **1471.90 ms**, within the original 2000 ms
gate. Chart-query deferral reduced competing work; the criteria/mark/threshold
were unchanged. The exact [installed load report](ci/observer-before-native-label-fix/NATIVE_LOAD_REPORT.json)
is preserved. Installer/upgrade passed 11 checks and production inspection passed 75. The only application/build job failure was the native background-button text
expectation described above. The next source changes tests/evidence only and must
repeat the native checks before ZIP publication.

## Final delivery verification

Final source **`a3512b0121e7ce19fc93b70bcd5bd969902b599d`**, merged through
[PR #31](https://github.com/kurasis/CoinControl/pull/31), changes only the native
expectations, a frontend regression and evidence after `7b9435f`; product code is
identical. [Main push CI 37601276788](https://github.com/kurasis/CoinControl/actions/runs/37601276788)
verifies that exact source. Prior failed/cancelled sources above remain separate.
Final source API suite: **16 PASS / 2 FAIL**; Alchemy's four mainnets (403) and
Chainstack Solana (401) remain unavailable by owner confirmation. dRPC BNB passes.

`npm run test:live` shares 50 requests/provider across all its suites, retries included;
actual final [usage](ci/final/live/usage.json) peaks at 44. Native public BTC is a
separate opt-in run with its own persisted usage/assertion; its usage is additional,
not included in that API-suite ceiling. Earlier main/branch calls are also additional.
The cancelled startup branch 37598436210 and native-label branch 37601222206 made
partial reads ([startup usage](ci/CANCELLED_STARTUP_PR_LIVE_USAGE.json),
[native-label usage](ci/CANCELLED_NATIVE_LABEL_PR_LIVE_USAGE.json)); the cancelled
observer branch 37599426696 produced no live artifact. No task-wide claim of 50
requests/provider is made. Cancelled checks are never claimed as passes.

### Final Windows results

All **six application/build jobs PASS**; the overall CI result is **failure** solely
because the live API job retains the two credential/access failures. There is no
claim that the whole workflow is green or that external release gates passed.

| Scope                                    | Exact final result                                                                                |
| ---------------------------------------- | ------------------------------------------------------------------------------------------------- |
| Linux checks/build/offline/bindings      | PASS; 195 deterministic Rust / 60 frontend tests                                                  |
| Browser mock IPC                         | PASS; 24 combinations and bounded 10000-row table                                                 |
| Native Windows actual IPC                | **30 PASS**, plus one installer check explicitly outside this job's scope                         |
| Native new operations                    | Wallet rename, cache evidence retention, quota IPC and interval persistence PASS                  |
| Native cancellation/recovery             | **19 ms** acknowledgement; restart/resume, OS-enforced offline and no duplicated BTC history PASS |
| Native usage                             | Esplora 12 / mempool 3 / DeFiLlama 14; separate from API-suite counts                             |
| Production upgrade/uninstall             | **11 PASS**, real previous 0.1.0 upgrade                                                          |
| Production EXE/NSIS inspection           | **75 PASS**; production excludes automation/mock/keys                                             |
| Installed 100000-movement startup/UI/CSV | **12 PASS**; first normal fresh-cache screen **1844.60 ms**, target 2000 ms unchanged             |
| Three later offline launches             | **801.90 / 770.20 / 765.00 ms**; distinct from the first normal gate                              |
| Release Store load Linux/Windows         | PASS; 50 accounts / 500 assets / 100000 movements                                                 |
| Live API suite                           | **16 PASS / 2 FAIL**; Alchemy 403 and Chainstack 401                                              |

[Exact CI job metadata](ci/final/CI_RUN.json), [native report](ci/final/NATIVE_REPORT.json),
[installer](ci/final/INSTALLER_REPORT.json), [production inspection](ci/final/RELEASE_REPORT.json)
and [installed load](ci/final/NATIVE_LOAD_REPORT.json) record those scopes. The single
native installer scope exclusion is covered by the independent passing installer
job; it is not counted as a native PASS. Inspected [reconnected BTC UI](ci/final/NATIVE_RECONNECTED_BTC.png)
and [real network console](ci/final/NATIVE_NETWORK_CONSOLE.png) use the public test
address, not user data. Hosted Windows Server remains distinct from Windows 11.

### Published ZIP

[Download Windows x64 ZIP — 0.1.7](https://github.com/kurasis/CoinControl/releases/download/v0.1.7/CoinControl-0.1.7-windows-x64.zip)
was published by [workflow 37602970378](https://github.com/kurasis/CoinControl/actions/runs/37602970378).
It reused the inspected installer from main CI 37601276788 after all six required
application/build jobs passed; the known failed live job is explicitly excluded
from this existing prerelease packaging gate, not relabelled as passed.
Application source is exactly `a3512b0121e7ce19fc93b70bcd5bd969902b599d`.
Later evidence commits change only documentation and do not change that payload.

The ZIP was downloaded again: CRC, exact four files, all internal checksums,
external SHA-256, inspected installer hash, installed application hash, CI source
and startup report matches **PASS**. [Build manifest](BUILD_INFO.json) and
[published download verification](PUBLISHED_ZIP_VERIFICATION.json) retain evidence.
ZIP SHA-256: `fd96d2b8bb0b910a74efeb499e38cd8b1025e1bfeeefcb53e51c7006f595480b` (8335521 bytes).
Contents: unsigned setup EXE, README, build manifest and checksums. No user profiles
or keys are embedded; enter your own keys in Settings → Data sources.
The prerelease label and Alchemy/Chainstack/physical Windows 11 acceptance limitations
remain explicit. Prior release assets and all earlier failed evidence are preserved.
