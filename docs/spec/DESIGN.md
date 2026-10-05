# Ledger Live-Inspired Desktop Design

## 1. Direction

Use the classic Ledger Live desktop portfolio as the interaction and visual reference: persistent left navigation, prominent balance, restrained typography, a large simple chart, and clear asset/account rows. Ledger's current site calls the product Ledger Wallet (formerly Ledger Live); the requested reference remains the familiar desktop portfolio experience. See [official reference](https://shop.ledger.com/pages/ledger-wallet) and [official desktop illustration](https://cdn.shopify.com/s/files/1/2974/4858/files/LedgerWalletAppDesktop.png?v=1775640912).

Use an original application name and icons. Do not place Ledger's logo or device/trading functionality into this information-only app. The measurements and tokens below are original implementation requirements, not claims that Ledger uses these exact values.

The target is a finished desktop product. Avoid a generic admin dashboard filled with unrelated KPI cards, a marketing landing page, decorative gradients, or a dense trading terminal.

## 2. Layout and tokens

Primary acceptance viewport: 1440 × 900 logical pixels. Secondary: 1280 × 800 and 1024 × 720. Default window: approximately 1440 × 900 when the display permits; minimum desktop window: 1024 × 720. Respect work area and display scaling.

| Element | Specification |
|---|---|
| Sidebar | 224 px wide; product mark at top, navigation, groups section, settings/help at bottom |
| Main content | Flexible width; 32 px outer padding at primary size, 24 px at secondary size |
| Top toolbar | Page title/breadcrumb, scope selector, privacy toggle, sync status/action |
| Spacing | 4, 8, 12, 16, 24, 32, 48 px scale |
| Cards/surfaces | 12 px radius, thin border, little/no shadow; avoid excessive nesting |
| Asset rows | Approximately 64 px; 32 px token icon; clear separators and hover state |
| Controls | 36–40 px desktop height; minimum 44 px touch targets in future mobile layouts |
| Chart | Approximately 260–300 px tall at primary viewport, unobtrusive grid, 2 px line, restrained area fill |
| Typography | Bundled Inter or a licensed metrically suitable system fallback; 14 px body, 12 px metadata, 24 px page title, 40 px primary balance |
| Numbers | Tabular numerals; right-aligned monetary/quantity columns |

Dark is the initial theme; light and system are required.

| Token | Dark | Light |
|---|---|---|
| Window background | `#101114` | `#F6F7F9` |
| Sidebar background | `#15161A` | `#FFFFFF` |
| Surface | `#1B1D22` | `#FFFFFF` |
| Elevated/hover | `#24272E` | `#F0F1F5` |
| Border | `#343842` | `#DEE1E8` |
| Primary text | `#F4F5F7` | `#17191F` |
| Secondary text | `#B1B7C4` | `#596171` |
| Accent/chart | `#B6A3FF` | `#6746CE` |
| Positive | `#63D9A4` | `#087849` |
| Negative | `#FF8992` | `#B52D40` |
| Warning | `#F0C572` | `#8A5B00` |

Verify contrast in the implementation; tokens may be adjusted slightly to meet accessibility. Positive/negative values include signs and text; never rely only on red/green.

## 3. Navigation

Main items: **Portfolio**, **Wallets**, **Activity**. Show **Groups** beneath, with create/manage actions. **Settings** sits at the bottom. Do not add Market News, Discover dApps, Buy/Swap, Hardware Manager, or staking promotions.

Preserve scope/date range when navigating from a portfolio row to an asset and back. Use breadcrumbs for account/asset detail, never nested modal stacks for primary navigation.

## 4. Portfolio screen

Reading order:

1. Page title and scope selection.
2. “Total balance” and the large USD value; a compact freshness/coverage line beneath.
3. Two compact metric rows or a restrained summary strip: unrealized P&L/return and selected-period gain/return. Tooltip explains each.
4. Portfolio chart, period controls, and timestamp/value hover tooltip.
5. “Assets” header with search, network filter, and display settings.
6. Asset table, then a compact recent-activity section.

Required table columns, left to right: Asset; Price (USD); Balance; Value (USD); 24h. The balance cell uses asset units; the value cell uses USD. Symbol and network metadata use subdued second lines. A holding spanning networks expands into child rows or links to a clearly scoped detail.

Default sort is Value descending with deterministic tie-break. Missing prices stay distinct from numeric zero during sorting. Retain the user's sort/filter selection. Hidden/spam filters show a count and let the user review excluded items.

Chart hover must not permanently overwrite the present balance. The hovered value appears in its tooltip or a clearly marked historical readout. A privacy toggle masks all visible sensitive values consistently, including chart tooltips and details.

## 5. Asset and account detail

Asset header: icon, full name, symbol, network chips, current market price and 24h change. Holdings section: amount, USD value, basis, and profit. Chart tabs: **Price** and **Your holdings**. Below: wallet distribution and asset activity.

Wallet/group header: name, scope description, total value, profit/return summaries. Show value chart, assets, accounts, and activity using the same components as the portfolio. Do not build visually unrelated pages for each network.

Account header includes copy-address and open-explorer actions. Address is truncated visually with an accessible full-value copy action. Group detail shows its constituent wallets and warns only when scope coverage is incomplete.

Transaction detail opens in a right drawer approximately 440–520 px wide, with scrollable content and persistent close control. Copy hash/address actions provide brief confirmation. Multi-leg transfers and fees must remain readable without horizontal overflow.

## 6. Forms and source configuration

Add address: wallet name/selection, network selector, public address, optional groups. Validate inline before submission. Explain BTC single-address scope inline. Button: **Add address**. No “Connect wallet” terminology.

Source card: provider name, purpose/networks, configured status, key field, Save/Replace/Remove, Test connection, usage and last error. After saving, never return/reveal the stored full key. Show “Configured” rather than a fake string implying the real key is retrievable. Password-manager-style reveal may apply only to text currently being entered, before it is saved.

Basis editor: exact quantity, acquisition date, total USD cost, provenance/known-versus-estimated selection, linked activity. Preview the changed result and affected quantity. Empty cost and zero cost must have visibly different semantics.

## 7. Required states

| State | Required appearance/behavior |
|---|---|
| Empty portfolio | Helpful short explanation + Add address; optional demo is visibly separate |
| First sync | Current known balances plus history progress; no fake complete total/profit |
| Loading | Layout-preserving skeletons; no repeated full-screen blocking spinner |
| Offline | Cached data visible; offline/stale timestamp and retry action |
| Missing key | Explain which provider/feature needs setup; link to Data Sources |
| Invalid/expired key | Specific source error; no repeated credential prompts |
| Quota exhausted | Pause state, next known reset, cached data preserved |
| Partial history | Date/category coverage with a path to details |
| Missing price | Quantity shown; USD `—`; total explicitly marked partial |
| Missing basis | Current value shown; P&L `—` or labeled known subset; Fix basis action |
| Unknown operation | Raw activity retained, unclassified label, review action |
| Empty search/filter | Explain no matches and offer Clear filters |

## 8. Accessibility, localization, and responsiveness

- Keyboard-accessible navigation, tables, drawers, tabs, dialogs, and chart range controls. Visible focus; Escape closes overlays; focus returns to the opener.
- Screen-reader labels for icons and formatted amounts. Provide a textual chart summary/data view so meaning is not hover-only.
- Respect reduced motion; use subtle 100–180 ms transitions, no looping decorative animation.
- Correct USD formatting and locale-aware dates/separators in English and Russian. No hard-coded UI strings scattered through components.
- At 125%, 150%, and 200% Windows display scaling, controls remain reachable. Resize/scroll the main area rather than clip numeric values.
- Below desktop widths in the future mobile layout, sidebar becomes a drawer/bottom navigation; asset rows become compact labeled rows/cards, with every required value accessible.
- Define touch-friendly layouts now, but do not claim iOS testing from a narrow browser screenshot.

## 9. Visual acceptance evidence

Capture at least: populated portfolio in dark and light themes; asset detail; wallet/group detail; activity drawer; provider setup; missing-basis state; partial/offline state; empty state. Include one real native Windows screenshot at the primary viewport and scaling checks. Synthetic data is acceptable for design screenshots if labeled; live integration evidence is separate.

Review screenshots for alignment, truncated required values, weak contrast, misleading zeros, inconsistent spacing, and excessive decoration. A running React dev-server screenshot alone is not proof of the delivered Tauri interface.
