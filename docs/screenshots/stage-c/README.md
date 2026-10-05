# Stage C screenshots

Captured from the native debug build on Linux (WebKitGTK under Xvfb with
software rendering), not on Windows, on 2026-10-05. The **demo profile** is
synthetic data and is labeled as such in the app; real-history accounting was
verified separately by the live test suite.

- `linux-demo-portfolio.png`: accounting metrics (unrealized P&L on the
  known-basis subset with coverage, period return unavailable with its reason,
  realized P&L, income, fees, total P&L pending) and the review banner.
- `linux-demo-review-drawer.png`: movement drawer after saving a decision
  through real IPC: estimated basis applied, the review list emptied, and
  version 1 in the decision history.
- `linux-demo-assets-pnl.png`: asset table with unrealized P&L per position and
  "Basis unknown" for the unpriced token.
- `linux-demo-asset-holdings.png`: Bitcoin asset page, **Your holdings value**
  tab over one year (the step is the second receipt).
- `linux-demo-asset-lots.png`: per-account split, open FIFO lots with known and
  estimated basis and acquisition vs. arrival dates, and activity for the asset.

Development evidence only; the DESIGN.md matrix must be captured on Windows 11.
