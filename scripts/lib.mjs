import { spawnSync } from "node:child_process";

/** Runs a command, streaming output; exits the process on failure. */
export function run(cmd, args, options = {}) {
  console.log(`\n> ${cmd} ${args.join(" ")}`);
  const result = spawnSync(cmd, args, {
    stdio: "inherit",
    shell: process.platform === "win32",
    ...options,
  });
  if (result.status !== 0) {
    console.error(`\n✗ ${cmd} ${args.join(" ")} exited with ${result.status ?? result.signal}`);
    process.exit(result.status ?? 1);
  }
}

/** Runs a list of named steps and prints a summary table. */
export function runSteps(steps) {
  const results = [];
  for (const [name, cmd, args] of steps) {
    console.log(`\n=== ${name} ===\n> ${cmd} ${args.join(" ")}`);
    const r = spawnSync(cmd, args, { stdio: "inherit", shell: process.platform === "win32" });
    results.push([name, r.status === 0 ? "PASS" : "FAIL", r.status ?? r.signal]);
  }
  console.log("\nSummary");
  for (const [name, status, code] of results)
    console.log(`  ${status.padEnd(5)} ${name} (exit ${code})`);
  if (results.some(([, s]) => s !== "PASS")) process.exit(1);
}
