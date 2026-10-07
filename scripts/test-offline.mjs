// npm run test:offline: deterministic tests with no external network access.
// Live provider tests are separate (npm run test:live) and never spend quota here.
import { runSteps } from "./lib.mjs";

process.env.RUN_LIVE_API_TESTS = "0";
runSteps([
  [
    "Live report/fallback evidence policy",
    "node",
    ["--test", "scripts/live-results.node-test.mjs"],
  ],
  ["Rust domain, accounting and storage tests", "cargo", ["test", "--workspace", "--all-targets"]],
  ["Frontend component and formatting tests", "npx", ["vitest", "run"]],
]);
