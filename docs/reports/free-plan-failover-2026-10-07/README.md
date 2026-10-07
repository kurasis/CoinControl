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
- Console limits have a neutral status. Live results distinguish RATE_LIMITED
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
discovery. [Sanitized results](local-anonymous/LIVE_REPORT.md). Credentialed
final-source CI and publication results will be recorded after execution.

## Remaining constraints

Alchemy Solana depends on enabled mainnet access. Indexed BNB history still
needs an independent adapter when Zerion is limited. Ankr Freemium Advanced API
is a documented candidate, not connected or verified; no new key is needed for
the mirrors activated here. [Routing, free-plan restrictions and alternatives](../../PROVIDER_MIRRORS.md).

The existing Windows 11 ARM64 first-start gate and Windows 11 Intel/AMD hardware
acceptance are separate from API failover. The 2000 ms gate is unchanged.
Older published assets and their evidence are preserved.
