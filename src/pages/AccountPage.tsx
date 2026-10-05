import { useParams, Link } from "react-router";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api } from "../ipc/client";
import { useApp } from "../app/AppContext";
import { Page } from "../components/Layout";
import { PortfolioView } from "./PortfolioPage";
import { MASK } from "../lib/format";

export function AccountPage() {
  const { id = "" } = useParams();
  const { t } = useTranslation();
  const { privacy } = useApp();
  const accounts = useQuery({ queryKey: ["accounts"], queryFn: () => api.listAccounts() });
  const status = useQuery({ queryKey: ["sync-status"], queryFn: api.listSyncStatus });
  const account = accounts.data?.find((a) => a.id === id);
  const sync = status.data?.find((s) => s.account_id === id);
  return (
    <Page title={account?.label ?? t("ops.account")} showScope={false}>
      {account && (
        <p className="meta">
          {account.network} ·{" "}
          <span className="address">{privacy ? MASK : account.display_address}</span>
        </p>
      )}
      {sync?.last_error && (
        <p role="alert" className="field-error">
          {sync.last_error}
        </p>
      )}
      {accounts.isSuccess && !account ? (
        <p>{t("errors.generic")}</p>
      ) : (
        <>
          <Link className="btn" to={`/activity?account=${encodeURIComponent(id)}`}>
            {t("nav.activity")}
          </Link>
          <PortfolioView scope={{ kind: "accounts", ids: [id] }} />
        </>
      )}
    </Page>
  );
}
