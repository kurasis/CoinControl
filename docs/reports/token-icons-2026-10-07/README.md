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
