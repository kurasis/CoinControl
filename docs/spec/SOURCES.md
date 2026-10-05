# Source Register and Verification Notes

Review date: **2026-10-04**. Sources are official provider/framework documentation or official product pages. Links may redirect as documentation evolves. The reviewer compared current pages, not only older search snippets. No key-authenticated API calls or native application tests were performed while creating this specification.

The specification distinguishes documented service facts from application policies. Refresh intervals, soft budgets, visual tokens, the finite network matrix, accounting conventions, and acceptance targets are deliberate project decisions. They are not promises made by the providers.

## Market data

| ID | Official reference | What was verified |
|---|---|---|
| P1 | [Live Coin Watch API offering](https://www.livecoinwatch.com/tools/api) | Free API key, published 10,000 daily requests, historical market data offering |
| P2 | [Live Coin Watch API documentation](https://livecoinwatch.github.io/lcw-api-docs/) | Authentication, POST requests, custom coin-map limit, price history, ratio-style change fields |
| P3 | [CoinLore public API](https://www.coinlore.com/cryptocurrency-data-api) | No registration/key, recommended request cadence, market endpoints |
| D3 | [DefiLlama API overview](https://api-docs.defillama.com/) | Free versus Pro separation, free authentication policy, documented base and price capabilities |
| D4 | [DefiLlama free endpoint reference](https://api-docs.defillama.com/llms-free.txt) | Current/historical prices, identity parameters, chart routes; actual live-route validation remains an implementation gate |

## EVM / multichain indexers

| ID | Official reference | What was verified |
|---|---|---|
| Z1 | [Zerion API product and pricing](https://zerion.io/api) | Developer quota/rate; restricted endpoint quota; distinction from paid plans |
| Z2 | [Zerion API network capabilities](https://zerion.io/api/chains/) | Per-chain tokens/transactions/DeFi coverage, including current Solana limitation |
| Z3 | [Zerion authentication](https://developers.zerion.io/authentication) | API key Basic Auth, empty password, separate paid per-request alternatives |
| Z4 | [Zerion quickstart](https://developers.zerion.io/quickstart) | Portfolio/position/history API integration entry points |
| Z5 | [Zerion transaction reference](https://developers.zerion.io/api-reference/wallets/get-wallet-transactions) | Current wallet transaction API reference and schema entry point |
| A1 | [Alchemy pricing](https://www.alchemy.com/pricing) | Free CU allowance and both request/compute throughput descriptions |
| A2 | [Alchemy asset transfers method](https://www.alchemy.com/docs/data/transfers-api/transfers-endpoints/alchemy-get-asset-transfers) | Transfer endpoint, method CU cost, pagination, network-specific internal-transfer availability |
| A3 | [Ankr service plans](https://www.ankr.com/docs/rpc-service/service-plans/) | Freemium recurring credits, sign-in requirement, personal token, Advanced API limits and indexed-versus-RPC distinction |

## Chain-specific services

| ID | Official reference | What was verified |
|---|---|---|
| B1 | [Blockstream Esplora API](https://github.com/Blockstream/esplora/blob/master/API.md) | Public base, satoshi units, confirmed-history paging, pending-data limits |
| R1 | [TronGrid rate limits](https://developers.tron.network/reference/rate-limits) | Quotas vary; console/service response is the source of truth; rate-limit handling |
| R2 | [TronGrid API key/network configuration](https://developers.tron.network/reference/select-network) | Mainnet base, key header, anonymous access caveat, keys do not grant blockchain signing rights |
| R3 | [TronGrid TRC-20 account history](https://developers.tron.network/reference/get-trc20-transaction-info-by-account-address) | Separate token history endpoint and continuation parameters |
| N1 | [TonAPI introduction/authentication](https://docs.tonapi.io/tonapi) | Anonymous access, stricter anonymous cadence, Bearer token and console |
| N2 | [TonAPI free limits and Tonkeeper proxy](https://docs.tonapi.io/tonapi/dapp/free-limits) | Default free rate and special browser-proxy context; not a general Tauri entitlement |
| N3 | [TonAPI account methods](https://docs.tonapi.io/tonapi/rest-api/accounts) | Account/event/Jetton-related reference entry point |
| H1 | [Helius pricing](https://www.helius.dev/pricing) | Free monthly credits and RPC rate |
| H2 | [Helius plans](https://www.helius.dev/docs/billing/plans) | Per-product availability; Parsed Events on Free; ordinary Dashboard signup versus separate agent signup |
| H3 | [Helius credits](https://www.helius.dev/docs/billing/credits) | Current metering, legacy status, paid-only getTransfersByAddress, newer result-based history billing |
| H4 | [Helius rate limits](https://www.helius.dev/docs/billing/rate-limits) | Endpoint-class rates, historical batch restrictions |
| H5 | [Helius getTransactionsForAddress](https://www.helius.dev/docs/api-reference/rpc/http/gettransactionsforaddress) | Token-account filter, paging, status filters, key requirement |
| H6 | [Helius original gTFA launch article](https://www.helius.dev/blog/introducing-gettransactionsforaddress) | Older paid-plan/flat-price statements; used to identify a documentation/entitlement caution, not as the current billing table |

## Platform, persistence, security, and methodology

| ID | Official reference | What was verified |
|---|---|---|
| T1 | [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) | Windows prerequisites; Mac/Xcode requirement for iOS development |
| T2 | [Tauri WebDriver guidance](https://v2.tauri.app/develop/tests/webdriver/) | Current WDIO service, external and embedded routes, distinction from browser-only tests |
| T3 | [Tauri Windows installer](https://v2.tauri.app/distribute/windows-installer/) | Windows packaging and WebView2 distribution options |
| T4 | [Tauri capabilities](https://v2.tauri.app/security/capabilities/) | Granular permissions; custom-command defaults require deliberate restriction |
| D1 | [SQLite floating point](https://www.sqlite.org/floatingpoint.html) | REAL precision limitations; rationale for exact quantity/decimal storage |
| D2 | [SQLite backup API](https://www.sqlite.org/backup.html) | Consistent backup approach |
| S1 | [GitHub Actions secrets](https://docs.github.com/en/actions/how-tos/write-workflows/choose-what-workflows-do/use-secrets) | Secret injection into job processes and restrictions |
| S2 | [Vite environment variables](https://vite.dev/guide/env-and-mode) | VITE-prefixed values enter frontend bundles; local-file exclusion guidance |
| F1 | [GIPS Handbook for Firms](https://www.gipsstandards.org/standards/gips-standards-for-firms/gips-standards-handbook-for-firms/) | General Modified Dietz methodology; this app defines its own timestamp convention and makes no GIPS-compliance claim |
| U1 | [Ledger desktop product reference](https://shop.ledger.com/pages/ledger-wallet) | Current Ledger Wallet / former Ledger Live product and desktop visual reference |

## Corrections and safeguards added after review

1. **An API key is not always required.** Selected public Esplora/DefiLlama/CoinLore reads are unauthenticated; TonAPI anonymous access is slower; TronGrid should be configured with a key for reliable application use.
2. **Ankr's current Advanced API limit is not the older 30/min figure.** Use the plan page's current minute/window limits and make them configurable.
3. **TronGrid's old fixed quota figures are not a dependable current contract.** This specification intentionally does not guarantee 100,000/day or 20 requests/sec.
4. **Helius free credits do not unlock every method.** getTransfersByAddress is paid-only in the current credit documentation. Parsed Events supersedes legacy Enhanced Transactions. Current gTFA billing is result-based; older launch material differs. Verify actual gTFA entitlement before using it in a Free-only path.
5. **Alchemy transfers are not complete account activity.** Internal-transfer coverage is method/network-specific. A generic RPC chain list is not a history capability matrix.
6. **Zerion's chart/DeFi/P&L quota is restricted.** The local accounting engine and local chart reconstruction cannot depend on unlimited provider P&L calls. Solana token/history support does not imply Solana DeFi support.
7. **Tauri macOS automation is more nuanced than “unsupported.”** Direct upstream tauri-driver is Windows/Linux-oriented; current docs describe an embedded WDIO route for macOS. Shipping artifacts must exclude embedded automation infrastructure. iOS still needs its own Mac/device validation.
8. **Price history cannot reveal a user's actual purchase basis.** The final model separates asset return, period performance, and unknown acquisition data.
9. **One Bitcoin address does not represent an entire HD wallet.** Extended public key/descriptor discovery is deferred explicitly, with an address-scope notice in v1.
10. **Free does not mean unlimited history, perfect decoding, or guaranteed uptime.** Coverage and data-quality states are mandatory product behavior.
11. **Provider quotas and facts here were documentation-checked, not account-tested.** Required live smoke tests remain part of implementation acceptance; this archive must not be presented as evidence that those tests already passed.

## Revalidation rule for the developer

Before shipping an adapter, verify its current official endpoint schema, free-plan entitlement, authentication method, quota/cost unit, pagination, supported networks, and terms/attribution requirements. Record the check date and the result in the provider manifest. Include appropriate source attribution in Settings/About where required. Do not copy entire external documentation into the repository or assume “free API” means unrestricted redistribution of a provider's dataset.

When official pages conflict, use current endpoint/billing references, a bounded account-specific capability test, and an explicit limitation. Do not silently rely on an old blog post or upgrade to a paid service.
