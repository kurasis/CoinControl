import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api, isCommandError } from "../ipc/client";

export function RecoveryControls() {
  const { t } = useTranslation();
  const client = useQueryClient();
  const [content, setContent] = useState("");
  const [verified, setVerified] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  async function run(fn: () => Promise<void>) {
    setBusy(true);
    setMessage("");
    try {
      await fn();
    } catch (e) {
      setMessage(isCommandError(e) ? e.message : t("errors.generic"));
    } finally {
      setBusy(false);
    }
  }
  const exported = async (fn: () => Promise<boolean>) =>
    setMessage(t((await fn()) ? "ops.saved" : "ops.cancelled"));
  return (
    <div className="stack">
      <p className="meta">{t("ops.safetyNote")}</p>
      <div className="row">
        <button
          className="btn"
          disabled={busy}
          onClick={() => void run(() => exported(api.exportBackup))}
        >
          {t("ops.backup")}
        </button>
        <button
          className="btn"
          disabled={busy}
          onClick={() => void run(() => exported(() => api.exportCsv("holdings")))}
        >
          {t("ops.csvHoldings")}
        </button>
        <button
          className="btn"
          disabled={busy}
          onClick={() => void run(() => exported(() => api.exportCsv("activity")))}
        >
          {t("ops.csvActivity")}
        </button>
        <button
          className="btn"
          disabled={busy}
          onClick={() => void run(() => exported(() => api.exportCsv("lots")))}
        >
          {t("ops.csvLots")}
        </button>
        <button
          className="btn"
          disabled={busy}
          onClick={() => void run(() => exported(() => api.exportCsv("decisions")))}
        >
          {t("ops.csvDecisions")}
        </button>
      </div>
      <label>
        {t("ops.restore")}
        <input
          type="file"
          accept=".ccbackup"
          disabled={busy}
          onChange={(e) => {
            const file = e.target.files?.[0];
            setVerified(false);
            setContent("");
            if (!file) return;
            if (file.size > 180 * 1024 * 1024) {
              setMessage(t("ops.fileTooLarge"));
              return;
            }
            void run(async () => {
              const text = await file.text();
              await api.inspectBackup(text);
              setContent(text);
              setVerified(true);
            });
          }}
        />
      </label>
      {verified && (
        <div className="notice">
          <p>{t("ops.restoreConfirm")}</p>
          <button
            className="btn"
            disabled={busy}
            onClick={() =>
              void run(async () => {
                await api.restoreBackup(content);
                setVerified(false);
                setContent("");
                await client.invalidateQueries();
                setMessage(t("ops.restored"));
              })
            }
          >
            {t("ops.applyRestore")}
          </button>
          <button
            className="btn"
            disabled={busy}
            onClick={() => {
              setVerified(false);
              setContent("");
            }}
          >
            {t("common.cancel")}
          </button>
        </div>
      )}
      <span role="status">{busy ? t("ops.working") : message}</span>
    </div>
  );
}
