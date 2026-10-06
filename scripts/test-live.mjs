// npm run test:live: real read-only provider requests (TESTING.md Layer B).
// Requires RUN_LIVE_API_TESTS=1 and LIVE_TEST_PROVIDERS. Credentials come from the
// process environment or an explicitly loaded .env.test.local; values are never printed.
// Writes a sanitized report to target/live-report/LIVE_REPORT.md.
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";

const KNOWN = {
  livecoinwatch: "LIVECOINWATCH_API_KEY",
  zerion: "ZERION_API_KEY",
  helius: "HELIUS_API_KEY",
  alchemy: "ALCHEMY_API_KEY",
  esplora: null,
  mempool: null,
  publicnode: null,
  blockscout: "BLOCKSCOUT_API_KEY",
  drpc: "DRPC_API_KEY",
  chainstack: "CHAINSTACK_API_KEY",
  toncenter: "TONCENTER_API_KEY",
  etherscan: "ETHERSCAN_API_KEY",
  defillama: null,
  trongrid: "TRONGRID_API_KEY",
  tonapi: "TONAPI_API_KEY",
};
// Optional provider not shipped in this build.
const LATER = {
  ankr: "ANKR_API_TOKEN",
};
const REPORT_DIR = "target/live-report";

if (existsSync(".env.test.local")) {
  for (const line of readFileSync(".env.test.local", "utf8").split(/\r?\n/)) {
    const m = /^([A-Z0-9_]+)=(.*)$/.exec(line.trim());
    if (m && process.env[m[1]] === undefined) process.env[m[1]] = m[2];
  }
  console.log("Loaded .env.test.local (values not shown).");
}

if (process.env.RUN_LIVE_API_TESTS !== "1") {
  console.error(
    "Live tests are opt-in: set RUN_LIVE_API_TESTS=1 and LIVE_TEST_PROVIDERS=<comma list>.",
  );
  process.exit(2);
}
const selected = (process.env.LIVE_TEST_PROVIDERS ?? "")
  .split(",")
  .map((s) => s.trim())
  .filter(Boolean);
if (selected.length === 0) {
  console.error("LIVE_TEST_PROVIDERS is empty; select providers explicitly.");
  process.exit(2);
}

let missing = false;
const blocked = [];
for (const p of selected) {
  if (p in LATER) {
    blocked.push(p);
    continue;
  }
  if (!(p in KNOWN)) {
    console.error(`Unknown provider: ${p}`);
    process.exit(2);
  }
  const key = KNOWN[p];
  const configured = key === null ? "not required" : process.env[key] ? "configured" : "MISSING";
  if (configured === "MISSING") missing = true;
  console.log(`${p.padEnd(14)} credential ${key ?? "-"}: ${configured}`);
}
for (const p of blocked)
  console.log(`${p.padEnd(14)} SKIPPED_NOT_IN_SCOPE: optional provider not used by this build`);
if (missing) {
  console.error("A selected provider has no credential configured; refusing to run.");
  process.exit(1);
}
const runnable = selected.filter((p) => p in KNOWN);
if (runnable.length === 0) process.exit(3);

rmSync(REPORT_DIR, { recursive: true, force: true });
mkdirSync(REPORT_DIR, { recursive: true });
const cargo = spawnSync(
  "cargo",
  ["test", "-p", "portfolio-providers", "--test", "live", "--", "--test-threads=1", "--nocapture"],
  {
    stdio: "inherit",
    shell: process.platform === "win32",
    env: { ...process.env, LIVE_TEST_PROVIDERS: runnable.join(",") },
  },
);

const rows = [];
let failed = cargo.status !== 0;
const expected = [
  ...runnable,
  ...(runnable.includes("publicnode") ? ["mirror-routing"] : []),
  ...(runnable.includes("esplora") ? ["vertical-slice"] : []),
  ...(["zerion", "trongrid", "tonapi"].every((p) => runnable.includes(p)) ? ["networks"] : []),
];
for (const name of expected) {
  const file = `${REPORT_DIR}/${name}.json`;
  if (!existsSync(file)) {
    failed = true;
    rows.push(`| ${name} | FAIL | - | - | no report (the test aborted; see output above) |`);
    continue;
  }
  const r = JSON.parse(readFileSync(file, "utf8"));
  const ok = r.checks.every((c) => c.result === "PASS");
  if (!ok) failed = true;
  rows.push(
    `| ${r.provider} | ${ok ? "PASS" : "FAIL"} | ${r.requests} | ${r.duration_ms} | ${[...r.endpoints].join(", ")} |`,
  );
  for (const c of r.checks) rows.push(`| | ${c.result} | | | ${c.name}: ${c.detail} |`);
}
for (const p of blocked)
  rows.push(`| ${p} | SKIPPED_NOT_IN_SCOPE | 0 | - | optional provider not used by this build |`);

const usageFile = `${REPORT_DIR}/usage.json`;
if (existsSync(usageFile)) {
  const usage = JSON.parse(readFileSync(usageFile, "utf8"));
  const ceiling = Number(process.env.LIVE_TEST_MAX_REQUESTS_PER_PROVIDER ?? 50);
  for (const [provider, requests] of Object.entries(usage)) {
    if (requests > ceiling) failed = true;
    rows.push(
      `| ${provider} total across suites | ${requests <= ceiling ? "PASS" : "FAIL"} | ${requests} | - | shared budget ceiling ${ceiling} |`,
    );
  }
}
const report = [
  "# Live provider test report",
  "",
  `Run at ${new Date().toISOString()}. Requests are counted locally, retries included.`,
  "Credentials, full URLs and response bodies are never recorded.",
  "",
  "| Provider | Result | Requests | Duration (ms) | Endpoints / check |",
  "| --- | --- | --- | --- | --- |",
  ...rows,
  "",
].join("\n");
writeFileSync(`${REPORT_DIR}/LIVE_REPORT.md`, report);
console.log(`\n${report}`);
process.exit(failed ? 1 : 0);
