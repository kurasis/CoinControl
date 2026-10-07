# API Selection, Contracts, and Free-Tier Budgets

Public documentation review: **2026-10-04**. Prices, quotas, entitlements, and chain coverage can change. These are documented observations, not authenticated tests of the user's account. `SOURCES.md` contains the official reference URLs.

## 1. Selected configuration

Use **Live Coin Watch** for market prices/history, **Zerion** for supported EVM/Solana account data, **Blockstream Esplora** for BTC, **TronGrid** for TRON, and **TonAPI** for TON. Use **DefiLlama** for additional contract-address price coverage. Helius and Alchemy are supplemental sources when needed; Ankr provides an optional authenticated balance/token-discovery reserve; CoinLore is a documented optional alternative.

Do not require the user to register with every provider before the first useful screen. Ask for keys only when enabling features that require them. One provider credential can query many public addresses within that account's shared quota.

| Provider | Selected role | Published recurring free allowance | Authentication | Important limitation |
|---|---|---|---|---|
| Live Coin Watch | Primary quotes, 24h changes, market history | 10,000 requests/day | Free account; `x-api-key` | Asset mapping/coverage and per-asset history vary |
| Zerion Developer | EVM/Solana balances and activity | 2,000 requests/day; 3 requests/sec | Free key via HTTP Basic Auth | DeFi positions, portfolio charts, and P&L have an additional 25% quota restriction |
| DefiLlama Free | Contract-address current/historical prices | Public free access; no numerical service guarantee used here | None | Not an address-transaction indexer; missing/low-quality quotes possible |
| Blockstream Esplora public API | BTC address balances/history | Public free service; no guaranteed quota/SLA assumed | None | Single-address scope; implement paging and fair-use throttling |
| TronGrid | TRX/TRC-20 account data | Use actual free account quota from console | `TRON-PRO-API-KEY` | Current docs say limits depend on plan/network/endpoint; no fixed 100k/day promise |
| TonAPI | TON/Jetton account data | Documented default free rate 1 request/sec | Free Bearer token recommended | Anonymous access is more restricted; high-limit Tonkeeper proxy is not a desktop entitlement |
| Helius Free | Supplemental Solana data/decoding | 1M credits/month; 10 RPC requests/sec; lower endpoint-class limits | API key in URL query | Credits are weighted; some methods are paid-only |
| Alchemy Free | Optional EVM receipts/transfers/reconciliation | 30M CU/month; 25 requests/sec marketing limit, 500 CU/sec throughput listed | Project API key in endpoint URL | CU cost and transfer/internal coverage vary by method/network |
| Ankr Freemium | Optional alternative indexed EVM source | 200M credits/month; configured tariff: Node API 30/sec, Advanced API 30/min | Authenticated personal token | Different credits from other providers; indexed chain coverage is smaller than RPC coverage |
| CoinLore | Optional mapped market fallback | Public free access; recommends about 1 request/sec | None | Market data only; do not map by symbol alone |

Sources: P1, Z1–Z3, D3, B1, R1–R2, N1–N2, H1–H4, A1–A2, A3, P3 in SOURCES.md. Request counts, credits, CUs, and per-second throughput are different units and must never be compared as if interchangeable.

Zerion's restricted endpoints should share a conservative **combined** sub-budget until the account's exact enforcement is verified. Current coverage lists Solana tokens and transactions, but Solana DeFi is marked coming soon. Use the API coverage list, not the broader consumer wallet's network list.

## 2. Credentials and setup

| Config name | Obtain credential | How it is used |
|---|---|---|
| `LIVECOINWATCH_API_KEY` | <https://www.livecoinwatch.com/tools/api> | Header `x-api-key` |
| `ZERION_API_KEY` | <https://dashboard.zerion.io/> | Basic authorization for `key:` — username is key, empty password |
| `HELIUS_API_KEY` | <https://dashboard.helius.dev/> | `api-key` query parameter; redact entire sensitive URL values |
| `ALCHEMY_API_KEY` | <https://dashboard.alchemy.com/> | Key in a fixed, validated network endpoint |
| `ANKR_API_KEY` | <https://www.ankr.com/rpc/> | Personal authenticated endpoint/token; use current documentation |
| `TRONGRID_API_KEY` | <https://www.trongrid.io/> | Header `TRON-PRO-API-KEY` |
| `TONAPI_API_KEY` | <https://tonconsole.com/> | `Authorization: Bearer ...` |

DefiLlama, public Esplora, and CoinLore require no key for the chosen routes. TonAPI can work anonymously at a stricter limit; use a free key for normal operation. TronGrid anonymous operation must not be the production design.

Create separate development credentials where the provider permits. Live Coin Watch documents one active key per profile: do not promise a second key on the same profile or regenerate the working key automatically. Free credentials are service credentials, not wallet ownership credentials.

The `.env.example` is only a development/test template. Production credentials are entered through Settings and stored with the OS credential adapter. Never embed any developer's key in a shipped binary. No frontend direct provider calls and no VITE-prefixed secrets.

## 3. Minimal provider contracts

### 3.1 Live Coin Watch

Base: `https://api.livecoinwatch.com`. Read endpoints use POST with JSON. Use `/coins/map` for a custom set of mapped assets, `/coins/single/history` for selected-asset charts, and `/credits` sparingly for account quota information. The map endpoint's documented result limit is 100; chunk and verify returned coverage. `delta.day` is a ratio, so normalize as `(delta.day - 1) * 100`.

Retain LCW's provider coin code independently from the human symbol. Request USD explicitly, validate millisecond timestamps where the endpoint uses them, and do not invent intraday history where only coarse samples exist. Source: P2.

### 3.2 Zerion

Base: `https://api.zerion.io/v1`. Use documented wallet positions and transactions endpoints with explicit scope/filtering. Discover actual routes, filter names, page limits, and schemas from the current reference/OpenAPI; older documentation URLs may redirect. Sources: Z3–Z5.

Use simple wallet positions for v1 holdings, and request the complete intended transaction categories. Do not let a default filter hide failed calls or relevant historical activity unnoticed. Follow documented pagination to exhaustion within the budget; validate that each continuation advances. A new address being indexed is loading, not empty.

The current network capability page is authoritative for whether an adapter can be enabled. EVM address identity is reused across networks, but account IDs remain chain-specific. Provider multi-chain results must be partitioned correctly and must not leak unselected networks into a wallet total.

Wallet charts and provider P&L are optional diagnostics/enrichment. They must not replace ACCOUNTING.md or bypass the restricted-endpoint quota. Do not add provider DeFi totals on top of the same underlying simple holdings.

### 3.3 DefiLlama

Current official free documentation lists `https://api.llama.fi` as its base, with current/historical price and chart routes, and no authentication. Map tokens as network/contract identities or verified provider IDs. The Pro API uses a separate authenticated base and is not part of the free configuration. Source: D3–D4.

The documentation route was reviewed; a live quote response was not verified during this handoff. Implement a small contract test against the documented route before depending on it. If the current service uses a different official host/path, confirm it from current official references and record the adapter change. Do not silently add a paid endpoint or treat a 404 as a zero price.

Persist quote timestamp/quality and report unavailable token mappings. No historical-price API can recover an exchange purchase cost that was never supplied.

### 3.4 Bitcoin — Blockstream Esplora

Base: `https://blockstream.info/api`. Relevant GET routes include `/address/{address}`, `/address/{address}/txs/chain[/{last_seen_txid}]`, `/address/{address}/txs/mempool`, and `/tx/{txid}`. Confirmed history pages contain 25 transactions. The initial combined address history response and mempool response have separate limits; do not assume all pending records fit. Amounts are satoshis. Source: B1.

Use confirmed history for the stable ledger, pending data separately, and compute account deltas from inputs/outputs. Fetch each shared transaction once and allocate economic effects by owned-address union. Cache immutable confirmed evidence, revisit the non-final tail, and expose incomplete pending coverage when the documented cap can hide records.

No xpub scanning is required in v1. Do not infer the entirety of a Ledger/HD Bitcoin wallet from a single address.

### 3.5 TRON — TronGrid

Base: `https://api.trongrid.io`. Use the account balance, account transactions, TRC-20 transfer, and necessary transaction-detail/receipt endpoints. TRX history and TRC-20 history are separate categories. The TRC-20 account-history reference is R3; derive the other exact routes from R2's official API index.

Persist page continuation/fingerprint with its original query parameters. Do not change filters halfway through a continuation. Normalize SUN/token decimals and distinguish successful effects from reverted calls. Account for actual bandwidth/energy charges from authoritative results; do not infer every fee from one constant.

A current account balance is not guaranteed to prove complete discovery of all historical token contracts. Maintain discovered assets across scans; where coverage is insufficient, expose a token-contract add/review path. No assumption of unlimited calls or a universal fixed quota.

### 3.6 TON — TonAPI

Base: `https://tonapi.io/v2`. Use account data, Jetton balances, account events, and blockchain transaction/trace details as necessary, according to N3. Normalize the account to workchain/account ID and a Jetton to its master identity, not each holder's Jetton wallet address.

Human-readable events and underlying transactions/traces may describe the same movement. Choose one canonical economic representation and link evidence; do not sum both. Preserve logical-time/hash identity, bounced messages, fees and refunds, and asynchronous trace completion. Update incomplete traces before finalizing accounting.

Anonymous docs describe approximately one request per four seconds; key-based free docs describe one per second. Configure limits by access mode, not one universal number. Do not depend on Tonkeeper's in-browser proxy from Tauri. Sources: N1–N2.

### 3.7 Solana — Zerion with optional Helius

Zerion is the initial portfolio/history source. Enable Helius when raw verification, missing data, or additional decoding is needed; it is not mandatory for a portfolio limited to verified Zerion coverage.

Helius read RPC uses `https://mainnet.helius-rpc.com/?api-key=...`. Current billing documents standard historical RPC calls separately from newer history methods. Parsed Events is available on all plans; legacy Enhanced Transactions is in maintenance mode. `getTransfersByAddress` is explicitly Developer-plan-and-above, so it is **not a free-tier dependency**. Source: H2–H3.

`getTransactionsForAddress` supports related token accounts via `filters.tokenAccounts`, including `all`, and keyset pagination. Current credit documentation meters full results at 10 credits per 100 transactions rounded up, minimum 10; signatures-only is 10 flat. Older launch material says paid plans and older per-call prices. Verify the current user's entitlement with one bounded request before enabling this optional method; no automatic upgrade. Sources: H3–H5.

Fallback standard RPC needs `getSignaturesForAddress` plus `getTransaction`, and token-account discovery. Querying only a wallet's main address, or only currently open token accounts, may miss historical token activity. Never declare full history on that basis. Include failed transactions for their actual fees; use slot/signature ordering, token decimals, inner instructions, and balance changes. Separate rent deposits/refunds, wrapped SOL operations, and transfers to avoid false purchases/income. Token-2022 behavior requires explicit support, otherwise mark affected accounting partial.

### 3.8 Optional EVM verification — Alchemy / Ankr

Use Alchemy for selected network receipts/native balances/token data or additional transfers. `alchemy_getAssetTransfers` is an asset-movement index, not an exhaustive list of failed transactions, approvals, or arbitrary calls. Current method documentation lists a narrower set of chains for internal transfers than for general RPC. Do not infer history coverage from “all mainnets.” Source: A2.

Alchemy's selected transfer method is documented at 120 CU per request; a per-second request limit alone is insufficient for scheduling. Source: A1–A2.

Ankr Freemium requires sign-in without funding; Advanced API is authenticated. Current plan docs list 200M monthly credits and one personal token. The configured user tariff overrides the generic documented rate: Node API 30/sec and Advanced API 30/min. Their indexed API and ordinary RPC have different network coverage. Source: A3.

Choose one source as canonical for a given scan. A fallback cross-check is an observation, not a second financial transaction. Never combine opaque provider event IDs blindly. Unsupported/capped categories remain visible as partial coverage.

## 4. Scheduler and budgets — application policy

These defaults are conservative product choices, not extra guarantees from providers. Every limit is configurable; verified account information and stricter service responses take precedence. Share budget across all windows, jobs, and network adapters using the same credential.

| Work | Initial policy |
|---|---|
| Visible portfolio prices | Batched every 60 seconds; cached immediately; 5 minutes while minimized |
| Complete account balance/history sweep | Every 60 minutes by default; adaptive to address count and quota |
| Actively viewed account | At most every 5 minutes; coalesced with whole-portfolio sweep |
| Historical market chart | On demand, cached by range/resolution; do not refetch on every hover or navigation |
| Token/chain metadata | Cache at least 24 hours unless corrected or needed for a new identity |
| Initial history import | Low priority, persisted pages; pause before budget is exhausted |
| Zerion local daily budget | Default 1,600 requests, including retries and enrichment; reserve 400 of advertised allowance |
| Zerion restricted endpoints | Default combined cap 400/day within the above budget; normally much less |
| Unspecified public API throughput | Start at no more than 1 request/sec with one concurrent request; reduce on limits |
| Helius/Alchemy/Ankr credits | Track method-specific estimated costs as well as actual reported usage |

An app soft limit is not permission to evade provider limits. Account quotas can also be used by other tools. Display both locally counted usage and provider-reported remaining quota where available, with clear labels.

### Worked 24-hour example

Assume <= 100 mapped market assets, 10 Zerion addresses, one positions page and one new-history page per address per sweep, and one actively viewed address:

- Price batch every minute: 1,440 requests/day, before limited chart/metadata requests, versus LCW's published 10,000/day.
- Hourly Zerion sweep: `10 * 2 * 24 = 480` requests/day.
- One active address every 5 minutes: conservative additional `2 * 288 = 576` requests/day, before overlap coalescing.
- Combined baseline <= 1,056 Zerion requests/day; within the 1,600 local cap, leaving 544 for extra pages, backfill, retries, and optional enrichment.

These are assumptions, not a guarantee that every wallet fits one page. Recompute the estimate when response pagination or the number of accounts grows. At 50 addresses, extend intervals and/or rotate accounts rather than promising the same cadence for free. Deep history may take multiple daily quotas; expose progress and resume automatically while the app is running.

## 5. Why this selection

The selection optimizes total cost and implementation work for one person: a generous batched market feed, one normalized EVM/Solana indexer, and small chain-specific adapters for BTC/TRON/TON. It avoids a paid all-in-one dependency and avoids building full blockchain indexers locally.

Do not implement a large provider catalog just because services exist. Alchemy/Ankr/Helius fill concrete gaps, and CoinLore provides an optional mapped market fallback. CoinGecko and other market services can be added later if an actual coverage gap warrants their extra integration/key/quota handling. A trial or one-time promotional credit is not a permanent free plan.

Free service access does not guarantee all years of every price, all operation categories, all assets, or exact profit. The app's coverage states, local caches, and user-supplied acquisition data are part of the core design, not optional error handling.
