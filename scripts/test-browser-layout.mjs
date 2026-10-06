import { chromium } from "playwright";
import { spawn, spawnSync } from "node:child_process";
import { arch, release } from "node:os";
import { resolve } from "node:path";
import { until } from "./lib/native-webdriver.mjs";
import { writeFileSync, mkdirSync } from "node:fs";
import { verifyNativePages } from "./lib/native-layout.mjs";
import { stripVTControlCharacters } from "node:util";
const output = "target/browser-layout-report";
mkdirSync(output, { recursive: true });
const report = {
  mode: "browser-preview",
  note: "Chromium with mock demo IPC; no Windows, native IPC, physical monitor or OS DPI assertion.",
  nativeLikePolicyIdentityFixture: "ethereum:token:0x00000000000000000000000000000000000d3e30",
  nativeLikeAccountAddressFixture:
    "0:0000000000000000000000000000000000000000000000000000000000000001",
  scrollbarFixture:
    "Chromium hide-scrollbars default removed; real scrollbars participate in layout.",
  scenarios: [],
  checks: [],
};
const environment = { ...process.env };
for (const key of Object.keys(environment)) if (key.endsWith("_API_KEY")) delete environment[key];
const server = spawn(
  process.execPath,
  [
    resolve("node_modules/vite/bin/vite.js"),
    "--host",
    "127.0.0.1",
    "--port",
    "4175",
    "--strictPort",
  ],
  { env: environment, stdio: ["ignore", "pipe", "pipe"] },
);
let serverError;
let serverStarted = false;
server.stdout.on("data", (chunk) => {
  if (stripVTControlCharacters(chunk.toString()).includes("http://127.0.0.1:4175/"))
    serverStarted = true;
});
// Drain stderr, without including environment or application data in the report.
server.stderr.on("data", () => {});
server.on("error", (error) => (serverError = error));
server.on("exit", (code) => (serverError = new Error(`Dev server exited (${code})`)));
let browser;
try {
  await until(async () => {
    if (serverError) throw serverError;
    if (!serverStarted) return false;
    try {
      return (await fetch("http://127.0.0.1:4175/")).ok;
    } catch {
      return false;
    }
  }, 20000);
  browser = await chromium.launch({
    headless: true,
    args: ["--no-sandbox"],
    ignoreDefaultArgs: ["--hide-scrollbars"],
  });
  report.environment = {
    os: process.platform,
    osVersion: release(),
    arch: arch(),
    node: process.version,
    chromium: browser.version(),
  };
  report.sourceSha = spawnSync("git", ["rev-parse", "HEAD"], { encoding: "utf8" }).stdout.trim();
  report.sourceDirty = Boolean(
    spawnSync("git", ["status", "--porcelain"], { encoding: "utf8" }).stdout.trim(),
  );
  const page = await browser.newPage();
  await page.route("**/*", (route) =>
    route.request().url().startsWith("http://127.0.0.1:4175/") ? route.continue() : route.abort(),
  );
  const ui = {
    execute: (script, args = []) =>
      page.evaluate(`(function(){${script}}).apply(null,${JSON.stringify(args)})`),
    route: async (path) => {
      await page.evaluate((hash) => (location.hash = hash), "#" + path);
      await page.waitForTimeout(300);
      if (path === "/settings/data") {
        // The lightweight browser demo uses short IDs. Exercise the same long
        // public contract identity rendered by the real native demo instead.
        await page.waitForFunction(() =>
          [...document.querySelectorAll(".card-pad span[title]")].some((e) =>
            e.textContent.includes("DEMO"),
          ),
        );
        await page.evaluate((id) => {
          const e = [...document.querySelectorAll(".card-pad span[title]")].find((e) =>
            e.textContent.includes("DEMO"),
          );
          e.textContent = `DEMO · ${id} · unverified`;
          e.title = id;
        }, report.nativeLikePolicyIdentityFixture);
      }
      if (path.startsWith("/accounts/")) {
        await page.waitForSelector(".content p.meta .address");
        await page.evaluate((address) => {
          const e = document.querySelector(".content p.meta .address");
          // Preserve privacy masking; the fixture supplies a full raw TON address.
          if (!e.textContent.includes("•")) e.textContent = address;
        }, report.nativeLikeAccountAddressFixture);
      }
    },
    screenshot: async (name) => {
      await page.screenshot({ path: `${output}/${name}.png` });
    },
    key: (value, shift = false) =>
      page.keyboard.press(
        (shift ? "Shift+" : "") +
          ({ "\uE004": "Tab", "\uE007": "Enter", "\uE00C": "Escape" }[value] ?? value),
      ),
  };
  try {
    await page.goto("http://127.0.0.1:4175/");
    await page.getByRole("button", { name: "Explore demo portfolio" }).click();
    for (const [width, height] of [
      [1440, 900],
      [1280, 800],
      [1024, 720],
      [800, 480],
      [672, 440],
      [504, 340],
    ]) {
      await page.setViewportSize({ width, height });
      for (const language of ["en", "ru"])
        for (const theme of ["dark", "light"]) {
          await ui.route("/settings");
          await page.locator("#language").selectOption(language);
          await page
            .locator('[aria-labelledby="theme-label"] button')
            .nth(theme === "dark" ? 0 : 1)
            .click();
          await page.waitForFunction(
            ([lang, theme]) =>
              document.documentElement.lang === lang &&
              document.documentElement.dataset.theme === theme,
            [language, theme],
          );
          const name = `browser-${width}x${height}-${language}-${theme}`;
          await verifyNativePages(
            ui,
            { name, capture: width === 504 && language === "ru" && theme === "dark" },
            report,
          );
          report.scenarios.push({ width, height, language, theme, result: "PASS" });
          console.log("PASS " + name);
        }
    }
    await page.goto("http://127.0.0.1:4175/tests/browser/table.html");
    await page.setViewportSize({ width: 504, height: 340 });
    const scroller = page.locator(".table-windowed");
    await scroller.waitFor();
    const mounted = () => page.locator("tbody tr[data-index]").count();
    if ((await mounted()) > 80) throw new Error("Long table mounted too many rows");
    const first = page.getByRole("button", { name: "Row 0", exact: true });
    await first.focus();
    await page.evaluate(() => (document.querySelector(".table-windowed").scrollTop = 320000));
    await page.waitForTimeout(200);
    if (!(await first.evaluate((e) => e === document.activeElement)))
      throw new Error("Scrolling removed the focused row");
    await scroller.focus();
    await scroller.press("End");
    await until(async () => {
      await scroller.press("End");
      const last = page.getByRole("button", { name: "Row 9999", exact: true });
      if (!(await last.isVisible())) return false;
      return await last.evaluate((e) => {
        const r = e.getBoundingClientRect(),
          b = e.closest(".table-windowed").getBoundingClientRect();
        // scrollHeight/clientHeight round to integers; DOM rectangles retain subpixels.
        return r.top >= b.top - 1 && r.bottom <= b.bottom + 1;
      });
    }, 10000);
    if ((await mounted()) > 80) throw new Error("Long table mounted too many final rows");
    report.longTable = {
      logicalRows: 10000,
      mountedFinalRows: await mounted(),
      variableHeight: true,
      scrollToLast: "PASS",
      focusRetained: "PASS",
      keyboardEnd: "PASS",
    };
    await page.screenshot({ path: `${output}/browser-long-table.png` });
    report.checks.push({
      name: "10000 variable-height rows: bounded DOM, last row, retained focus, keyboard End",
      result: "PASS",
    });
    report.checks.push({
      name: "12 pages and 2 modal panels across 24 size/language/theme combinations",
      result: "PASS",
    });
  } catch (error) {
    report.checks.push({ name: "Browser layout matrix", result: "FAIL", detail: error.message });
    await page.screenshot({ path: `${output}/failure.png` });
    console.error(error.message);
    process.exitCode = 1;
  } finally {
    writeFileSync(`${output}/BROWSER_LAYOUT_REPORT.json`, JSON.stringify(report, null, 2) + "\n");
  }
} catch (error) {
  report.checks.push({ name: "Browser infrastructure", result: "FAIL", detail: error.message });
  writeFileSync(`${output}/BROWSER_LAYOUT_REPORT.json`, JSON.stringify(report, null, 2) + "\n");
  process.exitCode = 1;
} finally {
  await browser?.close();
  server.kill();
}
