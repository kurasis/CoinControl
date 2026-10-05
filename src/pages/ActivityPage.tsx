import { useState } from "react";
import { useSearchParams } from "react-router";
import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api } from "../ipc/client";
import { useApp } from "../app/AppContext";
import { useAccountLabels, useNetworkNames } from "../app/hooks";
import { Page } from "../components/Layout";
import { ActivityTable } from "../components/ActivityTable";
import { LegDrawer } from "../components/LegDrawer";

export function ActivityPage() {
  const { t } = useTranslation();
  const { scope, privacy } = useApp();
  const networkNames = useNetworkNames();
  const accountLabels = useAccountLabels(privacy);
  const [params, setParams] = useSearchParams();
  const [filter, setFilter] = useState({
    network: "",
    asset: "",
    operation: "",
    status: "",
    start: "",
    end: "",
  });
  const accounts = useQuery({ queryKey: ["accounts"], queryFn: () => api.listAccounts() });
  const assets = useQuery({
    queryKey: ["holdings", { kind: "all" }],
    queryFn: () => api.listHoldings({ kind: "all" }),
  });
  const account = params.get("account") ?? "";
  const activeScope = account ? { kind: "accounts" as const, ids: [account] } : scope;
  const datesValid = !filter.start || !filter.end || filter.start <= filter.end;
  const applied = {
    account_id: account || null,
    network: (filter.network || null) as import("../ipc/client").NetworkId | null,
    asset_id: filter.asset || null,
    operation: filter.operation || null,
    status: filter.status || null,
    start: filter.start ? Date.parse(filter.start + "T00:00:00Z") / 1000 : null,
    end: filter.end ? Date.parse(filter.end + "T23:59:59Z") / 1000 : null,
  };
  const [unresolved, setUnresolved] = useState(false);
  const [openLeg, setOpenLeg] = useState<string | null>(null);
  const pages = useInfiniteQuery({
    queryKey: ["activity", activeScope, "all", applied, unresolved],
    enabled: datesValid,
    queryFn: ({ pageParam }) =>
      api.listActivity(activeScope, pageParam, 50, { ...applied, unresolved_only: unresolved }),
    initialPageParam: null as string | null,
    getNextPageParam: (last) => last.next_cursor,
  });
  const rows = pages.data?.pages.flatMap((p) => p.rows) ?? [];
  return (
    <Page title={t("nav.activity")}>
      <section className="card">
        <div className="section-header card-pad card-pad-head">
          <div className="row activity-filters">
            <label>
              {t("activity.colAccount")}
              <select
                className="select"
                value={account}
                onChange={(e) => setParams(e.target.value ? { account: e.target.value } : {})}
              >
                <option value="">{t("ops.any")}</option>
                {accounts.data?.map((a) => (
                  <option key={a.id} value={a.id}>
                    {accountLabels.get(a.id) ?? a.network}
                  </option>
                ))}
              </select>
            </label>
            <label>
              {t("activity.colNetwork")}
              <select
                className="select"
                value={filter.network}
                onChange={(e) => setFilter({ ...filter, network: e.target.value })}
              >
                <option value="">{t("ops.any")}</option>
                {[...networkNames].map(([id, name]) => (
                  <option key={id} value={id}>
                    {name}
                  </option>
                ))}
              </select>
            </label>
            <label>
              {t("ops.asset")}
              <select
                className="select"
                value={filter.asset}
                onChange={(e) => setFilter({ ...filter, asset: e.target.value })}
              >
                <option value="">{t("ops.any")}</option>
                {assets.data?.map((a) => (
                  <option key={a.asset_id} value={a.asset_id}>
                    {a.symbol ?? a.asset_id} · {a.network}
                  </option>
                ))}
              </select>
            </label>
            <label>
              {t("activity.colType")}
              <select
                className="select"
                value={filter.operation}
                onChange={(e) => setFilter({ ...filter, operation: e.target.value })}
              >
                <option value="">{t("ops.any")}</option>
                {["send", "receive", "swap", "approve", "fee"].map((v) => (
                  <option key={v} value={v}>
                    {t(`activity.op.${v}`)}
                  </option>
                ))}
              </select>
            </label>
            <label>
              {t("activity.colStatus")}
              <select
                className="select"
                value={filter.status}
                onChange={(e) => setFilter({ ...filter, status: e.target.value })}
              >
                <option value="">{t("ops.any")}</option>
                {["confirmed", "final", "pending", "failed", "reorged"].map((v) => (
                  <option key={v} value={v}>
                    {t(`activity.status.${v}`)}
                  </option>
                ))}
              </select>
            </label>
            <label>
              {t("ops.fromUtc")}
              <input
                className="input"
                type="date"
                value={filter.start}
                onChange={(e) => setFilter({ ...filter, start: e.target.value })}
              />
            </label>
            <label>
              {t("ops.toUtc")}
              <input
                className="input"
                type="date"
                value={filter.end}
                onChange={(e) => setFilter({ ...filter, end: e.target.value })}
              />
            </label>
          </div>
          <label className="row-inline">
            <input
              type="checkbox"
              checked={unresolved}
              onChange={(e) => setUnresolved(e.target.checked)}
            />
            {t("activity.unresolvedOnly")}
          </label>
        </div>
        {!datesValid && <p role="alert">{t("ops.invalidDates")}</p>}
        {pages.isError && <p role="alert">{t("errors.generic")}</p>}
        <ActivityTable
          rows={rows}
          networkNames={networkNames}
          accountLabels={accountLabels}
          onOpenLeg={setOpenLeg}
        />
        {pages.hasNextPage && (
          <div className="row card-pad">
            <button
              className="btn"
              onClick={() => void pages.fetchNextPage()}
              disabled={pages.isFetchingNextPage}
            >
              {t("activity.loadMore")}
            </button>
          </div>
        )}
      </section>
      {openLeg && <LegDrawer legId={openLeg} onClose={() => setOpenLeg(null)} />}
    </Page>
  );
}
