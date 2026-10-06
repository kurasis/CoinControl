# Network coverage (stage D)

The table below describes the original Zerion route. In 0.1.4 a configured
Alchemy key takes priority for Ethereum, Base, Arbitrum, Optimism and Polygon;
a configured Helius key takes priority for Solana. The application shows the
actual configured route, with partial-coverage limitations and no inherited live
verification date. See [provider integration](PROVIDER_INTEGRATION.md).

What each required network gets from its original data provider: The same
table is compiled into the app (`portfolio_providers::capabilities`) and shown
under **Settings → Networks**, so the UI and this document cannot drift apart
silently: a unit test checks that every network's listed provider is the one the
sync engine actually uses.

Live evidence: every row was exercised against the real provider on
2026-10-05 (`npm run test:live`, see [TEST_REPORT.md](../TEST_REPORT.md)).

| Network  | Provider            | Key      | Balances | Token discovery | History categories                                | Fees | Internal transfers |
| -------- | ------------------- | -------- | -------- | --------------- | ------------------------------------------------- | ---- | ------------------ |
| Bitcoin  | Blockstream Esplora | No       | Full     | —               | native, failed, pending                           | Full | —                  |
| Ethereum | Zerion              | Yes      | Full     | Full            | native, tokens, trades, failed, internal          | Full | Partial            |
| Base     | Zerion              | Yes      | Full     | Full            | native, tokens, trades, failed, internal          | Full | Partial            |
| Arbitrum | Zerion              | Yes      | Full     | Full            | native, tokens, trades, failed, internal          | Full | Partial            |
| Optimism | Zerion              | Yes      | Full     | Full            | native, tokens, trades, failed, internal          | Full | Partial            |
| Polygon  | Zerion              | Yes      | Full     | Full            | native, tokens, trades, failed, internal          | Full | Partial            |
| BNB      | Zerion              | Yes      | Full     | Full            | native, tokens, trades, failed, internal          | Full | Partial            |
| Solana   | Zerion              | Yes      | Full     | Full            | native, tokens, trades, failed                    | Full | Partial            |
| TRON     | TronGrid            | Yes      | Full     | Partial         | native, tokens (TRC-20), failed, internal         | Full | Partial            |
| TON      | TonAPI              | Optional | Full     | Full            | native, tokens (Jettons), trades, failed, pending | Full | Partial            |

## Limitations per network

- **Bitcoin**: one address per account (no xpub). Pending transactions beyond
  Esplora's 50-record mempool list may be hidden; the account is marked.
- **EVM networks (Zerion)**: wallet balances only; DeFi deposits, lending and
  NFTs are not in totals. History is Zerion's interpretation; internal transfers
  appear only where Zerion reports them.
- **Polygon**: Zerion reports native POL under the system contract
  `0x0000000000000000000000000000000000001010`; it is mapped to the native asset
  so it is not double counted.
- **Solana**: Zerion interpretation; rent deposits, wrapped SOL and closed token
  accounts are not separated. Mint addresses are case-sensitive and kept exactly
  (EVM addresses are compared case-insensitively). Token-2022 transfer-fee
  extensions are not verified.
- **TRON**:
  - The TRX holding includes staked TRX (frozen v2, unfreezing and delegated
    bandwidth/energy). Voting rewards count when claimed (`WithdrawBalance`
    becomes a staking-reward receipt).
  - TRC-10 tokens are not tracked; their transfers stay as partially decoded
    activity.
  - TronGrid lists TRC-20 balances without decimals. A balance appears once its
    metadata is known from history, or from a lookup of at most 10 tokens per
    sync (accounts can hold hundreds of spam tokens).
  - No token verification; TRC-20 tokens are priced by contract only.
  - Only confirmed transactions are imported (`only_confirmed=true`).
  - Native and TRC-20 histories are two separate cursors; the account is
    "complete" only when both are.
- **TON**:
  - One record per trace (the TonAPI account event). The event's `extra` is the
    net fee (negative) or a refund (positive).
  - Jettons are identified by their master contract in raw `0:…` form.
  - Events still in progress are stored as pending and re-checked by id.
  - Generic contract calls show the TON sent but are marked partially decoded;
    NFT transfers are shown but not valued.
  - Works without a key at about one request per 4 s; a free key is
    recommended.

## Prices for new native assets

| Network | Live Coin Watch code | CoinGecko id (DefiLlama)  |
| ------- | -------------------- | ------------------------- |
| Polygon | `POL`                | `polygon-ecosystem-token` |
| BNB     | `BNB`                | `binancecoin`             |
| Solana  | `SOL`                | `solana`                  |
| TRON    | `TRX`                | `tron`                    |
| TON     | `TONCOIN`            | `the-open-network`        |

On Live Coin Watch `TON` is an unrelated coin; Toncoin is `TONCOIN`.
DefiLlama keys TON Jettons by the bounceable friendly address
(`ton:EQ…`), not the raw form. Tokens are priced by `chain:contract`
with the contract's original case (Solana and TRON are case-sensitive).

A token that no price source knows is remembered for 24 hours
(`quote_misses` table) and not requested again in that window, so accounts with
many spam tokens do not spend the request budget on every refresh.
