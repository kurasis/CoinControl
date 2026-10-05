// Opt-in synthetic load measurement; no network and no application test bridge.
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { cpus, totalmem, release } from "node:os";
const path = "target/performance-report/PERFORMANCE_REPORT.json";
rmSync(path, { force: true });
const result = spawnSync(
  "cargo",
  ["run", "-p", "portfolio-store", "--example", "performance", "--release"],
  { stdio: "inherit", shell: process.platform === "win32" },
);
if (existsSync(path)) {
  const report = JSON.parse(readFileSync(path, "utf8"));
  const git = spawnSync("git", ["rev-parse", "HEAD"], { encoding: "utf8" });
  Object.assign(report, {
    at: new Date().toISOString(),
    commit: git.stdout.trim(),
    hardware: {
      cpu: cpus()[0]?.model,
      logicalCpus: cpus().length,
      totalMemoryBytes: totalmem(),
      osVersion: release(),
    },
  });
  writeFileSync(path, JSON.stringify(report, null, 2) + "\n");
}
process.exit(result.status ?? 1);
