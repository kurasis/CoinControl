# Accounting and Performance Contract

Version 1.0 · Normative calculation requirements

This is a personal analytics model. It does not implement jurisdiction-specific tax reporting. All USD arithmetic uses exact decimal values; quantities use exact raw units. USD is a reporting currency, not a claim that a USD bank balance is tracked.

## 1. Three different questions

| UI metric | Question answered | Required data |
|---|---|---|
| Market change 24h | How did the asset's market price change? | Comparable USD quotes now and 24 hours earlier |
| Unrealized P&L / return | How much have my remaining holdings gained relative to their acquisition basis? | Remaining lots, their basis, current prices |
| Period gain / period return | How did this account/group/portfolio perform during a time interval after external flows? | Beginning/end values and classified, valued external flows |

Also show realized P&L, income, fees/expenses, and total accounted P&L in the detailed breakdown. Never replace a missing acquisition basis with a 24h price change or label an incoming deposit as profit.

## 2. Reliable basis and provenance

A blockchain receipt proves a movement. It does not prove when the owner bought the coins, what fiat was paid, or whether the transfer was between accounts they own.

Basis sources, in priority order:

1. Reviewed user data attached to the actual lot or operation.
2. Explicit trade consideration from an interpreted operation, valued in USD using an appropriate execution value or historical reference price.
3. Basis inherited from an identified transfer of owned lots.
4. An explicitly selected estimated basis, such as receipt-time market value, labeled estimated.
5. Unknown.

An API's historical USD value is a valuation, not automatically purchase cost. A provider's computed P&L is not the authoritative ledger for this app.

Every basis component records quantity, USD basis, acquisition timestamp, source, certainty, and source operation. Zero basis is valid only when explicitly justified; unknown basis is `null`, not zero. Partial basis creates separate known and unknown lot fragments.

If the user chooses receipt-time estimated basis, retain that decision and the exact historical quote/quality. Estimated and known results must remain distinguishable in summaries and exports.

## 3. Lots and disposal policy

Use FIFO, fixed for v1. Consume the earliest eligible lot **held in the sending account for the exact chain-specific asset**. Preserve the original acquisition date/order through own-account transfers. Deterministic tie-breaks use original chain position and stable lot ID, not a database insertion race.

Do not consume a lot from another account merely because the symbol matches. Display grouping does not merge inventories. A future cost-basis method change requires explicit migration/replay and different reports.

Each sale/swap/disposal has an exact disposed quantity and proceeds. Consume basis proportionally when splitting a lot. Retain links from each consumed fragment to its acquisition. If inventory is insufficient, mark a reconciliation gap; do not create zero-cost inventory or allow a hidden negative lot.

A token swap has both disposal and acquisition legs. Assign a consistent USD gross trade value to the exchanged sides. Prefer known consideration/verified liquid-asset valuation; do not average incompatible illiquid token quotes. Material valuation conflicts require review. Explicit swap fees are handled separately under section 6.

## 4. Current valuation and unrealized return

For a scope containing a deduplicated set of accounts:

```text
V = sum(quantity[a] * current_USD_price[a])
C = sum(remaining_cost_basis_USD[lot])
U = V - C
UnrealizedReturnPercent = 100 * U / C
```

`U` and its percentage for the entire scope are available only when the corresponding quantities, prices, and bases are sufficient. If C = 0, show the dollar gain but percentage `N/A (zero basis)`; never infinity. If one necessary value is missing, show `—` and offer a labeled known-subset breakdown.

For partially known basis, calculate subset P&L using **only the market value of the known lot quantities**, not the market value of all holdings. Report basis coverage in units for one asset and in valued-holding share where meaningful for multi-asset scopes. Unpriced quantities cannot disappear from coverage counts.

The total balance may be a valued subtotal even when basis is missing. Price coverage and basis coverage are independent.

Price change uses `(P_now / P_24h - 1) * 100`, provided the older price is positive and the sources/timestamps are comparable. Prefer the provider's correctly normalized market-change field when it represents the same asset and USD pair. Do not average percentages to obtain portfolio performance.

## 5. Realized P&L, income, and total accounted P&L

```text
RealizedPnl = sum(gross_disposal_proceeds_USD - consumed_basis_USD)
TotalAccountedPnl = UnrealizedPnl + RealizedPnl + RecognizedIncome - Expenses
```

This is a breakdown of the tracked accounting history. It is not automatically a lifetime return for every investment the user has ever made. Only claim complete scope P&L when necessary data and classifications are complete. Otherwise expose known components and reasons for incompleteness.

Income treatment: an explicitly classified reward/airdrop received at a reliable value produces income at receipt and a lot with that same USD basis. Subsequent appreciation contributes to unrealized/realized P&L. This avoids counting the receipt twice. Unsolicited/spam assets do not become trusted income automatically; unknown value remains unknown. NFT and protocol-position valuation is outside v1.

Realized P&L belongs to the account that disposed of the lot. An own transfer does not move previously realized P&L. Remaining lots carry their basis to the recipient. Group selection recomputes the union of account results; an overlapping account is counted once.

Do not calculate a vague “total profit percentage” by dividing lifetime profit by today's holdings, summing purchase amounts repeatedly through swaps, or averaging asset returns. Show the defined unrealized return and period return instead.

## 6. Fee policy: expense once

V1 expenses all separately identified transaction/network/trading fees at the time incurred. Do not also capitalize those same fees into acquisition basis or subtract them a second time from gross disposal proceeds. Store gross consideration and fee separately when supplied; if a feed only gives net proceeds, normalize consistently and mark the convention so the same fee cannot be applied twice.

When a fee is paid in a tracked cryptocurrency:

1. Remove the fee quantity from the actual payer's inventory exactly once.
2. Consume its FIFO basis.
3. For analytical reconciliation, record a fee-asset disposal at the fee's reliable market value: `fee value - consumed fee-asset basis` contributes to realized P&L.
4. Record an expense equal to that same fee value.

The two entries in steps 3–4 prevent double counting while retaining fee expenses and asset appreciation as separate components. If fee price/basis/payer is unknown, do not fabricate the missing component; mark affected accounting incomplete.

Example: 2 ETH with $4,000 basis; pay 0.01 ETH fee when ETH = $3,000. Remaining inventory = 1.99 ETH, remaining basis = $3,980, value = $5,970, unrealized P&L = $1,990. Fee-asset realized P&L = $30 - $20 = $10. Expense = $30. Total accounted P&L = $1,970. The fee is not deducted again from the already reduced balance.

Fee handling is based on actual execution outcome. A failed contract call may pay a network fee while all intended token movements revert. Sponsored transactions charge the user only the assets they actually paid; do not impute another party's gas as their expense. For L2s use authoritative total-fee fields/receipts when needed, rather than assuming `gasUsed * effectiveGasPrice` captures every component.

## 7. Transfers and ownership

### 7.1 Between tracked owned accounts

Match on chain evidence, asset identity, quantities, and sender/recipient ownership; not merely equal amounts at similar times. Both accounts must be part of the same local owned portfolio. For same-chain direct movements the shared transaction/leg provides strong linkage.

Ownership records include archived accounts; the active selection is a separate filter. Group membership changes are view changes, not transactions. Apply the current account-set scope consistently to both endpoint values and historical external-flow classification.

Move lot fragments without resetting their basis or original acquisition date. This is neither income nor a sale. Network fees remain expenses. If the source has multiple FIFO lots, transfer the appropriate fragments and preserve their lineage.

At the whole-portfolio scope these transfers create no external capital flow. At a wallet/group scope, a transfer crossing the selected scope boundary is an external flow at fair market value for **period performance**, while inherited cost basis remains unchanged for **unrealized P&L**. This difference is intentional.

When adding another owned address later, replay affected classifications so what previously looked external can become an own transfer. Reviewed manual decisions must be preserved or surfaced as conflicts.

### 7.2 Unknown sender/recipient

- Incoming from an exchange or untracked address: receive inventory with unknown basis unless linked acquisition data exists. Do not presume the transfer timestamp is the purchase timestamp.
- Outgoing to an untracked address: do not presume a sale. The user may classify it as own transfer to an untracked account, withdrawal, payment, gift, sale, or unresolved.
- An explicitly classified external withdrawal preserves/export-links its lot lineage; it is not a realized sale. It is an external flow for the scope's period return.
- A payment/disposal can have explicitly supplied proceeds/consideration; lacking those values, realized P&L remains unresolved.
- Externally returned lots require linked lineage or supplied basis, not arbitrary matching by symbol/amount.

Do not silently apply capital-flow neutrality to a payment/gift whose economic classification is unknown. Until classified, affected period performance is incomplete.

### 7.3 Bitcoin-specific rules

Sum owned input/output effects across the union of tracked addresses. Change sent to a tracked owned address is not a purchase or income. Fee attribution must not be repeated per input/address or deducted again from a net delta that already includes it.

If not all change addresses are tracked, explain the scope limitation. Mixed-ownership inputs, CoinJoin, or ambiguous fee sharing require partial/unknown attribution unless verified. Do not assign an entire mixed-input transaction's fee to every observed address.

### 7.4 Bridges, wrapping, and protocol interactions

For a verified same-owner 1:1 wrap/unwrap, preserve lot basis through the asset transformation and record actual fees separately. For a bridge, preserve basis only after a supported decoder or reviewed manual link proves the relationship and quantities. Fees/slippage cannot be hidden as a basis reset.

Unsupported bridges, rebases, liquidity positions, staking changes without ordinary transfers, token migrations, rebating/fee-on-transfer tokens, or ambiguous swaps remain partially decoded. Preserve observations and balance discrepancies; do not invent economic interpretation to make the ledger balance.

## 8. Flow-adjusted period performance

Use **Modified Dietz** for the required period-return percentage. It is an approximation, not an IRR, exact time-weighted return, or annualized return. Display the method in an information tooltip. The general method is documented by GIPS [SOURCES.md, F1](SOURCES.md); this application uses actual elapsed seconds for weights.

For an interval `(t0, t1]`:

```text
V0 = scope market value at t0 (including events at t0)
V1 = scope market value at t1 (including events at t1)
Fi = signed external capital flow at ti; inflow positive, outflow negative
wi = (t1 - ti) / (t1 - t0)
PeriodGainUSD = V1 - V0 - sum(Fi)
DietzDenominator = V0 + sum(wi * Fi)
PeriodReturnPercent = 100 * PeriodGainUSD / DietzDenominator
```

Requirements:

- Include external flows with `t0 < ti <= t1`. Never count an event in both V0 and the flow sum.
- Value in-kind transfers crossing the selected scope at reliable historical USD market value, regardless of their acquisition basis.
- Trades entirely within the scope are not external capital flows. For an asset-only scope, value entering/leaving that asset through a trade crosses its scope boundary.
- Own transfers within the scope cancel. Transfers crossing a selected wallet/group boundary do not cancel there.
- Recognized rewards are investment income, not contributed capital. Fees/expenses reduce return and are not withdrawals to neutralize. Sales whose proceeds leave the tracked scope need a correctly classified external flow; proceeds retained in a tracked asset stay inside the scope.
- Show `N/A` if the duration is zero, required prices/balances/classifications are missing, or denominator is nonpositive. Avoid all NaN/infinity outputs.
- Label the result estimated if endpoint prices or cash-flow valuations are estimated. Hide the numeric return if uncertainty materially prevents the calculation, while exposing known data.
- ALL means the available tracked-history interval. It does not promise a return since the original purchase when that purchase is unknown.
- Do not sum daily percentage returns. If later implementing linked subperiod returns, specify compounding separately.

Example: V0 = $1,000, V1 = $1,650, one $500 deposit halfway through. Gain = $150; denominator = $1,250; return = 12%. A naive 65% balance increase would be wrong.

## 9. Historical quotes

Store requested timestamp, quote timestamp, provider, asset mapping version, and resolution. Execution values explicitly supported by the operation take precedence for its economic interpretation; a general market quote is an estimate of fair value.

Use the nearest suitable historical point within a documented tolerance. Suggested initial tolerances: 5 minutes for intraday precise-looking reference values; at most 24 hours for explicitly marked daily estimates. Do not use a later quote silently to fill a long gap. Provider-specific resolution/availability can require narrower tolerances or an unavailable result.

Never force a stablecoin price to exactly $1. Never map a token by ticker alone. Price/basis overrides are independent: overriding a market price does not rewrite original purchase cost.

## 10. Generic CSV basis import

Required import mode in v1: attach acquisition lots/basis and classifications to existing incoming history. Provide preview, column mapping, row-level errors, duplicate detection, and transactional commit/rollback.

Recommended columns:

```text
external_row_id,network_id,account_address,transaction_id,leg_id,
asset_identifier,quantity,acquired_at_utc,total_basis_usd,
basis_kind,classification,note
```

`quantity` and `total_basis_usd` are exact decimal strings. `basis_kind` is `known`, `estimated`, or `unknown`. A blank cost means unknown; literal `0` is an explicit zero. ISO 8601 timestamps must include a timezone. `leg_id` may be omitted only when matching is unambiguous and confirmed in preview.

Allow multiple acquisition lots to match one receipt, including dates earlier than the deposit date. Their sum cannot exceed the matched receipt's original quantity without a reviewable reconciliation action. Track prior sales/fee withdrawals separately; imported basis must not double-create a holding.

Validate imported acquisition fragments against the receipt's original quantity and its already consumed/transferred lineage, not only the remaining balance. Basis correction for an already sold receipt is allowed and triggers replay of its dependent disposals; it does not restore spent coins. Preserve both acquisition time and account-arrival time.

Opening lots are allowed for a declared history start with an explicit cutoff. They establish inventory at that boundary. Do not combine them with overlapping pre-cutoff acquisitions. If earlier history is later downloaded, pause and reconcile the opening position before replaying it together.

Manual classification/disposal editing has the same provenance/audit requirements even if separate from this initial CSV schema. Full exchange-account trading books and automatic vendor-specific import dialects are deferred.

## 11. Golden tests and invariants

Implement the cases in [fixtures/accounting_cases.json](fixtures/accounting_cases.json). The fixture is declarative acceptance data, not an implementation algorithm. Extend it with chain-specific examples while preserving the expected arithmetic.

Invariants:

- Deduplicated scope aggregation conserves exact quantities and basis.
- Own transfers preserve total basis apart from separately consumed fee lots.
- Replay with the same evidence, override versions, and prices is deterministic.
- Reimporting identical data or repeating synchronization has no economic effect.
- Missing values never become zero implicitly.
- Display rounding cannot change stored balances or lot conservation.
- Cost-based lifetime P&L, period gain, and market price change remain separately labeled and independently tested.
