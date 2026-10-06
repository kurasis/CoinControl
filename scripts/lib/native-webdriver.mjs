// External Windows automation. No mock server, command injection or app test bridge.
import { spawn, spawnSync } from "node:child_process";
import { writeFileSync } from "node:fs";
import { join } from "node:path";

export const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
export async function until(test, timeout = 30000) {
  const start = Date.now();
  while (Date.now() - start < timeout) {
    if (await test()) return;
    await delay(100);
  }
  throw new Error("Timed out waiting for native application");
}

export class NativeWebDriver {
  constructor({ application, output, webviewDirectory, environment = {} }) {
    this.application = application;
    this.output = output;
    this.webviewDirectory = webviewDirectory;
    this.environment = { ...process.env, ...environment };
    for (const key of Object.keys(this.environment))
      if (key.endsWith("_API_KEY")) delete this.environment[key];
    this.port = Number(process.env.E2E_DRIVER_PORT ?? 4444);
    this.debugPort = this.port + 2;
    this.firewallName = `CoinControl-load-${process.pid}`;
    this.firewallInstalled = false;
  }

  async request(path, method = "GET", body) {
    const response = await fetch(`http://127.0.0.1:${this.port}${path}`, {
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

  execute(script, args = []) {
    return this.request(`/session/${this.session}/execute/sync`, "POST", { script, args });
  }

  async startDriver() {
    this.driver = spawn(
      process.env.TAURI_DRIVER_PATH ?? "tauri-driver",
      [
        "--port",
        String(this.port),
        ...(process.env.EDGE_WEBDRIVER_PATH
          ? ["--native-driver", process.env.EDGE_WEBDRIVER_PATH]
          : []),
      ],
      { env: this.environment, stdio: ["ignore", "pipe", "pipe"], windowsHide: true },
    );
    let error;
    this.driver.on("error", (e) => (error = e));
    let log = "";
    for (const stream of [this.driver.stdout, this.driver.stderr])
      stream.on("data", (chunk) => {
        log = (log + chunk.toString()).slice(-100000);
        writeFileSync(join(this.output, "driver.log"), log);
      });
    await until(async () => {
      if (error) throw error;
      try {
        await this.request("/status");
        return true;
      } catch {
        return false;
      }
    });
  }

  async open() {
    this.spawnRequestedUtcMs = Date.now();
    this.app = spawn(this.application, [], {
      env: {
        ...this.environment,
        // Official WebView2 runtime environment options, outside production code.
        WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${this.debugPort}`,
        WEBVIEW2_USER_DATA_FOLDER: this.webviewDirectory,
      },
      stdio: ["ignore", "pipe", "pipe"],
    });
    let error;
    this.app.on("error", (e) => (error = e));
    this.app.on("exit", (code) => (error = new Error(`Application exited (${code})`)));
    let log = "";
    for (const stream of [this.app.stdout, this.app.stderr])
      stream.on("data", (chunk) => {
        log = (log + chunk.toString()).slice(-100000);
        writeFileSync(join(this.output, "application.log"), log);
      });
    await until(async () => {
      if (error) throw error;
      try {
        const response = await fetch(`http://127.0.0.1:${this.debugPort}/json/version`, {
          signal: AbortSignal.timeout(2000),
        });
        return response.ok;
      } catch {
        return false;
      }
    }, 60000);
    const created = await this.request("/session", "POST", {
      capabilities: {
        alwaysMatch: {
          browserName: "webview2",
          "ms:edgeChromium": true,
          "ms:edgeOptions": { debuggerAddress: `127.0.0.1:${this.debugPort}` },
        },
      },
    });
    this.session = created.sessionId;
    await until(() => this.execute("return Boolean(document.querySelector('h1'));"));
    if (!(await this.execute("return Boolean(window.__TAURI_INTERNALS__);")))
      throw new Error("Native IPC bridge missing");
    if ((await this.body()).includes("Browser preview")) throw new Error("Browser mock detected");
  }

  body() {
    return this.execute("return document.body.innerText;");
  }

  async route(path) {
    await this.execute("window.location.hash=arguments[0];return true;", ["#" + path]);
  }

  async clickText(text) {
    await until(() =>
      this.execute(
        "const e=[...document.querySelectorAll('button,a,label')].find(e=>e.textContent.trim()===arguments[0] || e.getAttribute('aria-label')===arguments[0]);if(!e||e.disabled)return false;e.click();return true;",
        [text],
      ),
    );
  }

  input(css, value) {
    return this.execute(
      "const e=document.querySelector(arguments[0]);if(!e)throw Error('missing input');const p=e.tagName==='TEXTAREA'?HTMLTextAreaElement.prototype:HTMLInputElement.prototype;Object.getOwnPropertyDescriptor(p,'value').set.call(e,arguments[1]);e.dispatchEvent(new Event('input',{bubbles:true}));return true;",
      [css, value],
    );
  }

  async upload(css, path) {
    const element = await this.execute("return document.querySelector(arguments[0]);", [css]);
    const id = element?.["element-6066-11e4-a52e-4f735466cecf"];
    if (!id) throw new Error("File input is missing");
    await this.request(`/session/${this.session}/element/${id}/value`, "POST", {
      text: path,
      value: Array.from(path),
    });
  }

  async screenshot(name) {
    await delay(250);
    const b64 = await this.request(`/session/${this.session}/screenshot`);
    writeFileSync(join(this.output, `${name}.png`), Buffer.from(b64, "base64"));
  }

  powershell(script, extra = {}) {
    const result = spawnSync("powershell.exe", ["-NoProfile", "-Command", script], {
      env: { ...this.environment, ...extra },
      encoding: "utf8",
      windowsHide: true,
    });
    if (result.status !== 0)
      throw new Error(`External Windows helper failed: ${String(result.stderr).slice(-500)}`);
    return result.stdout.trim();
  }

  firewall(remove = false) {
    if (remove && !this.firewallInstalled) return;
    this.powershell(
      remove
        ? "$ErrorActionPreference='Stop'; Get-NetFirewallRule -Name $env:COINCONTROL_FIREWALL_RULE -ErrorAction SilentlyContinue | Remove-NetFirewallRule -ErrorAction Stop; exit 0"
        : "New-NetFirewallRule -Name $env:COINCONTROL_FIREWALL_RULE -DisplayName 'CoinControl isolated native load' -Direction Outbound -Program $env:COINCONTROL_FIREWALL_APP -Action Block -Profile Any -ErrorAction Stop | Out-Null",
      {
        COINCONTROL_FIREWALL_RULE: this.firewallName,
        COINCONTROL_FIREWALL_APP: this.application,
      },
    );
    this.firewallInstalled = !remove;
  }

  async stopApplication() {
    if (this.session) {
      try {
        await this.request(`/session/${this.session}`, "DELETE");
      } catch {
        // An exited application may already have closed its WebDriver session.
      }
      this.session = undefined;
    }
    if (this.app) {
      spawnSync("taskkill.exe", ["/PID", String(this.app.pid), "/T", "/F"], {
        stdio: "ignore",
        windowsHide: true,
      });
      this.app = undefined;
      await delay(1000);
    }
  }

  async dispose() {
    await this.stopApplication();
    if (this.driver) {
      spawnSync("taskkill.exe", ["/PID", String(this.driver.pid), "/T", "/F"], {
        stdio: "ignore",
        windowsHide: true,
      });
      this.driver = undefined;
    }
  }
}
