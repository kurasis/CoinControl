# Crypto Portfolio Tracker — Developer Handoff

Version: 1.0. Reviewed: 2026-10-04. Language: English.

Build a personal, read-only cryptocurrency portfolio application for Windows using **Tauri 2 + React + TypeScript + Rust + SQLite**. Preserve a practical migration path to macOS and iOS. The visual reference is the classic Ledger Live desktop portfolio experience.

This archive is a specification, not an implemented application. Official documentation and public plan descriptions were reviewed. No private API keys were supplied, authenticated integration tests were not executed, and no application build was tested during specification preparation.

## Read in this order

1. [SPECIFICATION.md](SPECIFICATION.md) — product scope, architecture, data model, synchronization, delivery stages.
2. [DESIGN.md](DESIGN.md) — Ledger Live-inspired screens, design tokens, interactions, visual acceptance criteria.
3. [ACCOUNTING.md](ACCOUNTING.md) — authoritative valuation, profit, fee, transfer, and return calculations.
4. [API_PROVIDERS.md](API_PROVIDERS.md) — selected free services, authentication, limitations, routing, request budgets.
5. [TESTING.md](TESTING.md) — mandatory offline, live API, Windows, and release verification.
6. [SOURCES.md](SOURCES.md) — official references and corrections to earlier assumptions.

Supporting files:

- [.env.example](.env.example): empty development/test configuration template; contains no credentials.
- [.gitignore.example](.gitignore.example): recommended exclusions to merge into the implementation repository.
- [fixtures/accounting_cases.json](fixtures/accounting_cases.json): synthetic acceptance cases with explicit expected results.

## Instructions to the implementation agent

- Implement working software, not only a dashboard mockup. The approved stack is mandatory.
- Treat MUST requirements as release gates. A phase is an implementation order, not permission to omit later mandatory phases.
- `ACCOUNTING.md` owns calculation semantics; `API_PROVIDERS.md` owns provider routing; `TESTING.md` owns evidence requirements. Requirements in these files are part of the specification.
- Use current official API schemas. Read the linked documentation before writing adapters; record endpoint changes instead of guessing parameters.
- Start with a short environment report: operating system, toolchains, network access, and credential availability by name only. Never print credential values.
- Complete all work that the environment supports. If Windows execution or a required API credential is unavailable, finish offline work and report the exact blocked validation. Never claim a skipped check passed.
- Do not purchase services, enable paid overages, create funded blockchain accounts, or request wallet secrets. Mainnet read-only queries are sufficient for live tests.
- Do not silently reduce chain coverage or replace portfolio return with price change. Explain a concrete provider limitation and expose it in the application.
- Keep demo data in a separate demo database. Never mix it with real portfolios or use it to conceal an integration failure.
- Deliver source, lockfiles, migrations, tests, Windows installer, screenshots, a sanitized test report, and instructions for entering the user's own API keys. No keys belong in the archive or installer.

## Defaults already decided

Personal/local use; Windows 11 x64 first; no application server or login; USD valuation; dark theme initially with light/system options; English and Russian UI; FIFO position accounting; explicit treatment of missing basis; local database and caches; incremental polling; no transaction signing/broadcasting.

Use the temporary neutral product name **Portfolio Desk**. The application must have its own identity, not Ledger branding. The user may rename it later without changing the architecture.
