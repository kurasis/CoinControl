// Inspect production configuration, built resources and Windows release artifacts.
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { spawnSync } from "node:child_process";

const sourceOnly = process.argv.includes("--source-only");
const checks = [];
function check(name, ok, detail = "") {
  checks.push({ name, result: ok ? "PASS" : "FAIL", detail });
}
function files(dir) {
  return existsSync(dir)
    ? readdirSync(dir).flatMap((n) => {
        const p = join(dir, n);
        return statSync(p).isDirectory() ? files(p) : [p];
      })
    : [];
}
const config = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8"));
const capability = JSON.parse(readFileSync("src-tauri/capabilities/main.json", "utf8"));
const build = readFileSync("src-tauri/build.rs", "utf8");
const declared = [...build.matchAll(/^\s*"([a-z_]+)",/gm)].map((m) => m[1]);
const handler = readFileSync("src-tauri/src/lib.rs", "utf8");
const handlers = [...handler.matchAll(/commands::([a-z_]+),/g)].map((m) => m[1]);
const granted = capability.permissions
  .filter((p) => typeof p === "string" && p.startsWith("allow-"))
  .map((p) => p.slice(6).replaceAll("-", "_"));
check(
  "Every application command is declared and scoped",
  declared.length === handlers.length &&
    handlers.every((c) => declared.includes(c)) &&
    granted.length === declared.length &&
    declared.every((c) => granted.includes(c)),
);
check(
  "No generic privileged command",
  declared.every((c) => !/(^|_)(sql|shell|exec|rpc|broadcast|sign|url|http)(_|$)/.test(c)),
);
check(
  "Production CSP limits networking to IPC",
  config.app.security.csp.includes("connect-src ipc: http://ipc.localhost;") &&
    !/unsafe-eval|unsafe-inline|https:|ws:|\*/.test(config.app.security.csp),
);
check(
  "Production build uses packaged resources",
  config.build.frontendDist === "../dist" && !config.app.withGlobalTauri,
);
const resources = files("dist");
check(
  "Frontend build exists",
  resources.some((p) => p.endsWith("index.html")),
);
check(
  "No private resource files",
  resources.every(
    (p) =>
      !/(^|[\\/])(\.env[^\\/]*|secrets|private-test-targets\.json)([\\/]|$)|\.(key|p12|pfx|sqlite|db|ccbackup)$/i.test(
        p,
      ),
  ),
);
const sentinels = [
  "CC_RELEASE_SENTINEL_SECRET_74fcbf7c",
  "COINCONTROL_E2E_DATA_DIR",
  "COINCONTROL_E2E_DEBUG_PORT",
  "(browser preview: nothing is stored)",
];
const artifacts = files("target/release/bundle/nsis").filter((p) => p.endsWith(".exe"));
const application = join("target", "release", "portfolio-desk.exe");
if (!sourceOnly) {
  check("Windows release application exists", existsSync(application));
  check("NSIS installer exists", artifacts.length > 0);
  for (const path of [application, ...artifacts].filter(existsSync)) {
    const bytes = readFileSync(path);
    check(
      `PE artifact: ${path}`,
      bytes.length > 1024 && bytes.subarray(0, 2).toString() === "MZ",
      `sha256 ${createHash("sha256").update(bytes).digest("hex")}`,
    );
  }
  for (const path of artifacts) {
    const listed = spawnSync("7z", ["l", path], { encoding: "utf8" });
    check(
      `Installer contents: ${path}`,
      listed.status === 0 &&
        !/(?:^|[\\/ ])(?:\.env(?:\.[\w.-]+)?|private-test-targets\.json|[^\s]+\.(?:key|p12|pfx|ccbackup))\s*$/im.test(
          listed.stdout ?? "",
        ),
      listed.status === 0 ? "No credential or backup files" : "7z listing unavailable",
    );
  }
}
for (const path of [
  ...resources,
  ...(!sourceOnly ? [application, ...artifacts].filter(existsSync) : []),
]) {
  const bytes = readFileSync(path);
  check(
    `No sentinels or test-only code: ${path}`,
    sentinels.every(
      (s) => !bytes.includes(Buffer.from(s)) && !bytes.includes(Buffer.from(s, "utf16le")),
    ),
  );
}
mkdirSync("target/release-report", { recursive: true });
const report = {
  mode: sourceOnly ? "source-and-frontend" : "windows-release",
  at: new Date().toISOString(),
  signed: false,
  checks,
};
writeFileSync("target/release-report/RELEASE_REPORT.json", JSON.stringify(report, null, 2) + "\n");
for (const c of checks) console.log(`${c.result} ${c.name}${c.detail ? `: ${c.detail}` : ""}`);
process.exit(checks.every((c) => c.result === "PASS") ? 0 : 1);
