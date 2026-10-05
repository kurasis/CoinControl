import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api, type Scope } from "../ipc/client";
import { useApp } from "../app/AppContext";

function encode(scope: Scope): string {
  switch (scope.kind) {
    case "wallet":
      return `wallet:${scope.id}`;
    case "group":
      return `group:${scope.id}`;
    default:
      return "all";
  }
}

function decode(value: string): Scope {
  const [kind, id] = value.split(":", 2);
  if (kind === "wallet" && id) return { kind: "wallet", id };
  if (kind === "group" && id) return { kind: "group", id };
  return { kind: "all" };
}

/** Chooses which accounts the current view covers. */
export function ScopeSelector() {
  const { t } = useTranslation();
  const { scope, setScope } = useApp();
  const wallets = useQuery({ queryKey: ["wallets"], queryFn: api.listWallets });
  const groups = useQuery({ queryKey: ["groups"], queryFn: api.listGroups });

  return (
    <label className="row" title={t("scope.tooltip")}>
      <span className="visually-hidden">{t("scope.label")}</span>
      <select
        className="select"
        value={encode(scope)}
        onChange={(e) => setScope(decode(e.target.value))}
      >
        <option value="all">{t("scope.all")}</option>
        {(wallets.data ?? []).length > 0 && (
          <optgroup label={t("nav.wallets")}>
            {wallets.data!.map((w) => (
              <option key={w.id} value={`wallet:${w.id}`}>
                {w.label}
              </option>
            ))}
          </optgroup>
        )}
        {(groups.data ?? []).length > 0 && (
          <optgroup label={t("nav.groups")}>
            {groups.data!.map((g) => (
              <option key={g.id} value={`group:${g.id}`}>
                {g.label}
              </option>
            ))}
          </optgroup>
        )}
      </select>
    </label>
  );
}
