# Verification report — 2026-10-06

Development version **0.1.4** is delivered on main through [provider PR #11](https://github.com/kurasis/CoinControl/pull/11) and [startup correction PR #12](https://github.com/kurasis/CoinControl/pull/12). Exact final tested code: [54e82df306aa19c1408a9fbc6f66761db27222c5](https://github.com/kurasis/CoinControl/commit/54e82df306aa19c1408a9fbc6f66761db27222c5), [CI 37442084560](https://github.com/kurasis/CoinControl/actions/runs/37442084560): **six jobs passed; only the live API job failed because four Alchemy mainnets returned 403 and Zerion returned 429**. Credential availability is established through authenticated reads. This is a development build; full release acceptance remains blocked by those API gates and physical Windows 11 verification. Evidence follow-up commits change documentation only.

## Published Windows prerelease ZIP

[Windows x64 ZIP, v0.1.4](https://github.com/kurasis/CoinControl/releases/download/v0.1.4/CoinControl-0.1.4-windows-x64.zip) is published as a prerelease, not full release acceptance. [Packaging/publishing workflow 37445549616](https://github.com/kurasis/CoinControl/actions/runs/37445549616) passed on packaging source bea3d6c. It reused the exact production installer from CI 37442084560 after matching release/upgrade/startup reports and installer/application payload hashes. Application code remains the tested 54e82df source; subsequent changes add reports and packaging automation.

The published ZIP was downloaded again: CRC, exact four-file contents, inner checksums and external SHA-256 passed. ZIP SHA-256: `12f2ae2273b6d544dc94adef2fd44e2cd61b25d66b7b91fb8c8dc343b9018af4` (8016340 bytes). It contains the unsigned setup EXE, README, build information and file checksums. A tampered installer was rejected in a local negative control. The release gate limitations below remain in force.

## Final verification

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

The current production startup gate passed. Full release acceptance still requires Alchemy network access, Zerion quota and physical Windows 11 verification. The v0.1.4 tag publishes a prerelease ZIP; no full release pass, physical Windows 11 pass or complete blockchain coverage is claimed.
