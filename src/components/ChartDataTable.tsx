import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useApp } from "../app/AppContext";
import { MASK, formatDateTime, formatPrice, formatUsd } from "../lib/format";
import type { SeriesPoint } from "./SeriesChart";

const PAGE_SIZE = 50;

/** The same exact observations as the canvas, including gaps and coverage flags. */
export function ChartDataTable({
  points,
  kind,
  label,
}: {
  points: SeriesPoint[];
  kind: "value" | "price";
  label: string;
}) {
  const { t } = useTranslation();
  const { locale, privacy, timeZone } = useApp();
  const [page, setPage] = useState(0);
  const pages = Math.max(1, Math.ceil(points.length / PAGE_SIZE));
  const currentPage = Math.min(page, pages - 1);
  const masked = kind === "value" && privacy;
  const format = kind === "price" ? formatPrice : formatUsd;
  const name = t("chart.dataTitle", { label });

  return (
    <details className="chart-data">
      <summary>{t("chart.viewData", { label })}</summary>
      <p className="meta">
        {t("chart.dataSummary", {
          count: points.length,
          gaps: points.filter((p) => p.value === null).length,
        })}
      </p>
      <div className="table-scroll chart-data-scroll" tabIndex={0} role="region" aria-label={name}>
        <table className="table">
          <caption>{name}</caption>
          <thead>
            <tr>
              <th scope="col">{t("chart.observedAt")}</th>
              <th scope="col" className="num">
                {t("chart.usdValue")}
              </th>
              <th scope="col">{t("chart.quality")}</th>
            </tr>
          </thead>
          <tbody>
            {points.slice(currentPage * PAGE_SIZE, (currentPage + 1) * PAGE_SIZE).map((p, i) => {
              const flags = [
                p.value === null ? t("chart.gap") : null,
                p.estimated ? t("chart.estimated") : null,
                p.partial ? t("chart.partial") : null,
              ].filter(Boolean);
              return (
                <tr key={`${p.t}:${i}`}>
                  <th scope="row">{formatDateTime(p.t, locale, timeZone)}</th>
                  <td className="num">
                    {p.value === null ? "—" : masked ? MASK : format(p.value, locale)}
                  </td>
                  <td>{flags.length ? flags.join(" · ") : t("chart.available")}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      {pages > 1 && (
        <nav className="row chart-data-pages" aria-label={t("chart.dataPages", { label })}>
          <button
            className="btn"
            disabled={currentPage === 0}
            onClick={() => setPage(currentPage - 1)}
          >
            {t("chart.previousPage")}
          </button>
          <span role="status">{t("chart.pageNumber", { page: currentPage + 1, pages })}</span>
          <button
            className="btn"
            disabled={currentPage === pages - 1}
            onClick={() => setPage(currentPage + 1)}
          >
            {t("chart.nextPage")}
          </button>
        </nav>
      )}
    </details>
  );
}
