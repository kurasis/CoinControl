// Actual WebView2 pages and native keyboard input, independent of browser previews.
import { spawnSync } from "node:child_process";
import { resolve } from "node:path";
import { until } from "./native-webdriver.mjs";

const inspect = `
const main=document.querySelector('.main');
const visible=e=>e.checkVisibility() && e.getBoundingClientRect().width>0;
const controls=[...document.querySelectorAll('.toolbar button,.toolbar select')].filter(visible);
const bounds=main.getBoundingClientRect();
const sidebar=document.querySelector('.sidebar');
const settingsLink=sidebar.querySelector('a[href="#/settings"]');settingsLink.scrollIntoView({block:'nearest'});
const navBounds=sidebar.getBoundingClientRect(),linkBounds=settingsLink.getBoundingClientRect();
const sideNavigationReachable=linkBounds.top>=navBounds.top-2 && linkBounds.bottom<=navBounds.bottom+2;
sidebar.scrollTop=0;
const tables=[...document.querySelectorAll('.table-scroll')].filter(visible).map(e=>{
  e.scrollLeft=e.scrollWidth;
  const last=e.querySelector('th:last-child')?.getBoundingClientRect();
  const reach=!last || last.right<=e.getBoundingClientRect().right+2;
  e.scrollLeft=0;
  return {columns:e.querySelectorAll('th').length,lastColumnReachable:reach};
});
return {path:location.hash,width:innerWidth,height:innerHeight,pixelRatio:devicePixelRatio,
mainOverflow:main.scrollWidth>main.clientWidth+2,sideNavigationReachable,
controlsReachable:controls.every(e=>{const r=e.getBoundingClientRect();return r.left>=bounds.left-2 && r.right<=bounds.right+2;}),tables};`;

async function ready(ui) {
  await until(() =>
    ui.execute(
      "return Boolean(document.querySelector('h1')) && !document.querySelector('.skeleton-balance,.skeleton-table');",
    ),
  );
}

export async function verifyNativePages(ui, { name, capture }, report) {
  const routes = [
    "/",
    "/wallets",
    "/activity",
    "/review",
    "/settings",
    "/settings/sources",
    "/settings/networks",
    "/settings/data",
  ];
  await ui.route("/wallets");
  await ready(ui);
  const account = await ui.execute(
    "return document.querySelector('a[href^=\"#/accounts/\"]')?.getAttribute('href').slice(1);",
  );
  await ui.route("/");
  await ready(ui);
  const asset = await ui.execute(
    "return document.querySelector('a[href^=\"#/assets/\"]')?.getAttribute('href').slice(1);",
  );
  const group = await ui.execute(
    "return document.querySelector('.sidebar a[href^=\"#/groups/\"]')?.getAttribute('href').slice(1);",
  );
  if (!account || !asset || !group)
    throw new Error("Demo lacks account/asset/group navigation fixtures");
  routes.push(account, asset, group);
  for (const [index, route] of routes.entries()) {
    await ui.route(route);
    await ready(ui);
    const layout = await ui.execute(inspect);
    if (
      layout.mainOverflow ||
      !layout.controlsReachable ||
      !layout.sideNavigationReachable ||
      layout.tables.some((t) => !t.lastColumnReachable)
    ) {
      throw new Error(
        `Unreachable native page content: ${name}${route}: ${JSON.stringify(layout)}`,
      );
    }
    report.pageLayouts ??= [];
    report.pageLayouts.push({ scenario: name, ...layout });
    if (capture) await ui.screenshot(`${name}-page-${index}`);
    await until(() => ui.execute("return !document.querySelector('.chart.skeleton');"));
    const charts = await ui.execute("return document.querySelectorAll('.chart-data').length;");
    for (let chart = 0; chart < charts; chart++) {
      await ui.execute(
        "document.querySelectorAll('.chart-data summary')[arguments[0]].focus();return true;",
        [chart],
      );
      await ui.key("\uE007");
      await until(() =>
        ui.execute("return document.querySelectorAll('.chart-data')[arguments[0]].open;", [chart]),
      );
      const data = await ui.execute(
        `const e=document.querySelectorAll('.chart-data')[arguments[0]],b=e.querySelector('[role=region]');return {rows:e.querySelectorAll('tbody tr').length,label:b.getAttribute('aria-label'),overflow:document.querySelector('.main').scrollWidth>document.querySelector('.main').clientWidth+2};`,
        [chart],
      );
      if (!data.rows || data.rows > 50 || !data.label || data.overflow)
        throw new Error(`Inaccessible chart data: ${name}${route}`);
      if (capture) await ui.screenshot(`${name}-page-${index}-chart-data-${chart}`);
      await ui.key(" ");
      await until(() =>
        ui.execute("return !document.querySelectorAll('.chart-data')[arguments[0]].open;", [chart]),
      );
      report.chartData ??= [];
      report.chartData.push({ scenario: name, route, chart, ...data, keyboard: "PASS" });
    }
  }
  // Verify actual Tab/Shift+Tab and Escape, including restoration to the opening control.
  await ui.route("/review");
  await ready(ui);
  for (const panel of ["import", "leg"]) {
    const selector =
      panel === "import" ? ".toolbar button.btn:not(.btn-icon)" : ".table tbody button";
    await until(() =>
      ui.execute("return Boolean(document.querySelector(arguments[0]));", [selector]),
    );
    await ui.execute(
      "window.__layoutOpener=document.querySelector(arguments[0]);window.__layoutOpener.focus();window.__layoutOpener.click();return true;",
      [selector],
    );
    await until(() =>
      ui.execute(
        "return Boolean(document.querySelector('[role=dialog]')) && !document.querySelector('[role=dialog] .skeleton-table');",
      ),
    );
    const geometry =
      await ui.execute(`const p=document.querySelector('[role=dialog]'),b=p.querySelector('.drawer-body');
const r=p.getBoundingClientRect(),h=p.querySelector('.drawer-head').getBoundingClientRect();
return {width:r.width,height:r.height,bodyOverflow:b.scrollWidth>b.clientWidth+2,closeReachable:h.bottom<=innerHeight+2 && h.right<=innerWidth+2};`);
    if (
      geometry.bodyOverflow ||
      !geometry.closeReachable ||
      geometry.width > (await ui.execute("return innerWidth;"))
    )
      throw new Error(`Panel overflow: ${name}/${panel}`);
    await ui.execute(
      `const p=document.querySelector('[role=dialog]');window.__panelTabStops=[...p.querySelectorAll('button,a[href],input,select,textarea,summary,[tabindex]')].filter(e=>e.tabIndex>=0&&!e.matches(':disabled')&&e.checkVisibility());window.__panelTabStops.at(-1).focus();return true;`,
    );
    await ui.key("\uE004");
    if (!(await ui.execute("return document.activeElement===window.__panelTabStops[0];")))
      throw new Error("Tab escaped the modal");
    await ui.key("\uE004", true);
    if (!(await ui.execute("return document.activeElement===window.__panelTabStops.at(-1);")))
      throw new Error("Shift+Tab escaped the modal");
    await ui.key("\uE00C");
    await until(() =>
      ui.execute(
        "return !document.querySelector('[role=dialog]') && document.activeElement===window.__layoutOpener;",
      ),
    );
    // Capture the open panel separately after keyboard assertions.
    if (capture) {
      await ui.execute("window.__layoutOpener.click();return true;");
      await until(() =>
        ui.execute("return !document.querySelector('[role=dialog] .skeleton-table');"),
      );
      await ui.screenshot(`${name}-${panel}-panel`);
      await ui.key("\uE00C");
      await until(() => ui.execute("return !document.querySelector('[role=dialog]');"));
    }
    report.panelLayouts ??= [];
    report.panelLayouts.push({ scenario: name, panel, ...geometry, keyboard: "PASS" });
  }
  if (!report.chartPrivacy && report.chartData?.length) {
    await ui.route("/settings");
    await until(() => ui.execute("return Boolean(document.querySelector('#privacy'));"));
    const original = await ui.execute("return document.querySelector('#privacy').checked;");
    try {
      if (!original) await ui.execute("document.querySelector('#privacy').click();return true;");
      await until(() => ui.execute("return document.querySelector('#privacy').checked;"));
      await ui.route("/");
      await until(() =>
        ui.execute("return Boolean(document.querySelector('.chart-data summary'));"),
      );
      await ui.execute("document.querySelector('.chart-data summary').focus();return true;");
      await ui.key("\uE007");
      const privateValues = await ui.execute(
        "return [...document.querySelectorAll('.chart-data tbody td.num')].map(e=>e.textContent.trim());",
      );
      if (!privateValues.length || privateValues.some((v) => v !== "•••••" && v !== "—"))
        throw new Error("Chart observations expose private values");
      await ui.route("/wallets");
      await ready(ui);
      await ui.route("/");
      await ready(ui);
      const asset = await ui.execute(
        "return document.querySelector('a[href^=\"#/assets/\"]')?.getAttribute('href').slice(1);",
      );
      await ui.route(asset);
      await until(() =>
        ui.execute("return Boolean(document.querySelector('.chart-data tbody td.num'));"),
      );
      const publicValues = await ui.execute(
        "return [...document.querySelectorAll('.chart-data tbody td.num')].map(e=>e.textContent.trim());",
      );
      if (publicValues.every((v) => v === "•••••" || v === "—"))
        throw new Error("Privacy incorrectly hid public market prices");
      report.chartPrivacy = {
        result: "PASS",
        privateObservations: privateValues.length,
        publicPrices: publicValues.length,
      };
    } finally {
      await ui.route("/settings");
      await until(() => ui.execute("return Boolean(document.querySelector('#privacy'));"));
      await ui.execute(
        "const e=document.querySelector('#privacy');if(e.checked!==arguments[0])e.click();return true;",
        [original],
      );
      await until(() =>
        ui.execute("return document.querySelector('#privacy').checked===arguments[0];", [original]),
      );
    }
  }
}

export async function verifyDisplayScaling(ui, record, report) {
  const change = (percent) => {
    const response = spawnSync(
      "powershell.exe",
      [
        "-NoProfile",
        "-File",
        resolve("scripts/native-display-scale.ps1"),
        "-ApplicationPid",
        String(ui.pid()),
        "-Percent",
        String(percent),
      ],
      { encoding: "utf8", timeout: 60000 },
    );
    if (response.status !== 0)
      throw new Error(`Display Settings automation failed: ${response.stderr.slice(-500)}`);
    return JSON.parse(response.stdout);
  };
  const baseline = change(0);
  report.displayScaling = { baseline, scenarios: [] };
  if (baseline.result !== "PASS") {
    record("Windows Display Settings 125/150/200% acceptance", "BLOCKED", baseline.detail);
    return;
  }
  try {
    for (const percent of [125, 150, 200]) {
      const result = change(percent);
      report.displayScaling.scenarios.push(result);
      if (result.result !== "PASS") {
        record(`Windows ${percent}% display scale`, "BLOCKED", result.detail);
        continue;
      }
      await until(() =>
        ui.execute("return Math.abs(devicePixelRatio-arguments[0])<0.01;", [percent / 100]),
      );
      const [l, t, r, b] = result.outerPhysical;
      const [wl, wt, wr, wb] = result.workAreaPhysical;
      if (l < wl || t < wt || r > wr || b > wb)
        throw new Error(`Scaled window exceeds physical work area at ${percent}%`);
      await verifyNativePages(ui, { name: `os-scale-${percent}`, capture: true }, report);
      record(
        `Windows ${percent}% display scale: HWND, physical work area, pages and panels`,
        "PASS",
      );
    }
  } finally {
    const restored = change(baseline.originalPercent);
    report.displayScaling.restored = restored;
    record("Restore original Windows display scale", restored.result, restored.detail);
  }
}
