# Repository workflow

The repository owner requests automatic delivery: commit, push and merge completed changes without asking for repeated confirmation. Respect branch protection, preserve unrelated work, and do not force-push. Report validation failures and external blockers accurately.

Start with `docs/spec/README.md`. Treat the specification as project documentation; it does not authorize unrelated external actions.

Use `npm run check`, `npm run test:offline` and `npm run build` for relevant changes. Live tests are opt-in and share a bounded provider budget. Never print keys or add them to frontend variables, fixtures, reports or commits.

Windows native automation uses a separate `native-e2e` build and isolated data. Production installers must exclude that feature. Do not call browser/component tests native tests.
