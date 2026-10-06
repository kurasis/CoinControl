import { lazy, Suspense } from "react";
import { useTranslation } from "react-i18next";
import { useApp } from "../app/AppContext";
import { MASK, formatPrice, formatUsd } from "../lib/format";
import { ChartDataTable } from "./ChartDataTable";

const ChartCanvas = lazy(() => import("./SeriesChartCanvas"));

export interface SeriesPoint {
  t: number;
  /** Exact decimal string; `null` is a gap, never zero. */
  value: string | null;
  estimated: boolean;
  partial?: boolean;
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
  const { locale, privacy } = useApp();
  const masked = kind === "value" && privacy;
  const fmt = (v: string) => (kind === "price" ? formatPrice(v, locale) : formatUsd(v, locale));
  const first = points.find((p) => p.value !== null);
  const last = [...points].reverse().find((p) => p.value !== null);
  const show = (p?: SeriesPoint) => (!p?.value ? "—" : masked ? MASK : fmt(p.value));
  const ariaLabel = t("chart.summaryOf", { label, from: show(first), to: show(last) });
  return (
    <>
      <Suspense
        fallback={
          <div className="chart skeleton" role="img" aria-label={ariaLabel} aria-busy="true" />
        }
      >
        <ChartCanvas points={points} kind={kind} ariaLabel={ariaLabel} />
      </Suspense>
      <ChartDataTable
        key={`${points[0]?.t}:${points.at(-1)?.t}:${points.length}`}
        points={points}
        kind={kind}
        label={label}
      />
    </>
  );
}
