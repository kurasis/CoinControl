import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api, type ChartRange, type Scope } from "../ipc/client";
import type { ChartPoint } from "../ipc/bindings/ChartPoint";
import { useApp } from "../app/AppContext";
import { formatDate } from "../lib/format";
import { SeriesChart } from "./SeriesChart";

export const RANGES: ChartRange[] = ["24h", "7d", "1m", "3m", "1y", "all"];

export function chartQuery(
  scope: Scope,
  range: ChartRange,
  window?: { start: number; end: number },
) {
  return {
    queryKey: ["chart", scope, range, window],
    queryFn: () => api.chart(scope, range, window),
  };
}

export function toSeries(points: ChartPoint[]) {
  return points.map((p) => ({
    t: p.t,
    value: p.value_usd,
    estimated: p.estimated,
    partial: p.partial,
  }));
}

export function RangePicker({
  range,
  onRangeChange,
  custom = false,
}: {
  range: ChartRange;
  onRangeChange: (r: ChartRange) => void;
  custom?: boolean;
}) {
  const { t } = useTranslation();
  return (
    <div className="segmented" role="group" aria-label={t("chart.range")}>
      {RANGES.map((r) => (
        <button key={r} aria-pressed={!custom && r === range} onClick={() => onRangeChange(r)}>
          {t(`chart.ranges.${r}`)}
        </button>
      ))}
    </div>
  );
}

export function PortfolioChart({
  scope,
  range,
  onRangeChange,
  window,
  enabled = true,
}: {
  scope: Scope;
  range: ChartRange;
  window?: { start: number; end: number };
  onRangeChange: (r: ChartRange) => void;
  enabled?: boolean;
}) {
  const { t } = useTranslation();
  const { locale, timeZone } = useApp();
  const chart = useQuery({ ...chartQuery(scope, range, window), enabled });
  const points = useMemo(() => toSeries(chart.data?.points ?? []), [chart.data]);
  const hasData = points.some((p) => p.value !== null);
  const anyEstimated = points.some((p) => p.estimated);
  const anyPartial = points.some((p) => p.partial);

  return (
    <section className="card chart-card" aria-label={t("chart.label")}>
      <div className="chart-toolbar">
        <RangePicker range={range} custom={!!window} onRangeChange={onRangeChange} />
        <div className="toolbar-spacer" />
        {chart.data?.history_available_since != null && (
          <span className="meta">
            {t("chart.since", {
              date: formatDate(chart.data.history_available_since, locale, timeZone),
            })}
          </span>
        )}
        {anyEstimated && <span className="chip">{t("chart.estimatedChip")}</span>}
        {anyPartial && <span className="chip chip-warning">{t("chart.partialChip")}</span>}
      </div>
      {chart.isPending ? (
        <div className="chart skeleton" />
      ) : hasData ? (
        <SeriesChart points={points} kind="value" label={t("chart.label")} />
      ) : (
        <div className="chart-empty">
          <p>{t("chart.empty")}</p>
        </div>
      )}
    </section>
  );
}
