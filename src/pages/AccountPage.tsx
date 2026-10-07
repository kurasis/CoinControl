import { useEffect } from "react";
import { useParams, Link } from "react-router";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api } from "../ipc/client";
import { useApp } from "../app/AppContext";
import { Page } from "../components/Layout";
import { PortfolioView } from "./PortfolioPage";
import { AccountManagement } from "../components/AccountManagement";
import { SyncStatus } from "../components/SyncStatus";
import { openExternal } from "../lib/external";
import { MASK, formatDateTime } from "../lib/format";
import { SyncButton } from "./WalletsPage";

export function AccountPage() {
  const { id = "" } = useParams();
  const { t } = useTranslation();
  const { privacy, locale, timeZone, setScope } = useApp();
  useEffect(() => setScope({ kind: "accounts", ids: [id] }), [id, setScope]);
  const wallets = useQuery({ queryKey: ["wallets"], queryFn: api.listWallets });
  const coverage = useQuery({
    queryKey: ["account-coverage", id],
    queryFn: () => api.accountCoverage(id),
    refetchInterval: 5000,
  });
  const explorer = useQuery({
    queryKey: ["account-explorer", id],
    queryFn: () => api.accountExplorer(id),
  });
  const accounts = useQuery({ queryKey: ["accounts"], queryFn: () => api.listAccounts() });
  const status = useQuery({ queryKey: ["sync-status"], queryFn: api.listSyncStatus });
  const account = accounts.data?.find((a) => a.id === id);
  const sync = status.data?.find((s) => s.account_id === id);
  return (
    <Page title={account?.label ?? t("ops.account")} showScope={false}>
      <Link to="/wallets">{t("nav.wallets")}</Link>
      {account && (
        <p className="meta">
          {account.network} ·{" "}
          <span className="address">{privacy ? MASK : account.display_address}</span>
        </p>
      )}
      {account && (
        <section className="card card-pad stack">
          <div className="row">
            <SyncButton accountId={id} />
            <button
              className="btn"
              onClick={() => void navigator.clipboard?.writeText(account.display_address)}
            >
              {t("wallets.copyAddress")}
            </button>
            {explorer.data && (
              <button className="link-button" onClick={() => void openExternal(explorer.data!)}>
                {t("leg.explorer")}
              </button>
            )}
          </div>
          <AccountManagement account={account} wallets={wallets.data ?? []} />
          <p>
            {t("sync.source", { provider: sync?.provider ?? "—" })} ·{" "}
            {t("manage.earliest", {
              date: sync?.earliest_covered_at
                ? formatDateTime(sync.earliest_covered_at, locale, timeZone)
                : "—",
            })}
          </p>
          <ul>
            {(coverage.data ?? []).map((c) => (
              <li key={`${c.provider}:${c.category}`}>
                {c.provider} ·{" "}
                {t(`manage.categories.${c.category.replaceAll(":", "_")}`, {
                  defaultValue: t("manage.categories.history"),
                })}{" "}
                · {t(`manage.coverage.${c.coverage}`)} ·{" "}
                {c.earliest_covered_at
                  ? formatDateTime(c.earliest_covered_at, locale, timeZone)
                  : "—"}
              </li>
            ))}
          </ul>
          <SyncStatus />
        </section>
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
