# Token logos and Windows 11 follow-up — 2026-10-07

## Implementation

`portfolio-providers/src/icons.rs` adds optional anonymous image retrieval from
[Trust Wallet's public assets repository](https://github.com/trustwallet/assets).
This catalog is image metadata, not evidence that a token is verified.
Native logos cover the ten supported networks. Token paths use the network and
locally validated contract/mint/master, including EIP-55 and bounceable TON
normalization. ETH on Base/Arbitrum/Optimism shares Ethereum's native logo.
Missing catalog entries keep the existing monogram. No ticker-based matching.

The additive `get_asset_icon(asset_id)` command first resolves an existing asset
from the current local profile. No URL, path, provider secret or HTTP headers can
be supplied through this command. The dedicated client sends no API credentials;
production URLs use only HTTPS `raw.githubusercontent.com/trustwallet/assets`.
Redirects are disabled. Requests time out after 5 seconds; failures are cached for
one hour. At most 128 requests per UTC day per application process; restarting
resets this anonymous budget. Financial/provider request quotas remain separate.

PNG MIME and input body limits (128 KiB), decoder allocation limit (2 MiB),
maximum dimensions (512×512) and animation rejection apply before re-encoding
64×64 RGBA pixels. No source metadata or trailing payload reaches the renderer.
The command returns a PNG data URL under the existing CSP; no new renderer
HTTP/filesystem permission. Opt-in network diagnostics show anonymous origin,
operation and status, never asset path or secret values.

At most 512 entries are retained in memory and the app-data `cache/token-icons`
directory. Cached files are decoded again before use. Successful cached logos
remain usable offline and across restarts, until Settings → Clear caches.
Successful logos do not automatically refresh. Disk/HTTP failures preserve the
monogram and do not change holdings, prices, accounting or verification labels.
Certificate loading, directory creation and downloads are deferred from startup;
UI requests follow its useful paint. Duplicate identities are coalesced.

## Validation and delivery

Before edits: `npm run check`, `npm run test:offline`, `npm run build` PASS on
main `086523df48fcce3bf0cdc06345476d2718521d0a`.
Targeted tests cover raster normalization, metadata removal, malformed/large
images, fixed catalog identity, credential-free HTTP, redirect/MIME rejection,
request budget, duplicate requests, disk reuse and clearing. Frontend cases
cover identity-based coalescing, missing images, rejected URL/SVG values, decode
failure and optional IPC failure. Native automation also checks a real BTC logo
and its offline rendering.

Final local checks: type checking, ESLint, Prettier, rustfmt, Clippy (including
`--all-features`), production frontend build and generated bindings PASS.
Offline suite: **200 deterministic Rust cases PASS** plus 18 disabled live
entrypoints (not live evidence), **66 frontend cases PASS**. Exact CI/published
artifact evidence is recorded after execution. No Windows or release pass is
claimed from local Linux. Browser/mock layout also PASS: 24 combinations,
12 pages/two panels and a 10000-row virtualized table (8 mounted final rows).
Initial browser launch was blocked by absent Chromium; installing the pinned
Playwright browser into the workspace cache resolved that environment issue.

## Windows 11 scope

The owner supplied `runs-on: windows-11-arm` rather than a self-hosted machine.
[GitHub's hosted-runner documentation](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)
lists this Windows 11 ARM64 label. A separate CI job verifies OS/CPU identity,
then installs the same inspected production **x64** installer and runs existing
startup (unchanged 2000 ms gate), 100000-movement UI and CSV import/cancellation
checks through actual native IPC and SQLite. Its artifacts have distinct names.
The ordinary Windows Server build/recovery/load jobs remain in place.

Windows 11 ARM64 running x64 emulation is separate evidence from Windows 11 on
Intel/AMD hardware. The latter is unrun; do not claim it passed. Screenshots,
performance reports, partial failures and OS identities remain traceable to
their selected run and payload.

## Initial main CI (retained failure)

PR #34 merged source `b84d2388496ca73ac3826bdfa250b2d475a363ef`.
[CI 37615103351](https://github.com/kurasis/CoinControl/actions/runs/37615103351):
offline/browser, both storage loads, installer inspection (78 checks), upgrade
(11 checks), and isolated native scenarios (32 checks) PASS. Production load
startup gates FAIL: Server 2543.30 ms; Windows 11 ARM64/x64 emulation 3348.90 ms
normal and 2306.00 ms worst offline. Both otherwise finish UI/import checks.
No ZIP is published from this failed startup candidate. [Server report](initial/server-load/NATIVE_LOAD_REPORT.json),
[Windows 11 report](initial/windows11-load/NATIVE_LOAD_REPORT.json), [host identity](initial/windows11-host/HOST.json).

Live: 14 actual suites PASS / 4 FAIL (Zerion, dRPC, networks, vertical-slice);
429 caused the source/integration failures. [Exact report](main-live/LIVE_REPORT.md).
After a cooldown, separate API-only [CI 37615854857](https://github.com/kurasis/CoinControl/actions/runs/37615854857)
on the identical source: dRPC all six EVM networks PASS (18 requests),
Zerion FAIL (3 requests, 429 again). Other 16 entrypoints are disabled, not
17 provider passes. Native/build jobs were skipped. No further blind retry.
Cancelled PR CI 37615076728 retains [partial usage](cancelled-pr-live/usage.json).

The Windows 11 slow log identifies grouped remaining-lot valuation: 1.346 s.
A local identical fixture query uses a temporary GROUP BY B-tree; a partial
covering index removes that sort and returns identical 500 grouped rows at
13.38× median query speed in five samples. [Plan/timing proof](INDEX_PROOF.json).
This is a demonstrated database optimization, not a native startup pass.

## Startup corrections (PR #35)

Source `4e7b48da7dcc02ad9544d4b8e82093f0cd6e625f` adds schema-8 covering
index `lots_valuation_remaining`, preserving exact textual grouping and FIFO
identity/order. A populated pre-index upgrade test preserves portfolio JSON,
integrity and validates no temporary GROUP BY B-tree. Production upgrade now
also requires the physical index; schema-6 golden baseline remains unchanged.

[SQLx's stable-Rust migration documentation](https://docs.rs/sqlx/latest/sqlx/macro.migrate.html)
requires a build script to track newly added migration paths. Workspace-cached
builds reproduced the stale-migrator failure; `portfolio-store/build.rs` now
watches `migrations`. Table policies/account/network labels are deferred after
the existing useful paint. Hidden assets wait for policies before table render.
No accounting formulas or public financial DTOs change.

[Final local checks](STARTUP_FIX_LOCAL_VALIDATION.json): 201 deterministic Rust
and 66 frontend PASS, static/all-features/build/bindings/browser PASS.
[New main CI 37619951445](https://github.com/kurasis/CoinControl/actions/runs/37619951445)
is tracked separately from the retained initial failure. Storage load checks
PASS on both OSes; Windows open+summary 83.74 ms. [Exact live report](index-main-live/LIVE_REPORT.md):
15 actual suites PASS / 3 FAIL (Zerion + two dependent integration suites, 429).
The rerun is the normal CI for changed application/schema code; previous
failed runs and cancelled-PR usage remain visible. No standalone unchanged
Zerion retry follows the failed targeted check.

## Final production and Windows 11 results

[Main CI 37619951445](https://github.com/kurasis/CoinControl/actions/runs/37619951445)
on source `4e7b48da7dcc02ad9544d4b8e82093f0cd6e625f` completed. Six existing
application/build jobs PASS. Overall CI remains FAIL: the live job and the
additional Windows 11 ARM64 job have explicit failures.

| Scope                                            | Result                                                                                                                                               |
| ------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------- |
| Static/offline/build/bindings                    | PASS; 201 deterministic Rust / 66 frontend tests; 18 disabled live entrypoints are not live passes                                                   |
| Browser mock IPC layout                          | PASS; 24 combinations and bounded 10000-row table                                                                                                    |
| Windows actual native IPC                        | 32 PASS, one installer-only scenario skipped and independently covered                                                                               |
| Installer inspection / production upgrade        | 78 / 12 PASS; populated schema 6 upgrades to 8 with the new physical index                                                                           |
| Windows Server installed production load         | 12 PASS; first normal useful screen 1617.20 ms; three offline process starts 993.80 / 666.30 / 690.40 ms                                             |
| Windows 11 ARM64, production x64 under emulation | 11 PASS / 1 FAIL; first normal useful screen 2938.90 ms exceeds unchanged 2000 ms gate; all three offline starts pass (1458.90 / 678.00 / 713.10 ms) |
| Live providers                                   | 15 actual suites PASS / 3 FAIL; Zerion 429 and two dependent integration suites                                                                      |

[Server load](server-load/NATIVE_LOAD_REPORT.json),
[Windows 11 load](windows11-load/NATIVE_LOAD_REPORT.json),
[Windows 11 host identity](windows11-host/HOST.json),
[Windows 11 startup phases](windows11-load/STARTUP_PHASES.json).
On the first ARM64 launch, cached summary is ready in 401 ms after the Rust
startup clock begins; WebView becomes ready at 2536 ms. The measured screen
latency includes pre-Rust startup and frontend paint, so these phase values
are not substituted for the process-to-screen acceptance measure. Later
launches improve; the failed first normal launch is retained and not discarded.
The database correction resolves the prior offline startup failure; it does
not establish the first normal ARM64 launch gate or Windows 11 Intel/AMD acceptance.

Root release/native/browser/load reports and logo screenshots now represent
this final source. The superseded reports/screenshots are preserved under
`initial/general`; the initial failed native-load reports remain under `initial`.
`INDEX_LOAD_*` retain the separately downloaded storage measurements of the
same final source. Local query timing is separate from all native startup gates.

Alchemy's five enabled mainnets, dRPC's six EVM networks and Chainstack native
SOL pass real reads. Chainstack SPL discovery still returns 403 for
`getTokenAccountsByOwner`; enable that method/permission on the selected Solana
RPC node if full Chainstack token discovery is required. Helius remains the
available token/history source. Zerion needs available project quota/rate
capacity; the bounded retries still receive 429. Keys are not requested in chat.
Logos and their offline rendering pass native checks; incomplete RPC mirrors
and provider history categories remain explicitly partial in the application.

## Published ZIP

[Download 0.1.8 Windows x64 ZIP](https://github.com/kurasis/CoinControl/releases/download/v0.1.8/CoinControl-0.1.8-windows-x64.zip).
[Publisher 37622823297](https://github.com/kurasis/CoinControl/actions/runs/37622823297)
PASS. This workflow requires the six existing application/build jobs; it does
not require live provider or additional ARM64 acceptance passes. The packaged
source is exactly `4e7b48da7dcc02ad9544d4b8e82093f0cd6e625f`, installer hash
`5a57a1d62baccf12a3f68506dda5e65b188003d8ddbd06df0e9ee138d5078ee9`,
application hash `bca18be243c98e46efa01672d901067759ff04e024996c5e64bcee7029b51778`.

Re-downloaded ZIP checks PASS: CRC, exact four files, internal/external SHA-256,
manifest/source/run identity, installer payload and production reports including
the unchanged Server startup gate. SHA-256:
`dba94a81b86b8758d4c98b1d104460016d4522c3d1c3e021dceadcd626d8d578`
(8463065 bytes). [Build manifest](BUILD_INFO.json),
[verification](PUBLISHED_ZIP_VERIFICATION.json). No keys or user data. Unsigned
prerelease; `fullReleaseAcceptance: false`. Previous published ZIPs are untouched.

Cloud installation/start instructions now include the workspace Playwright
Chromium cache and disable Rust incremental compilation for this limited disk.
The saved environment configuration is a draft: review/save the settings and
publish the environment to activate it. No credential values were added to it.
