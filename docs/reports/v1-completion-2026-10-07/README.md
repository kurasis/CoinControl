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
| Windows delivery                                             | Version 0.1.7; separate native automation adds real-IPC management/settings checks. Source, native, installer and publication evidence will be recorded after CI.                                                                                                                                                                                                                                                       |

The initial browser regression found a Russian wallet row overflowing at 672×440;
its [failure report](INITIAL_BROWSER_LAYOUT_REPORT.json) is retained. Rows now wrap
with the new controls. This browser matrix uses mock IPC and is separate from native
Windows acceptance.

## Local verification

Baseline: static checks, build and offline tests passed with 178 deterministic Rust
and 52 frontend cases. Final: **195 deterministic Rust / 56 frontend cases PASS**;
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
Keep these failures visible; do not repeat unchanged authentication tests.

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
3. **dRPC BNB:** prior 429 requires available key quota/free BNB service or its reset;
   local counts are not the provider's global allowance. Other reserves retain
   partial balances and cached history. Record the new run's result separately.
4. **Windows 11 x64:** hosted Windows Server automation does not establish the
   original Windows 11 installation/UI acceptance. A Windows 11 machine is needed
   for that gate. The unsigned ZIP remains a prerelease while external gates remain.

GitHub Actions secrets are CI-only; production builds never embed or read them.
Never send actual keys in chat. macOS/iOS are migration targets, not tested V1 platforms.
