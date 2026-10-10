import { useEffect, useRef, useState } from "react";
import { useViewState } from "../app/useViewState";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api, isCommandError, type LegOverride } from "../ipc/client";
import type { BasisKind } from "../ipc/bindings/BasisKind";
import type { LegClassification } from "../ipc/bindings/LegClassification";
import type { LegDetail } from "../ipc/bindings/LegDetail";
import { useApp } from "../app/AppContext";
import { useAccountLabels, useNetworkNames } from "../app/hooks";
import { openExternal } from "../lib/external";
import { MASK, formatDateTime, formatUsd, formatPrice } from "../lib/format";
import { Quantity, Usd } from "./Amount";
import { useDialogFocus } from "./useDialogFocus";

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
interface EditorGuard {
  dirty: boolean;
  pending: boolean;
  discard: () => void;
}

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
  const panelRef = useRef<HTMLDivElement>(null);
  const guardRef = useRef<EditorGuard | null>(null);
  const [closing, setClosing] = useState(false);
  const [editorPending, setEditorPending] = useState(false);
  const discardRef = useRef<HTMLButtonElement>(null);
  function requestClose() {
    if (guardRef.current?.pending || guardRef.current?.dirty) setClosing(true);
    else onClose();
  }
  useEffect(() => {
    if (closing) discardRef.current?.focus();
  }, [closing]);
  const detail = useQuery({ queryKey: ["leg", legId], queryFn: () => api.legDetail(legId) });

  useDialogFocus(panelRef, requestClose);

  return (
    <div className="drawer-backdrop" onClick={requestClose}>
      <div
        ref={panelRef}
        tabIndex={-1}
        className="drawer"
        role="dialog"
        aria-modal="true"
        aria-labelledby="leg-drawer-title"
        onClick={(e) => e.stopPropagation()}
      >
        <header className="drawer-head">
          <h2 id="leg-drawer-title">{t("leg.title")}</h2>
          <div className="toolbar-spacer" />
          <button className="btn btn-ghost" onClick={requestClose}>
            {t("common.close")}
          </button>
        </header>
        <div className="drawer-body">
          {closing && (
            <section className="notice" role="alert" aria-label={t("common.unsaved")}>
              <p>{editorPending ? t("common.saving") : t("common.unsaved")}</p>
              <div className="row">
                <button
                  ref={discardRef}
                  className="btn"
                  disabled={editorPending}
                  onClick={() => {
                    guardRef.current?.discard();
                    onClose();
                  }}
                >
                  {t("common.discard")}
                </button>
                <button
                  className="btn"
                  onClick={() => {
                    setClosing(false);
                    panelRef.current
                      ?.querySelector<HTMLButtonElement>(".drawer-head button")
                      ?.focus();
                  }}
                >
                  {t("common.cancel")}
                </button>
              </div>
            </section>
          )}
          {detail.isLoading && <div className="skeleton skeleton-table" />}
          {detail.isError && <p className="field-error">{t("errors.generic")}</p>}
          {detail.data && (
            <LegBody
              key={detail.data.history.length}
              leg={detail.data}
              guardRef={guardRef}
              onPending={setEditorPending}
            />
          )}
        </div>
      </div>
    </div>
  );
}

function LegBody({
  leg,
  guardRef,
  onPending,
}: {
  leg: LegDetail;
  guardRef: React.RefObject<EditorGuard | null>;
  onPending: (pending: boolean) => void;
}) {
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
      <LegEditor leg={leg} guardRef={guardRef} onPending={onPending} />
      <History leg={leg} />
    </>
  );
}

function initialDraft(leg: LegDetail) {
  const cur = leg.current ?? EMPTY_OVERRIDE;
  return {
    classification: (cur.classification ?? "") as LegClassification | "",
    basisMode: (cur.basis_lots?.length
      ? "lots"
      : cur.basis_from_market
        ? "market"
        : "unknown") as BasisMode,
    lots: cur.basis_lots?.map((l) => ({
      quantity: l.quantity,
      basis: l.basis_usd ?? "",
      kind: l.basis_kind,
      acquired: toUtcInput(l.acquired_at),
    })) ?? [
      {
        quantity: leg.quantity,
        basis: "",
        kind: "known" as BasisKind,
        acquired: toUtcInput(leg.occurred_at),
      },
    ],
    proceedsMode: (cur.proceeds_usd !== null
      ? "manual"
      : cur.proceeds_from_market
        ? "market"
        : "none") as ProceedsMode,
    proceeds: cur.proceeds_usd ?? "",
    price: cur.price_usd ?? "",
    pairWith: cur.pair_with ?? "",
    note: cur.note ?? "",
  };
}

function LegEditor({
  leg,
  guardRef,
  onPending,
}: {
  leg: LegDetail;
  guardRef: React.RefObject<EditorGuard | null>;
  onPending: (pending: boolean) => void;
}) {
  const { t } = useTranslation();
  const { locale, timeZone, privacy, togglePrivacy } = useApp();
  const queryClient = useQueryClient();
  const incoming = leg.direction === "in";
  const initial = initialDraft(leg);
  const [draft, setDraft] = useViewState<ReturnType<typeof initialDraft> | null>(
    `leg:${leg.leg_id}:draft`,
    null,
  );
  const { classification, basisMode, lots, proceedsMode, proceeds, price, pairWith, note } =
    draft ?? initial;
  function change<K extends keyof typeof initial>(key: K, value: (typeof initial)[K]) {
    setDraft((previous) => ({ ...(previous ?? initial), [key]: value }));
  }
  const setClassification = (value: LegClassification | "") => change("classification", value);
  const setBasisMode = (value: BasisMode) => change("basisMode", value);
  const setLots = (value: LotDraft[]) => change("lots", value);
  const setProceedsMode = (value: ProceedsMode) => change("proceedsMode", value);
  const setProceeds = (value: string) => change("proceeds", value);
  const setPrice = (value: string) => change("price", value);
  const setPairWith = (value: string) => change("pairWith", value);
  const setNote = (value: string) => change("note", value);
  const formRef = useRef<HTMLFormElement>(null);
  const [invalid, setInvalid] = useState<string | null>(null);
  const [reverting, setReverting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const save = useMutation({
    mutationFn: (decision: LegOverride) => api.updateBasis(leg.leg_id, decision),
    onSuccess: async () => {
      setError(null);
      setInvalid(null);
      setDraft(null);
      setReverting(false);
      await queryClient.invalidateQueries();
    },
    onError: (e) => setError(isCommandError(e) ? e.message : t("errors.generic")),
  });

  useEffect(() => {
    onPending(save.isPending);
    return () => onPending(false);
  }, [save.isPending, onPending]);
  const dirty = draft !== null && JSON.stringify(draft) !== JSON.stringify(initial);
  useEffect(() => {
    guardRef.current = { dirty, pending: save.isPending, discard: () => setDraft(null) };
    return () => {
      guardRef.current = null;
    };
  }, [dirty, save.isPending, guardRef, setDraft]);
  useEffect(() => {
    if (!dirty && !save.isPending) return;
    const warn = (event: BeforeUnloadEvent) => {
      event.preventDefault();
      event.returnValue = "";
    };
    window.addEventListener("beforeunload", warn);
    return () => window.removeEventListener("beforeunload", warn);
  }, [dirty, save.isPending]);

  function invalidField(field: string, message: string): string {
    setInvalid(field);
    formRef.current?.querySelector<HTMLElement>(`[name="${field}"]`)?.focus();
    return message;
  }
  function fieldProps(name: string) {
    return {
      name,
      autoComplete: "off",
      "aria-invalid": invalid === name ? true : undefined,
      "aria-describedby": invalid === name ? "decision-error" : undefined,
    };
  }

  function build(): LegOverride | string {
    if (price && !DECIMAL.test(price))
      return invalidField("decision-price", t("leg.errDecimal", { field: t("leg.price") }));
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
        for (const [index, l] of lots.entries()) {
          if (!DECIMAL.test(l.quantity))
            return invalidField(
              `lot-${index}-quantity`,
              t("leg.errDecimal", { field: t("leg.lotQuantity") }),
            );
          if (l.basis && !DECIMAL.test(l.basis))
            return invalidField(
              `lot-${index}-basis`,
              t("leg.errDecimal", { field: t("leg.lotBasis") }),
            );
          const at = fromUtcInput(l.acquired);
          if (at === null) return invalidField(`lot-${index}-acquired`, t("leg.errDate"));
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
          if (!DECIMAL.test(proceeds))
            return invalidField(
              "decision-proceeds",
              t("leg.errDecimal", { field: t("leg.proceeds") }),
            );
          decision.proceeds_usd = proceeds;
        }
      }
    }
    return decision;
  }

  function submit(e: React.FormEvent) {
    e.preventDefault();
    if (save.isPending) return;
    setInvalid(null);
    const d = build();
    if (typeof d === "string") setError(d);
    else save.mutate(d);
  }

  if (privacy)
    return (
      <div className="notice">
        <p>{MASK}</p>
        <button className="btn" onClick={togglePrivacy}>
          {t("common.revealEditor")}
        </button>
      </div>
    );

  const classes = incoming ? INCOMING : OUTGOING;
  const sale = classification === "sale" || classification === "payment";
  return (
    <form ref={formRef} className="editor" onSubmit={submit} aria-labelledby="editor-heading">
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
                      {...fieldProps(`lot-${i}-quantity`)}
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
                      {...fieldProps(`lot-${i}-basis`)}
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
                      {...fieldProps(`lot-${i}-acquired`)}
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
              {...fieldProps("decision-proceeds")}
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
          {...fieldProps("decision-price")}
          value={price}
          onChange={(e) => setPrice(e.target.value)}
        />
      </label>
      <label className="field">
        <span>{t("leg.note")}</span>
        <input
          name="decision-note"
          autoComplete="off"
          className="input"
          value={note}
          onChange={(e) => setNote(e.target.value)}
        />
      </label>
      {error && (
        <p id="decision-error" className="field-error" role="alert">
          {error}
        </p>
      )}
      {save.isSuccess && !error && (
        <p className="meta" role="status">
          {t("leg.saved")}
        </p>
      )}
      {reverting && (
        <div className="notice" role="group" aria-label={t("common.revertConfirm")}>
          <p>{t("common.revertConfirm")}</p>
          <button
            type="button"
            className="btn"
            disabled={save.isPending}
            onClick={() => save.mutate(EMPTY_OVERRIDE)}
          >
            {t("common.confirm")}
          </button>
          <button
            type="button"
            className="btn"
            disabled={save.isPending}
            onClick={() => setReverting(false)}
          >
            {t("common.cancel")}
          </button>
        </div>
      )}
      <div className="row">
        <button className="btn btn-primary" type="submit" disabled={save.isPending}>
          {t(save.isPending ? "common.saving" : "leg.save")}
        </button>
        {leg.current && (
          <button
            type="button"
            className="btn btn-ghost"
            disabled={save.isPending}
            onClick={() => setReverting(true)}
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
  const { locale, timeZone, privacy } = useApp();
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
            <span className="meta">{privacy ? MASK : summarize(v.payload, t, locale)}</span>
          </li>
        ))}
      </ol>
    </section>
  );
}

function summarize(
  o: LegOverride,
  t: (k: string, v?: Record<string, unknown>) => string,
  locale: string,
): string {
  const parts: string[] = [];
  if (o.classification) parts.push(t(`classification.${o.classification}`));
  if (o.pair_with) parts.push(t("leg.summaryPaired"));
  if (o.basis_lots?.length) parts.push(t("leg.summaryLots", { count: o.basis_lots.length }));
  if (o.basis_from_market) parts.push(t("leg.basisMode.market"));
  if (o.proceeds_usd) parts.push(`${t("leg.proceeds")}: ${formatUsd(o.proceeds_usd, locale)}`);
  if (o.proceeds_from_market) parts.push(t("leg.proceedsMode.market"));
  if (o.price_usd) parts.push(`${t("leg.price")}: ${formatPrice(o.price_usd, locale)}`);
  if (o.note) parts.push(`“${o.note}”`);
  return parts.length ? parts.join(" · ") : t("leg.summaryDefault");
}
