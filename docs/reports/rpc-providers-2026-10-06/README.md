# Initial Helius / Alchemy live evidence — 0.1.4

Latest corrected source and all completed CI results: [final evidence](final/README.md). This directory preserves the initial integration source, including its startup failure.

Exact source [9f782af5aee62a644735c062e0280b7345716afa](https://github.com/kurasis/CoinControl/commit/9f782af5aee62a644735c062e0280b7345716afa),
[main CI 37439865917](https://github.com/kurasis/CoinControl/actions/runs/37439865917),
[merged PR #11](https://github.com/kurasis/CoinControl/pull/11).
Credential presence was confirmed through actual Actions reads, without key
values, authenticated URLs or arbitrary response bodies in these reports.

| Scope                       | Result                     | Executed evidence                                                                                                              |
| --------------------------- | -------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| Helius Solana               | PASS                       | 814 supported holdings identities; two pages of 20 full related-account transactions, no duplicate signatures                  |
| Alchemy Ethereum            | PASS                       | Correct mainnet, native balance, USDC contract/decimals, two indexed pages, independent known 79 ETH principal and receipt fee |
| Alchemy Base                | FAIL / access gate BLOCKED | Network endpoint returns 403                                                                                                   |
| Alchemy Arbitrum            | FAIL / access gate BLOCKED | Network endpoint returns 403                                                                                                   |
| Alchemy Optimism            | FAIL / access gate BLOCKED | Network endpoint returns 403                                                                                                   |
| Alchemy Polygon             | FAIL / access gate BLOCKED | Network endpoint returns 403                                                                                                   |
| Zerion-dependent suites     | FAIL / live gate BLOCKED   | HTTP 429; shared budget stops at three actual Zerion requests                                                                  |
| Other independent providers | PASS                       | Esplora, Live Coin Watch, DefiLlama, TronGrid, TonAPI                                                                          |

There are **six passing / four failing suites** in the complete live job; the
Alchemy suite fails because its four additional mainnets cannot be verified.
Its passing Ethereum checks do not establish access to those networks. Live
history is bounded rather than a complete public-wallet import. Solana holdings
exclude NFTs/unclassified zero-decimal mints; program/rent effects stay partial.
The Helius source's partial scope is exposed in Settings → Networks.

Actual totals across all suites are in [usage.json](usage.json): Alchemy 12,
Helius 5, Zerion 3, Esplora 13, Live Coin Watch 5, DefiLlama 10, TronGrid 33 and
TonAPI 11 requests. All include retries and stay within the original 50/provider
ceiling. [Estimated credits](estimated-credits.json): Helius 23 / 5000 and
Alchemy 4270 / 10000 CU. These are local method-based reservations, not the
provider's remaining quota. See [full report](LIVE_REPORT.md),
[Helius](helius.json), [Alchemy](alchemy.json) and [provenance](PROVENANCE.json).

Earlier exact-source runs are identified in provenance. The initial Helius run
passed. A later run exposed RPC -32016 (minimum context slot not reached) during
a replica transition; the final adapter now uses at most two bounded retries.
A controlled regression actually exercises the retry and its charged cost;
the final live run used five requests and did not encounter that transient.
Ethereum receipt/USDC checks passed before the external access failures.

The required next external action is to grant the Alchemy key access to Base,
Arbitrum, Optimism and Polygon mainnet, or supply an appropriate per-network
configuration if the account requires separate apps. No paid plan, overage or
key change was performed automatically. Zerion quota and physical Windows 11
verification remain separate release gates. Full release acceptance is not
declared verified.

Implementation, source routing, persistence and accounting limits:
[PROVIDER_INTEGRATION.md](../../PROVIDER_INTEGRATION.md).
Current complete CI / installer / native status: [TEST_REPORT.md](../../../TEST_REPORT.md).
