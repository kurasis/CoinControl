import { Link } from "react-router";
import { useTranslation } from "react-i18next";
import { useApp } from "../app/AppContext";
import { formatDateTime } from "../lib/format";

export function Spinner() {
  return <span className="spinner" aria-hidden="true" />;
}

/** Same backend state for manual, newly added and scheduled account/price runs. */
export function SyncStatus({ compact = false }: { compact?: boolean }) {
  const { t } = useTranslation();
  const { syncProgress: progress, profile, locale, timeZone } = useApp();
  if (profile !== "real") return null;
  const running = progress?.running;
  const d = progress?.details;
  const outcome = d?.outcome || "idle";
  const label = running
    ? progress.cancel_requested
      ? t("syncState.stopping")
      : t(`syncState.phases.${d?.phase || "preparing"}`)
    : outcome === "completed" && d?.kind === "prices"
      ? t("syncState.pricesDone")
      : t(`syncState.outcomes.${outcome}`);
  const tone =
    outcome === "errors" || outcome === "failed"
      ? "negative"
      : outcome === "partial"
        ? "warning"
        : "";
  const content = (
    <>
      {running ? <Spinner /> : <span className={`sync-dot ${tone}`} aria-hidden="true" />}
      <span className="sync-status-text">
        <strong>{label}</strong>
        {running && d && d.total_accounts > 0 && (
          <span className="meta">
            {t("syncState.accounts", { done: d.completed_accounts, total: d.total_accounts })}
          </span>
        )}
        {!compact && running && d && (
          <span className="meta">{t("syncState.pages", { count: d.pages_fetched })}</span>
        )}
        {!running && d && d.error_count > 0 && (
          <span className="meta">{t("syncState.errors", { count: d.error_count })}</span>
        )}
        {!compact && !running && d?.finished_at && (
          <span className="meta">{formatDateTime(d.finished_at, locale, timeZone)}</span>
        )}
        {!compact && outcome === "partial" && (
          <span className="meta">{t("syncState.partialHint")}</span>
        )}
        {!compact && (outcome === "errors" || outcome === "failed") && (
          <span className="meta">{t("syncState.errorHint")}</span>
        )}
      </span>
    </>
  );
  return compact ? (
    <Link to="/wallets" className={`sync-status compact ${tone}`} title={label} aria-label={label}>
      <span className="sync-status-inner" role="status">
        {content}
      </span>
    </Link>
  ) : (
    <section
      className={`sync-status card card-pad ${tone}`}
      role="status"
      aria-label={t("syncState.label")}
    >
      {content}
    </section>
  );
}
