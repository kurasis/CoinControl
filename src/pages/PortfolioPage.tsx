import { useState } from "react";
import { Link } from "react-router";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api, type ChartRange, type Scope } from "../ipc/client";
import { useApp } from "../app/AppContext";
import { useAccountLabels, useNetworkNames } from "../app/hooks";
import { Page } from "../components/Layout";
import { Usd } from "../components/Amount";
import { PortfolioChart, chartQuery } from "../components/PortfolioChart";
import {
  Metric,
  PartialAmount,
  PeriodMetric,
  ReviewBanner,
  UnrealizedMetric,
} from "../components/AccountingMetrics";
import { AssetTable } from "../components/AssetTable";
import { ActivityTable } from "../components/ActivityTable";
import { DASH, formatDateTime } from "../lib/format";

export function PortfolioPage() {
  const { t } = useTranslation();
  const { scope, profile } = useApp();
  const wallets = useQuery({ queryKey: ["wallets"], queryFn: api.listWallets });

  if (wallets.isSuccess && wallets.data.length === 0 && profile !== "demo") {
    return (
      <Page title={t("nav.portfolio")} showScope={false}>
        <Welcome />
      </Page>
    );
  }
  return (
    <Page title={t("nav.portfolio")}>
      <PortfolioView scope={scope} />
    </Page>
  );
}

/** Shared portfolio body used by the portfolio, wallet and group views. */
export function PortfolioView({ scope }: { scope: Scope }) {
  const { t } = useTranslation();
  const { locale, timeZone, privacy } = useApp();
  const [range, setRange] = useState<ChartRange>("1m");
  const networkNames = useNetworkNames();
  const accountLabels = useAccountLabels(privacy);
  const summary = useQuery({
    queryKey: ["summary", scope],
    queryFn: () => api.portfolioSummary(scope),
  });
  const holdings = useQuery({
    queryKey: ["holdings", scope],
    queryFn: () => api.listHoldings(scope),
  });
  const activity = useQuery({
    queryKey: ["activity", scope, "preview"],
    queryFn: () => api.listActivity(scope, null, 5),
  });
  const chart = useQuery(chartQuery(scope, range));
  const s = summary.data;
  const a = s?.accounting;

  return (
    <>
      <section aria-labelledby="balance-label">
        <div id="balance-label" className="balance-label">
          {t("portfolio.totalBalance")}
        </div>
        {summary.isLoading ? (
          <div className="skeleton skeleton-balance" />
        ) : (
          <div className="balance">
            {s?.total_value_usd == null ? (
              <span className="muted">{t("portfolio.valueUnavailable")}</span>
            ) : (
              <Usd value={s.total_value_usd} />
            )}
          </div>
        )}
        {s && (
          <div className="row meta">
            {s.last_successful_sync_at != null ? (
              <span>
                {t("portfolio.updated", {
                  time: formatDateTime(s.last_successful_sync_at, locale, timeZone),
                })}
              </span>
            ) : (
              <span>{t("portfolio.notSynced")}</span>
            )}
            {s.unpriced_count > 0 && (
              <span className="chip chip-warning">
                {t("portfolio.partialPrices", { count: s.unpriced_count })}
              </span>
            )}
            {s.stale_count > 0 && (
              <span className="chip chip-warning">
                {t("portfolio.staleBalances", { count: s.stale_count })}
              </span>
            )}
            {s.excluded_spam_count > 0 && (
              <span className="chip" title={t("portfolio.excludedHint")}>
                {t("portfolio.excluded", { count: s.excluded_spam_count })}
              </span>
            )}
          </div>
        )}
      </section>

      {s && (
        <ReviewBanner
          review={s.accounting.review_count}
          reconciliation={s.accounting.reconciliation_count}
        />
      )}

      <section className="card card-pad metrics" aria-label={t("portfolio.metrics")}>
        <UnrealizedMetric
          pnl={a?.unrealized_pnl_usd ?? null}
          percent={a?.unrealized_return_percent ?? null}
          reason={a?.unrealized_reason ?? null}
          knownSubsetPnl={a?.known_subset_pnl_usd ?? "0"}
          coveragePercent={a?.basis_coverage_percent ?? null}
          estimated={a?.has_estimated_basis ?? false}
        />
        <PeriodMetric performance={chart.data?.performance ?? null} />
        <Metric label={t("accounting.realized")} hint={t("accounting.realizedHint")}>
          {a ? <PartialAmount value={a.realized} /> : DASH}
        </Metric>
        <Metric label={t("accounting.income")} hint={t("accounting.incomeHint")}>
          {a ? <PartialAmount value={a.income} /> : DASH}
        </Metric>
        <Metric
          label={t("accounting.expenses")}
          hint={t("accounting.expensesHint")}
          note={a ? t("accounting.feeCharges", { count: a.fee_charges }) : null}
        >
          {a ? <PartialAmount value={a.expenses} /> : DASH}
        </Metric>
        <Metric
          label={t("accounting.total")}
          hint={t("accounting.totalHint")}
          note={a && a.total_accounted_pnl_usd === null ? t("accounting.totalIncomplete") : null}
        >
          <Usd value={a?.total_accounted_pnl_usd ?? null} />
        </Metric>
      </section>

      <PortfolioChart scope={scope} range={range} onRangeChange={setRange} />

      {holdings.isLoading ? (
        <div className="card skeleton skeleton-table" />
      ) : (
        <AssetTable rows={holdings.data ?? []} networkNames={networkNames} />
      )}

      <section className="card" aria-labelledby="recent-heading">
        <div className="section-header card-pad card-pad-head">
          <h2 id="recent-heading">{t("activity.recent")}</h2>
          <div className="toolbar-spacer" />
          <Link to="/activity">{t("activity.viewAll")}</Link>
        </div>
        <ActivityTable
          rows={activity.data?.rows ?? []}
          networkNames={networkNames}
          accountLabels={accountLabels}
        />
      </section>
    </>
  );
}

function Welcome() {
  const { t } = useTranslation();
  const { switchProfile } = useApp();
  return (
    <section className="card empty" aria-labelledby="welcome-heading">
      <img src="/app-icon.svg" alt="" width={56} height={56} />
      <h2 id="welcome-heading">{t("welcome.title")}</h2>
      <p>{t("welcome.body")}</p>
      <p className="meta">{t("welcome.privacy")}</p>
      <div className="empty-actions">
        <Link className="btn btn-primary" to="/wallets?add=1">
          {t("welcome.addAddress")}
        </Link>
        <Link className="btn" to="/settings/sources">
          {t("welcome.configureSources")}
        </Link>
        <button className="btn btn-ghost" onClick={() => void switchProfile("demo")}>
          {t("welcome.demo")}
        </button>
      </div>
    </section>
  );
}
