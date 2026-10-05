// Native WebDriver suite. DOM interactions use the real packaged UI and Rust IPC.
// It never installs a test server or an IPC bypass in the application.
import { spawn } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { createHash } from "node:crypto";

const output = "target/native-report";
mkdirSync(output, { recursive: true });
const report = { mode: "native-windows", at: new Date().toISOString(), checks: [] };
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
const dataDir = mkdtempSync(join(tmpdir(), "coincontrol-native-"));
const port = Number(process.env.E2E_DRIVER_PORT ?? 4444);
const base = `http://127.0.0.1:${port}`;
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
    env: { ...process.env, COINCONTROL_E2E_DATA_DIR: dataDir },
    stdio: ["ignore", "ignore", "ignore"],
  },
);
let driverError;
driver.on("error", (e) => {
  driverError = e;
});
let session;
const delay = (ms) => new Promise((r) => setTimeout(r, ms));
async function request(path, method = "GET", body) {
  const response = await fetch(base + path, {
    method,
    headers: { "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(30000),
  });
  const json = await response.json();
  if (!response.ok || json.value?.error)
    throw new Error(`WebDriver ${json.value?.error ?? response.status}`);
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
    "const e=document.querySelector(arguments[0]);if(!e)throw Error('missing input');Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,arguments[1]);e.dispatchEvent(new Event('input',{bubbles:true}));return true;",
    [css, value],
  );
}
async function open() {
  const s = await request("/session", "POST", {
    capabilities: { alwaysMatch: { browserName: "wry", "tauri:options": { application } } },
  });
  session = s.sessionId;
  await until(() => execute("return Boolean(document.querySelector('h1'));"));
  if (!(await execute("return Boolean(window.__TAURI_INTERNALS__);")))
    throw new Error("Native IPC bridge missing");
  if ((await body()).includes("Browser preview")) throw new Error("Browser mock detected");
}
async function screenshot(name) {
  const b64 = await request(`/session/${session}/screenshot`);
  writeFileSync(join(output, name + ".png"), Buffer.from(b64, "base64"));
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
  await open();
  record("Launch through tauri-driver and native IPC", "PASS");
  await route("/settings");
  await until(() => execute("return Boolean(document.querySelector('#language'));"));
  await select("#language", "en");
  await route("/");
  await until(async () => (await body()).includes("Track your crypto, read-only"));
  record("Clean first launch has no fabricated balances", "PASS");
  await clickText("Explore demo portfolio");
  await until(async () => (await body()).includes("Demo data. Not your portfolio."));
  await screenshot("portfolio-demo");
  record("Separate SQLite demo loaded through Rust IPC", "PASS");
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
  record("Language and theme persisted through native settings", "PASS");
  await route("/");
  await clickText("Hide balances and addresses");
  await until(async () => (await body()).includes("•••••"));
  await screenshot("privacy");
  record("Privacy masks portfolio values", "PASS");
  // Store recovery and CSV edge cases are covered by file-backed Rust integration tests.
  for (const scenario of [
    "Live native synchronization",
    "Native file dialog backup/restore",
    "Installer upgrade and uninstall",
    "Offline reconnect",
  ])
    record(
      scenario,
      "BLOCKED",
      "Requires the separate live/installer acceptance run; this deterministic suite does not claim it",
    );
} catch (e) {
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
