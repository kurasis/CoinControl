import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import * as echarts from "echarts/core";
import { LineChart } from "echarts/charts";
import { GridComponent, TooltipComponent } from "echarts/components";
import { CanvasRenderer } from "echarts/renderers";
import { useApp } from "../app/AppContext";
import { MASK, formatDateTime, formatPrice, formatUsd } from "../lib/format";

echarts.use([LineChart, GridComponent, TooltipComponent, CanvasRenderer]);

export interface SeriesPoint {
  t: number;
  /** Exact decimal string; `null` is a gap, never zero. */
  value: string | null;
  estimated: boolean;
  partial?: boolean;
}

function cssVar(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

/**
 * A single USD line. `kind="value"` is private (masked in privacy mode);
 * `kind="price"` is a public market price.
 */
export function SeriesChart({
  points,
  kind,
  label,
}: {
  points: SeriesPoint[];
  kind: "value" | "price";
  label: string;
}) {
  const { t } = useTranslation();
  const { locale, privacy, timeZone, settings } = useApp();
  const el = useRef<HTMLDivElement>(null);
  const masked = kind === "value" && privacy;
  const fmt = (v: string) => (kind === "price" ? formatPrice(v, locale) : formatUsd(v, locale));

  useEffect(() => {
    if (!el.current) return;
    const instance = echarts.init(el.current, undefined, { renderer: "canvas" });
    const accent = cssVar("--accent");
    const secondary = cssVar("--text-secondary");
    const border = cssVar("--border");
    instance.setOption({
      animationDuration: 150,
      grid: { left: 8, right: 8, top: 16, bottom: 24, containLabel: true },
      xAxis: {
        type: "time",
        axisLine: { lineStyle: { color: border } },
        axisLabel: { color: secondary, hideOverlap: true },
        splitLine: { show: false },
      },
      yAxis: {
        type: "value",
        scale: true,
        axisLabel: {
          color: secondary,
          formatter: (v: number) =>
            masked ? MASK : formatUsd(String(Math.round(v)), locale).replace(/[.,]00$/, ""),
        },
        splitLine: { lineStyle: { color: border, opacity: 0.5 } },
      },
      tooltip: {
        trigger: "axis",
        backgroundColor: cssVar("--bg-surface"),
        borderColor: border,
        textStyle: { color: cssVar("--text-primary") },
        formatter: (params: Array<{ dataIndex: number }>) => {
          const p = points[params[0]?.dataIndex ?? 0];
          if (!p) return "";
          const when = formatDateTime(p.t, locale, timeZone);
          const value = p.value === null ? t("chart.gap") : masked ? MASK : fmt(p.value);
          const flags = [
            p.estimated ? t("chart.estimated") : null,
            p.partial ? t("chart.partial") : null,
          ]
            .filter(Boolean)
            .join(" · ");
          // ECharts renders tooltip strings as HTML; every interpolated value is app-formatted text.
          return `${when}<br/><b>${value}</b>${flags ? `<br/><span class="chart-tooltip-meta">${flags}</span>` : ""}`;
        },
      },
      series: [
        {
          type: "line",
          showSymbol: false,
          connectNulls: false,
          lineStyle: { width: 2, color: accent },
          itemStyle: { color: accent },
          areaStyle: kind === "value" ? { color: cssVar("--accent-fill") } : undefined,
          data: points.map((p) => [p.t * 1000, p.value === null ? null : Number(p.value)]),
        },
      ],
    });
    const resize = new ResizeObserver(() => instance.resize());
    resize.observe(el.current);
    return () => {
      resize.disconnect();
      instance.dispose();
    };
    // fmt depends only on kind and locale.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [points, kind, masked, locale, timeZone, t, settings?.theme]);

  const first = points.find((p) => p.value !== null);
  const last = [...points].reverse().find((p) => p.value !== null);
  const show = (p?: SeriesPoint) => (!p?.value ? "—" : masked ? MASK : fmt(p.value));
  return (
    <div
      ref={el}
      className="chart"
      role="img"
      aria-label={t("chart.summaryOf", { label, from: show(first), to: show(last) })}
    />
  );
}
