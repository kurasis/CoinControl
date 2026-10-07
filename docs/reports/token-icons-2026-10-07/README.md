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
