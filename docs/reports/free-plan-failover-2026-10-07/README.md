# Free-plan sources and quota failover — 2026-10-07

Chainstack Developer does not support `getTokenAccountsByOwner`. Its Solana
mainnet limit is 5 RPS. [Official sources](RESEARCH.json) distinguish that
restriction from an invalid credential and the generic pricing limit.

## Changes

- Chainstack no longer sends paid owner scans. It retains observed native SOL
  while independent reserves try token discovery. Existing cached tokens stay
  stale when discovery is unavailable.
- The existing Alchemy credential adds standard Solana RPC balance/SPL reads,
  sharing EVM request/credit budgets. Enable Solana mainnet on that application;
  the adapter does not claim Solana history or finality coverage.
- The first sync HTTP/RPC 429 advances to a mirror without retrying or waiting.
  Retry-After still pauses the source. If all sources are limited, coverage is
  paused and cached amounts remain visible, stale, without a fatal quota error.
- Native price quotes also use the available reserve. Price-source pauses
  survive restarts; historical price throttling stays pending. Authentication,
  wrong-chain, malformed responses and storage failures remain real errors.
- Console limits have a neutral status; sync quotas do not populate the fatal
  source-usage error shown in Settings. Actual node-health errors still do.
  Live results distinguish RATE_LIMITED
  and PARTIAL from source PASS. Missing successful independent mirror evidence
  still fails the live gate.

## Local verification

Baseline at `0ca4887b8c49387575244c7415ad9a31066d3007`: checks, build,
201 deterministic Rust / 66 frontend tests PASS. Final working tree:
209 deterministic Rust / 67 frontend / 6 Node report-policy tests PASS;
20 disabled live entrypoints are **not live evidence**. Type/lint/format,
Clippy including all features, build and unchanged generated bindings PASS.
[Cases and scope](LOCAL_VALIDATION.json).

The earlier candidate's actual anonymous test made 49 requests to PublicNode
(50-request ceiling), using real native responses on six EVM chains, SOL and
TRON. PublicNode owner scans returned 403; this does **not** establish SPL
discovery. [Sanitized results](local-anonymous/LIVE_REPORT.md).

## Exact-source live verification

Source `99123fce346d09a31e037d84306bdef205351359`,
[main CI 37636073946](https://github.com/kurasis/CoinControl/actions/runs/37636073946):
live gate PASS. [Actual reports](main-live/LIVE_REPORT.md): 17 PASS,
1 RATE_LIMITED (Zerion), 2 PARTIAL (network and vertical-slice history).
Alchemy Solana native/SPL/Token-2022 reads and independent routing on all ten
networks PASS. No new secret or paid plan was needed. Source quotas are not
claimed as source passes; partial indexed history is not full acceptance.
The final run uses the shared 50-request/provider ceiling including retries.

The preceding source `28bb3a10736159f44d83623b4e1a2f25df350750` passed live,
offline/browser, installer, native recovery and both cached-load jobs. Its
production first-start checks failed: Windows Server 2308 ms and Windows 11
ARM64 x64 emulation 2915.5 ms, both above 2000 ms. That candidate was **not
published**. [Preserved evidence](quota-state-main/server-load/NATIVE_LOAD_REPORT.json)
and [all attempts including cancelled-run usage](CI_ATTEMPTS.json).
The final source changes quota diagnostics; it does not claim a startup fix or
retry the unchanged failed source.

## Final installed application

[Exact CI status](CI_STATUS.json): seven jobs PASS, overall FAIL because of the
additional Windows 11 ARM64 first-start gate. All six required application/build
gates and the separate actual live gate PASS.

| Scope                                    | Result                                                                                             |
| ---------------------------------------- | -------------------------------------------------------------------------------------------------- |
| Offline/type/lint/format/build/bindings  | PASS; 209 deterministic Rust / 67 frontend / 6 Node tests                                          |
| Browser mock IPC                         | PASS; 24 layout combinations and bounded 10000-row table                                           |
| Native Windows IPC                       | 32 PASS, one installer-only case skipped and independently covered                                 |
| Installer inspection / populated upgrade | 78 / 12 PASS                                                                                       |
| Installed Windows Server production      | 12 PASS; first normal useful screen 1894.80 ms; offline 534.50 / 511.50 / 531.60 ms                |
| Windows 11 ARM64, x64 emulation          | 11 PASS / 1 FAIL; first normal useful screen 2992.70 ms; offline 1609.80 / 669.60 / 677.20 ms PASS |

The unchanged 2000 ms gate includes the first normal launch. The preceding
Server failure remains preserved; the final Server pass is a measurement of
this exact payload, not a claim that startup variation is fixed. Windows 11
Intel/AMD hardware remains unrun. [Server evidence](server-load/NATIVE_LOAD_REPORT.json)
· [Windows 11 evidence](windows11-load/NATIVE_LOAD_REPORT.json).
The inspected production payload is eligible for an unsigned **prerelease**;
publication and redownload verification are recorded separately.

## Remaining constraints

Alchemy Solana depends on enabled mainnet access. Indexed BNB history still
needs an independent adapter when Zerion is limited. Ankr Freemium Advanced API
is a documented candidate, not connected or verified; no new key is needed for
the mirrors activated here. [Routing, free-plan restrictions and alternatives](../../PROVIDER_MIRRORS.md).

The existing Windows 11 ARM64 first-start gate and Windows 11 Intel/AMD hardware
acceptance are separate from API failover. The 2000 ms gate is unchanged.
Older published assets and their evidence are preserved.
