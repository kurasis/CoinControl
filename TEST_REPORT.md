# Verification report — 2026-10-07

## Ankr reserve and shared free-plan limits (0.1.10)

[Implementation, official sources and preserved failed attempts](docs/reports/ankr-2026-10-07/README.md).
Ankr Node RPC and exact Advanced EVM token balances are optional reserves.
Separate credential-shared transport gates enforce the owner's 30 requests/second
Node and 30 requests/minute Advanced limits. Both share request/credit ceilings.
First 429 advances to an independent mirror; local-budget exhaustion persists a
nonfatal pause until the next UTC day. Cached balances stay stale rather than
being zeroed. Duplicate known-token Node reads after an Advanced response were
removed without changing public DTOs or accounting formulas.

Type/lint/format/rustfmt/Clippy/build PASS; **229 deterministic Rust / 67 frontend /
8 report-policy tests PASS**. 23 disabled live entrypoints are not API evidence.
Exact application source `c9913a1076079dd0ce7b0dd551a391477f8374be`,
[main CI 37652973461](https://github.com/kurasis/CoinControl/actions/runs/37652973461):
actual live gate PASS, **17 PASS / 2 RATE_LIMITED / 4 PARTIAL suites**.
Ankr uses the unchanged shared 50-request / 50000-estimated-credit test ceiling;
actual usage **50 requests / 16300 estimated credits**.

Ankr Node native/mainnet reads PASS on Ethereum, Base, Arbitrum, Polygon and BNB.
Optimism and Solana Node return HTTP 403: PARTIAL, with actual independently
persisted Blockscout / Alchemy balances. Check those scopes and access/allowlist
in the Ankr project; 403 does not establish a paid-plan requirement. Advanced
returns valid exact tokens on all six EVM networks; Ethereum ignores requested
page size and returns 1106 assets, so only 200 bounded fungible tokens are fresh
and coverage remains PARTIAL. BNB Ankr persistence and all ten independent
balance routes PASS. Zerion and dRPC are RATE_LIMITED with real mirror evidence;
indexed BNB history is still not established. Existing Alchemy SOL/SPL/Token-2022
reserve PASS; no Ankr Solana token acceptance is claimed.

Installer inspection / populated upgrade: **78 / 12 PASS**. Native Windows IPC:
32 PASS and one independently covered installer skip. Installed Windows Server
production load: **12 PASS**, first normal useful screen **1499.80 ms**, offline
starts **844.10 / 713.60 / 683.20 ms**. Additional Windows 11 ARM64 x64 emulation:
**11 PASS / 1 FAIL**, first normal useful screen **4559.80 ms** against the unchanged
2000 ms gate; offline starts **1669.60 / 676.50 / 666.70 ms** PASS. All six required
app/build jobs and the live job PASS; overall CI FAIL due to the additional ARM64
gate. This source was not rerun to seek a passing sample. Windows 11 Intel/AMD
hardware acceptance remains unrun, and full release acceptance is not claimed.

The unchanged ZIP packager verified the exact CI installer locally: 8486521 bytes,
SHA-256 `70eaca0d0854359913ef13cb615cdd3dac1835fd0888eef0a8422a5d4eccf2ac`.
GitHub returned HTTP 500 for both publisher dispatch and direct release creation;
**0.1.10 is not published**. [Local verification](docs/reports/ankr-2026-10-07/LOCAL_ZIP_VERIFICATION.json).
The previously published 0.1.9 remains the latest downloadable GitHub release.

## Free-plan throttling follow-up (0.1.9)

[Download Windows x64 ZIP](https://github.com/kurasis/CoinControl/releases/download/v0.1.9/CoinControl-0.1.9-windows-x64.zip).

Chainstack Developer owner scans are officially paid-only, not a bad credential.
The adapter now uses native SOL/finality only and tries an independent token
mirror without requiring an upgrade. The existing Alchemy key adds a standard
Solana balance/token reserve with a shared EVM budget, conditional on Solana
mainnet access. First 429 advances immediately to reserves;
source quotas and partial history are distinct from source acceptance.
[Current routing and evidence policy](docs/PROVIDER_MIRRORS.md) ·
[Exact source, official restrictions and preserved attempts](docs/reports/free-plan-failover-2026-10-07/README.md).
Sync quotas no longer populate fatal source diagnostics in Settings.
Type/lint/format, all-feature Clippy, build, unchanged bindings and
209 deterministic Rust / 67 frontend / 6 report-policy tests PASS;
20 opt-out live entrypoints are not live evidence.

Source `99123fce346d09a31e037d84306bdef205351359`,
[main CI 37636073946](https://github.com/kurasis/CoinControl/actions/runs/37636073946):
actual live gate PASS, 17 PASS / 1 RATE_LIMITED / 2 PARTIAL suites.
Alchemy's existing key passed SOL and both owner-token programs; all ten
independent balance routes passed. Zerion remains limited; native mirrors do
not establish indexed BNB history. Installer inspection / upgrade: 78 / 12 PASS.
Final installed production: Windows Server 12 PASS, first normal useful screen
1894.80 ms; native IPC 32 PASS plus one independently covered installer skip.
Additional Windows 11 ARM64 x64 emulation: 11 PASS / 1 FAIL, first normal useful
screen 2992.70 ms against the unchanged 2000 ms gate. Its three offline starts
PASS. All six required application/build jobs and the actual live job PASS;
overall CI FAIL due to the extra ARM64 gate. Windows 11 Intel/AMD remains unrun.

The preceding source's Server first launch (2308 ms) and Windows 11 ARM64
x64 emulation (2915.5 ms) both exceeded the unchanged 2000 ms gate. That
candidate was not published; failures and cancelled-run API usage are preserved.
Historical 0.1.8 quota/plan failures below remain unchanged.

[Publisher 37639630437](https://github.com/kurasis/CoinControl/actions/runs/37639630437)
PASS; redownload CRC, exact four files, internal/external SHA-256,
source/run/version and tested installer/payload/report matches PASS.
SHA-256 `7f6e4f80fc13b211c79c48ab18788f27a72d78926be1ebd6aef6187b54c5dac6`
(8477871 bytes). [Manifest](docs/reports/free-plan-failover-2026-10-07/BUILD_INFO.json)
· [Verification](docs/reports/free-plan-failover-2026-10-07/PUBLISHED_ZIP_VERIFICATION.json).
Unsigned prerelease, `fullReleaseAcceptance: false`; the extra ARM64 failure
and Windows 11 Intel/AMD acceptance remain separate limitations.

## Published token logos and startup correction (0.1.8)

[Download Windows x64 ZIP](https://github.com/kurasis/CoinControl/releases/download/v0.1.8/CoinControl-0.1.8-windows-x64.zip) · [Implementation and exact evidence](docs/reports/token-icons-2026-10-07/README.md).

Bounded sanitized PNG cache and monogram fallback are implemented, with native
logo/offline checks. Schema-8 covering index accelerates remaining-lot valuation
without changing accounting results; SQLx now tracks new migration paths.
Optional table metadata is fetched after the first useful paint.

Exact source `4e7b48da7dcc02ad9544d4b8e82093f0cd6e625f`,
[main CI 37619951445](https://github.com/kurasis/CoinControl/actions/runs/37619951445):
six existing application/build jobs PASS. Overall CI FAIL: additional Windows 11
ARM64 startup gate and live-provider checks have the failures detailed below.

| Scope                                      | Result                                                                                                                                   |
| ------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------- |
| Type/lint/format/Clippy/build/bindings     | PASS                                                                                                                                     |
| Offline tests                              | 201 deterministic Rust / 66 frontend PASS; 18 opt-out live entrypoints are not live evidence                                             |
| Browser mock IPC                           | PASS; 24 combinations and 10000-row bounded table                                                                                        |
| Native real Windows IPC                    | 32 PASS; one installer-only case skipped, independently covered                                                                          |
| Installer inspection / populated upgrade   | 78 / 12 PASS; schema 6 to 8, physical covering index checked                                                                             |
| Installed Windows Server production load   | 12 PASS; normal useful screen 1617.20 ms; three offline starts all below one second                                                      |
| Windows 11 ARM64, production x64 emulation | 11 PASS / 1 FAIL; normal useful screen 2938.90 ms exceeds unchanged 2000 ms gate; three offline starts 1458.90 / 678.00 / 713.10 ms PASS |
| Actual live suites                         | 15 PASS / 3 FAIL; Zerion 429 plus network/vertical-slice suites depending on it                                                          |

ARM64 first-start logs show cached summary ready at 401 ms, WebView ready at
2536 ms after the Rust startup clock begins. This does not replace the failed
process-to-screen measurement. Windows 11 Intel/AMD remains unrun. Alchemy's
five mainnets and dRPC's six EVM networks pass; Chainstack native SOL passes,
while `getTokenAccountsByOwner` still returns 403 (partial SPL discovery).
Zerion needs available project quota/rate capacity; its bounded retries still
receive 429. Full original-spec acceptance is not claimed.

[Publisher 37622823297](https://github.com/kurasis/CoinControl/actions/runs/37622823297)
PASS using the exact inspected production payload. Its six application/build
gates passed; live and the extra ARM64 job are separate acceptance constraints.
Re-downloaded ZIP CRC, exact four files, internal/external SHA-256 and
manifest/source/installer/production-report matches PASS.
SHA-256: `dba94a81b86b8758d4c98b1d104460016d4522c3d1c3e021dceadcd626d8d578`
(8463065 bytes). [Manifest](docs/reports/token-icons-2026-10-07/BUILD_INFO.json) ·
[ZIP verification](docs/reports/token-icons-2026-10-07/PUBLISHED_ZIP_VERIFICATION.json).
Unsigned prerelease; `fullReleaseAcceptance: false`. Original failed candidate,
targeted API retries and cancelled-PR usage are preserved; older published
assets and their historical reports remain unchanged.

## Updated provider access — 2026-10-07

After the owner updated Alchemy/Chainstack credentials, targeted real access
[CI 37606760853](https://github.com/kurasis/CoinControl/actions/runs/37606760853)
passed, followed by the complete API-only
[CI 37607032943](https://github.com/kurasis/CoinControl/actions/runs/37607032943):
**18 suites PASS / 0 FAIL**. Alchemy Ethereum/Base/Arbitrum/Optimism/Polygon and
Chainstack native SOL access now pass. Chainstack `getTokenAccountsByOwner` still
returns 403: SPL discovery is explicitly partial, with Helius/Zerion supplying
required token coverage. API-only jobs skip native/build checks; the published
0.1.7 payload and its historical Windows evidence below are unchanged.

[Exact new reports and remaining tasks](docs/reports/provider-access-2026-10-07/README.md):
at that revalidation, token-icon caching and Windows 11 acceptance were still
open. The 0.1.8 follow-up above implements the cache and records actual ARM64 tests.
Its first normal startup gate remains failed; Intel/AMD acceptance is unrun.
These remaining requirements prevent a claim of complete original-specification acceptance. Historical credential errors
below describe their original source/run, not current access.

## Published 0.1.7 specification completion

[Download Windows x64 ZIP](https://github.com/kurasis/CoinControl/releases/download/v0.1.7/CoinControl-0.1.7-windows-x64.zip) · [Requirement mapping, all evidence and configuration steps](docs/reports/v1-completion-2026-10-07/README.md).

Account/wallet management, safe removal, cache/diagnostics/rescan jobs, queued
manual synchronization, retained chart/table state, shared editable quotas,
scheduling and bounded chain finality/rollback checks are delivered. Existing
public contracts remain compatible. Initial scope, native test, fixture-launch
and startup failures are preserved with their corrections.

Exact source `a3512b0121e7ce19fc93b70bcd5bd969902b599d`,
[main CI 37601276788](https://github.com/kurasis/CoinControl/actions/runs/37601276788):
**six application/build jobs PASS**. Overall CI is failed solely on the live API
job: **16 suites PASS / 2 FAIL**, Alchemy 403 on Base/Arbitrum/Optimism/Polygon and
Chainstack Solana 401. dRPC BNB passes after an intermittent earlier 429; no
unchanged authentication-only retry was used to conceal a failure.

| Scope                                       | Exact final result                                                                                              |
| ------------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| Static/offline/build/bindings               | PASS; 195 deterministic Rust / 60 frontend cases                                                                |
| Browser mock IPC                            | PASS; 24 combinations and bounded 10000-row table                                                               |
| Native actual Windows IPC                   | 30 PASS; installer scope is covered independently                                                               |
| Native cancel/resume/offline/console        | PASS; cancellation acknowledgement 19 ms                                                                        |
| Production upgrade/uninstall and inspection | 11 / 75 PASS                                                                                                    |
| Installed production 100000-movement load   | 12 PASS; normal useful screen **1844.60 ms**, unchanged 2000 ms gate; later offline 801.90 / 770.20 / 765.00 ms |
| Storage load Linux/Windows                  | PASS; 50 accounts / 500 assets / 100000 movements                                                               |

[Publisher 37602970378](https://github.com/kurasis/CoinControl/actions/runs/37602970378)
passed and packaged that exact inspected/tested installer. Re-downloaded ZIP CRC,
four-file contents, internal/external checksums and source/payload report matching
passed ([manifest](docs/reports/v1-completion-2026-10-07/BUILD_INFO.json),
[verification](docs/reports/v1-completion-2026-10-07/PUBLISHED_ZIP_VERIFICATION.json)).
SHA-256: `fd96d2b8bb0b910a74efeb499e38cd8b1025e1bfeeefcb53e51c7006f595480b` (8335521 bytes).

This is an unsigned prerelease. Fix Alchemy mainnet entitlements and Chainstack
node RPC credentials using the linked configuration instructions; Actions secrets
are CI-only and must also be entered separately in app Settings → Data sources.
Hosted Windows Server validation does not establish physical Windows 11 acceptance.
Historical results and prior immutable release assets below remain separate.

## Current development follow-up

[Code audit](docs/reports/code-audit-2026-10-07/README.md) merged as PR #24.
Its exact source `153c246` CI completed: offline/build/load/native recovery passed;
live had Alchemy 403 / Chainstack 401 / dRPC BNB 429, and the first production
normal-network launch exceeded the unchanged 2000 ms gate (3499.30 ms).
The other 11 production load checks passed. These results do not replace the
successful earlier release-specific evidence below.

The four owner-approved chart/timezone/Rust/Actions improvements and their
[follow-up verification](docs/reports/chart-timezone-actions-2026-10-07/README.md)
are delivered in [PR #25](https://github.com/kurasis/CoinControl/pull/25).
Exact code `cc2215edb3ef13899dc2e6a230efceae96decfc5`, full manual
[CI 37583312836](https://github.com/kurasis/CoinControl/actions/runs/37583312836):
six application/build jobs PASS; 12 production native checks PASS, first normal
useful screen 1456.70 ms within the unchanged 2000 ms gate. Live still has 15
passing suites and 3 failures (Alchemy 403, Chainstack 401, dRPC BNB 429).
The old startup failure remains recorded. The published ZIP retains its exact
previous source; publishing workflow was not invoked in this code task.

## Published 0.1.6 verification

Automatic provider reserves are merged in main. Exact application source [acb280dbe73c4b6c4b8ede81f8523516d9769cc9](https://github.com/kurasis/CoinControl/commit/acb280dbe73c4b6c4b8ede81f8523516d9769cc9), [CI 37471182519](https://github.com/kurasis/CoinControl/actions/runs/37471182519), attempt 2: **six application/build jobs passed; the live job failed on external API restrictions**. The installer retry retained the same source and every assertion after a failure launching pinned previous 0.1.0, before any upgrade. Full [sanitized reports, failure history and native screenshot](docs/reports/provider-reserves-2026-10-06/README.md).

| Scope                                   | Result                                                                                                                            |
| --------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| Static/offline/build/bindings           | PASS; 172 deterministic Rust / 37 frontend tests                                                                                  |
| Browser with mock IPC                   | PASS; 24 combinations, 12 pages, 2 panels, bounded 10000-row table                                                                |
| Native Windows / actual display scaling | 26 PASS; recovery, independent source status, network console and bounded public reads                                            |
| Production upgrade / EXE inspection     | 11 / 64 PASS                                                                                                                      |
| Installed startup / large UI / CSV      | 12 PASS; normal 1567.90 ms, offline 969.50 / 823.00 / 840.50 ms; unchanged ≤2000 ms target                                        |
| Release Store load, Linux and Windows   | PASS; 50 accounts / 500 assets / 100000 normalized legs; cached query p95 <300 ms                                                 |
| New live reserves                       | PASS: mempool, PublicNode, Blockscout free ETH/ARB/OP, Etherscan ETH/ARB/POL, TON Center; engine budget-to-reserve routing passed |
| dRPC / Chainstack                       | Five EVM chains passed; BNB throttled. Chainstack Solana credential rejected with 401; needs its mainnet node endpoint            |
| Existing live services                  | Helius/Alchemy Ethereum passed; four other Alchemy mainnets remain blocked by 403 and Zerion suites by 429                        |
| Physical Windows 11                     | BLOCKED; hosted Windows Server is separate evidence                                                                               |

Reserves keep cached history, fees and cost-basis data. Non-BTC reserves expose balance-only partial coverage, exact observed amounts and stale omitted assets. Keyless public quotes are supported; keys stay in the OS credential store and are not included in builds. [Routing and free-plan contracts](docs/PROVIDER_MIRRORS.md).

[Download Windows x64 ZIP — v0.1.6](https://github.com/kurasis/CoinControl/releases/download/v0.1.6/CoinControl-0.1.6-windows-x64.zip). [Publisher 37474555540](https://github.com/kurasis/CoinControl/actions/runs/37474555540) passed, using the exact inspected installer and tested application payload. Re-downloaded ZIP CRC, four contents, internal/external checksums and source/installer matching passed. SHA-256: `2180e9be116c5b77ef627f2603abe241ad97a6cde56a935136a0c44442f1649f` (8108206 bytes). [Build manifest](docs/reports/provider-reserves-2026-10-06/BUILD_INFO.json) · [verification](docs/reports/provider-reserves-2026-10-06/PUBLISHED_ZIP_VERIFICATION.json). This remains an unsigned prerelease; API and physical Windows 11 gates above are explicit.

## Historical 0.1.5 verification

[Sync status and console PR #16](https://github.com/kurasis/CoinControl/pull/16) is merged. Exact tested application code: [a1492978c4adf962dde4ec1af7a7e788c5495845](https://github.com/kurasis/CoinControl/commit/a1492978c4adf962dde4ec1af7a7e788c5495845), [CI 37449823930](https://github.com/kurasis/CoinControl/actions/runs/37449823930). **Six application/build jobs passed; the live job failed on existing Alchemy 403 and Zerion 429 gates.** Historical 0.1.4 evidence below remains source-specific.

| Scope                                         | Result                                                                                         |
| --------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| Check/offline/build/bindings                  | PASS; 158 deterministic Rust / 36 frontend tests                                               |
| Browser with mock IPC                         | PASS; 24 combinations including the console                                                    |
| Native Windows / actual display scale         | 26 PASS, including sync spinner/final outcome and real network console controls/persistence    |
| Production upgrade and EXE/NSIS inspection    | 10 / 64 PASS                                                                                   |
| Installed production startup / large UI / CSV | 12 PASS; first normal 1643.30 ms, offline 902.00 / 858.50 / 840.90 ms, unchanged ≤2000 ms gate |
| Release Store load, Linux and Windows         | PASS; 50 accounts / 500 assets / 100000 legs; cached query p95 <300 ms                         |
| Helius / Alchemy Ethereum                     | PASS; real read-only evidence                                                                  |
| Four other Alchemy mainnets / Zerion suites   | FAIL / external BLOCKED; HTTP 403 / 429                                                        |
| Physical Windows 11                           | BLOCKED; hosted Windows Server is separate evidence                                            |

[Full sanitized reports and inspected native console screenshot](docs/reports/sync-console-2026-10-06/README.md). The console uses bounded opt-in memory diagnostics with no keys, wallet addresses, URL paths, headers or bodies. Old settings remain compatible. Connection tests fix an existing double-lock wait.

[Download Windows x64 ZIP — v0.1.5](https://github.com/kurasis/CoinControl/releases/download/v0.1.5/CoinControl-0.1.5-windows-x64.zip). [Publisher 37451154966](https://github.com/kurasis/CoinControl/actions/runs/37451154966) passed, reusing the inspected/tested production payload. Re-downloaded ZIP CRC, four contents, internal/external checksums and exact source/installer matching passed. SHA-256: `06d28b08c9037f648017ecf2285df31483d2798ba1bf2a6c01745f6bf567d046` (8032522 bytes). It remains an unsigned prerelease; API and physical Windows 11 gates are explicit.

## Historical 0.1.4 verification

Development version **0.1.4** is delivered on main through [provider PR #11](https://github.com/kurasis/CoinControl/pull/11) and [startup correction PR #12](https://github.com/kurasis/CoinControl/pull/12). Exact final tested code: [54e82df306aa19c1408a9fbc6f66761db27222c5](https://github.com/kurasis/CoinControl/commit/54e82df306aa19c1408a9fbc6f66761db27222c5), [CI 37442084560](https://github.com/kurasis/CoinControl/actions/runs/37442084560): **six jobs passed; only the live API job failed because four Alchemy mainnets returned 403 and Zerion returned 429**. Credential availability is established through authenticated reads. This is a development build; full release acceptance remains blocked by those API gates and physical Windows 11 verification. Evidence follow-up commits change documentation only.

## Published Windows prerelease ZIP

[Windows x64 ZIP, v0.1.4](https://github.com/kurasis/CoinControl/releases/download/v0.1.4/CoinControl-0.1.4-windows-x64.zip) is published as a prerelease, not full release acceptance. [Packaging/publishing workflow 37445549616](https://github.com/kurasis/CoinControl/actions/runs/37445549616) passed on packaging source bea3d6c. It reused the exact production installer from CI 37442084560 after matching release/upgrade/startup reports and installer/application payload hashes. Application code remains the tested 54e82df source; subsequent changes add reports and packaging automation.

The published ZIP was downloaded again: CRC, exact four-file contents, inner checksums and external SHA-256 passed. ZIP SHA-256: `12f2ae2273b6d544dc94adef2fd44e2cd61b25d66b7b91fb8c8dc343b9018af4` (8016340 bytes). It contains the unsigned setup EXE, README, build information and file checksums. A tampered installer was rejected in a local negative control. The release gate limitations below remain in force.

## 0.1.4 final verification

| Scope                                                            | Result                                                                                   |
| ---------------------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| Static/type/lint/Clippy, offline tests, frontend build, bindings | PASS; 153 deterministic Rust / 34 frontend tests; ten live functions opt out locally     |
| Browser with mock IPC                                            | PASS; 24 combinations; separate from native verification                                 |
| Native Windows and real 125/150/200% scale                       | 24 PASS; independent installer scope is checked separately                               |
| Production upgrade/restart/uninstall / EXE and NSIS inspection   | 10 / 64 PASS                                                                             |
| Release Store load on Linux and Windows                          | PASS; 50 accounts, 500 assets, 100000 legs; cached p95 <300 ms                           |
| Installed production startup / large UI / CSV                    | **12 PASS / 0 FAIL**; first normal 1730.1 ms; offline 870.8 / 738.7 / 1273.9 ms          |
| Helius / Alchemy Ethereum live                                   | PASS; real read-only balances/history; independent exact Ethereum principal/fee evidence |
| Alchemy four other mainnets / Zerion-dependent suites            | FAIL / external BLOCKED; 403 / 429 respectively                                          |
| Physical Windows 11                                              | BLOCKED; not established by hosted Windows Server                                        |

[Final sanitized reports, inspected screenshot and exact provenance](docs/reports/rpc-providers-2026-10-06/final/README.md). All four production launches retain the original ≤2000 ms gate and startup marks. Final cancellation was 24 ms; one CSV decision replayed all 100000 existing legs in 30751 ms; maximum renderer frame gap 265.6 ms passed the original responsiveness check. Store reopen/summary measured 173.44 ms on Linux and 296.87 ms on Windows. These measurements have distinct scopes; the synthetic dataset is not 100000 API downloads or CSV rows.

## Initial integration verification (historical failure)

Initial source [9f782af5aee62a644735c062e0280b7345716afa](https://github.com/kurasis/CoinControl/commit/9f782af5aee62a644735c062e0280b7345716afa), [CI 37439865917](https://github.com/kurasis/CoinControl/actions/runs/37439865917), had five passing jobs, the same external API failures and one production startup failure. Its exact results remain below rather than being relabeled as final evidence.

### Initial results

| Scope                                                     | Result                                                                                                 |
| --------------------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| Static checks / offline tests / production frontend build | PASS; 152 deterministic Rust and 34 frontend tests; ten live functions opt out locally                 |
| Chromium with mock IPC                                    | PASS; 24 combinations, separate from native verification                                               |
| Windows native app and display scaling                    | PASS; 24 checks, actual 125/150/200% scale scenarios                                                   |
| Populated production upgrade/restart/uninstall            | PASS; ten checks, 0.1.0/schema 6 → 0.1.4/schema 7                                                      |
| Production EXE/NSIS inspection                            | PASS; 64 checks; production excludes native-e2e and credentials                                        |
| Release-mode Store load                                   | PASS on Linux and Windows; 50 accounts / 500 assets / 100000 normalized legs; cached query p95 <300 ms |
| Installed production startup and large UI/CSV             | **11 PASS / 1 FAIL**; first normal useful screen 2124.4 ms exceeds the unchanged 2000 ms gate          |
| Helius live Solana                                        | PASS; supported fungible balances and two related-account history pages                                |
| Alchemy live Ethereum                                     | PASS; native/USDC balances, history pagination, independent 79 ETH payment and exact receipt fee       |
| Alchemy Base / Arbitrum / Optimism / Polygon              | FAIL / access BLOCKED; all four mainnet endpoints return 403                                           |
| Other independent live providers                          | PASS; Esplora, Live Coin Watch, DefiLlama, TronGrid, TonAPI                                            |
| Zerion-dependent live suites                              | FAIL / quota BLOCKED; HTTP 429                                                                         |
| Physical Windows 11                                       | BLOCKED; hosted Windows Server does not establish this gate                                            |

[Sanitized integration live reports, source provenance and initial production/load results](docs/reports/rpc-providers-2026-10-06/README.md). Original CI artifacts contain the full native/browser/installer evidence; earlier inspected screenshots and acceptance details remain in [0.1.3 native evidence](docs/reports/native-ci-2026-10-06/README.md) and [accessible browser charts](docs/reports/accessible-charts-2026-10-06/README.md). These older results are source-specific and do not replace current failed gates.

## Provider behavior and limits

With a configured desktop key, Solana uses Helius; Ethereum, Base, Arbitrum, Optimism and Polygon use Alchemy. BNB retains Zerion, and Bitcoin/TRON/TON retain existing sources. Without the new key, the original route remains. Installed production keys belong in Settings → Data sources and the OS credential store; GitHub Actions secrets supply CI only.

Helius includes parsed SPL/Token-2022 balances, exact native/owned-token deltas and finalized related-account history. NFTs/unclassified zero-decimal mints are excluded; omitted cached holdings remain stale. Only independently evidenced plain native transfers get automatic counterparties. Program/rent effects and incompletely decoded movements require review. Minimum-context-slot lag receives bounded charged retries. Complete Solana history is not claimed.

Alchemy provides exact native/ERC-20 balances and incoming/outgoing transfer discovery, durable block/event cursors, reused metadata and receipt-derived fees (including Base/Optimism L1 evidence). Capped discovery retains omitted holdings as stale. The transfer index excludes some failed calls, approvals and internal movements; history remains partial. Existing richer transactions from another source retain their legs, fees and review links. A forbidden network stops that endpoint while other accessible networks continue.

Both adapters enforce request and estimated-credit budgets, count retries, support checkpointed cancellation and redact URL-key error bodies. Settings shows routing, coverage and estimated costs. [Full implementation and official API contracts](docs/PROVIDER_INTEGRATION.md).

The complete live job has **six passing / four failing suites**. Actual shared totals: Alchemy 12 requests / 4270 estimated CU, Helius 5 / 23 credits; all eight providers stayed within 50 requests each. Alchemy's Ethereum success does not prove access to the four rejected networks. Its key needs those mainnet entitlements; separate-key configuration is not implemented. Zerion stops after three actual requests due to 429. No paid plan or overage was enabled.

## Startup failure and corrective work

Initial integration source first normal-network launch: **2124.4 ms**. Three offline process launches: **1525.5 / 1486.4 / 1505.0 ms**. All four retain the original **≤2000 ms** requirement. The failed first normal launch is preserved in [INITIAL_NATIVE_LOAD_REPORT.json](docs/reports/rpc-providers-2026-10-06/INITIAL_NATIVE_LOAD_REPORT.json).

Remaining large-data checks passed: cancellation 20 ms, maximum rendering gap 250.0 ms, and one basis CSV decision committed/replayed 100000 existing legs in 33439 ms. The dataset is synthetic normalized SQL evidence, not 100000 API downloads or CSV rows. Memory measurements cover the main process and exclude child WebViews. Useful screen means real balance/navigation after two frames, not completion of every chart or row.

The corrective implementation groups identical remaining lot values in SQLite and multiplies their counts using Rust arbitrary-precision decimals. Summary/holdings no longer reconstruct every FIFO identity and acquisition timestamp. SQL floating-point SUM is never used, and detailed lot/audit views retain the individual records. The regression covers duplicate lots, amounts above 2^53, fine decimal basis, unknown/estimated/zero basis, missing/zero prices, mismatched observations and account scope. The unchanged Windows startup gate passed on corrected source 54e82df; all 12 checks and original failure evidence are preserved.

## Evidence history and release gates

0.1.3 tested source [8830f1827a002f52a47eba14713d4fdfc0edbc64](https://github.com/kurasis/CoinControl/commit/8830f1827a002f52a47eba14713d4fdfc0edbc64), CI 37430180430, had six passing jobs and a Zerion-dependent live failure. Its original 1815.1 ms normal startup and three passing offline launches remain valid historical evidence. Parallel native-window/cache priming and lazy chart canvas rendering remain in the implementation.

Earlier source-specific evidence: [CI recovery](docs/reports/ci-recovery-2026-10-06/README.md), [responsive UI](docs/reports/responsive-ui-2026-10-05/README.md), [native upgrade](docs/reports/upgrade-and-native-2026-10-05/README.md), [release follow-up](docs/reports/release-acceptance-2026-10-05/README.md), [imported report](docs/reports/IMPORTED_TEST_REPORT.md).

The 0.1.4 production startup gate passed. Full release acceptance still requires Alchemy network access, Zerion quota and physical Windows 11 verification. The v0.1.4 tag publishes a prerelease ZIP; no full release pass, physical Windows 11 pass or complete blockchain coverage is claimed.
