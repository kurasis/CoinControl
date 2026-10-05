import { useEffect } from "react";
import { useParams } from "react-router";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api } from "../ipc/client";
import { useApp } from "../app/AppContext";
import { Page } from "../components/Layout";
import { PortfolioView } from "./PortfolioPage";

/** Group detail: the same portfolio components, scoped to the group's wallets. */
export function GroupPage() {
  const { t } = useTranslation();
  const { id = "" } = useParams();
  const { setScope } = useApp();
  const groups = useQuery({ queryKey: ["groups"], queryFn: api.listGroups });
  const group = groups.data?.find((g) => g.id === id);
  useEffect(() => setScope({ kind: "group", id }), [id, setScope]);
  return (
    <Page title={group?.label ?? t("nav.groups")} showScope={false}>
      <p className="meta" title={t("scope.tooltip")}>
        {t("groups.scopeNote")}
      </p>
      <PortfolioView scope={{ kind: "group", id }} />
    </Page>
  );
}
