# Stage B screenshots

Captured from the native debug build on Linux (WebKitGTK under Xvfb with
software rendering), not on Windows, on 2026-10-04. The real profile was empty
except for two public addresses from
[tests/live/public-targets.json](../../../tests/live/public-targets.json); all
balances, prices, and activity come from live provider responses.

- `linux-live-portfolio.png`: total, partial-price and spam-exclusion badges.
- `linux-live-wallets.png`: per-account synchronization state (Bitcoin history
  complete; Ethereum history still importing across sweeps) and **Sync now**.
- `linux-live-activity.png`: synchronized Ethereum activity, including incoming
  legs awaiting review.

Development evidence only; the DESIGN.md matrix must be captured on Windows 11.
