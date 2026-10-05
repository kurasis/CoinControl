// Native WebDriver suite. DOM interactions use the real packaged UI and Rust IPC.
// It never installs a test server or an IPC bypass in the application.
import { spawn, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir, release, arch } from "node:os";
import { join, resolve } from "node:path";
import { createHash } from "node:crypto";
import { DatabaseSync } from "node:sqlite";
import { ownedFixture, seedOwnedFixture, acceptanceSnapshot } from "./acceptance-fixture.mjs";

const output = "target/native-report";
mkdirSync(output, { recursive: true });
const report = { mode: "native-windows", at: new Date().toISOString(), checks: [] };
report.environment = {
  os: process.platform,
  osVersion: release(),
  arch: arch(),
  node: process.version,
};
function record(name, result, detail = "") {
  report.checks.push({ name, result, detail });
  console.log(`${result} ${name}${detail ? `: ${detail}` : ""}`);
}
function save() {
  writeFileSync(join(output, "NATIVE_REPORT.json"), JSON.stringify(report, null, 2) + "\n");
}
if (process.platform !== "win32") {
  record(
    "Windows native runner",
    "BLOCKED",
    `Requires Windows; current platform ${process.platform}`,
  );
  save();
  process.exit(1);
}
const application = resolve(
  process.env.E2E_APP_PATH ?? "target/native-e2e/debug/portfolio-desk.exe",
);
if (!existsSync(application) || !application.includes("native-e2e")) {
  record(
    "Isolated native test binary",
    "BLOCKED",
    "Build with --features native-e2e --target-dir target/native-e2e",
  );
  save();
  process.exit(1);
}
report.applicationSha256 = createHash("sha256").update(readFileSync(application)).digest("hex");
let dataDir = mkdtempSync(join(tmpdir(), "coincontrol-native-"));
const liveBtc = process.argv.includes("--live-btc");
if (liveBtc && process.env.RUN_LIVE_API_TESTS !== "1") {
  record("Native live opt-in", "BLOCKED", "--live-btc requires RUN_LIVE_API_TESTS=1");
  save();
  process.exit(1);
}
const port = Number(process.env.E2E_DRIVER_PORT ?? 4444);
const base = `http://127.0.0.1:${port}`;
const driverEnvironment = { ...process.env, COINCONTROL_E2E_DATA_DIR: dataDir };
for (const key of ["LIVECOINWATCH_API_KEY", "ZERION_API_KEY", "TRONGRID_API_KEY", "TONAPI_API_KEY"])
  delete driverEnvironment[key];
const driver = spawn(
  process.env.TAURI_DRIVER_PATH ?? "tauri-driver",
  [
    "--port",
    String(port),
    ...(process.env.EDGE_WEBDRIVER_PATH
      ? ["--native-driver", process.env.EDGE_WEBDRIVER_PATH]
      : []),
  ],
  {
    env: driverEnvironment,
    stdio: ["ignore", "pipe", "pipe"],
  },
);
for (const [name, command] of [
  ["tauriDriver", process.env.TAURI_DRIVER_PATH ?? "tauri-driver"],
  ["webViewMatchingDriver", process.env.EDGE_WEBDRIVER_PATH],
]) {
  if (command)
    report.environment[name] = spawnSync(command, ["--version"], {
      encoding: "utf8",
      env: driverEnvironment,
    }).stdout.trim();
}
let driverLog = "";
for (const stream of [driver.stdout, driver.stderr])
  stream.on("data", (chunk) => {
    driverLog = (driverLog + chunk.toString()).slice(-200000);
    writeFileSync(join(output, "driver.log"), driverLog);
  });
let driverError;
driver.on("error", (e) => {
  driverError = e;
});
let session;
let nativeApp;
let applicationLog = "";
const debugPort = port + 2;
const delay = (ms) => new Promise((r) => setTimeout(r, ms));
async function request(path, method = "GET", body) {
  const response = await fetch(base + path, {
    method,
    headers: { "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(path === "/session" ? 120000 : 30000),
  });
  const json = await response.json();
  if (!response.ok || json.value?.error)
    throw new Error(
      `WebDriver ${method} ${path}: ${json.value?.error ?? response.status}: ${String(json.value?.message ?? "").slice(0, 600)}`,
    );
  return json.value;
}
async function until(test, timeout = 20000) {
  const started = Date.now();
  while (Date.now() - started < timeout) {
    if (await test()) return;
    await delay(200);
  }
  throw new Error("Timed out waiting for native UI");
}
const execute = (script, args = []) =>
  request(`/session/${session}/execute/sync`, "POST", { script, args });
const body = () => execute("return document.body.innerText;");
const route = async (path) => {
  await execute("window.location.hash=arguments[0];return true;", ["#" + path]);
  await delay(300);
};
async function clickText(text) {
  await until(() =>
    execute(
      "const el=[...document.querySelectorAll('button,a,label')].find(e=>e.textContent.trim()===arguments[0] || e.getAttribute('aria-label')===arguments[0]); if(!el || el.disabled)return false;el.click();return true;",
      [text],
    ),
  );
}
async function select(css, value) {
  await execute(
    "const e=document.querySelector(arguments[0]);if(!e)throw Error('missing select');e.value=arguments[1];e.dispatchEvent(new Event('change',{bubbles:true}));return true;",
    [css, value],
  );
}
async function input(css, value) {
  await execute(
    "const e=document.querySelector(arguments[0]);if(!e)throw Error('missing input');const prototype=e.tagName==='TEXTAREA'?HTMLTextAreaElement.prototype:HTMLInputElement.prototype;Object.getOwnPropertyDescriptor(prototype,'value').set.call(e,arguments[1]);e.dispatchEvent(new Event('input',{bubbles:true}));return true;",
    [css, value],
  );
}
function closeApplication() {
  if (nativeApp) {
    spawnSync("taskkill.exe", ["/PID", String(nativeApp.pid), "/T", "/F"], { stdio: "ignore" });
    nativeApp = undefined;
  }
}
async function open() {
  // Microsoft supports attaching WebDriver to an explicitly started WebView2.
  // This avoids the driver's DevToolsActivePort launch-file handshake.
  nativeApp = spawn(application, [], {
    env: {
      ...driverEnvironment,
      COINCONTROL_E2E_DATA_DIR: dataDir,
      TAURI_WEBVIEW_AUTOMATION: "true",
      COINCONTROL_E2E_DEBUG_PORT: String(debugPort),
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${debugPort}`,
      WEBVIEW2_USER_DATA_FOLDER: join(dataDir, "webview"),
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  let startupError;
  nativeApp.on("error", (e) => {
    startupError = e.message;
  });
  nativeApp.on("exit", (code) => {
    startupError = `Application exited (${code}): ${applicationLog.slice(-1500)}`;
  });
  for (const stream of [nativeApp.stdout, nativeApp.stderr])
    stream.on("data", (chunk) => {
      applicationLog = (applicationLog + chunk.toString()).slice(-100000);
      writeFileSync(join(output, "application.log"), applicationLog);
    });
  await until(async () => {
    if (startupError) throw new Error(startupError);
    try {
      const response = await fetch(`http://127.0.0.1:${debugPort}/json/version`, {
        signal: AbortSignal.timeout(2000),
      });
      return response.ok;
    } catch {
      return false;
    }
  }, 45000);
  const s = await request("/session", "POST", {
    capabilities: {
      alwaysMatch: {
        browserName: "webview2",
        "ms:edgeChromium": true,
        "ms:edgeOptions": { debuggerAddress: `127.0.0.1:${debugPort}` },
      },
    },
  });
  session = s.sessionId;
  await until(() => execute("return Boolean(document.querySelector('h1'));"));
  if (!(await execute("return Boolean(window.__TAURI_INTERNALS__);")))
    throw new Error("Native IPC bridge missing");
  if ((await body()).includes("Browser preview")) throw new Error("Browser mock detected");
}
async function screenshot(name) {
  // Scrolling updates DOM geometry before WebView2 presents its next frame.
  // Let the compositor and the UI's <=180 ms transitions finish first.
  await delay(250);
  const b64 = await request(`/session/${session}/screenshot`);
  writeFileSync(join(output, name + ".png"), Buffer.from(b64, "base64"));
}
async function restart() {
  await request(`/session/${session}`, "DELETE");
  session = undefined;
  closeApplication();
  await delay(1000);
  await open();
}
async function saveNativeFile(button, destination) {
  const helper = spawn(
    "powershell.exe",
    [
      "-NoProfile",
      "-STA",
      "-File",
      resolve("scripts/native-save-dialog.ps1"),
      "-ApplicationPid",
      String(nativeApp.pid),
      "-Destination",
      destination,
    ],
    { stdio: ["ignore", "pipe", "pipe"] },
  );
  let log = "";
  for (const stream of [helper.stdout, helper.stderr])
    stream.on("data", (chunk) => {
      log += chunk.toString();
    });
  const finished = new Promise((resolve) => {
    helper.on("error", () => resolve({ code: -1 }));
    helper.on("exit", (code) => resolve({ code }));
  });
  await clickText(button);
  const { code } = await finished;
  if (code !== 0) throw new Error(`Native Save dialog failed: ${log.slice(-1000)}`);
  await until(() => existsSync(destination));
  await until(async () => (await body()).includes("File saved."));
}
function readProfile(sql, profile = "real") {
  const db = new DatabaseSync(join(dataDir, "profiles", `${profile}.sqlite`), { readOnly: true });
  try {
    return db.prepare(sql).all();
  } finally {
    db.close();
  }
}
function networkRule(remove = false) {
  const result = spawnSync(
    "powershell.exe",
    [
      "-NoProfile",
      "-Command",
      remove
        ? "Remove-NetFirewallRule -Name $env:COINCONTROL_FIREWALL_RULE -ErrorAction SilentlyContinue"
        : "New-NetFirewallRule -Name $env:COINCONTROL_FIREWALL_RULE -DisplayName 'CoinControl isolated native offline check' -Direction Outbound -Program $env:COINCONTROL_FIREWALL_APP -Action Block -Profile Any -ErrorAction Stop | Out-Null",
    ],
    {
      encoding: "utf8",
      env: {
        ...driverEnvironment,
        COINCONTROL_FIREWALL_RULE: `CoinControl-native-${process.pid}`,
        COINCONTROL_FIREWALL_APP: application,
      },
    },
  );
  if (result.status !== 0)
    throw new Error("Unable to configure the application-scoped offline firewall rule");
}
function btcSnapshot() {
  return readProfile(
    "SELECT asset_id, raw_quantity FROM balance_observations WHERE id IN (SELECT MAX(id) FROM balance_observations GROUP BY account_id,asset_id) ORDER BY asset_id",
  );
}
async function waitSync() {
  await until(
    () =>
      execute(
        "return [...document.querySelectorAll('button')].some(e=>e.textContent.trim()==='Sync now' && !e.disabled);",
      ),
    180000,
  );
}
async function ownedAccountScenarios() {
  // Addresses and membership are entered through the shipped UI. Only the
  // deterministic chain evidence is seeded externally while the app is closed.
  networkRule();
  try {
    for (let i = 0; i < 2; i++) {
      await route("/wallets?add=1");
      await until(() => execute("return Boolean(document.querySelector('textarea'));"));
      await select("form select", "new");
      await input('input[placeholder="e.g. Cold storage"]', ownedFixture.wallets[i]);
      await execute(
        "const select=[...document.querySelectorAll('form select')].find(e=>[...e.options].some(o=>o.value==='ethereum'));select.value='ethereum';select.dispatchEvent(new Event('change',{bubbles:true}));return true;",
      );
      await input("textarea", ownedFixture.addresses[i]);
      await until(() =>
        execute(
          "return Boolean(document.querySelector('form button[type=submit]:not(:disabled)'));",
        ),
      );
      await execute("document.querySelector('form').requestSubmit();return true;");
      await until(() => execute("return !document.querySelector('textarea');"));
    }
    record("Two owned synthetic accounts added through native address validation and UI", "PASS");
    await request(`/session/${session}`, "DELETE");
    session = undefined;
    closeApplication();
    seedOwnedFixture(join(dataDir, "profiles", "real.sqlite"), { groups: false });
    await open();
    await route("/wallets");
    for (const [label, members] of [
      ["Acceptance both", [0, 1]],
      ["Acceptance A only", [0]],
    ]) {
      await input('input[aria-label="New group name"]', label);
      await clickText("Create group");
      await until(() =>
        execute(
          "return [...document.querySelectorAll('#groups a')].some(e=>e.textContent===arguments[0]);",
          [label],
        ),
      );
      for (const index of members) {
        await execute(
          "const row=[...document.querySelectorAll('#groups a')].find(e=>e.textContent===arguments[0]).closest('.list-row');const label=[...row.querySelectorAll('label')].find(e=>e.textContent.trim()===arguments[1]);label.querySelector('input').click();return true;",
          [label, ownedFixture.wallets[index]],
        );
        await until(() =>
          execute(
            "const row=[...document.querySelectorAll('#groups a')].find(e=>e.textContent===arguments[0]).closest('.list-row');return [...row.querySelectorAll('label')].find(e=>e.textContent.trim()===arguments[1]).querySelector('input').checked;",
            [label, ownedFixture.wallets[index]],
          ),
        );
        await until(
          () =>
            readProfile("SELECT COUNT(*) AS n FROM group_wallets")[0].n ===
            (label === "Acceptance both" ? index + 1 : 3),
        );
      }
    }
    const scopes = readProfile("SELECT id,label FROM groups ORDER BY label");
    report.ownedFixture = {
      expectedTotalUsd: ownedFixture.totalUsd,
      expectedFeeUsd: ownedFixture.feeUsd,
      scopes: [],
    };
    for (const group of scopes) {
      await route(`/groups/${group.id}`);
      const total = group.label === "Acceptance both" ? "$5,970.00" : "$2,970.00";
      await until(() =>
        execute("return document.querySelector('.balance')?.textContent.trim()===arguments[0];", [
          total,
        ]),
      );
      await until(() =>
        execute(
          "const card=[...document.querySelectorAll('.metric')].find(e=>e.textContent.includes('Fees and expenses'));return card?.textContent.includes('$30.00') && card.textContent.includes('1 fee');",
        ),
      );
      report.ownedFixture.scopes.push({
        label: group.label,
        totalUsd: total,
        feeUsd: "30.00",
        feeCharges: 1,
      });
      await screenshot(
        group.label === "Acceptance both" ? "owned-both-group" : "owned-single-group",
      );
    }
    const snapshot = acceptanceSnapshot(join(dataDir, "profiles", "real.sqlite"));
    if (
      snapshot.ownTransferLegs !== 2 ||
      snapshot.feeCharges !== 1 ||
      snapshot.fingerprints.group_wallets.rows !== 3 ||
      snapshot.fingerprints.accounting_overrides.rows !== 2
    )
      throw new Error("Owned transfer, fee deduplication, audit or overlapping membership differs");
    writeFileSync(
      join(output, "OWNED_FIXTURE_REPORT.json"),
      JSON.stringify(snapshot, null, 2) + "\n",
    );
    await restart();
    await route(`/groups/${scopes.find((g) => g.label === "Acceptance both").id}`);
    await until(() =>
      execute("return document.querySelector('.balance')?.textContent.trim()==='$5,970.00';"),
    );
    record(
      "Overlapping native groups preserve scope totals, one fee and owned-transfer accounting across restart",
      "PASS",
      "Synthetic offline evidence: 1.99 ETH total; $5970; $30 fee exactly once",
    );
  } finally {
    networkRule(true);
  }
  dataDir = mkdtempSync(join(tmpdir(), "coincontrol-native-demo-"));
  await restart();
  await route("/settings");
  await until(() => execute("return Boolean(document.querySelector('#language'));"));
  await select("#language", "en");
  await route("/");
  await until(async () => (await body()).includes("Track your crypto, read-only"));
}

async function viewportScenarios() {
  report.viewports = [];
  for (const [width, height] of [
    [1440, 900],
    [1280, 800],
    [1024, 720],
  ]) {
    const ratio = await execute("return devicePixelRatio;");
    const result = spawnSync(
      "powershell.exe",
      [
        "-NoProfile",
        "-File",
        resolve("scripts/native-window-size.ps1"),
        "-ApplicationPid",
        String(nativeApp.pid),
        "-Width",
        String(width),
        "-Height",
        String(height),
        "-PixelRatio",
        String(ratio),
      ],
      { encoding: "utf8" },
    );
    if (result.status !== 0)
      throw new Error(`Native HWND resize failed: ${result.stderr.slice(-500)}`);
    await until(() =>
      execute(
        "return Math.abs(innerWidth-arguments[0])<=1 && Math.abs(innerHeight-arguments[1])<=1;",
        [width, height],
      ),
    );
    const native = JSON.parse(result.stdout);
    for (const language of ["en", "ru"]) {
      for (const theme of ["dark", "light"]) {
        await route("/settings");
        await until(() => execute("return Boolean(document.querySelector('#language'));"));
        await select("#language", language);
        await until(() =>
          execute("return document.documentElement.lang===arguments[0];", [language]),
        );
        await execute(
          "document.querySelectorAll('[aria-labelledby=theme-label] button')[arguments[0]==='dark'?0:1].click();return true;",
          [theme],
        );
        await until(() =>
          execute(
            "return document.documentElement.lang===arguments[0] && document.documentElement.dataset.theme===arguments[1];",
            [language, theme],
          ),
        );
        await route("/");
        await until(() =>
          execute(
            "return document.querySelectorAll('.table').length>=2 && !document.querySelector('.skeleton-balance');",
          ),
        );
        const layout = await execute(
          "const main=document.querySelector('.main');const tables=[...document.querySelectorAll('.table-scroll')].map(e=>{e.scrollLeft=e.scrollWidth;const last=e.querySelector('th:last-child').getBoundingClientRect();const bounds=e.getBoundingClientRect();return {columns:e.querySelectorAll('th').length,scrollable:e.scrollWidth>e.clientWidth,lastColumnReachable:last.right<=bounds.right+2};});return {width:innerWidth,height:innerHeight,pixelRatio:devicePixelRatio,screen:{width:screen.width,height:screen.height,availableWidth:screen.availWidth,availableHeight:screen.availHeight},mainOverflow:main.scrollWidth>main.clientWidth+2,tables};",
        );
        if (
          layout.mainOverflow ||
          layout.tables.some((t) => !t.lastColumnReachable) ||
          layout.tables[0]?.columns !== 6 ||
          layout.tables[1]?.columns !== 8
        )
          throw new Error(
            `Required native columns are not reachable at ${width}x${height}/${language}/${theme}`,
          );
        await execute(
          "document.querySelector('.main').scrollTop=0;document.querySelectorAll('.table-scroll').forEach(e=>e.scrollLeft=0);return true;",
        );
        const name = `viewport-${width}x${height}-${language}-${theme}`;
        await screenshot(name);
        if (language === "ru" && theme === "dark") {
          // Capture the actual columns as well as the portfolio overview.
          // Left/right views make every value reviewable in narrow windows.
          for (const [index, kind] of [
            [0, "assets"],
            [1, "activity"],
          ]) {
            for (const edge of ["left", "right"]) {
              await execute(
                "const main=document.querySelector('.main');const table=document.querySelectorAll('.table-scroll')[arguments[0]];main.scrollTop+=table.getBoundingClientRect().top-main.getBoundingClientRect().top-72;table.scrollLeft=arguments[1]==='right'?table.scrollWidth:0;return true;",
                [index, edge],
              );
              await screenshot(`${name}-${kind}-${edge}`);
            }
          }
        }
        report.viewports.push({ name, ...layout, native });
      }
    }
  }
  await route("/settings");
  await select("#language", "en");
  await clickText("Dark");
  await route("/");
  record(
    "Native HWND viewport matrix: 3 sizes × 2 languages × 2 themes; all 6 asset and 8 activity columns reachable",
    "PASS",
    "Actual WebView2 CSS dimensions measured; hosted Windows DPI recorded, no display-scale emulation",
  );
}
try {
  await until(async () => {
    if (driverError) throw driverError;
    try {
      await request("/status");
      return true;
    } catch {
      return false;
    }
  }, 20000);
  record("WebDriver intermediary and native driver ready", "PASS");
  await open();
  record("Attach WebDriver to the native application and real IPC", "PASS");
  await route("/settings");
  await until(() => execute("return Boolean(document.querySelector('#language'));"));
  await select("#language", "en");
  await route("/");
  await until(async () => (await body()).includes("Track your crypto, read-only"));
  record("Clean first launch has no fabricated balances", "PASS");
  await screenshot("empty-first-launch");
  await ownedAccountScenarios();
  await clickText("Explore demo portfolio");
  await until(async () => (await body()).includes("Demo data. Not your portfolio."));
  await screenshot("portfolio-demo");
  record("Separate SQLite demo loaded through Rust IPC", "PASS");
  await viewportScenarios();
  await route("/wallets");
  await until(() => execute("return Boolean(document.querySelector('a[href^=\"#/accounts/\"]'));"));
  await execute("document.querySelector('a[href^=\"#/accounts/\"]').click();return true;");
  await until(() => execute("return Boolean(document.querySelector('.balance'));"));
  record("Account navigation and scoped holdings", "PASS");
  await route("/review");
  await until(() =>
    execute("return Boolean(document.querySelector('[aria-label=\"Filter by reason\"]'));"),
  );
  await select('[aria-label="Filter by reason"]', "unknown_basis");
  await clickText("Resolve");
  await until(() => execute("return Boolean(document.querySelector('.drawer .editor'));"));
  await screenshot("missing-basis-review");
  await clickText("Enter acquisition lots");
  await until(() => execute("return Boolean(document.querySelector('.lot-row'));"));
  await input(".lot-row label:nth-child(2) input", "123.45");
  await input(".editor input[name=decision-note]", "native-restart-audit");
  await clickText("Save and recalculate");
  await until(
    async () =>
      (await body()).includes("Version 1") && (await body()).includes("native-restart-audit"),
  );
  record("Cost basis saved and audit version displayed", "PASS");
  await clickText("Close");
  await route("/activity");
  await until(() =>
    execute("return Boolean(document.querySelector('button[aria-label^=\"Open details for\"]'));"),
  );
  const movement = await execute(
    "return document.querySelector('button[aria-label^=\"Open details for\"]').getAttribute('aria-label');",
  );
  await route("/settings/sources");
  await until(() => execute("return Boolean(document.querySelector('#key-zerion'));"));
  await input("#key-zerion", "native-credential-sentinel-no-api-use");
  await execute(
    "document.querySelector('#key-zerion').closest('form').requestSubmit();return true;",
  );
  await until(() =>
    execute(
      "return document.querySelector('#zerion-name').closest('article').querySelector('.chip').textContent.trim()==='Configured';",
    ),
  );
  if (!(await execute("return document.querySelector('#key-zerion').value==='';")))
    throw new Error("Credential input did not clear after save");
  record("Credential saved in isolated Windows Credential Manager entry", "PASS");
  // Reopen the edited movement and check secure storage after a restart.
  await request(`/session/${session}`, "DELETE");
  session = undefined;
  closeApplication();
  await delay(1000);
  await open();
  await route("/settings/sources");
  await until(() =>
    execute(
      "return document.querySelector('#zerion-name')?.closest('article').querySelector('.chip').textContent.trim()==='Configured';",
    ),
  );
  if (!(await execute("return document.querySelector('#key-zerion').value==='';")))
    throw new Error("Stored credential was returned to the renderer");
  await execute(
    "const card=document.querySelector('#zerion-name').closest('article');[...card.querySelectorAll('button')].find(e=>e.textContent.trim()==='Remove').click();return true;",
  );
  await until(() =>
    execute(
      "return document.querySelector('#zerion-name').closest('article').querySelector('.chip').textContent.trim()==='Not configured';",
    ),
  );
  record("OS credential persists across restart and can be removed without echoing it", "PASS");
  await screenshot("provider-setup");
  for (const name of ["real.sqlite", "demo.sqlite"]) {
    const path = join(dataDir, "profiles", name);
    for (const suffix of ["", "-wal", "-shm"]) {
      if (
        existsSync(path + suffix) &&
        readFileSync(path + suffix).includes(Buffer.from("native-credential-sentinel-no-api-use"))
      )
        throw new Error("Credential persisted in SQLite rather than OS storage");
    }
  }
  record("SQLite databases and WAL exclude credentials", "PASS");
  await route("/settings/data");
  await clickText("Explore demo portfolio");
  await until(async () => (await body()).includes("Demo data. Not your portfolio."));
  await route("/activity");
  await until(() =>
    execute(
      "return document.querySelectorAll('button[aria-label^=\"Open details for\"]').length>0;",
    ),
  );
  const count = await execute(
    "return document.querySelectorAll('button[aria-label^=\"Open details for\"]').length;",
  );
  let persisted = false;
  for (let i = 0; i < count; i++) {
    await execute(
      "document.querySelectorAll('button[aria-label^=\"Open details for\"]')[arguments[0]].click();return true;",
      [i],
    );
    await until(() => execute("return Boolean(document.querySelector('.drawer .facts'));"));
    if ((await body()).includes("native-restart-audit")) {
      persisted = true;
      break;
    }
    await clickText("Close");
  }
  if (!persisted) throw new Error(`Audit note missing after restart (${movement})`);
  record("SQLite basis decision survives process restart", "PASS");
  await screenshot("audit-reopened");
  await clickText("Close");
  await route("/settings");
  await select("#language", "ru");
  await until(() => execute("return document.documentElement.lang==='ru';"));
  await screenshot("settings-russian");
  await select("#language", "en");
  await clickText("Light");
  await until(() => execute("return document.documentElement.dataset.theme==='light';"));
  record("Language and theme change through native settings", "PASS");
  await route("/");
  await clickText("Hide balances and addresses");
  await until(async () => (await body()).includes("•••••"));
  await screenshot("privacy");
  record("Privacy masks portfolio values", "PASS");
  await clickText("Show balances and addresses");
  // A dummy OS credential remains configured while exporting, proving exclusion.
  await route("/settings/sources");
  await until(() => execute("return Boolean(document.querySelector('#key-zerion'));"));
  await input("#key-zerion", "native-credential-sentinel-no-api-use");
  await execute(
    "document.querySelector('#key-zerion').closest('form').requestSubmit();return true;",
  );
  await until(() =>
    execute(
      "return document.querySelector('#zerion-name').closest('article').querySelector('.chip').textContent.trim()==='Configured';",
    ),
  );
  await route("/settings/data");
  const archive = join(tmpdir(), `coincontrol-recovery-${process.pid}.ccbackup`);
  await saveNativeFile("Save backup", archive);
  const manifest = JSON.parse(readFileSync(archive, "utf8"));
  if (
    Buffer.from(manifest.database_base64, "base64").includes(
      Buffer.from("native-credential-sentinel-no-api-use"),
    )
  )
    throw new Error("OS credential leaked into backup payload");
  const baseline = {};
  const csvButtons = {
    holdings: "Export holdings CSV",
    activity: "Export activity CSV",
    lots: "Export lots CSV",
    decisions: "Export decisions CSV",
  };
  for (const [kind, button] of Object.entries(csvButtons)) {
    const path = join(tmpdir(), `coincontrol-before-${process.pid}-${kind}.csv`);
    await saveNativeFile(button, path);
    baseline[kind] = readFileSync(path, "utf8");
    if (baseline[kind].trim().split(/\r?\n/).length < 2) throw new Error(`Empty ${kind} export`);
  }
  const groups = readProfile(
    "SELECT g.id, g.label, x.wallet_id FROM groups g LEFT JOIN group_wallets x ON x.group_id=g.id ORDER BY g.id,x.wallet_id",
    "demo",
  );
  await screenshot("backup-export");
  record("Native Save dialogs export backup and four populated CSV files; OS key absent", "PASS");
  // Remove the temporary secure entry before opening the fresh installation.
  await route("/settings/sources");
  await execute(
    "[...document.querySelector('#zerion-name').closest('article').querySelectorAll('button')].find(e=>e.textContent.trim()==='Remove').click();return true;",
  );
  await until(() =>
    execute(
      "return document.querySelector('#zerion-name').closest('article').querySelector('.chip').textContent.trim()==='Not configured';",
    ),
  );
  const originalDirectory = dataDir;
  dataDir = mkdtempSync(join(tmpdir(), "coincontrol-native-restored-"));
  await restart();
  await route("/settings");
  await until(() => execute("return Boolean(document.querySelector('#language'));"));
  await select("#language", "en");
  await route("/settings/data");
  await clickText("Explore demo portfolio");
  await until(async () => (await body()).includes("Demo data. Not your portfolio."));
  const file = await request(`/session/${session}/element`, "POST", {
    using: "css selector",
    value: 'input[type="file"][accept=".ccbackup"]',
  });
  await request(
    `/session/${session}/element/${file["element-6066-11e4-a52e-4f735466cecf"]}/value`,
    "POST",
    { text: archive },
  );
  await clickText("Restore this profile");
  await until(async () =>
    (await body()).includes("Portfolio restored. Provider keys were not imported."),
  );
  for (const [kind, button] of Object.entries(csvButtons)) {
    const path = join(tmpdir(), `coincontrol-after-${process.pid}-${kind}.csv`);
    await saveNativeFile(button, path);
    if (readFileSync(path, "utf8") !== baseline[kind])
      throw new Error(`Restored ${kind} differs from exported source`);
  }
  if (
    JSON.stringify(
      readProfile(
        "SELECT g.id,g.label,x.wallet_id FROM groups g LEFT JOIN group_wallets x ON x.group_id=g.id ORDER BY g.id,x.wallet_id",
        "demo",
      ),
    ) !== JSON.stringify(groups)
  )
    throw new Error("Restored groups or overlapping membership differ");
  await route("/settings/sources");
  await until(() =>
    execute(
      "return document.querySelector('#zerion-name')?.closest('article').querySelector('.chip').textContent.trim()==='Not configured';",
    ),
  );
  const safetyDirectory = join(dataDir, "profiles", "safety-backups");
  if (!existsSync(safetyDirectory)) throw new Error("Restore did not create a safety snapshot");
  record(
    "Restore in a fresh installation preserves exact holdings/history/lots/audit CSV and groups; keys absent",
    "PASS",
    "Fresh demo destination is automatically seeded by the app before restoring its matching demo archive",
  );
  await screenshot("backup-restored");
  // No external request is made unless this separate public-address path is opted in.
  if (liveBtc) {
    await route("/settings/data");
    await clickText("Leave demo");
    await route("/wallets?add=1");
    await until(() => execute("return Boolean(document.querySelector('textarea'));"));
    const target = JSON.parse(readFileSync("tests/live/public-targets.json", "utf8")).bitcoin
      .sync_address.address;
    await input('input[placeholder="e.g. Cold storage"]', "Native public BTC");
    await input("textarea", target);
    await until(() =>
      execute(
        "return !document.querySelector('form[aria-labelledby=add-heading] button[type=submit]').disabled;",
      ),
    );
    await execute(
      "document.querySelector('form[aria-labelledby=add-heading]').requestSubmit();return true;",
    );
    await until(() =>
      execute("return Boolean(document.querySelector('a[href^=\"#/accounts/\"]'));"),
    );
    await until(async () => (await body()).includes("Syncing…"), 15000);
    const cancelStarted = Date.now();
    await clickText("Cancel synchronization");
    await until(
      async () =>
        (await body()).includes("Stopping at a safe checkpoint…") ||
        (await execute(
          "return [...document.querySelectorAll('button')].some(e=>e.textContent.trim()==='Sync now' && !e.disabled);",
        )),
      1000,
    );
    report.cancellationFeedbackMs = Date.now() - cancelStarted;
    if (report.cancellationFeedbackMs > 1000)
      throw new Error("Cancellation acknowledgement exceeded one second");
    await waitSync();
    await restart();
    await route("/wallets");
    await delay(3500); // allow the normal startup scheduler to enter its sweep
    await waitSync();
    const countSql = "SELECT COUNT(*) AS count FROM account_transactions";
    if (readProfile(countSql)[0].count < 17) {
      await clickText("Sync now");
      await waitSync();
    }
    const count = readProfile(countSql)[0].count;
    if (count < 17) throw new Error("Public BTC history did not import its known bounded history");
    if (!(await body()).includes("History complete"))
      throw new Error("BTC history coverage did not finish");
    const snapshot = btcSnapshot();
    if (!snapshot.length) throw new Error("No live BTC balance observation persisted");
    await screenshot("live-bitcoin-wallet");
    await route("/");
    await until(() => execute("return Boolean(document.querySelector('.balance'));"));
    await execute("document.querySelector('a[href^=\"#/assets/\"]').click();return true;");
    await until(() => execute("return Boolean(document.querySelector('.metrics'));"));
    await screenshot("live-bitcoin-asset");
    await route("/wallets");
    await execute("document.querySelector('a[href^=\"#/accounts/\"]').click();return true;");
    await until(() => execute("return Boolean(document.querySelector('.balance'));"));
    await execute(
      "document.querySelector('a[href^=\"#/activity?account=\"]').click();return true;",
    );
    await until(() =>
      execute(
        "return Boolean(document.querySelector('button[aria-label^=\"Open details for\"]'));",
      ),
    );
    await execute(
      "document.querySelector('button[aria-label^=\"Open details for\"]').click();return true;",
    );
    await until(() => execute("return Boolean(document.querySelector('.drawer .facts'));"));
    await screenshot("live-bitcoin-detail");
    record(
      "Native public BTC import, cancellation/restart/resume and Portfolio/Asset/Account/Activity navigation",
      "PASS",
      `${count} transactions; cancellation feedback ${report.cancellationFeedbackMs} ms`,
    );
    await route("/wallets");
    try {
      networkRule();
      await clickText("Sync now");
      await until(
        () => readProfile("SELECT id FROM balance_observations WHERE status='stale'").length > 0,
        150000,
      );
      // The failed balance read establishes the offline condition. Stop the
      // remaining price backfill at its checkpoint to conserve the live budget.
      await execute(
        "const button=[...document.querySelectorAll('button')].find(e=>e.textContent.trim()==='Cancel synchronization');if(button&&!button.disabled)button.click();return true;",
      );
      await waitSync();
      if (JSON.stringify(btcSnapshot()) !== JSON.stringify(snapshot))
        throw new Error("Offline failure replaced cached quantities");
      if (!readProfile("SELECT id FROM balance_observations WHERE status='stale'").length)
        throw new Error("Offline failure did not mark observations stale");
      await route("/");
      await until(async () => (await body()).toLowerCase().includes("stale"));
      await screenshot("offline-cached-bitcoin");
    } finally {
      networkRule(true);
    }
    await route("/wallets");
    await clickText("Sync now");
    await waitSync();
    if (readProfile(countSql)[0].count !== count) throw new Error("Reconnect duplicated history");
    if (
      readProfile("SELECT COUNT(*) AS count FROM balance_observations WHERE status='fresh'")[0]
        .count === 0
    )
      throw new Error("Reconnect did not refresh the balance");
    const errors = readProfile(
      "SELECT json_extract(retry_state,'$.last_error') AS error FROM sync_checkpoints WHERE json_extract(retry_state,'$.last_error') IS NOT NULL",
    );
    if (errors.length) throw new Error("Reconnect retained a synchronization error");
    report.providerUsage = readProfile(
      "SELECT provider,SUM(requests) AS requests FROM provider_usage GROUP BY provider",
    );
    if (report.providerUsage.some((row) => row.requests > 50))
      throw new Error("Native public provider budget exceeded 50 requests");
    await screenshot("reconnected-bitcoin");
    record(
      "OS-enforced offline failure retains cached amounts and stale UI; reconnect refreshes without duplicates",
      "PASS",
    );
  } else {
    record(
      "Live native synchronization and offline reconnect",
      "BLOCKED",
      "Run --live-btc with RUN_LIVE_API_TESTS=1; public reads only",
    );
  }
  // This job drives a distinct binary; production installer acceptance is separate.
  record(
    "Installer upgrade and uninstall",
    "SKIPPED_NOT_IN_SCOPE",
    "Executed independently by windows-installer job",
  );
  report.originalIsolatedProfile = originalDirectory;
} catch (e) {
  const processes = spawnSync(
    "powershell.exe",
    [
      "-NoProfile",
      "-Command",
      "Get-CimInstance Win32_Process | Where-Object { $_.Name -match 'portfolio|msedge|tauri' } | Select-Object Name,ProcessId,CommandLine | ConvertTo-Json",
    ],
    { encoding: "utf8" },
  );
  writeFileSync(join(output, "processes.json"), processes.stdout || "[]");
  try {
    await screenshot("failure");
  } catch {
    /* A closed process may have no screenshot. */
  }
  record("Native scenario execution", "FAIL", e instanceof Error ? e.message : "Unknown failure");
} finally {
  if (session) {
    try {
      await request(`/session/${session}`, "DELETE");
    } catch {
      /* Preserve report even if process exited. */
    }
  }
  closeApplication();
  driver.kill();
  save();
}
// Acceptance gaps remain visible, while CI fails on any executed scenario failure.
process.exit(
  report.checks.some(
    (c) =>
      c.result === "FAIL" || (c.result === "BLOCKED" && !process.argv.includes("--smoke-only")),
  )
    ? 1
    : 0,
);
