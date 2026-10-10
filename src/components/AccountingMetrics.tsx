import type { ReactNode } from "react";
import { Link } from "react-router";
import { useTranslation } from "react-i18next";
import type { PartialUsd } from "../ipc/bindings/PartialUsd";
import type { PeriodPerformanceDto } from "../ipc/bindings/PeriodPerformanceDto";
import type { UnavailableReason } from "../ipc/bindings/UnavailableReason";
import { DASH } from "../lib/format";
import { Percent, Usd } from "./Amount";

export function Metric({
  label,
  hint,
  children,
  note,
}: {
  label: string;
  hint?: string;
  children: ReactNode;
  note?: ReactNode;
}) {
  return (
    <div className="metric">
      <span className="meta">{label}</span>
      {hint && (
        <details className="metric-help">
          <summary aria-label={label + " — " + hint}>ⓘ</summary>
          <p className="meta">{hint}</p>
        </details>
      )}
      <span className="metric-value">{children}</span>
      {note && <span className="meta">{note}</span>}
    </div>
  );
}

/** A sum that may miss components: the known part is shown, never as complete. */
export function PartialAmount({ value }: { value: PartialUsd }) {
  const { t } = useTranslation();
  return (
    <span className="row-inline">
      <Usd value={value.known_usd} />
      {!value.complete && (
        <span className="chip chip-warning" title={t("accounting.partialHint")}>
          {t("accounting.partial")}
        </span>
      )}
    </span>
  );
}

export function reasonText(
  t: (k: string) => string,
  reason: UnavailableReason | null | undefined,
): string | null {
  return reason ? t(`accounting.reason.${reason}`) : null;
}

/** Unrealized P&L: complete figure, or the known-basis subset with its coverage. */
export function UnrealizedMetric({
  pnl,
  percent,
  reason,
  knownSubsetPnl,
  coveragePercent,
  estimated,
}: {
  pnl: string | null;
  percent: string | null;
  reason: UnavailableReason | null;
  knownSubsetPnl: string;
  coveragePercent: string | null;
  estimated: boolean;
}) {
  const { t } = useTranslation();
  let note: ReactNode = null;
  if (pnl === null && coveragePercent !== null && coveragePercent !== "0") {
    note = (
      <>
        {t("accounting.knownSubset")} <Usd value={knownSubsetPnl} /> ·{" "}
        {t("accounting.coverage", { percent: Number(coveragePercent).toFixed(0) })}
      </>
    );
  } else if (pnl === null) {
    note = reasonText(t, reason);
  } else if (estimated) {
    note = t("accounting.includesEstimated");
  }
  return (
    <Metric label={t("portfolio.unrealizedPnl")} hint={t("portfolio.unrealizedHint")} note={note}>
      {pnl === null ? (
        DASH
      ) : (
        <>
          <Usd value={pnl} /> <Percent value={percent} sensitive />
        </>
      )}
    </Metric>
  );
}

export function PeriodMetric({ performance }: { performance: PeriodPerformanceDto | null }) {
  const { t } = useTranslation();
  const p = performance;
  let note: ReactNode = t("portfolio.periodPending");
  if (p) {
    if (p.gain_usd === null) note = reasonText(t, p.reason);
    else
      note = (
        <>
          {t("accounting.netFlows")} <Usd value={p.net_flows_usd} />
          {p.estimated && ` · ${t("chart.estimatedChip")}`}
        </>
      );
  }
  return (
    <Metric label={t("portfolio.periodGain")} hint={t("portfolio.periodHint")} note={note}>
      {p?.gain_usd == null ? (
        DASH
      ) : (
        <>
          <Usd value={p.gain_usd} /> <Percent value={p.return_percent} sensitive />
        </>
      )}
    </Metric>
  );
}

export function ReviewBanner({
  review,
  reconciliation,
}: {
  review: number;
  reconciliation: number;
}) {
  const { t } = useTranslation();
  if (review + reconciliation === 0) return null;
  return (
    <div className="notice notice-center" role="status">
      <span>
        {review > 0 && t("accounting.reviewCount", { count: review })}
        {review > 0 && reconciliation > 0 && " · "}
        {reconciliation > 0 && t("accounting.reconciliationCount", { count: reconciliation })}
      </span>
      <div className="toolbar-spacer" />
      <Link className="btn" to="/review">
        {t("review.open")}
      </Link>
    </div>
  );
}
