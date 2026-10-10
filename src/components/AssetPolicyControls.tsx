import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api, isCommandError } from "../ipc/client";
import type { AssetPolicy } from "../ipc/bindings/AssetPolicy";

export function AssetPolicyControls() {
  const { t } = useTranslation();
  const client = useQueryClient();
  const [search, setSearch] = useState("");
  const [page, setPage] = useState(0);
  const policies = useQuery({ queryKey: ["asset-policies"], queryFn: api.listAssetPolicies });
  const update = useMutation({
    mutationFn: (p: AssetPolicy) => api.setAssetPolicy(p.asset_id, p.hidden, p.exclude_override),
    onSuccess: () => client.invalidateQueries(),
  });
  const matches = (policies.data ?? []).filter((p) =>
    `${p.symbol ?? ""} ${p.name ?? ""} ${p.asset_id}`
      .toLocaleLowerCase()
      .includes(search.toLocaleLowerCase()),
  );
  const lastPage = Math.max(0, Math.ceil(matches.length / 50) - 1);
  const currentPage = Math.min(page, lastPage);
  return (
    <section className="stack" aria-label={t("ops.policies")}>
      <h2>{t("ops.policies")}</h2>
      <p className="meta">{t("ops.policyNote")}</p>
      {update.isError && (
        <p role="alert">
          {isCommandError(update.error) ? update.error.message : t("errors.generic")}
        </p>
      )}
      <input
        className="input"
        type="search"
        name="asset-policy-search"
        autoComplete="off"
        aria-label={t("assets.search")}
        placeholder={t("common.search")}
        value={search}
        onChange={(e) => {
          setSearch(e.target.value);
          setPage(0);
        }}
      />
      {policies.isError && (
        <div role="alert">
          {t("errors.generic")}{" "}
          <button className="btn" onClick={() => void policies.refetch()}>
            {t("common.retry")}
          </button>
        </div>
      )}
      {matches.slice(currentPage * 50, (currentPage + 1) * 50).map((p) => (
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
      {matches.length > 50 && (
        <div className="row">
          <button
            className="btn"
            disabled={currentPage === 0}
            onClick={() => setPage(currentPage - 1)}
          >
            {t("common.previous")}
          </button>
          <span role="status">
            {currentPage + 1} / {lastPage + 1}
          </span>
          <button
            className="btn"
            disabled={currentPage === lastPage}
            onClick={() => setPage(currentPage + 1)}
          >
            {t("common.next")}
          </button>
        </div>
      )}
    </section>
  );
}
