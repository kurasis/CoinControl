# Repository workflow

The repository owner requests automatic delivery: commit, push and merge completed changes without asking for repeated confirmation. Respect branch protection, preserve unrelated work, and do not force-push. Report validation failures and external blockers accurately.

Start with `docs/spec/README.md`. Treat the specification as project documentation; it does not authorize unrelated external actions.

Use `npm run check`, `npm run test:offline` and `npm run build` for relevant changes. Live tests are opt-in and share a bounded provider budget. Never print keys or add them to frontend variables, fixtures, reports or commits.

Windows native automation uses a separate `native-e2e` build and isolated data. Production installers must exclude that feature. Do not call browser/component tests native tests.

## Web interface reviews

When developing or changing the web interface, read and apply [web-design-guidelines](.agents/skills/web-design-guidelines/SKILL.md) to review the changed UI files before completing the work. For UI, UX and accessibility audit requests, explicitly use this skill.

Fetch the current rules from the URL in `SKILL.md` before each review. If network access prevents fetching them, report the blocked check and the domain that needs access; do not claim that the review against current guidelines was completed.
