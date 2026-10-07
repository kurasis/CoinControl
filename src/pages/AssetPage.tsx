import { useMemo, useState } from "react";
import { Link, useParams } from "react-router";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api, type ChartRange } from "../ipc/client";
import { useViewState } from "../app/useViewState";
import { useApp } from "../app/AppContext";
import { useAccountLabels, useNetworkNames } from "../app/hooks";
import { Page } from "../components/Layout";
import { Percent, Price, Quantity, Usd } from "../components/Amount";
import { Metric, PartialAmount, UnrealizedMetric } from "../components/AccountingMetrics";
import { RangePicker, toSeries } from "../components/PortfolioChart";
import { SeriesChart } from "../components/SeriesChart";
import { ActivityTable } from "../components/ActivityTable";
import { LegDrawer } from "../components/LegDrawer";
import { TokenIcon } from "../components/TokenIcon";
import { openExternal } from "../lib/external";
import { DASH, formatDate } from "../lib/format";

type Tab = "price" | "holdings";

/** Asset detail: identity, position, accounting, two charts, accounts, lots and activity. */
export function AssetPage() {
  const { t } = useTranslation();
  const { assetId: raw = "" } = useParams();
  const assetId = decodeURIComponent(raw);
  const { scope, locale, timeZone, privacy } = useApp();
  const networkNames = useNetworkNames();
  const accountLabels = useAccountLabels(privacy);
  const [range, setRange] = useViewState<ChartRange>("chart:range", "1m");
  const [window, setWindow] = useViewState<{ start: number; end: number } | undefined>(
    "chart:window",
    undefined,
  );
  const [tab, setTab] = useState<Tab>("price");
  const [openLeg, setOpenLeg] = useState<string | null>(null);

  const detail = useQuery({
    queryKey: ["asset", scope, assetId],
    queryFn: () => api.assetDetail(scope, assetId),
  });
  const chart = useQuery({
    queryKey: ["asset-chart", scope, assetId, range, window],
    queryFn: () => api.assetChart(scope, assetId, range, window),
  });
  const activity = useQuery({
    queryKey: ["activity", scope, "asset", assetId],
    queryFn: () => api.listActivity(scope, null, 50, { asset_id: assetId }),
  });

  const series = useMemo(() => {
    if (!chart.data) return [];
    return tab === "price"
      ? chart.data.price.map((p) => ({ t: p.t, value: p.price_usd, estimated: p.estimated }))
      : toSeries(chart.data.holdings.points);
  }, [chart.data, tab]);
  const hasData = series.some((p) => p.value !== null);

  const d = detail.data;
  const title = d ? (d.name ?? d.symbol ?? t("assets.unknownAsset")) : t("asset.title");
  return (
    <Page title={title}>
      <nav aria-label={t("asset.breadcrumb")} className="meta">
        <Link to="/">{t("nav.portfolio")}</Link> / {d?.symbol ?? assetId}
      </nav>
      {detail.isError && <p className="field-error">{t("asset.notFound")}</p>}
      {detail.isLoading && <div className="skeleton skeleton-balance" />}
      {d && (
        <>
          <section className="row" aria-label={t("asset.identity")}>
            <TokenIcon assetId={d.asset_id} symbol={d.symbol} />
            <span>
              {d.symbol ?? "?"} · {networkNames.get(d.network) ?? d.network}
              {d.verification !== "verified" && ` · ${t("assets.unverified")}`}
            </span>
            {d.contract && (
              <code className="address meta truncate" title={d.contract}>
                {d.contract}
              </code>
            )}
            {d.explorer_url && (
              <button className="link-button" onClick={() => void openExternal(d.explorer_url!)}>
                {t("leg.explorer")}
              </button>
            )}
          </section>
          <section className="card card-pad metrics" aria-label={t("asset.position")}>
            <Metric label={t("assets.colPrice")}>
              <Price value={d.price_usd} /> <Percent value={d.change_24h_percent} />
            </Metric>
            <Metric label={t("asset.quantity")}>
              <Quantity value={d.quantity} symbol={d.symbol} />
            </Metric>
            <Metric label={t("assets.colValue")}>
              <Usd value={d.value_usd} />
            </Metric>
            <Metric
              label={t("asset.remainingBasis")}
              note={
                d.basis_coverage_quantity_percent !== null
                  ? t("asset.basisCoverage", {
                      percent: Number(d.basis_coverage_quantity_percent).toFixed(0),
                    })
                  : null
              }
            >
              <Usd value={d.remaining_basis_usd} />
            </Metric>
            <UnrealizedMetric
              pnl={d.unrealized_pnl_usd}
              percent={d.unrealized_return_percent}
              reason={d.unrealized_reason}
              knownSubsetPnl={d.known_subset_pnl_usd}
              coveragePercent={d.basis_coverage_quantity_percent}
              estimated={d.has_estimated_basis}
            />
            <Metric label={t("accounting.realized")} hint={t("accounting.realizedHint")}>
              <PartialAmount value={d.realized} />
            </Metric>
            <Metric label={t("accounting.income")}>
              <PartialAmount value={d.income} />
            </Metric>
            <Metric label={t("accounting.expenses")}>
              <PartialAmount value={d.expenses} />
            </Metric>
          </section>
        </>
      )}

      <section className="card chart-card" aria-label={t("asset.charts")}>
        <div className="chart-toolbar">
          <div className="segmented" role="tablist" aria-label={t("asset.charts")}>
            {(["price", "holdings"] as Tab[]).map((x) => (
              <button
                key={x}
                role="tab"
                aria-selected={tab === x}
                aria-pressed={tab === x}
                onClick={() => setTab(x)}
              >
                {t(`asset.tab.${x}`)}
              </button>
            ))}
          </div>
          <div className="toolbar-spacer" />
          {window && (
            <span className="meta">
              {formatDate(window.start, locale, timeZone)} —{" "}
              {formatDate(window.end, locale, timeZone)}
            </span>
          )}
          <RangePicker
            custom={!!window}
            range={range}
            onRangeChange={(r) => {
              setRange(r);
              setWindow(undefined);
            }}
          />
        </div>
        {chart.isLoading ? (
          <div className="chart skeleton" />
        ) : hasData ? (
          <SeriesChart
            points={series}
            kind={tab === "price" ? "price" : "value"}
            label={t(`asset.tab.${tab}`)}
          />
        ) : (
          <div className="chart-empty">
            <p>{t("chart.empty")}</p>
          </div>
        )}
      </section>

      {d && d.accounts.length > 0 && (
        <section className="card" aria-labelledby="accounts-heading">
          <div className="section-header card-pad card-pad-head">
            <h2 id="accounts-heading">{t("asset.byAccount")}</h2>
          </div>
          <div className="table-scroll">
            <table className="table">
              <thead>
                <tr>
                  <th>{t("activity.colAccount")}</th>
                  <th className="right">{t("assets.colBalance")}</th>
                  <th className="right">{t("assets.colValue")}</th>
                  <th className="right">{t("asset.basis")}</th>
                  <th className="right">{t("assets.colPnl")}</th>
                </tr>
              </thead>
              <tbody>
                {d.accounts.map((a) => (
                  <tr key={a.account_id}>
                    <td>
                      <Link to={`/accounts/${a.account_id}`}>
                        {accountLabels.get(a.account_id) ?? "—"}
                      </Link>
                      {a.balance_status !== "fresh" && (
                        <span className="meta warning">
                          {" "}
                          · {t(`balanceStatus.${a.balance_status}`)}
                        </span>
                      )}
                    </td>
                    <td className="right">
                      <Quantity value={a.quantity} symbol={d.symbol} />
                    </td>
                    <td className="right">
                      <Usd value={a.value_usd} />
                    </td>
                    <td className="right">
                      {a.basis_usd !== null ? (
                        <Usd value={a.basis_usd} />
                      ) : (
                        <span className="meta">{t(`basisCoverage.${a.basis_coverage}`)}</span>
                      )}
                    </td>
                    <td className="right">
                      <Usd value={a.unrealized_pnl_usd} />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </section>
      )}

      {d && d.lots.length > 0 && (
        <section className="card" aria-labelledby="lots-heading">
          <div className="section-header card-pad card-pad-head">
            <h2 id="lots-heading">{t("asset.lots")}</h2>
            <span className="meta">{t("asset.lotsHint")}</span>
          </div>
          <div className="table-scroll">
            <table className="table table-compact">
              <thead>
                <tr>
                  <th>{t("asset.acquired")}</th>
                  <th>{t("activity.colAccount")}</th>
                  <th className="right">{t("asset.remaining")}</th>
                  <th className="right">{t("asset.basis")}</th>
                  <th>{t("asset.basisKind")}</th>
                </tr>
              </thead>
              <tbody>
                {d.lots.map((l) => (
                  <tr key={l.id}>
                    <td className="num">
                      {formatDate(l.acquired_at, locale, timeZone)}
                      {l.arrived_at !== l.acquired_at && (
                        <span className="meta">
                          {" "}
                          ·{" "}
                          {t("asset.arrived", { date: formatDate(l.arrived_at, locale, timeZone) })}
                        </span>
                      )}
                    </td>
                    <td>{accountLabels.get(l.account_id) ?? "—"}</td>
                    <td className="right">
                      <Quantity value={l.remaining_quantity} symbol={d.symbol} />
                    </td>
                    <td className="right">
                      {l.remaining_basis_usd === null ? (
                        DASH
                      ) : (
                        <Usd value={l.remaining_basis_usd} />
                      )}
                    </td>
                    <td>{t(`basisKind.${l.basis_kind}`)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </section>
      )}

      <section className="card" aria-labelledby="asset-activity-heading">
        <div className="section-header card-pad card-pad-head">
          <h2 id="asset-activity-heading">{t("asset.activity")}</h2>
        </div>
        <ActivityTable
          rows={activity.data?.rows ?? []}
          networkNames={networkNames}
          accountLabels={accountLabels}
          onOpenLeg={setOpenLeg}
        />
      </section>
      {openLeg && <LegDrawer legId={openLeg} onClose={() => setOpenLeg(null)} />}
    </Page>
  );
}
