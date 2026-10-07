import { WindowedTableBody } from "./WindowedTableBody";
import { useMemo } from "react";
import { Link } from "react-router";
import { useTranslation } from "react-i18next";
import type { HoldingRow } from "../ipc/bindings/HoldingRow";
import type { NetworkId } from "../ipc/client";
import { Percent, Price, Quantity, Usd } from "./Amount";
import { useViewState } from "../app/useViewState";
import { TokenIcon } from "./TokenIcon";

type SortKey = "value" | "price" | "change" | "name" | "pnl";

export function assetPath(assetId: string): string {
  return `/assets/${encodeURIComponent(assetId)}`;
}

/** Compares exact decimal strings without converting them to floats. */
export function compareDecimal(a: string, b: string): number {
  const neg = (s: string) => s.startsWith("-");
  if (neg(a) !== neg(b)) return neg(a) ? -1 : 1;
  const sign = neg(a) ? -1 : 1;
  const [ai = "", af = ""] = a.replace("-", "").split(".");
  const [bi = "", bf = ""] = b.replace("-", "").split(".");
  const aInt = ai.replace(/^0+(?=\d)/, "");
  const bInt = bi.replace(/^0+(?=\d)/, "");
  if (aInt.length !== bInt.length) return sign * (aInt.length - bInt.length);
  if (aInt !== bInt) return sign * (aInt < bInt ? -1 : 1);
  const len = Math.max(af.length, bf.length);
  const fa = af.padEnd(len, "0");
  const fb = bf.padEnd(len, "0");
  return fa === fb ? 0 : sign * (fa < fb ? -1 : 1);
}

/** Missing values always sort after present ones, in either direction. */
function compareNullable(a: string | null, b: string | null, dir: 1 | -1): number {
  if (a === null && b === null) return 0;
  if (a === null) return 1;
  if (b === null) return -1;
  return dir * compareDecimal(a, b);
}

export function AssetTable({
  rows,
  networkNames,
}: {
  rows: HoldingRow[];
  networkNames: Map<NetworkId, string>;
}) {
  const { t } = useTranslation();
  const [sort, setSort] = useViewState<{ key: SortKey; dir: 1 | -1 }>("assets:sort", {
    key: "value",
    dir: -1,
  });
  const [query, setQuery] = useViewState("assets:query", "");
  const [network, setNetwork] = useViewState<NetworkId | "">("assets:network", "");

  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    const filtered = rows.filter(
      (r) =>
        (!network || r.network === network) &&
        (!q || [r.symbol, r.name, r.asset_id].some((v) => v?.toLowerCase().includes(q))),
    );
    return [...filtered].sort((a, b) => {
      let c = 0;
      if (sort.key === "value") c = compareNullable(a.value_usd, b.value_usd, sort.dir);
      else if (sort.key === "price") c = compareNullable(a.price_usd, b.price_usd, sort.dir);
      else if (sort.key === "pnl")
        c = compareNullable(a.unrealized_pnl_usd, b.unrealized_pnl_usd, sort.dir);
      else if (sort.key === "change")
        c = compareNullable(a.change_24h_percent, b.change_24h_percent, sort.dir);
      else c = sort.dir * (a.symbol ?? "").localeCompare(b.symbol ?? "");
      return c || a.asset_id.localeCompare(b.asset_id);
    });
  }, [rows, query, network, sort]);

  const networksPresent = [...new Set(rows.map((r) => r.network))];

  function header(key: SortKey, label: string, right = false) {
    const active = sort.key === key;
    return (
      <th
        className={right ? "right" : undefined}
        aria-sort={active ? (sort.dir === 1 ? "ascending" : "descending") : "none"}
      >
        <button
          onClick={() =>
            setSort({ key, dir: active ? (sort.dir === 1 ? -1 : 1) : key === "name" ? 1 : -1 })
          }
        >
          {label}
          {active ? (sort.dir === 1 ? " ↑" : " ↓") : ""}
        </button>
      </th>
    );
  }

  return (
    <section className="card" aria-labelledby="assets-heading">
      <div className="section-header card-pad card-pad-head">
        <h2 id="assets-heading">{t("assets.title")}</h2>
        <div className="toolbar-spacer" />
        <input
          className="input"
          type="search"
          placeholder={t("assets.search")}
          aria-label={t("assets.search")}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <select
          className="select"
          aria-label={t("assets.network")}
          value={network}
          onChange={(e) => setNetwork(e.target.value as NetworkId | "")}
        >
          <option value="">{t("assets.allNetworks")}</option>
          {networksPresent.map((n) => (
            <option key={n} value={n}>
              {networkNames.get(n) ?? n}
            </option>
          ))}
        </select>
      </div>
      {visible.length === 0 ? (
        <div className="empty">
          <p>{t("assets.noMatches")}</p>
          <button
            className="btn"
            onClick={() => {
              setQuery("");
              setNetwork("");
            }}
          >
            {t("assets.clearFilters")}
          </button>
        </div>
      ) : (
        <div className="table-scroll">
          <table className="table">
            <thead>
              <tr>
                {header("name", t("assets.colAsset"))}
                {header("price", t("assets.colPrice"), true)}
                <th className="right">{t("assets.colBalance")}</th>
                {header("value", t("assets.colValue"), true)}
                {header("change", t("assets.col24h"), true)}
                {header("pnl", t("assets.colPnl"), true)}
              </tr>
            </thead>
            <WindowedTableBody rows={visible} columns={6} rowKey={(r) => r.asset_id}>
              {(r) => (
                <>
                  <td>
                    <div className="asset-cell">
                      <TokenIcon symbol={r.symbol} />
                      <div className="stack">
                        <Link className="row-link" to={assetPath(r.asset_id)}>
                          {r.name ?? r.symbol ?? t("assets.unknownAsset")}
                        </Link>
                        <span className="meta">
                          {r.symbol ?? "?"} · {networkNames.get(r.network) ?? r.network}
                          {r.verification === "unverified" && ` · ${t("assets.unverified")}`}
                        </span>
                      </div>
                    </div>
                  </td>
                  <td className="right">
                    <Price value={r.price_usd} />
                  </td>
                  <td className="right">
                    <div className="stack">
                      <Quantity value={r.quantity} symbol={r.symbol} />
                      {r.balance_status !== "fresh" && (
                        <span className="meta warning">
                          {t(`balanceStatus.${r.balance_status}`)}
                        </span>
                      )}
                    </div>
                  </td>
                  <td className="right">
                    <Usd value={r.value_usd} />
                  </td>
                  <td className="right">
                    <Percent value={r.change_24h_percent} />
                  </td>
                  <td className="right">
                    {r.unrealized_pnl_usd !== null ? (
                      <div className="stack">
                        <Usd value={r.unrealized_pnl_usd} />
                        <Percent value={r.unrealized_return_percent} sensitive />
                      </div>
                    ) : (
                      <span className="meta" title={t("assets.basisHint")}>
                        {t(`basisCoverage.${r.basis_coverage}`)}
                      </span>
                    )}
                  </td>
                </>
              )}
            </WindowedTableBody>
          </table>
        </div>
      )}
    </section>
  );
}
