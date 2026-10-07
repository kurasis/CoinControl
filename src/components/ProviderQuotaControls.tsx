import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api, isCommandError } from "../ipc/client";
import type { ProviderStatus } from "../ipc/bindings/ProviderStatus";
import type { ProviderQuota } from "../ipc/bindings/ProviderQuota";
import { useApp } from "../app/AppContext";
import { formatDateTime } from "../lib/format";
export function ProviderQuotaControls({ provider }: { provider: ProviderStatus }) {
  const { t } = useTranslation();
  const { locale, timeZone } = useApp();
  const client = useQueryClient();
  const quota = useQuery({
    queryKey: ["provider-quota", provider.id],
    queryFn: () => api.getProviderQuota(provider.id),
  });
  const [edits, setEdits] = useState<Partial<ProviderQuota>>({});
  const values = quota.data ? { ...quota.data, ...edits } : null;
  const save = useMutation({
    mutationFn: () => api.setProviderQuota(provider.id, values!),
    onSuccess: async () => {
      setEdits({});
      await client.invalidateQueries({ queryKey: ["provider-quota", provider.id] });
    },
  });
  const error = save.error ?? quota.error;
  const [resetAt] = useState(() => Math.floor(Date.now() / 86400000) * 86400 + 86400);
  return (
    <details>
      <summary>{t("maintenance.quotas")}</summary>
      <p className="meta">{t("maintenance.quotaNote")}</p>
      {values && (
        <form
          className="stack"
          onSubmit={(e) => {
            e.preventDefault();
            save.mutate();
          }}
        >
          {(["daily_requests", "daily_credits", "monthly_requests"] as const)
            .filter(
              (key) =>
                key !== "daily_credits" ||
                ["alchemy", "helius", "drpc", "blockscout"].includes(provider.id),
            )
            .map((key) => (
              <label className="row" key={key}>
                {t(`maintenance.${key}`)}
                <input
                  className="input"
                  type="number"
                  min={1}
                  max={1000000}
                  step={1}
                  required
                  value={values[key]}
                  onChange={(e) =>
                    setEdits((previous) => ({ ...previous, [key]: Number(e.target.value) }))
                  }
                />
              </label>
            ))}
          <button className="btn" disabled={save.isPending || !Object.keys(edits).length}>
            {t("maintenance.saveQuotas")}
          </button>
        </form>
      )}
      <p className="meta">
        {t("maintenance.localReset", { time: formatDateTime(resetAt, locale, timeZone) })}
      </p>
      {save.isSuccess && <span role="status">{t("ops.saved")}</span>}
      {error && (
        <span role="alert">{isCommandError(error) ? error.message : t("errors.generic")}</span>
      )}
    </details>
  );
}
