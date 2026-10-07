import { useEffect } from "react";
import { useParams, Link } from "react-router";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api } from "../ipc/client";
import { useApp } from "../app/AppContext";
import { Page } from "../components/Layout";
import { WalletList } from "./WalletsPage";
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

export function WalletPage() {
  const { id = "" } = useParams();
  const { t } = useTranslation();
  const { setScope } = useApp();
  const wallets = useQuery({ queryKey: ["wallets"], queryFn: api.listWallets });
  const wallet = wallets.data?.find((w) => w.id === id);
  useEffect(() => setScope({ kind: "wallet", id }), [id, setScope]);
  return (
    <Page title={wallet?.label ?? t("nav.wallets")} showScope={false}>
      <Link to="/wallets">{t("nav.wallets")}</Link>
      {wallets.isSuccess && !wallet ? (
        <p>{t("errors.generic")}</p>
      ) : (
        <>
          <PortfolioView scope={{ kind: "wallet", id }} />
          <WalletList walletId={id} />
        </>
      )}
    </Page>
  );
}
