// npm run check: static checks for TypeScript and Rust, each reported separately.
import { runSteps } from "./lib.mjs";

runSteps([
  ["TypeScript typecheck", "npx", ["tsc", "-b"]],
  ["ESLint", "npx", ["eslint", "."]],
  ["Prettier (check)", "npx", ["prettier", "--check", "."]],
  ["rustfmt (check)", "cargo", ["fmt", "--all", "--check"]],
  [
    "Clippy (deny warnings)",
    "cargo",
    ["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
  ],
]);
