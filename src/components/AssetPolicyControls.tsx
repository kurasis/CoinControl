import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api, isCommandError } from "../ipc/client";
import type { AssetPolicy } from "../ipc/bindings/AssetPolicy";

export function AssetPolicyControls() {
  const { t } = useTranslation();
  const client = useQueryClient();
  const policies = useQuery({ queryKey: ["asset-policies"], queryFn: api.listAssetPolicies });
  const update = useMutation({
    mutationFn: (p: AssetPolicy) => api.setAssetPolicy(p.asset_id, p.hidden, p.exclude_override),
    onSuccess: () => client.invalidateQueries(),
  });
  return (
    <section className="stack" aria-label={t("ops.policies")}>
      <h3>{t("ops.policies")}</h3>
      <p className="meta">{t("ops.policyNote")}</p>
      {update.isError && (
        <p role="alert">
          {isCommandError(update.error) ? update.error.message : t("errors.generic")}
        </p>
      )}
      {policies.data?.map((p) => (
        <div className="stack" key={p.asset_id}>
          <span className="asset-policy-identity" title={p.asset_id}>
            {p.symbol ?? p.name ?? p.asset_id} · {p.asset_id} · {p.verification}
          </span>
          <div className="row">
            <label>
              <input
                type="checkbox"
                checked={p.hidden}
                disabled={update.isPending}
                onChange={(e) => update.mutate({ ...p, hidden: e.target.checked })}
              />
              {t("ops.hide")}
            </label>
            <select
              className="select"
              aria-label={`${t("ops.policies")} ${p.asset_id}`}
              value={p.exclude_override === null ? "auto" : String(p.exclude_override)}
              disabled={update.isPending}
              onChange={(e) =>
                update.mutate({
                  ...p,
                  exclude_override: e.target.value === "auto" ? null : e.target.value === "true",
                })
              }
            >
              <option value="auto">{t("ops.followProvider")}</option>
              <option value="false">{t("ops.include")}</option>
              <option value="true">{t("ops.exclude")}</option>
            </select>
          </div>
        </div>
      ))}
    </section>
  );
}
