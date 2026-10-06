import { useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { useApp } from "../app/AppContext";
import { api } from "../ipc/client";
import { formatDateTime } from "../lib/format";
import { Spinner } from "./SyncStatus";

export function NetworkConsoleToggle() {
  const { t } = useTranslation();
  const { settings, updateSettings } = useApp();
  return (
    <label className="row-inline">
      <input
        type="checkbox"
        checked={settings?.network_console_enabled ?? false}
        disabled={!settings}
        onChange={(e) => updateSettings({ network_console_enabled: e.target.checked })}
      />
      {t("networkLog.enable")}
    </label>
  );
}

export function NetworkConsole() {
  const { t } = useTranslation();
  const { settings, locale, timeZone } = useApp();
  const enabled = settings?.network_console_enabled ?? false;
  const [paused, setPaused] = useState(false);
  const [errorsOnly, setErrorsOnly] = useState(false);
  const [page, setPage] = useState(0);
  const client = useQueryClient();
  const log = useQuery({
    queryKey: ["network-log"],
    queryFn: api.networkLog,
    enabled: enabled && !paused,
    refetchInterval: 1000,
  });
  const clear = useMutation({
    mutationFn: api.clearNetworkLog,
    onSuccess: () => {
      client.setQueryData(["network-log"], []);
      setPage(0);
    },
  });
  const rows = (enabled ? (log.data ?? []) : []).filter(
    (r) => !errorsOnly || !["pending", "success"].includes(r.status),
  );
  const currentPage = Math.min(page, Math.max(0, Math.ceil(rows.length / 50) - 1));
  const visible = rows.slice(currentPage * 50, (currentPage + 1) * 50);
  return (
    <section className="stack" aria-labelledby="network-console-title">
      <h2 id="network-console-title">{t("networkLog.title")}</h2>
      <NetworkConsoleToggle />
      <p className="meta">{t("networkLog.hint")}</p>
      {enabled ? (
        <>
          <div className="row">
            <span className="chip" role="status">
              {paused ? t("networkLog.paused") : t("networkLog.live")}
            </span>
            <button className="btn" onClick={() => setPaused(!paused)}>
              {paused ? t("networkLog.resume") : t("networkLog.pause")}
            </button>
            <button className="btn" onClick={() => clear.mutate()} disabled={clear.isPending}>
              {t("networkLog.clear")}
            </button>
            <label className="row-inline">
              <input
                type="checkbox"
                checked={errorsOnly}
                onChange={(e) => {
                  setErrorsOnly(e.target.checked);
                  setPage(0);
                }}
              />
              {t("networkLog.errorsOnly")}
            </label>
          </div>
          {(log.isError || clear.isError) && (
            <p className="field-error" role="alert">
              {t("errors.generic")}
            </p>
          )}
          {rows.length === 0 ? (
            <p className="notice">{t("networkLog.empty")}</p>
          ) : (
            <>
              <div
                className="card table-scroll network-console-scroll"
                tabIndex={0}
                role="region"
                aria-label={t("networkLog.table")}
              >
                <table className="table network-console-table">
                  <thead>
                    <tr>
                      {["time", "provider", "request", "server", "status", "duration"].map(
                        (key) => (
                          <th key={key}>{t(`networkLog.columns.${key}`)}</th>
                        ),
                      )}
                    </tr>
                  </thead>
                  <tbody>
                    {visible.map((r) => (
                      <tr key={r.id}>
                        <td>
                          {formatDateTime(Math.floor(r.started_at_ms / 1000), locale, timeZone)}
                        </td>
                        <td>{r.provider}</td>
                        <td>
                          <span className="mono">
                            {r.method} {r.operation}
                          </span>
                          <br />
                          <span className="meta">
                            {t("networkLog.attempt", { count: r.attempt })}
                          </span>
                        </td>
                        <td className="mono">{r.origin}</td>
                        <td
                          className={
                            r.status === "success"
                              ? "positive"
                              : r.status === "pending"
                                ? ""
                                : "negative"
                          }
                        >
                          {r.status === "pending" && <Spinner />}{" "}
                          {t(`networkLog.statuses.${r.status}`)}
                          {r.http_status !== null && (
                            <span className="meta"> · HTTP {r.http_status}</span>
                          )}
                          {r.rpc_code !== null && <span className="meta"> · RPC {r.rpc_code}</span>}
                        </td>
                        <td>
                          {r.duration_ms === null
                            ? "—"
                            : t("networkLog.ms", { value: r.duration_ms })}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              <div className="row">
                <button
                  className="btn"
                  onClick={() => setPage(currentPage - 1)}
                  disabled={currentPage === 0}
                >
                  {t("networkLog.previous")}
                </button>
                <span className="meta">
                  {t("networkLog.range", {
                    start: currentPage * 50 + 1,
                    end: Math.min((currentPage + 1) * 50, rows.length),
                    total: rows.length,
                  })}
                </span>
                <button
                  className="btn"
                  onClick={() => setPage(currentPage + 1)}
                  disabled={(currentPage + 1) * 50 >= rows.length}
                >
                  {t("networkLog.next")}
                </button>
              </div>
            </>
          )}
        </>
      ) : (
        <p className="notice">{t("networkLog.disabled")}</p>
      )}
    </section>
  );
}
