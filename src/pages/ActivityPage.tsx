import { useState } from "react";
import { useInfiniteQuery } from "@tanstack/react-query";
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
  const [unresolved, setUnresolved] = useState(false);
  const [openLeg, setOpenLeg] = useState<string | null>(null);
  const pages = useInfiniteQuery({
    queryKey: ["activity", scope, "all", unresolved],
    queryFn: ({ pageParam }) =>
      api.listActivity(scope, pageParam, 50, { unresolved_only: unresolved }),
    initialPageParam: null as string | null,
    getNextPageParam: (last) => last.next_cursor,
  });
  const rows = pages.data?.pages.flatMap((p) => p.rows) ?? [];
  return (
    <Page title={t("nav.activity")}>
      <section className="card">
        <div className="section-header card-pad card-pad-head">
          <label className="row-inline">
            <input
              type="checkbox"
              checked={unresolved}
              onChange={(e) => setUnresolved(e.target.checked)}
            />
            {t("activity.unresolvedOnly")}
          </label>
        </div>
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
