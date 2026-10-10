import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api, isCommandError } from "../ipc/client";
import { useApp } from "../app/AppContext";
import { MASK } from "../lib/format";
import { useViewState } from "../app/useViewState";
import { SyncStatus } from "./SyncStatus";

export function MaintenanceControls() {
  const { t } = useTranslation();
  const { privacy, profile, syncProgress } = useApp();
  const client = useQueryClient();
  const accounts = useQuery({ queryKey: ["accounts"], queryFn: () => api.listAccounts() });
  const [selected, setSelected] = useState<string[]>([]);
  const [jobId, setJobId] = useViewState<string | undefined>("maintenance:job", undefined);
  const job = useQuery({
    queryKey: ["sync-job", jobId],
    queryFn: () => api.syncJob(jobId!),
    enabled: !!jobId,
    refetchInterval: (q) =>
      ["queued", "running", "cancelling"].includes(q.state.data?.status ?? "queued") ? 1000 : false,
  });
  const [confirm, setConfirm] = useState(false);
  const maintenance = useMutation({
    mutationFn: async (operation: "cache" | "diagnostics" | "rescan") => {
      if (operation === "cache") {
        await api.clearCaches();
        return t("maintenance.cacheCleared");
      }
      if (operation === "diagnostics")
        return t((await api.exportDiagnostics()) ? "ops.saved" : "ops.cancelled");
      const result = await api.startRescan(selected);
      setJobId(result.id);
      return t("maintenance.queued");
    },
    onSettled: () => client.invalidateQueries(),
  });
  const busy =
    maintenance.isPending ||
    !!syncProgress?.running ||
    ["queued", "running", "cancelling"].includes(
      job.data?.status ?? (jobId && !job.isError ? "queued" : "idle"),
    );
  return (
    <section className="stack" aria-label={t("maintenance.title")}>
      <h2>{t("maintenance.title")}</h2>
      <p className="meta">{t("maintenance.cacheNote")}</p>
      <div className="row">
        <button className="btn" disabled={busy} onClick={() => maintenance.mutate("cache")}>
          {t("maintenance.clearCache")}
        </button>
        <button
          className="btn"
          disabled={maintenance.isPending}
          onClick={() => maintenance.mutate("diagnostics")}
        >
          {t("maintenance.diagnostics")}
        </button>
      </div>
      {profile === "real" && (
        <>
          <p>{t("maintenance.rescanNote")}</p>
          <div className="list">
            {(accounts.data ?? []).map((a) => (
              <label className="row-inline" key={a.id}>
                <input
                  type="checkbox"
                  checked={selected.includes(a.id)}
                  disabled={busy}
                  onChange={(e) => {
                    setSelected((prev) =>
                      e.target.checked ? [...prev, a.id] : prev.filter((id) => id !== a.id),
                    );
                    setConfirm(false);
                  }}
                />
                {a.network} · {privacy ? MASK : a.display_address}
              </label>
            ))}
          </div>
          <label className="row-inline">
            <input
              type="checkbox"
              checked={confirm}
              disabled={busy || !selected.length}
              onChange={(e) => setConfirm(e.target.checked)}
            />
            {t("maintenance.rescanConfirm")}
          </label>
          <button
            className="btn"
            disabled={busy || !selected.length || selected.length > 50 || !confirm}
            onClick={() => maintenance.mutate("rescan")}
          >
            {t("maintenance.rescan")}
          </button>
          {syncProgress?.running && !["running", "cancelling"].includes(job.data?.status ?? "") && (
            <button
              className="btn"
              disabled={syncProgress.cancel_requested}
              onClick={() => void api.cancelSync()}
            >
              {t("ops.cancelSync")}
            </button>
          )}
          <SyncStatus />
        </>
      )}
      {job.data && ["queued", "running"].includes(job.data.status) && (
        <button
          className="btn"
          onClick={() =>
            void api
              .cancelSyncJob(job.data.id)
              .then(() => client.invalidateQueries({ queryKey: ["sync-job", jobId] }))
          }
        >
          {t("ops.cancelSync")}
        </button>
      )}
      {job.isError && <p role="alert">{t("errors.generic")}</p>}
      {job.data && (
        <p role="status">
          {t(`maintenance.jobs.${job.data.status}`, { defaultValue: t("errors.generic") })}
        </p>
      )}
      <span role="status">{maintenance.isPending ? t("ops.working") : maintenance.data}</span>
      {maintenance.isError && (
        <span role="alert" className="field-error">
          {isCommandError(maintenance.error) ? maintenance.error.message : t("errors.generic")}
        </span>
      )}
    </section>
  );
}
