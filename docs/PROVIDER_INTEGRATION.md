# Helius and Alchemy integration — 0.1.4

These adapters are implemented in the Rust provider crate and used by normal
desktop synchronization. GitHub Actions credentials only authorize CI tests;
an installed application needs its own keys in **Settings → Data sources**.
Production reads those keys from the OS credential store, never environment
variables or the renderer. Debug builds can use the named development variables.

## Source selection

| Network                                     | With the new key configured          | Without it |
| ------------------------------------------- | ------------------------------------ | ---------- |
| Solana                                      | Helius (`HELIUS_API_KEY`)            | Zerion     |
| Ethereum, Base, Arbitrum, Optimism, Polygon | Alchemy (`ALCHEMY_API_KEY`)          | Zerion     |
| BNB Chain                                   | Zerion                               | Zerion     |
| Bitcoin / TRON / TON                        | Existing Esplora / TronGrid / TonAPI | Same       |

Alchemy's key must have access to each selected mainnet. Saving a key activates
the source; a rejected key, method entitlement failure or exhausted budget is
reported and keeps cached data. As of 0.1.6, [explicit reserves](PROVIDER_MIRRORS.md) keep balances usable and preserve history with truthful partial coverage after such failures.
Removing the key returns that network to the original route.

Settings → Networks changes with configuration and explains each source's
coverage. New adapters do not inherit Zerion's historical live verification date.
No key readback or generic RPC command is exposed to the renderer. Chainstack accepts its own restricted HTTPS node endpoint as a stored secret in 0.1.6.

## Holdings and history

**Helius** uses `getBalance` plus parsed `getTokenAccountsByOwner` for the original
SPL and Token-2022 programs. NFTs and unclassified zero-decimal mints are
excluded from fungible holdings/history; omitted cached holdings remain stale,
so this source advertises partial balance/discovery coverage. Multiple token accounts of the same case-sensitive
mint aggregate with exact integers. Zero and missing data remain distinct.
Finalized `getTransactionsForAddress` reads 20 full `jsonParsed` transactions per
page with `filters.tokenAccounts=all`, all statuses and keyset pagination. It
includes closed token accounts where indexed. The current account's fee is
separated from its native balance delta; failed transactions import only fees.
Only an evidenced plain System Program transfer receives an automatic native
counterparty. Program/rent effects, wrapped SOL and token deltas remain unresolved
for review. Token-2022 extensions are not fully classified. History coverage
remains partial even when all indexed pages have been imported. An unavailable
history method is an error, never an empty history or an automatic paid upgrade.

**Alchemy** checks `eth_chainId`, reads native balance and paginated ERC-20 token
balances, and uses exact contract metadata. Discovery caps at eight pages and 50
nonzero tokens per account per sweep. A capped scan refreshes observed holdings
and leaves omitted cached holdings stale; it never turns them into zero. Stored
metadata is reused across processes, and lookups are memoized within a sweep.
Polygon's native pseudo-contract `0x…1010` is not an additional ERC-20 holding.

Incoming and outgoing `alchemy_getAssetTransfers` discovery have separate
checkpoints. Each normalized page handles at most five indexed events. Whole
transaction `eth_getTransactionByHash` plus `eth_getTransactionReceipt` supplies
the native movement, all relevant ERC-20 receipt logs and one sender fee. ERC-721
Transfer logs and self transfers are excluded from fungible movements. Base and
Optimism sender fees include the separately reported L1 fee; missing fee evidence
fails instead of inventing zero. Transfers do not discover failed calls,
approval-only calls or all internal movements: the account remains partial.
Direct transaction lookup can normalize a known failed call, but does not prove
that the transfer index discovered all failures.

Opaque Alchemy page keys are used only within a request sequence. Persisted
continuations contain a block and exact event boundary and are reconstructed on
later runs. A boundary lookup reads at most eight upstream pages (up to 1000
events per upstream response). If a boundary disappears or exceeds that limit,
the adapter reports the gap and preserves the checkpoint. A receipt crossing a
normalized event page boundary is not imported twice. The normal eight-page
history allowance is shared between both directions (at least one per direction).

Both sources use canonical chain transaction IDs. Partial replacements preserve
an account's transactions previously imported from another provider, including
their legs, fees and review links. Repeated runs are idempotent. This deliberately
does not reinterpret existing transactions with a less complete source.

## Limits and cancellation

| Source  | Persisted daily local cap       | Per-attempt cost estimate                                                                                    |
| ------- | ------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| Helius  | 5000 requests and 20000 credits | Standard reads: 1; a ≤20-transaction history page: 10                                                        |
| Alchemy | 5000 requests and 150000 CU     | Transfers: 120; token balances: 20; metadata: 10; other shipped RPC reads reserve a conservative 500 CU each |

All networks share one credential budget per provider; retries count again.
Desktop connection probes and sweeps share the same run lock and persisted budget.
Locally estimated credit/CU totals persist in the existing `provider_usage`
column and are displayed alongside request counts. They are not the provider's
remaining allocation and do not include another application's use of the key.
RPC errors inside HTTP 200 are classified without echoing messages. Helius
minimum-context-slot lag / unhealthy-node responses receive at most two bounded
retries using the transport backoff, counting each attempt. Authentication,
rate limiting and exhausted local budgets stop further requests for that provider
in the run. Alchemy HTTP 403 instead stops that mainnet endpoint for the run;
other enabled mainnets keep working, and the inaccessible network remains failed. HTTP error bodies from these URL-key providers are not echoed either.
Cancellation prevents the next RPC; already started requests can finish before
checkpointed cancellation. Neither source signs or broadcasts transactions.

## Verification

Controlled fixtures exercise exact amounts above IEEE-754's integer range,
fee-only failures, sponsor fees, closed token accounts, unresolved rent/program
effects, duplicate logs, receipt fee allocation including L1, preserved foreign
evidence, portable pagination, capped scans, cancellation, wrong-chain detection,
short-key redaction and weighted retry budgets. The UI regression caches network
coverage, saves/removes a preview key and verifies routing invalidation.

`npm run test:live` now accepts `helius,alchemy`. CI reads `HELIUS_API_KEY` and
`ALCHEMY_API_KEY` only through Actions Secrets. Each live run shares a maximum
50 requests/provider across suites, plus 5000 Helius credits / 10000 estimated
Alchemy CU. A provider-only workflow dispatch can select these two adapters
without repeating unrelated Windows jobs. Missing keys do not count as a pass.

Current executed evidence and external access restrictions are recorded in
[TEST_REPORT.md](../TEST_REPORT.md). Physical Windows 11 acceptance remains a
separate release requirement.

## Official contracts reviewed on 2026-10-06

- [Helius full transactions, filters and keyset pagination](https://www.helius.dev/docs/api-reference/rpc/http/gettransactionsforaddress)
- [Helius parsed token accounts](https://www.helius.dev/docs/api-reference/rpc/http/gettokenaccountsbyowner)
- [Helius per-method credits and entitlement distinctions](https://www.helius.dev/docs/billing/credits)
- [Alchemy transfer index and 120-CU cost](https://www.alchemy.com/docs/data/transfers-api/transfers-endpoints/alchemy-get-asset-transfers)
- [Alchemy token balances, ERC-20 discovery and 20-CU cost](https://www.alchemy.com/docs/data/token-api/token-api-endpoints/alchemy-get-token-balances)
- [Alchemy contract metadata and 10-CU cost](https://www.alchemy.com/docs/data/token-api/token-api-endpoints/alchemy-get-token-metadata)
