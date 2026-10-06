# Verification report — 2026-10-06

Development version **0.1.4** adds Helius and Alchemy desktop synchronization and is delivered through [PR #11](https://github.com/kurasis/CoinControl/pull/11). Exact initial source: [9f782af5aee62a644735c062e0280b7345716afa](https://github.com/kurasis/CoinControl/commit/9f782af5aee62a644735c062e0280b7345716afa), [CI 37439865917](https://github.com/kurasis/CoinControl/actions/runs/37439865917): **five jobs passed; live API and installed production startup jobs failed**. This is a development build, not a verified release. Provider credentials were confirmed by actual authenticated Actions tests without disclosing their values.

## Executed verification on the initial integration source

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

The corrective implementation groups identical remaining lot values in SQLite and multiplies their counts using Rust arbitrary-precision decimals. Summary/holdings no longer reconstruct every FIFO identity and acquisition timestamp. SQL floating-point SUM is never used, and detailed lot/audit views retain the individual records. The regression covers duplicate lots, amounts above 2^53, fine decimal basis, unknown/estimated/zero basis, missing/zero prices, mismatched observations and account scope. The unchanged Windows startup gate must be rerun on this correction before claiming success.

## Evidence history and release gates

0.1.3 tested source [8830f1827a002f52a47eba14713d4fdfc0edbc64](https://github.com/kurasis/CoinControl/commit/8830f1827a002f52a47eba14713d4fdfc0edbc64), CI 37430180430, had six passing jobs and a Zerion-dependent live failure. Its original 1815.1 ms normal startup and three passing offline launches remain valid historical evidence. Parallel native-window/cache priming and lazy chart canvas rendering remain in the implementation.

Earlier source-specific evidence: [CI recovery](docs/reports/ci-recovery-2026-10-06/README.md), [responsive UI](docs/reports/responsive-ui-2026-10-05/README.md), [native upgrade](docs/reports/upgrade-and-native-2026-10-05/README.md), [release follow-up](docs/reports/release-acceptance-2026-10-05/README.md), [imported report](docs/reports/IMPORTED_TEST_REPORT.md).

Full release acceptance requires the current production startup gate, Alchemy network access, Zerion quota and physical Windows 11 verification. No release tag, physical Windows 11 pass or complete blockchain coverage is claimed.
