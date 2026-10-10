import { WindowedTableBody } from "../components/WindowedTableBody";
import { useState } from "react";
import { Link } from "react-router";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api } from "../ipc/client";
import { useApp } from "../app/AppContext";
import { useAccountLabels, useNetworkNames } from "../app/hooks";
import { Page } from "../components/Layout";
import { Quantity, Usd } from "../components/Amount";
import { LegDrawer } from "../components/LegDrawer";
import { ImportDialog } from "../components/ImportDialog";
import { assetPath } from "../components/AssetTable";
import { formatDateTime } from "../lib/format";

const REASONS = [
  "unknown_basis",
  "unclassified_outgoing",
  "missing_proceeds",
  "missing_price",
  "missing_trade_value",
] as const;

/** "Review missing data": movements that need a decision and reconciliation findings. */
export function ReviewPage() {
  const { t } = useTranslation();
  const { scope, locale, timeZone, privacy } = useApp();
  const networkNames = useNetworkNames();
  const accountLabels = useAccountLabels(privacy);
  const [reason, setReason] = useState<string>("");
  const [open, setOpen] = useState<string | null>(null);
  const [importing, setImporting] = useState(false);
  const list = useQuery({
    queryKey: ["review", scope],
    queryFn: () => api.listReviewItems(scope, 500),
  });
  const items = (list.data?.items ?? []).filter((i) => !reason || i.reason === reason);

  return (
    <Page
      title={t("review.title")}
      actions={
        <button className="btn" onClick={() => setImporting(true)}>
          {t("import.button")}
        </button>
      }
    >
      <p className="meta">{t("review.intro")}</p>
      <section className="card" aria-labelledby="review-heading">
        <div className="section-header card-pad card-pad-head">
          <h2 id="review-heading">
            {t("review.items")}
            {list.data && ` · ${list.data.total}`}
          </h2>
          <div className="toolbar-spacer" />
          <select
            className="select"
            aria-label={t("review.filter")}
            value={reason}
            onChange={(e) => setReason(e.target.value)}
          >
            <option value="">{t("review.allReasons")}</option>
            {REASONS.map((r) => (
              <option key={r} value={r}>
                {t(`review.reason.${r}`)}
              </option>
            ))}
          </select>
        </div>
        {list.isError && (
          <div className="card-pad" role="alert">
            <p>{t("errors.generic")}</p>
            <button className="btn" disabled={list.isFetching} onClick={() => void list.refetch()}>
              {t("common.retry")}
            </button>
          </div>
        )}
        {list.isLoading ? (
          <div className="skeleton skeleton-table" />
        ) : list.isError && !list.data ? null : items.length === 0 ? (
          <div className="empty">
            <p>{t("review.empty")}</p>
          </div>
        ) : (
          <div className="table-scroll">
            <table className="table">
              <thead>
                <tr>
                  <th>{t("activity.colTime")}</th>
                  <th>{t("review.colReason")}</th>
                  <th className="right">{t("activity.colMovement")}</th>
                  <th className="right">{t("activity.colValue")}</th>
                  <th>{t("activity.colAccount")}</th>
                  <th>{t("activity.colNetwork")}</th>
                  <th>
                    <span className="visually-hidden">{t("review.action")}</span>
                  </th>
                </tr>
              </thead>
              <WindowedTableBody rows={items} columns={7} rowKey={(i) => i.leg_id}>
                {(i) => (
                  <>
                    <td className="num">{formatDateTime(i.occurred_at, locale, timeZone)}</td>
                    <td>
                      <span className="chip chip-warning">{t(`review.reason.${i.reason}`)}</span>
                    </td>
                    <td className="right">
                      <span className={i.quantity.startsWith("-") ? "negative" : "positive"}>
                        {i.quantity.startsWith("-") ? "" : "+"}
                        <Quantity value={i.quantity} symbol={i.symbol} />
                      </span>
                    </td>
                    <td className="right">
                      <Usd value={i.value_usd} />
                    </td>
                    <td>{accountLabels.get(i.account_id) ?? "—"}</td>
                    <td>{networkNames.get(i.network) ?? i.network}</td>
                    <td className="right">
                      <button className="btn" onClick={() => setOpen(i.leg_id)}>
                        {t("review.resolve")}
                      </button>
                    </td>
                  </>
                )}
              </WindowedTableBody>
            </table>
          </div>
        )}
      </section>

      {(list.data?.reconciliation.length ?? 0) > 0 && (
        <section className="card" aria-labelledby="recon-heading">
          <div className="section-header card-pad card-pad-head">
            <h2 id="recon-heading">{t("review.reconciliation")}</h2>
          </div>
          <ul className="list">
            {list.data!.reconciliation.map((r) => (
              <li key={r.id} className="list-row">
                <div className="stack">
                  <span>{t(`reconciliation.${r.kind}`, { defaultValue: r.kind })}</span>
                  <span className="meta">
                    {r.account_id && (accountLabels.get(r.account_id) ?? "")}
                    {r.quantity && (
                      <>
                        {" · "}
                        <Quantity value={r.quantity} symbol={r.symbol} />
                      </>
                    )}
                    {" · "}
                    {t(`reconciliation.explain.${r.kind}`, { defaultValue: r.detail })}
                  </span>
                </div>
                {r.asset_id && (
                  <Link to={assetPath(r.asset_id)} className="meta">
                    {t("review.viewAsset")}
                  </Link>
                )}
              </li>
            ))}
          </ul>
        </section>
      )}

      {open && <LegDrawer legId={open} onClose={() => setOpen(null)} />}
      {importing && <ImportDialog onClose={() => setImporting(false)} />}
    </Page>
  );
}
