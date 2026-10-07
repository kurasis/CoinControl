# Provider reserves — 0.1.9

Reserves are implemented in normal desktop synchronization, rather than only in
connection probes. GitHub Actions uses repository secrets for bounded read-only
verification. Production needs keys in **Settings → Data sources** and stores them
in the OS credential store. No secret is returned to the renderer or baked into a
build. Public reserves activate without a key.

| Network            | Primary / indexed alternative       | Independent balance reserves                                         |
| ------------------ | ----------------------------------- | -------------------------------------------------------------------- |
| Bitcoin            | Blockstream Esplora → mempool.space | Both cover address balance and paginated Bitcoin history             |
| Ethereum, Arbitrum | Alchemy → Zerion                    | Blockscout PRO → Etherscan V2 → dRPC → PublicNode                    |
| Polygon            | Alchemy → Zerion                    | Etherscan V2 → dRPC → PublicNode                                     |
| Optimism           | Alchemy → Zerion                    | Blockscout PRO → dRPC → PublicNode                                   |
| Base               | Alchemy → Zerion                    | dRPC → PublicNode                                                    |
| BNB Chain          | Zerion                              | dRPC → PublicNode                                                    |
| Solana             | Helius → Zerion                     | Alchemy Solana (same key) → Chainstack SOL → PublicNode              |
| TRON               | TronGrid                            | PublicNode confirmed Fullnode API                                    |
| TON                | TonAPI                              | TON Center v3, with or without a key                                 |
| Native prices      | Live Coin Watch                     | Existing DefiLlama identity-based fallback also handles LCW failures |

A configured indexed alternative is attempted before a balance reserve. Reserves
with keys are skipped until configured. Etherscan's free plan excludes Base,
Optimism and BNB; Blockscout PRO excludes BNB, and its free key returned HTTP 402 for Base and Polygon (featured chains requiring a paid plan). Only Ethereum, Arbitrum and Optimism activate with the free adapter.
One provider budget is shared across its chains and clients, including retries.

## Coverage and accounting

Bitcoin's mirror uses the same Esplora normalization, exact satoshi amounts, fees,
pending/reorg handling and independent persisted forward/backfill state. Each
history checkpoint is written under the actual provider, not the configured primary.
Repeated pages remain idempotent by canonical network/hash/component identities.

The other new reserves deliberately serve **balances**. They do not invent a
complete indexed wallet history from ordinary RPC or import TON raw transaction
hashes as TonAPI event IDs. Already stored history, components, fees, cost-basis
links and overrides stay intact. Their successful result is `partial` and
`balance_only`; Wallets shows the actual source, coverage and fallback reasons.
The active source is persisted atomically with the main checkpoint, independently
of timestamp ties, so the UI returns one current status per account.

EVM RPC checks `eth_chainId`, takes a block number, then reads native balance and
up to 20 already known token contracts via `balanceOf` at that block. Etherscan
reads native plus up to 20 known contracts. Blockscout reads native plus at most
four keyset-paginated ERC-20 pages; cursor fields are allowlisted and cannot change
the endpoint, chain, type filter or credential. Polygon's system POL contract is
excluded from the token list so it cannot duplicate native POL.

Solana reserves validate the mainnet genesis hash. **Chainstack Developer is a
native SOL/finality reserve**: its owner scans are paid-only and are never called.
[Official method restriction](https://docs.chainstack.com/reference/solana-gettokenaccountsbyowner).
[Solana mainnet Developer throughput](https://docs.chainstack.com/docs/limits)
is **5 RPS**, despite the pricing page's generic 25 RPS; local reads stay at 1/sec.
SPL/Token-2022 discovery and history primarily use Helius, with Zerion as indexed
alternative. The existing Alchemy key additionally supplies a standard Solana
balance/token-discovery reserve before Chainstack; enable Solana mainnet in the
Alchemy app. [Standard owner scans](https://www.alchemy.com/docs/chains/solana/solana-api-endpoints/get-token-accounts-by-owner)
are documented at 10 CU on the [Free platform](https://www.alchemy.com/pricing).
EVM and Solana share the same request/CU budget; conservatively reserve 100
estimated CU per Solana genesis/balance/owner read (not an actual bill). This
adapter claims balances only, not Solana history/finality. A denied Solana network
does not disable the EVM networks. After Chainstack returns SOL, PublicNode can
supplement token balances.
If this mirror is also limited/unavailable, the observed SOL stays usable and
known tokens stay stale; the active successful source and partial coverage are
persisted. No scan denial is reported as a bad Chainstack key or a demand to upgrade.
PublicNode reads finalized native/SPL/Token-2022 quantities and aggregates mint
accounts; unavailable token discovery remains an explicit sanitized limitation. Unclassified zero-decimal assets are not guessed to be
fungible. A PublicNode method's HTTP 403 does not poison other RPC methods or
networks; dRPC denials are scoped by network and method.

TRON reads confirmed liquid + owned Stake 2.0/delegated/unfreezing TRX and up to
20 known TRC-20 contracts; an absent account response does not fabricate zero.
TON Center reads native and up to 1000 Jetton wallets, accepting exact cached or
indexed decimals only; missing metadata is not replaced by a guessed default.

All balance reserves first mark old observations stale and record only observed
assets as fresh. An omitted asset is never inferred to have a zero balance.
Amounts use integers, including values above JavaScript's safe-integer limit.

## Failure policy and local limits

429, local quota exhaustion, authentication rejection, denied access, unavailable
method/plan entitlement (including HTTP 402 and the dRPC gateway's HTTP 400), timeouts and transient server/transport errors advance to the
next source. Wrong chain identity, malformed data, cancellation and local storage
errors are exposed rather than hidden by another source. A real cancellation
stops all attached clients and does not become a fictitious provider error.

In normal synchronization the **first HTTP 429 immediately advances to the
mirror**, without buffering its error body or first retrying/sleeping on the throttled source. HTTP-200 JSON-RPC
quota errors follow the same route. Standalone clients retain bounded retries.
The console labels throttling separately from request failures; the source cause
remains visible. A successful mirror gives a usable partial result, not a fatal
synchronization error. If all sources are throttled, synchronization is paused without a false fresh
balance or failed-credential label; cached amounts/history remain and balances
are stale. The source tooltip says routing details, not that an unavailable
reserve succeeded. Wrong-chain/malformed/local failures are not masked.

Price-source 429 is also a pause: LCW advances to DefiLlama, quotes that remain
unavailable stay unpriced, and historical downloads remain pending. Price pauses
are persisted separately in SQLite without changing the account history source;
a new process respects the deadline. Credential changes clear these pauses
along with account breakers. Storage and credential errors remain explicit.

Retry-After is respected by a persistent breaker. Otherwise throttling pauses
for 60 seconds, denied/authenticated access for one hour, temporary failures for
30 seconds and an exhausted daily budget until the next UTC day. Credential
changes clear the provider's persisted pause. An explicit Sync now retries connection/server errors immediately after reconnect, but preserves Retry-After, credential/plan rejection and hard budget pauses. Provider-wide errors are shared
across accounts; network restrictions remain isolated to their network.

New reserves have a conservative **1000 requests/day** shared allowance;
mempool.space has 5000/day. Chainstack and dRPC also share a conservative
30000-request/month local ceiling. Blockscout reserves 20 actual credits per
shipped REST call under an 80000-credit/day ceiling. Provider-reported quotas and
other applications using a key can still impose stricter limits. dRPC compute
units are not claimed to equal locally counted requests; RPC calls conservatively reserve
100 estimated units each with an 80000-unit local daily ceiling. Editable local ceilings
can only be lowered, without resetting consumed usage. All public reads pace
at no more than one request per second. Native acceptance retains the separate
50-request ceiling per provider across restarts; live suites share 50/provider.

## Credentials and current official contracts

- `BLOCKSCOUT_API_KEY`: [PRO authorization/routes](https://docs.blockscout.com/devs/pro-api-responses-and-routes), central `api.blockscout.com`, `Authorization: Bearer`, `/{chain_id}/api/v2/...`. The [v12 schema](https://docs.blockscout.com/openapi-specs/pro-api-v12.json) was reviewed ahead of its October 7 rollout; no deprecated instance-key route is used. [Supported-chain config](https://github.com/blockscout/backend-configs/blob/main/pro-api/prod/chains-config.json).
- `DRPC_API_KEY`: [first request](https://drpc.org/docs/gettingstarted/firstrequest), current `https://lb.drpc.live/{network}/{key}`. Shared free public-node access for six EVM chains, not guaranteed indexed history or premium nodes. Solana is excluded: its [current mainnet metadata](https://drpc.org/chainlist/solana) has no free nodes.
- `CHAINSTACK_API_KEY`: **Solana node auth token or complete HTTPS endpoint** from the node's Access and credentials. [Node and platform authentication differ](https://docs.chainstack.com/docs/authentication-methods-for-different-scenarios). A platform management key cannot authenticate RPC. Full endpoints must use a Chainstack or p2pify HTTPS host without URL userinfo; genesis validation rejects another chain/testnet. Free native SOL/finality only; owner scans are paid-only. One free node does not authorize every protocol with the same token.
- `TONCENTER_API_KEY`: [TON Center v3](https://docs.ton.org/api/v3/overview), `X-API-Key`, `accountStates` and `jetton/wallets`; works anonymously at the lower rate.
- `ETHERSCAN_API_KEY`: [V2 selected free chains](https://docs.etherscan.io/supported-chains), one key, `chainid`, native/account token balance actions. Semantic HTTP-200 throttling and credential errors are sanitized and classified.
- [PublicNode](https://www.publicnode.com/) and [mempool.space](https://mempool.space/docs/api/rest): no keys, fair use and no guaranteed quota.

Docs and schemas reviewed October 6–7, 2026. This is implementation scope, not a
claim that every credential or provider passed live verification. Exact results
are recorded with the CI source and published build evidence.

## Live verification policy

`RATE_LIMITED` identifies a source quota, not a failed application or a source
PASS. Remaining direct source checks are not run or claimed. `PARTIAL` means
real mirror balances succeeded without establishing indexed history acceptance.
Integration engines now attach the configured production primary/reserve types.
The routing test forces a zero primary budget without wasting a real primary
request, then checks actual independent responses and persisted sources for the
six EVM chains, SOL, TRON, and selected TON/Bitcoin mirrors. Controlled tests
separately force HTTP/RPC 429 and verify switching, pauses and preserved data.

A source marked RATE_LIMITED requires successful independent routing evidence
for its affected networks; otherwise the live job still fails. Auth, malformed
responses, assertions and local storage errors remain failures. The ordinary
50-request/provider cap includes the whole run and retries. Price routing uses
an isolated seeded BTC balance and a real quote; that seeded balance is not
provider holdings/history or native UI evidence.

## Are more free sources needed?

Existing sources cover independent native/known-token balances for all ten
networks; no extra key is required to activate the current anonymous mirrors.
The local anonymous check also observes PublicNode owner scans returning 403.
Alchemy Solana therefore fills a concrete SPL reserve gap when enabled on the key.
**Indexed BNB history remains the main independent-source gap** when Zerion is
limited. A plain RPC balance response does not close it.

[Ankr Freemium](https://www.ankr.com/docs/rpc-service/service-plans/) is a useful
optional next adapter: documentation offers 200M free API credits/month,
Advanced API 50 requests/minute (500/10 min) and supports BNB Smart Chain in its
[Advanced API](https://www.ankr.com/docs/advanced-api/overview/), including
`ankr_getTransactionsByAddress`. Solana Node API could additionally reserve
standard token reads if Alchemy cannot provide sufficient redundancy. It needs a new private endpoint token and its own adapter,
normalization and bounded live verification; it is **not connected or verified**
by this change. No deposits, subscriptions or paid overages are enabled.

Solana Foundation's [public mainnet endpoint](https://solana.com/docs/references/clusters)
is explicitly not intended for production apps and can return 403/429. It is not
added as a default reliable mirror. dRPC's current free Solana-node limitation
also remains. New providers should fill a demonstrated coverage gap rather than
duplicate existing low-limit public endpoints.
