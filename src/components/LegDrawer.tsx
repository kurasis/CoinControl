import { useEffect, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api, isCommandError, type LegOverride } from "../ipc/client";
import type { BasisKind } from "../ipc/bindings/BasisKind";
import type { LegClassification } from "../ipc/bindings/LegClassification";
import type { LegDetail } from "../ipc/bindings/LegDetail";
import { useApp } from "../app/AppContext";
import { useAccountLabels, useNetworkNames } from "../app/hooks";
import { openExternal } from "../lib/external";
import { formatDateTime } from "../lib/format";
import { Quantity, Usd } from "./Amount";

const INCOMING: LegClassification[] = ["deposit", "reward", "unclassified"];
const OUTGOING: LegClassification[] = [
  "unclassified",
  "withdrawal",
  "gift",
  "own_untracked",
  "sale",
  "payment",
];

const DECIMAL = /^\d+(\.\d+)?$/;

interface LotDraft {
  quantity: string;
  basis: string;
  kind: BasisKind;
  acquired: string;
}

type BasisMode = "unknown" | "market" | "lots";
type ProceedsMode = "none" | "market" | "manual";

export const EMPTY_OVERRIDE: LegOverride = {
  classification: null,
  basis_lots: null,
  basis_from_market: false,
  proceeds_usd: null,
  proceeds_from_market: false,
  price_usd: null,
  pair_with: null,
  note: null,
};

/** `YYYY-MM-DDTHH:mm` in UTC for a datetime-local input. */
function toUtcInput(unix: number): string {
  return new Date(unix * 1000).toISOString().slice(0, 16);
}

function fromUtcInput(value: string): number | null {
  const ms = Date.parse(`${value}:00Z`);
  return Number.isNaN(ms) ? null : Math.floor(ms / 1000);
}

/** Right-hand drawer with a movement's facts, the decision editor and its audit trail. */
export function LegDrawer({ legId, onClose }: { legId: string; onClose: () => void }) {
  const { t } = useTranslation();
  const closeRef = useRef<HTMLButtonElement>(null);
  const detail = useQuery({ queryKey: ["leg", legId], queryFn: () => api.legDetail(legId) });

  useEffect(() => {
    const opener = document.activeElement as HTMLElement | null;
    closeRef.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
      opener?.focus?.();
    };
  }, [onClose]);

  return (
    <div className="drawer-backdrop" onClick={onClose}>
      <aside
        className="drawer"
        role="dialog"
        aria-modal="true"
        aria-labelledby="leg-drawer-title"
        onClick={(e) => e.stopPropagation()}
      >
        <header className="drawer-head">
          <h2 id="leg-drawer-title">{t("leg.title")}</h2>
          <div className="toolbar-spacer" />
          <button ref={closeRef} className="btn btn-ghost" onClick={onClose}>
            {t("common.close")}
          </button>
        </header>
        <div className="drawer-body">
          {detail.isLoading && <div className="skeleton skeleton-table" />}
          {detail.isError && <p className="field-error">{t("errors.generic")}</p>}
          {detail.data && <LegBody key={detail.data.history.length} leg={detail.data} />}
        </div>
      </aside>
    </div>
  );
}

function LegBody({ leg }: { leg: LegDetail }) {
  const { t } = useTranslation();
  const { locale, timeZone, privacy } = useApp();
  const networkNames = useNetworkNames();
  const accountLabels = useAccountLabels(privacy);
  const incoming = leg.direction === "in";
  return (
    <>
      <dl className="facts">
        <dt>{t("leg.when")}</dt>
        <dd>{formatDateTime(leg.occurred_at, locale, timeZone)}</dd>
        <dt>{t("leg.movement")}</dt>
        <dd className={incoming ? "positive" : "negative"}>
          {incoming ? "+" : "−"}
          <Quantity value={leg.quantity} symbol={leg.symbol} />
        </dd>
        <dt>{t("leg.account")}</dt>
        <dd>
          {accountLabels.get(leg.account_id) ?? "—"} ·{" "}
          {networkNames.get(leg.network) ?? leg.network}
        </dd>
        <dt>{t("leg.status")}</dt>
        <dd>
          {t(`activity.status.${leg.tx_status}`, { defaultValue: leg.tx_status })}
          {leg.block_height !== null && ` · ${t("leg.block", { height: leg.block_height })}`}
        </dd>
        <dt>{t("leg.transaction")}</dt>
        <dd className="row-inline">
          <code className="address truncate">{privacy ? "•••••" : leg.tx_hash}</code>
          {leg.explorer_url && (
            <button className="link-button" onClick={() => void openExternal(leg.explorer_url!)}>
              {t("leg.explorer")}
            </button>
          )}
        </dd>
        <dt>{t("leg.source")}</dt>
        <dd>{leg.provider}</dd>
        <dt>{t("leg.treatment")}</dt>
        <dd>
          {leg.treatment ? t(`treatment.${leg.treatment}`, { defaultValue: leg.treatment }) : "—"}
          {leg.counterparty_account_id &&
            ` · ${accountLabels.get(leg.counterparty_account_id) ?? ""}`}
        </dd>
        <dt>{t("leg.value")}</dt>
        <dd>
          <Usd value={leg.value_usd} />
          {leg.value_usd !== null && leg.value_estimated && ` · ${t("chart.estimatedChip")}`}
        </dd>
        {incoming && (
          <>
            <dt>{t("leg.basis")}</dt>
            <dd>
              <Usd value={leg.basis_usd} />
              {leg.basis_kind && ` · ${t(`basisKind.${leg.basis_kind}`)}`}
            </dd>
          </>
        )}
        {!incoming && leg.proceeds_usd !== null && (
          <>
            <dt>{t("leg.proceeds")}</dt>
            <dd>
              <Usd value={leg.proceeds_usd} />
            </dd>
          </>
        )}
      </dl>
      {leg.review && (
        <p className="notice">{t(`review.explain.${leg.review}`, { defaultValue: leg.review })}</p>
      )}
      <LegEditor leg={leg} />
      <History leg={leg} />
    </>
  );
}

function LegEditor({ leg }: { leg: LegDetail }) {
  const { t } = useTranslation();
  const { locale, timeZone } = useApp();
  const queryClient = useQueryClient();
  const incoming = leg.direction === "in";
  const cur = leg.current ?? EMPTY_OVERRIDE;
  const [classification, setClassification] = useState<LegClassification | "">(
    cur.classification ?? "",
  );
  const [basisMode, setBasisMode] = useState<BasisMode>(
    cur.basis_lots?.length ? "lots" : cur.basis_from_market ? "market" : "unknown",
  );
  const [lots, setLots] = useState<LotDraft[]>(
    cur.basis_lots?.map((l) => ({
      quantity: l.quantity,
      basis: l.basis_usd ?? "",
      kind: l.basis_kind,
      acquired: toUtcInput(l.acquired_at),
    })) ?? [
      { quantity: leg.quantity, basis: "", kind: "known", acquired: toUtcInput(leg.occurred_at) },
    ],
  );
  const [proceedsMode, setProceedsMode] = useState<ProceedsMode>(
    cur.proceeds_usd !== null ? "manual" : cur.proceeds_from_market ? "market" : "none",
  );
  const [proceeds, setProceeds] = useState(cur.proceeds_usd ?? "");
  const [price, setPrice] = useState(cur.price_usd ?? "");
  const [pairWith, setPairWith] = useState(cur.pair_with ?? "");
  const [note, setNote] = useState(cur.note ?? "");
  const [error, setError] = useState<string | null>(null);

  const save = useMutation({
    mutationFn: (decision: LegOverride) => api.updateBasis(leg.leg_id, decision),
    onSuccess: async () => {
      setError(null);
      await queryClient.invalidateQueries();
    },
    onError: (e) => setError(isCommandError(e) ? e.message : t("errors.generic")),
  });

  function build(): LegOverride | string {
    if (price && !DECIMAL.test(price)) return t("leg.errDecimal", { field: t("leg.price") });
    const decision: LegOverride = {
      ...EMPTY_OVERRIDE,
      classification: classification || null,
      price_usd: price || null,
      note: note.trim() || null,
    };
    if (incoming) {
      if (basisMode === "market") decision.basis_from_market = true;
      if (basisMode === "lots") {
        const out = [];
        for (const l of lots) {
          if (!DECIMAL.test(l.quantity))
            return t("leg.errDecimal", { field: t("leg.lotQuantity") });
          if (l.basis && !DECIMAL.test(l.basis))
            return t("leg.errDecimal", { field: t("leg.lotBasis") });
          const at = fromUtcInput(l.acquired);
          if (at === null) return t("leg.errDate");
          out.push({
            quantity: l.quantity,
            basis_usd: l.basis || null,
            basis_kind: l.basis ? l.kind : ("unknown" as const),
            acquired_at: at,
          });
        }
        decision.basis_lots = out;
      }
    } else {
      decision.pair_with = pairWith || null;
      if (classification === "sale" || classification === "payment") {
        if (proceedsMode === "market") decision.proceeds_from_market = true;
        if (proceedsMode === "manual") {
          if (!DECIMAL.test(proceeds)) return t("leg.errDecimal", { field: t("leg.proceeds") });
          decision.proceeds_usd = proceeds;
        }
      }
    }
    return decision;
  }

  function submit(e: React.FormEvent) {
    e.preventDefault();
    const d = build();
    if (typeof d === "string") setError(d);
    else save.mutate(d);
  }

  const classes = incoming ? INCOMING : OUTGOING;
  const sale = classification === "sale" || classification === "payment";
  return (
    <form className="editor" onSubmit={submit} aria-labelledby="editor-heading">
      <h3 id="editor-heading">{t("leg.editTitle")}</h3>
      {!incoming && leg.pair_candidates.length > 0 && (
        <label className="field">
          <span>{t("leg.pairWith")}</span>
          <select className="select" value={pairWith} onChange={(e) => setPairWith(e.target.value)}>
            <option value="">{t("leg.noPair")}</option>
            {leg.pair_candidates.map((c) => (
              <option key={c.leg_id} value={c.leg_id}>
                {formatDateTime(c.occurred_at, locale, timeZone)} · {c.quantity} · {c.network}
              </option>
            ))}
          </select>
          <span className="meta">{t("leg.pairHint")}</span>
        </label>
      )}
      {!pairWith && (
        <label className="field">
          <span>{t("leg.classification")}</span>
          <select
            className="select"
            value={classification}
            onChange={(e) => setClassification(e.target.value as LegClassification | "")}
          >
            <option value="">{t("leg.defaultClass")}</option>
            {classes.map((c) => (
              <option key={c} value={c}>
                {t(`classification.${c}`)}
              </option>
            ))}
          </select>
        </label>
      )}
      {incoming && classification !== "reward" && (
        <fieldset className="field">
          <legend>{t("leg.basis")}</legend>
          {(["unknown", "market", "lots"] as BasisMode[]).map((m) => (
            <label key={m} className="radio">
              <input
                type="radio"
                name="basis-mode"
                checked={basisMode === m}
                onChange={() => setBasisMode(m)}
              />
              {t(`leg.basisMode.${m}`)}
            </label>
          ))}
          {basisMode === "lots" && (
            <div className="lots-editor">
              {lots.map((l, i) => (
                <div className="lot-row" key={i}>
                  <label className="field">
                    <span>{t("leg.lotQuantity")}</span>
                    <input
                      className="input"
                      inputMode="decimal"
                      value={l.quantity}
                      onChange={(e) =>
                        setLots(
                          lots.map((x, j) => (j === i ? { ...x, quantity: e.target.value } : x)),
                        )
                      }
                    />
                  </label>
                  <label className="field">
                    <span>{t("leg.lotBasis")}</span>
                    <input
                      className="input"
                      inputMode="decimal"
                      placeholder={t("leg.unknownPlaceholder")}
                      value={l.basis}
                      onChange={(e) =>
                        setLots(lots.map((x, j) => (j === i ? { ...x, basis: e.target.value } : x)))
                      }
                    />
                  </label>
                  <label className="field">
                    <span>{t("leg.lotKind")}</span>
                    <select
                      className="select"
                      value={l.kind}
                      onChange={(e) =>
                        setLots(
                          lots.map((x, j) =>
                            j === i ? { ...x, kind: e.target.value as BasisKind } : x,
                          ),
                        )
                      }
                    >
                      <option value="known">{t("basisKind.known")}</option>
                      <option value="estimated">{t("basisKind.estimated")}</option>
                    </select>
                  </label>
                  <label className="field">
                    <span>{t("leg.lotAcquired")}</span>
                    <input
                      className="input"
                      type="datetime-local"
                      value={l.acquired}
                      onChange={(e) =>
                        setLots(
                          lots.map((x, j) => (j === i ? { ...x, acquired: e.target.value } : x)),
                        )
                      }
                    />
                  </label>
                  {lots.length > 1 && (
                    <button
                      type="button"
                      className="btn btn-ghost"
                      onClick={() => setLots(lots.filter((_, j) => j !== i))}
                    >
                      {t("leg.removeLot")}
                    </button>
                  )}
                </div>
              ))}
              <button
                type="button"
                className="btn"
                onClick={() =>
                  setLots([
                    ...lots,
                    {
                      quantity: "",
                      basis: "",
                      kind: "known",
                      acquired: toUtcInput(leg.occurred_at),
                    },
                  ])
                }
              >
                {t("leg.addLot")}
              </button>
              <span className="meta">{t("leg.lotsHint")}</span>
            </div>
          )}
        </fieldset>
      )}
      {!incoming && sale && !pairWith && (
        <fieldset className="field">
          <legend>{t("leg.proceeds")}</legend>
          {(["none", "market", "manual"] as ProceedsMode[]).map((m) => (
            <label key={m} className="radio">
              <input
                type="radio"
                name="proceeds-mode"
                checked={proceedsMode === m}
                onChange={() => setProceedsMode(m)}
              />
              {t(`leg.proceedsMode.${m}`)}
            </label>
          ))}
          {proceedsMode === "manual" && (
            <input
              className="input"
              inputMode="decimal"
              aria-label={t("leg.proceedsUsd")}
              value={proceeds}
              onChange={(e) => setProceeds(e.target.value)}
            />
          )}
        </fieldset>
      )}
      <label className="field">
        <span>{t("leg.price")}</span>
        <input
          className="input"
          inputMode="decimal"
          placeholder={t("leg.pricePlaceholder")}
          value={price}
          onChange={(e) => setPrice(e.target.value)}
        />
      </label>
      <label className="field">
        <span>{t("leg.note")}</span>
        <input className="input" value={note} onChange={(e) => setNote(e.target.value)} />
      </label>
      {error && (
        <p className="field-error" role="alert">
          {error}
        </p>
      )}
      {save.isSuccess && !error && (
        <p className="meta" role="status">
          {t("leg.saved")}
        </p>
      )}
      <div className="row">
        <button className="btn btn-primary" type="submit" disabled={save.isPending}>
          {t("leg.save")}
        </button>
        {leg.current && (
          <button
            type="button"
            className="btn btn-ghost"
            disabled={save.isPending}
            onClick={() => save.mutate(EMPTY_OVERRIDE)}
          >
            {t("leg.revert")}
          </button>
        )}
      </div>
    </form>
  );
}

function History({ leg }: { leg: LegDetail }) {
  const { t } = useTranslation();
  const { locale, timeZone } = useApp();
  if (leg.history.length === 0) return <p className="meta">{t("leg.noHistory")}</p>;
  return (
    <section aria-labelledby="history-heading">
      <h3 id="history-heading">{t("leg.history")}</h3>
      <ol className="history">
        {[...leg.history].reverse().map((v) => (
          <li key={v.version}>
            <span>
              {t("leg.version", { version: v.version })} ·{" "}
              {formatDateTime(v.created_at, locale, timeZone)} ·{" "}
              {v.source === "manual" ? t("leg.sourceManual") : t("leg.sourceCsv")}
              {v.orphaned && ` · ${t("leg.orphaned")}`}
            </span>
            <span className="meta">{summarize(v.payload, t)}</span>
          </li>
        ))}
      </ol>
    </section>
  );
}

function summarize(o: LegOverride, t: (k: string, v?: Record<string, unknown>) => string): string {
  const parts: string[] = [];
  if (o.classification) parts.push(t(`classification.${o.classification}`));
  if (o.pair_with) parts.push(t("leg.summaryPaired"));
  if (o.basis_lots?.length) parts.push(t("leg.summaryLots", { count: o.basis_lots.length }));
  if (o.basis_from_market) parts.push(t("leg.basisMode.market"));
  if (o.proceeds_usd) parts.push(`${t("leg.proceeds")}: $${o.proceeds_usd}`);
  if (o.proceeds_from_market) parts.push(t("leg.proceedsMode.market"));
  if (o.price_usd) parts.push(`${t("leg.price")}: $${o.price_usd}`);
  if (o.note) parts.push(`“${o.note}”`);
  return parts.length ? parts.join(" · ") : t("leg.summaryDefault");
}
