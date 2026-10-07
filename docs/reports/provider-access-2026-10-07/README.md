# Provider access revalidation — 2026-10-07

The owner updated Alchemy and Chainstack credentials after publishing 0.1.7.
Application source and published assets remain unchanged. Revalidation runs on
main `bc449edb7a2397ed0f2d0defa965b5ed74ea7117`; differences from the tested
application source `a3512b0` are documentation/evidence only.

## Targeted real access check

[CI 37606760853](https://github.com/kurasis/CoinControl/actions/runs/37606760853),
`providers_only=true`, `live_providers=alchemy,chainstack`: **PASS**.
Other build/native jobs were explicitly skipped for this API-only verification,
not rerun or counted as new passes. [Exact API report](targeted/LIVE_REPORT.md).

- Alchemy: Ethereum, Base, Arbitrum, Optimism and Polygon correct chain IDs,
  balances/history access PASS; historical known ETH principal/fee, USDC raw
  balance and pagination remain checked. 20 requests; estimated 6750 CU.
- Chainstack: Solana mainnet native SOL access/precision PASS; 3 requests.
  **SPL discovery remains partial**: `getTokenAccountsByOwner` returns HTTP 403.
  The test records this limitation and accepts the documented native-balance
  reserve contract; its PASS does not mean all SPL methods or history work.
- Per-provider ceiling: 50 including retries for this run. Follow-up full-suite
  requests are additional, not included in the targeted count.

## Confirmed remaining tasks

| Priority                         | Task                                                                                     | Evidence / next step                                                                                                                                                                                                                                                                                                                                                                                             |
| -------------------------------- | ---------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| P2, required product requirement | Cache sanitized raster token icons with size/content-type limits and a fallback monogram | [SPECIFICATION §10](../../spec/SPECIFICATION.md#10-local-security-privacy-and-resilience) requests this. [TokenIcon.tsx](../../../src/components/TokenIcon.tsx) currently renders only letters; table/detail call it with a symbol, and no image-fetch/cache implementation exists. Add bounded safe caching and tests before claiming full specification compliance. Existing monograms remain a safe fallback. |
| P1, required acceptance          | Windows 11 x64 clean installation and real configuration/use                             | Existing upgrade/native/load evidence is hosted Windows Server. On Windows 11 enter keys in Settings, add public addresses, sync/restart, verify UI/scaling and export/restore. Server checks remain valid for their reported environment.                                                                                                                                                                       |
| P2, source limitation            | Chainstack SPL RPC method access                                                         | SOL works, but `getTokenAccountsByOwner` is forbidden. Check the node endpoint's method/plan permissions if available on the free tier. Otherwise retain Chainstack as the partial SOL reserve and use Helius/Zerion for SPL tokens. Do not purchase services or hide partial coverage.                                                                                                                          |

The earlier 403 for Alchemy's four networks and Chainstack native 401 are resolved
by this exact targeted evidence. Historical CI failures and the immutable unsigned
0.1.7 prerelease remain unchanged. Updated Actions secrets are for CI; enter the
keys separately in the installed application's Settings → Data sources.
The full provider revalidation outcome follows.

## Full provider revalidation

[CI 37607032943](https://github.com/kurasis/CoinControl/actions/runs/37607032943),
`providers_only=true`, configured default providers: **18 suites PASS / 0 FAIL**
([exact report](full/LIVE_REPORT.md), [job metadata](full/CI_RUN.json)).
This includes native/token normalization, price sources, required-network matrix,
vertical slice, mirror routing and Ethereum/Solana/TON finality checks. dRPC BNB
also passes in this run; earlier intermittent 429 remains historical evidence.
Chainstack's SPL-method 403 is still recorded as partial reserve coverage rather
than hidden or presented as full token/history access. Required Solana token
coverage is supplied by Helius/Zerion, whose relevant checks pass.

The full API suite uses at most 50 requests/provider, retries included
([actual usage](full/usage.json)). These requests are additional to the targeted
20 Alchemy / 3 Chainstack requests. No unchanged endpoint was retried just to hide
an error; the configuration changed and the targeted successes were followed by
one complete API matrix. The published installer remains the previously verified
payload; no new native/build pass is claimed for this API-only run.

Both credential/native-access blockers identified at publication are resolved.
The icon-cache implementation and physical Windows 11 acceptance above remain
open, so full original-specification acceptance is still pending. Signing the
installer and Apple targets are optional follow-up work, not newly required gates.
