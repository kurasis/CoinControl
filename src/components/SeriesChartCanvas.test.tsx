import { afterEach, expect, it, vi } from "vitest";
import { render, waitFor } from "@testing-library/react";
import * as echarts from "echarts/core";
import "../i18n";
import SeriesChartCanvas from "./SeriesChartCanvas";

const app = vi.hoisted(() => ({
  privacy: false,
  locale: "ru-RU",
  timeZone: "America/Los_Angeles",
  settings: { theme: "dark" },
}));
vi.mock("../app/AppContext", () => ({ useApp: () => app }));
afterEach(() => vi.restoreAllMocks());

it("formats the time axis in the selected locale and zone at a UTC date boundary", async () => {
  const setOption = vi.fn();
  vi.spyOn(echarts, "init").mockReturnValue({
    setOption,
    resize: vi.fn(),
    dispose: vi.fn(),
  } as unknown as echarts.EChartsType);
  const t = Date.parse("2026-10-01T00:00:00Z") / 1000;
  render(
    <SeriesChartCanvas
      points={[
        { t, value: "1", estimated: false },
        { t: t + 86400 * 3, value: "2", estimated: false },
      ]}
      kind="price"
      ariaLabel="Price"
    />,
  );
  await waitFor(() => expect(setOption).toHaveBeenCalled());
  const option = setOption.mock.calls[0]![0];
  const label = option.xAxis.axisLabel.formatter(t * 1000);
  expect(label).toBe("30 сент. 2026 г.");
  expect(option.tooltip.formatter([{ dataIndex: 0 }])).toContain("30 сент. 2026 г.");
  expect(option.useUTC).toBe(true);
});
