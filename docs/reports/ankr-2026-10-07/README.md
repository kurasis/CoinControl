# Ankr integration and bounded free-plan checks — 2026-10-07

Application source: `c9913a1076079dd0ce7b0dd551a391477f8374be`, version **0.1.10**.
[Main CI 37652973461](https://github.com/kurasis/CoinControl/actions/runs/37652973461)
has six required application/build jobs and the actual live-provider job passing.
Overall CI is **FAIL** because the additional Windows 11 ARM64 startup gate fails.
[Exact run](CI_RUN.json) · [Jobs](CI_JOBS.json) · [Live report](LIVE_REPORT.md).

[Download Windows x64 ZIP 0.1.10](https://github.com/kurasis/CoinControl/releases/download/v0.1.10/CoinControl-0.1.10-windows-x64.zip).
[Publisher 37656795461](https://github.com/kurasis/CoinControl/actions/runs/37656795461)
PASS. Redownloaded ZIP CRC, exact four files, internal/external SHA-256,
source/run/version, installer/payload/report matches and unchanged startup gate
PASS. Published ZIP: **8486521 bytes**, SHA-256
`d107898a0a89b685bb8f6e26cd309e89f5c6a0bcc62c2e3f5d24f536518f6a2d`.
[Build manifest](BUILD_INFO.json) · [Published verification](PUBLISHED_ZIP_VERIFICATION.json)
· [Publisher run](PUBLISHER_RUN.json) · [Delivery status](DELIVERY.json).
Unsigned prerelease, `fullReleaseAcceptance: false`.

Initial workflow dispatch/release creation returned HTTP 500. A minimal REST
request created draft release 405978713, but CLI and explicitly authenticated
local uploads returned HTTP 401. The exact CI installer had already passed local
packaging; [that local verification](LOCAL_ZIP_VERIFICATION.json) is preserved.
Publication succeeded using the Actions token on temporary branch
`delivery/ankr-0.1.10`, source `865ebf252ebee7f7d2a84421925951e2a7145515`.
Only a branch-specific push trigger and explicit run-ID fallback were added
there; the unchanged packager, six required verification gates, tested app source
and provider ceilings were retained. No API tests were repeated. The temporary
trigger is not merged into main. Source reports are stored with normalized JSON
formatting; verification compares their data and the inspected installer/payload,
not the formatting of report files.

## Implementation and limits

Ankr is an optional credential-backed reserve in Settings → Data sources. Set the
key there for the installed application; `ANKR_API_KEY` in GitHub Actions enables
read-only CI probes, and is never embedded in the installer or frontend.
Existing public DTOs, provider fields and accounting formulas are unchanged.
No dependency or database migration was added.

- Node RPC: seven supported scopes (Ethereum, Base, Arbitrum, Optimism, Polygon,
  BNB Chain and Solana), independently validated against the expected mainnet.
- Advanced API: exact raw fungible token balances on the six required EVM
  networks; discovered tokens remain unverified. It does not establish indexed
  history, transaction fees, historical cost basis or a common snapshot height.
- The owner's **30 Node requests/second** and **30 Advanced requests/minute**
  limits are enforced separately, with minimum 34 ms / 2010 ms start spacing.
  Gates are shared per credential across chains, retries, rebuilt budgets and
  probes; raw credentials are not retained in the pacing registry.
- Both classes share daily request/estimated-credit and monthly estimated-credit
  ceilings. Defaults: 1000 requests/day, 700000 estimated credits/day and
  180000000 estimated credits/month. Settings can lower these ceilings.
  Estimates are 200 credits/EVM call, 500/Solana call and 700/Advanced call;
  provider-side quotas can still be stricter.
- HTTP/RPC 429 immediately advances to an independent mirror and pauses the
  limited source. Local budget exhaustion is also a neutral pause until the next
  UTC day. Cached balances remain visible and stale when no mirror can refresh
  them; neither condition creates a fatal sync/source error. Invalid credentials,
  malformed responses and mainnet/identity mismatches remain real failures.
- Advanced responses are bounded to 2 MiB, 200 fungible tokens/page and two pages.
  Ankr returned 1106 Ethereum assets despite `pageSize: 200`; only the bounded
  accepted subset is fresh, coverage is explicitly partial, and an opaque cursor
  is not followed past unprocessed entries. Exact raw integer precision is kept.
- Already returned known tokens are not redundantly queried through Node RPC;
  trusted metadata and verification are retained. Missing known tokens use the
  existing bounded, block-pinned Node calls. Changed known decimals are rejected.

[Official sources and selected limits](RESEARCH.json) ·
[Routing and coverage contract](../../PROVIDER_MIRRORS.md).

## Actual API evidence

The live job passed with **17 PASS / 2 RATE_LIMITED / 4 PARTIAL suites**.
Those are distinct outcomes; a quota or restricted scope is not a source PASS.

| Scope                   | Observed result                                                                                                                                                                                                 |
| ----------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Ankr Node               | Ethereum, Base, Arbitrum, Polygon and BNB mainnet/native balances PASS. Optimism and Solana return HTTP 403: PARTIAL, with independently persisted Blockscout / Alchemy mirror balances.                        |
| Ankr Advanced           | Valid exact token-balance responses on all six EVM networks. Ethereum has 200 accepted tokens out of 1106 returned assets: PARTIAL. Base 46, Arbitrum 12, Optimism 37, Polygon 85 and BNB 117 accepted tokens.  |
| Production Ankr routing | Actual BNB Ankr balances persisted through the production synchronization engine: PASS. The intentionally unavailable primary is a zero-budget fixture, not fabricated provider traffic.                        |
| All-network integration | All ten holdings/resync checks PASS, no duplicate transactions on resync, prices and replay checked. Suite PARTIAL because several indexed histories were not refreshed. Actual Base and BNB holdings use Ankr. |
| Independent mirrors     | All ten balance routes PASS. Source-specific Ankr markers require an actual responding provider other than Ankr; generic balance success cannot substitute for restricted-source evidence.                      |
| Zerion / dRPC           | RATE_LIMITED; independently persisted mirror evidence present, no fatal quota diagnostics. Direct checks not completed are not counted as provider passes.                                                      |
| Solana token reserve    | Existing Alchemy SOL/SPL/Token-2022 reserve PASS. No Ankr SPL acceptance is claimed while its Solana Node endpoint is restricted.                                                                               |

The unchanged Ankr live ceiling is **50 physical requests / 50000 estimated
credits**, shared across all suites, retries and integration runs. Actual usage:
**50 requests / 16300 estimated credits**. [Usage](usage.json) ·
[Credit estimates](estimated-credits.json). Suite-specific counters must not be
summed as a replacement for the shared totals.

## Local and native verification

TypeScript, ESLint, Prettier, rustfmt, workspace Clippy and production build PASS.
**229 deterministic Rust / 67 frontend / 8 evidence-policy tests PASS**;
23 opt-out live entrypoints are not real API evidence. Targeted tests cover
separate pacing, exact balances, identity validation, bounded/cyclic pages,
short-key redaction, per-network 403 isolation, retry costs, independent 429
failover, nonfatal budget pauses and removal of duplicate known-token RPC calls.

| Installed/build scope                                           | Actual result                                                                                                                                                                                      |
| --------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Installer inspection / populated upgrade                        | 78 / 12 PASS. Production 0.1.10; previous populated schema 6 migrates to 8.                                                                                                                        |
| Native Windows IPC/recovery                                     | 32 PASS; one installer-only skip is covered separately by the actual upgrade job.                                                                                                                  |
| Installed Windows Server production                             | 12 PASS; first normal useful screen **1499.80 ms**, three offline starts **844.10 / 713.60 / 683.20 ms**. Real installed production binary; synthetic 50-account/500-asset/100000-leg SQL fixture. |
| Windows 11 ARM64, same production x64 installer under emulation | 11 PASS / 1 FAIL; first normal useful screen **4559.80 ms** exceeds the unchanged **2000 ms** gate. Offline starts **1669.60 / 676.50 / 666.70 ms** PASS.                                          |

[Installer inspection](RELEASE_REPORT.json) · [Upgrade](INSTALLER_REPORT.json) ·
[Native IPC](native/NATIVE_REPORT.json) ·
[Production Server load](server-load/NATIVE_LOAD_REPORT.json) ·
[Additional Windows 11 ARM64 load](windows-11-arm/NATIVE_LOAD_REPORT.json).
Browser mock tests are not native acceptance. OS disk/page caches are not flushed;
the first normal start uses a fresh WebView2 folder. The startup gate was not
relaxed, the failed first sample was not omitted, and this source was not rerun
to seek a passing ARM64 sample.

## Preserved corrections and remaining limits

Earlier runs are retained under `attempts/` with their actual quota usage:

| Run / source                                             | Observed failure and subsequent correction                                                                                                                                                          |
| -------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 37647170689 / `fe61bc23a685cd9368723ae9ee71d9953a32df38` | Initial main run cancelled after finding restricted Node scopes and the oversized Ethereum response. Source-level access isolation and bounded partial page handling were corrected.                |
| 37647932046 / `9808e1da8491d52263029740491af8067c0cdc1c` | Branch diagnostics failed the strict 200-asset response assumption; not a successful live acceptance.                                                                                               |
| 37648695950 / `88d8b0d5689a67534beb0893ec9db2146e919659` | Branch diagnostics still failed the page assumption after allowing one native entry; the actual response has 1106 assets.                                                                           |
| 37649935845 / `fb74e0e30ba77bef2ef68b59c18cc40acb0e47dc` | Direct Ankr probes were usable/partial, but full integration failed on duplicate known-token calls and fatal local-budget pauses. Those bugs were corrected; the 50-request ceiling was not raised. |

Optimism/Solana Node 403 requires checking those scopes in the Ankr project and
its access restrictions/allowlist. It does **not** establish that a paid upgrade
is necessary; Advanced Optimism works and independent balance mirrors already
operate. Ethereum's partial page is not full token discovery. Independent indexed
BNB history, the additional ARM64 first-start gate and Windows 11 Intel/AMD
hardware acceptance remain open. Full release acceptance is not claimed.
