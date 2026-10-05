import { WindowedTableBody } from "./WindowedTableBody";
import { useTranslation } from "react-i18next";
import type { ActivityRow } from "../ipc/bindings/ActivityRow";
import type { NetworkId } from "../ipc/client";
import { useApp } from "../app/AppContext";
import { formatDateTime } from "../lib/format";
import { Quantity, Usd } from "./Amount";

export function ActivityTable({
  rows,
  networkNames,
  accountLabels,
  onOpenLeg,
}: {
  rows: ActivityRow[];
  networkNames: Map<NetworkId, string>;
  accountLabels: Map<string, string>;
  /** Opens the movement detail drawer. */
  onOpenLeg?: (legId: string) => void;
}) {
  const { t } = useTranslation();
  const { locale, timeZone } = useApp();
  if (rows.length === 0) {
    return (
      <div className="empty">
        <p>{t("activity.empty")}</p>
      </div>
    );
  }
  return (
    <div className="table-scroll">
      <table className="table">
        <thead>
          <tr>
            <th>{t("activity.colTime")}</th>
            <th>{t("activity.colType")}</th>
            <th>{t("activity.colStatus")}</th>
            <th className="right">{t("activity.colMovement")}</th>
            <th className="right">{t("activity.colValue")}</th>
            <th className="right">{t("activity.colFee")}</th>
            <th>{t("activity.colAccount")}</th>
            <th>{t("activity.colNetwork")}</th>
          </tr>
        </thead>
        <WindowedTableBody
          rows={rows}
          columns={8}
          rowKey={(r) => `${r.transaction_id}:${r.account_id}`}
        >
          {(r) => (
            <>
              <td className="num">{formatDateTime(r.occurred_at, locale, timeZone)}</td>
              <td>
                <span className="row">
                  {t(`activity.op.${r.operation}`, { defaultValue: r.operation })}
                  {r.unresolved && (
                    <span className="chip chip-warning">{t("activity.needsReview")}</span>
                  )}
                </span>
              </td>
              <td>{t(`activity.status.${r.status}`, { defaultValue: r.status })}</td>
              <td className="right">
                <div className="stack">
                  {r.legs.map((l) => (
                    <span key={l.leg_id} className="row-inline">
                      <span className={l.signed_quantity.startsWith("-") ? "negative" : "positive"}>
                        {l.signed_quantity.startsWith("-") ? "" : "+"}
                        <Quantity value={l.signed_quantity} symbol={l.symbol} />
                      </span>
                      {onOpenLeg && (
                        <button
                          className={
                            l.review ? "chip chip-warning chip-button" : "chip chip-button"
                          }
                          onClick={() => onOpenLeg(l.leg_id)}
                          aria-label={t("activity.openLeg", {
                            symbol: l.symbol ?? "",
                          })}
                        >
                          {l.review
                            ? t(`review.reason.${l.review}`, { defaultValue: l.review })
                            : l.treatment
                              ? t(`treatment.${l.treatment}`, { defaultValue: l.treatment })
                              : t("activity.details")}
                        </button>
                      )}
                    </span>
                  ))}
                </div>
              </td>
              <td className="right">
                <div className="stack">
                  {r.legs.map((l) => (
                    <span
                      key={l.leg_id}
                      title={l.value_estimated ? t("chart.estimated") : undefined}
                    >
                      <Usd value={l.value_usd} />
                      {l.value_usd !== null && l.value_estimated && (
                        <span className="meta"> ≈</span>
                      )}
                    </span>
                  ))}
                </div>
              </td>
              <td className="right">
                {r.fee_quantity ? (
                  <div className="stack">
                    <Quantity value={r.fee_quantity} symbol={r.fee_symbol} />
                    {r.fee_value_usd !== null && (
                      <span className="meta">
                        <Usd value={r.fee_value_usd} />
                      </span>
                    )}
                  </div>
                ) : (
                  <span className="muted">—</span>
                )}
              </td>
              <td>{accountLabels.get(r.account_id) ?? "—"}</td>
              <td>{networkNames.get(r.network) ?? r.network}</td>
            </>
          )}
        </WindowedTableBody>
      </table>
    </div>
  );
}
