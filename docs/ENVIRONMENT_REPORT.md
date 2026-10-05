# Environment report

Recorded 2026-10-04 at the start of Stage A (stage B update at the end), as required by
[docs/spec/README.md](spec/README.md). Credentials are reported by name only.

## Development machine

| Item          | Value                                                  |
| ------------- | ------------------------------------------------------ |
| OS            | Ubuntu 24.04.4 LTS (Linux container), x86_64           |
| CPU / memory  | 4 vCPU, 15 GiB RAM                                     |
| Node.js / npm | 22.22.0 / 10.9.4                                       |
| Rust          | rustc 1.97.0, cargo 1.97.0 (pinned in rust-toolchain)  |
| Tauri CLI     | 2.12.1                                                 |
| TypeScript    | 5.9.3                                                  |
| Vitest        | 5.0.3                                                  |
| Display       | None; native checks ran under Xvfb (software GL)       |
| Windows       | Not available locally. Windows builds run in GitHub CI |

## Network access

Outbound HTTPS works through a proxy (package registries and GitHub reachable).
No provider API was called: no adapters exist yet in Stage A.

## Credentials (names only)

Every variable from [`.env.example`](spec/.env.example) was checked; none is set.

| Variable                             | State  |
| ------------------------------------ | ------ |
| `LIVECOINWATCH_API_KEY`              | absent |
| `ZERION_API_KEY`                     | absent |
| `TRONGRID_API_KEY`                   | absent |
| `TONAPI_API_KEY`                     | absent |
| `HELIUS_API_KEY`                     | absent |
| `ALCHEMY_API_KEY`                    | absent |
| `ANKR_API_TOKEN`                     | absent |
| `RUN_LIVE_API_TESTS`                 | absent |
| `LIVE_TEST_PROVIDERS`                | absent |
| `LIVE_TEST_TARGETS_FILE`             | absent |
| `LIVE_TEST_MAX_*` budgets            | absent |
| Code-signing certificate for Windows | absent |

## Consequences

- Live provider tests are `BLOCKED` until adapters exist (Stage B) and the user
  supplies keys through `.env.test.local` or CI secrets.
- Native Windows end-to-end tests are `BLOCKED` locally; the NSIS installer is
  built by the `windows-installer` CI job but has not been installed or driven.
- The installer is unsigned.

## Stage B update (2026-10-04)

Same machine and toolchains. The cloud environment now provides provider keys
as environment variables; checked by name only:

| Variable                | State                                 |
| ----------------------- | ------------------------------------- |
| `LIVECOINWATCH_API_KEY` | set (used)                            |
| `ZERION_API_KEY`        | set (used)                            |
| `TRONGRID_API_KEY`      | set (not used yet: TRON arrives in D) |
| `TONAPI_API_KEY`        | set (not used yet: TON arrives in D)  |
| `HELIUS_API_KEY`        | set (not used: optional supplemental) |
| `ALCHEMY_API_KEY`       | set (not used: optional supplemental) |
| `ANKR_API_TOKEN`        | absent (optional; not shipped)        |

Reachable from this environment: `blockstream.info`, `api.zerion.io`,
`api.livecoinwatch.com`, `coins.llama.fi`. `api.llama.fi/prices/*` answers 404;
see the endpoint note in TEST_REPORT.md.
