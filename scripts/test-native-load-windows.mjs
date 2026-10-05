// Production installed EXE + native WebView2. Run only in a disposable CI user.
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { cpus, release, totalmem } from "node:os";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { DatabaseSync } from "node:sqlite";
import { NativeWebDriver, until } from "./lib/native-webdriver.mjs";

const output = resolve("target/native-load-report");
mkdirSync(output, { recursive: true });
const report = {
  mode: "windows-production-native-load",
  at: new Date().toISOString(),
  sourceSha: process.env.ACCEPTANCE_SOURCE_SHA ?? null,
  ciSha: spawnSync("git", ["rev-parse", "HEAD"], { encoding: "utf8" }).stdout.trim(),
  buildMode: "production-release",
  checks: [],
  startupTargetMs: 2000,
  environment: {
    os: process.platform,
    osVersion: release(),
    node: process.version,
    cpu: cpus()[0]?.model,
    logicalCpus: cpus().length,
    totalMemoryBytes: totalmem(),
  },
  limits: [
    "Synthetic 50-account/500-asset/100000-leg normalized SQL fixture; not a production provider import.",
    "Cold application processes; operating-system disk/page caches are not flushed.",
    "First sample uses a fresh WebView2 data folder; later samples retain that browser cache.",
    "First useful paint means cached balance and navigation, after two animation frames; not all charts/rows finished.",
    "Closing a pending CSV preview acknowledges cancellation immediately; background calculation can finish before its staging is discarded.",
    "Production binary uses only external documented WebView2 environment options for driver attachment; no native-e2e feature.",
  ],
};
function record(name, ok, detail = "") {
  const result = ok ? "PASS" : "FAIL";
  report.checks.push({ name, result, detail });
  console.log(`${result} ${name}${detail ? `: ${detail}` : ""}`);
}
function save() {
  writeFileSync(join(output, "NATIVE_LOAD_REPORT.json"), JSON.stringify(report, null, 2) + "\n");
}
if (process.platform !== "win32" || !process.env.CI || !process.env.RUNNER_TEMP) {
  report.checks.push({
    name: "Disposable Windows CI user profile",
    result: "BLOCKED",
    detail:
      "Production data path requires an isolated Windows CI machine; no existing user portfolio is touched.",
  });
  save();
  process.exit(1);
}
const application = resolve(process.env.E2E_APP_PATH ?? "");
const profile = join(process.env.APPDATA, "com.coincontrol.portfoliodesk/profiles/real.sqlite");
const fixture = JSON.parse(readFileSync(join(output, "NATIVE_FIXTURE.json"), "utf8"));
report.dataset = fixture.dataset;
const native = new NativeWebDriver({
  application,
  output,
  webviewDirectory: join(process.env.RUNNER_TEMP, "coincontrol-load-webview"),
});
function read(sql) {
  const db = new DatabaseSync(profile, { readOnly: true });
  try {
    return db.prepare(sql).all();
  } finally {
    db.close();
  }
}
function counts() {
  return read(
    "SELECT (SELECT COUNT(*) FROM accounts) accounts, (SELECT COUNT(*) FROM assets) assets, (SELECT COUNT(*) FROM activity_legs) legs, (SELECT COUNT(*) FROM accounting_overrides) overrides, (SELECT COUNT(*) FROM import_batches WHERE status='preview') previews, (SELECT COUNT(*) FROM import_batches WHERE status='rolled_back') discarded, (SELECT COUNT(*) FROM import_batches WHERE status='committed') committed",
  )[0];
}
async function ready() {
  await until(() =>
    native.execute(
      "return document.querySelector('.balance')?.textContent.trim()==='$50,000.00' && performance.getEntriesByName('portfolio-first-useful').length===1;",
    ),
  );
}
async function startupSample(index) {
  await native.open();
  await ready();
  const measured = await native.execute(
    "const mark=performance.getEntriesByName('portfolio-first-useful')[0];return {navigationEpochMs:performance.timeOrigin,usefulFrameMs:mark.startTime,measuredEpochMs:performance.timeOrigin+mark.startTime,pixelRatio:devicePixelRatio,width:innerWidth,height:innerHeight};",
  );
  const processStartUtcMs = Number(
    native.powershell(
      "$p=Get-Process -Id ([int]$env:COINCONTROL_APP_PID);([DateTimeOffset]($p.StartTime.ToUniversalTime())).ToUnixTimeMilliseconds()",
      { COINCONTROL_APP_PID: String(native.app.pid) },
    ),
  );
  const processToUsefulMs = measured.measuredEpochMs - processStartUtcMs;
  if (!Number.isFinite(processToUsefulMs) || processToUsefulMs < 0)
    throw new Error("Invalid Windows process/renderer timestamp measurement");
  return {
    index,
    ...measured,
    processStartUtcMs,
    spawnRequestedUtcMs: native.spawnRequestedUtcMs,
    processToUsefulMs,
    browserCache: index === 0 ? "fresh" : "retained",
  };
}
async function beginPreview() {
  await native.clickText("Import cost basis CSV");
  await until(() =>
    native.execute("return Boolean(document.querySelector('.dialog input[type=file]'));"),
  );
  await native.upload(".dialog input[type=file]", join(output, "native-basis.csv"));
}
try {
  if (!existsSync(application) || application.includes("native-e2e"))
    throw new Error("Installed production binary is required");
  const releaseReport = JSON.parse(
    readFileSync("target/native-load-production/RELEASE_REPORT.json", "utf8").replace(
      /^\uFEFF/,
      "",
    ),
  );
  report.applicationSha256 = createHash("sha256").update(readFileSync(application)).digest("hex");
  if (
    !releaseReport.checks.some(
      (c) =>
        c.result === "PASS" &&
        c.name.startsWith("PE artifact:") &&
        c.detail === `sha256 ${report.applicationSha256}`,
    )
  )
    throw new Error("Installed binary does not match inspected production artifact");
  record("Installed production binary matches inspected release hash", true);
  const initial = counts();
  if (
    initial.accounts !== 50 ||
    initial.assets !== 500 ||
    initial.legs !== 100000 ||
    initial.overrides !== 0
  )
    throw new Error("Native load fixture does not match the specified dataset");
  record("Real profile has exactly 50 accounts, 500 assets and 100000 legs", true);
  native.firewall();
  await native.startDriver();
  report.startupSamples = [];
  for (let i = 0; i < 3; i++) {
    report.startupSamples.push(await startupSample(i));
    if (i < 2) await native.stopApplication();
  }
  const worstStartupMs = Math.max(...report.startupSamples.map((s) => s.processToUsefulMs));
  record(
    "Cached first useful native screen within 2 seconds in all three process launches",
    worstStartupMs <= 2000,
    `${worstStartupMs.toFixed(2)} ms maximum`,
  );
  await until(() =>
    native.execute(
      "const b=document.querySelector('tbody[data-row-count=\"500\"]');return b?.dataset.windowed==='true' && b.querySelectorAll('tr[data-index]').length<80;",
    ),
  );
  await native.execute(
    "const table=document.querySelector('.table-windowed');table.scrollTop=table.scrollHeight;return true;",
  );
  await until(() =>
    native.execute("return Boolean(document.querySelector('tr[data-index=\"499\"]'));"),
  );
  record("All 500 native holdings remain scrollable with fewer than 80 mounted rows", true);
  await native.screenshot("load-assets-500-windowed");
  await native.screenshot("load-portfolio");
  await native.execute(
    "window.__nativeFrameGaps=[];let previous=performance.now();function frame(now){window.__nativeFrameGaps.push(now-previous);previous=now;if(window.__nativeFrameGaps.length<20000)requestAnimationFrame(frame)}requestAnimationFrame(frame);return true;",
  );
  await native.route("/review");
  await until(() =>
    native.execute("return document.querySelectorAll('.table tbody tr').length>0;"),
  );
  await beginPreview();
  await until(async () => (await native.body()).includes("Reading the file…"));
  const cancellationStarted = Date.now();
  await native.clickText("Close");
  await until(() => native.execute("return !document.querySelector('.dialog');"), 1000);
  report.previewCancellationFeedbackMs = Date.now() - cancellationStarted;
  record(
    "Large-dataset pending CSV preview closes within one second",
    report.previewCancellationFeedbackMs <= 1000,
    `${report.previewCancellationFeedbackMs} ms`,
  );
  await native.route("/activity");
  await until(() =>
    native.execute("return document.querySelectorAll('.table tbody tr').length===50;"),
  );
  await native.clickText("Load more");
  await until(() =>
    native.execute("return document.querySelectorAll('.table tbody tr').length===100;"),
  );
  record("Activity renders two 50-row pages while background preview calculates", true);
  await native.screenshot("load-activity-100");
  for (let page = 0; page < 2; page++) {
    await native.clickText("Load more");
    await until(() =>
      native.execute(
        "return document.querySelector('tbody')?.dataset.rowCount===String(arguments[0]);",
        [150 + page * 50],
      ),
    );
  }
  await until(() =>
    native.execute(
      "const body=document.querySelector('tbody[data-windowed=true]');return body?.dataset.rowCount==='200' && body.querySelectorAll('tr[data-index]').length<80;",
    ),
  );
  await native.execute(
    "const table=document.querySelector('.table-windowed');table.focus();table.scrollTop=table.scrollHeight;return true;",
  );
  await until(() =>
    native.execute("return Boolean(document.querySelector('tr[data-index=\"199\"]'));"),
  );
  record("Virtualized native activity reaches row 200 with fewer than 80 mounted rows", true);
  await native.screenshot("load-activity-200-windowed");
  await until(() => counts().discarded === 1, 180000);
  const cancelled = counts();
  if (
    cancelled.previews !== 0 ||
    cancelled.committed !== 0 ||
    cancelled.overrides !== 0 ||
    cancelled.legs !== 100000
  )
    throw new Error("Closed CSV preview changed the ledger or retained pending staging");
  record("Late CSV preview is discarded without applying decisions or duplicating history", true);
  await native.route("/review");
  await until(() =>
    native.execute("return Boolean(document.querySelector('.toolbar button.btn:not(.btn-icon)'));"),
  );
  const previewStarted = Date.now();
  await beginPreview();
  await until(async () => (await native.body()).includes("Ready: 1 · errors: 0"), 180000);
  report.previewMs = Date.now() - previewStarted;
  await native.screenshot("load-import-preview");
  const commitStarted = Date.now();
  await native.clickText("Apply 1 row");
  await until(() =>
    native.execute(
      "return document.querySelector('.dialog .drawer-head button')?.disabled===true;",
    ),
  );
  await until(async () => (await native.body()).includes("Applied 1 row."), 180000);
  report.commitAndReplayMs = Date.now() - commitStarted;
  const applied = counts();
  if (
    applied.committed !== 1 ||
    applied.overrides !== 1 ||
    applied.previews !== 0 ||
    applied.legs !== 100000
  )
    throw new Error("Native CSV import did not commit exactly once over existing history");
  const gaps = await native.execute("return window.__nativeFrameGaps;");
  const sorted = gaps.slice().sort((a, b) => a - b);
  report.rendererFrameGaps = {
    samples: gaps.length,
    p95Ms: sorted[Math.ceil(sorted.length * 0.95) - 1],
    maxMs: Math.max(...gaps),
    scope: "Review/Activity navigation, cancelled preview, completed preview and commit/replay",
  };
  record(
    "Native renderer continues presenting frames during large-dataset preview/replay",
    gaps.length > 100 && report.rendererFrameGaps.maxMs <= 1000,
    `${report.rendererFrameGaps.maxMs.toFixed(2)} ms maximum frame gap`,
  );
  record(
    "CSV basis decision commits through native UI and replays all 100000 legs",
    true,
    `${report.commitAndReplayMs} ms commit/replay; one imported row`,
  );
  await native.screenshot("load-import-applied");
  await native.clickText("Close");
  report.applicationMemory = JSON.parse(
    native.powershell(
      "$p=Get-Process -Id ([int]$env:COINCONTROL_APP_PID);@{workingSetBytes=$p.WorkingSet64;peakWorkingSetBytes=$p.PeakWorkingSet64;scope='Application process only; WebView2 child processes are excluded'}|ConvertTo-Json -Compress",
      { COINCONTROL_APP_PID: String(native.app.pid) },
    ),
  );
  await native.stopApplication();
  await native.open();
  await ready();
  const reopened = counts();
  if (JSON.stringify(reopened) !== JSON.stringify(applied))
    throw new Error("Restart changed committed import or history counts");
  if (read("PRAGMA integrity_check")[0].integrity_check !== "ok")
    throw new Error("Native profile integrity check failed");
  record("Large portfolio and exact import/history counts survive process restart", true);
  report.finalCounts = reopened;
  report.providerUsage = read(
    "SELECT provider,SUM(requests) requests FROM provider_usage GROUP BY provider",
  );
  await native.screenshot("load-reopened");
} catch (e) {
  try {
    await native.screenshot("failure");
  } catch {
    /* Preserve a missing-window failure. */
  }
  record(
    "Production native load execution",
    false,
    e instanceof Error ? e.message : "Unknown failure",
  );
} finally {
  try {
    native.firewall(true);
  } catch (e) {
    record("Remove isolated firewall rule", false, e.message);
  }
  await native.dispose();
  save();
}
process.exit(report.checks.some((c) => c.result !== "PASS") ? 1 : 0);
