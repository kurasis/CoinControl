import { useEffect, useRef, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api, isCommandError } from "../ipc/client";
import type { ImportPreview } from "../ipc/bindings/ImportPreview";
import type { RecalcSummary } from "../ipc/bindings/RecalcSummary";
import { Usd } from "./Amount";

const MAX_BYTES = 5 * 1024 * 1024;

/** Import fields in the order of ACCOUNTING.md §10 (plus the opening-lot cutoff). */
const FIELDS = [
  "external_row_id",
  "network_id",
  "account_address",
  "transaction_id",
  "leg_id",
  "asset_identifier",
  "quantity",
  "acquired_at_utc",
  "total_basis_usd",
  "basis_kind",
  "classification",
  "note",
  "opening_cutoff_utc",
] as const;

/** CSV cost-basis import: choose a file, preview every row and the recalculation, then apply. */
export function ImportDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const closeRef = useRef<HTMLButtonElement>(null);
  const closedRef = useRef(false);
  const previewRef = useRef<ImportPreview | null>(null);
  const committingRef = useRef(false);
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [applied, setApplied] = useState<number | null>(null);
  const [file, setFile] = useState<{ name: string; content: string } | null>(null);

  useEffect(() => {
    closedRef.current = false;
    const opener = document.activeElement as HTMLElement | null;
    closeRef.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") close();
    };
    window.addEventListener("keydown", onKey);
    return () => {
      closedRef.current = true;
      if (previewRef.current)
        void api.discardBasisImport(previewRef.current.batch_id).catch(() => undefined);
      previewRef.current = null;
      window.removeEventListener("keydown", onKey);
      opener?.focus?.();
    };
    // close reads the latest lifecycle state through refs.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const fail = (e: unknown) => {
    if (!closedRef.current) setError(isCommandError(e) ? e.message : t("errors.generic"));
  };

  const load = useMutation({
    mutationFn: async (input: { file: File } | { mapping: Record<string, string> }) => {
      if ("file" in input) {
        if (input.file.size > MAX_BYTES)
          throw { code: "invalid_input", message: t("import.tooLarge") };
        const next = { name: input.file.name, content: await input.file.text() };
        if (closedRef.current) throw new DOMException("Import closed", "AbortError");
        setFile(next);
        return api.previewBasisImport(next.name, next.content);
      }
      if (!file) throw { code: "invalid_input", message: t("errors.generic") };
      if (preview) void api.discardBasisImport(preview.batch_id).catch(() => undefined);
      return api.previewBasisImport(file.name, file.content, input.mapping);
    },
    onSuccess: (p) => {
      if (closedRef.current) {
        void api.discardBasisImport(p.batch_id).catch(() => undefined);
        return;
      }
      if (previewRef.current && previewRef.current.batch_id !== p.batch_id)
        void api.discardBasisImport(previewRef.current.batch_id).catch(() => undefined);
      previewRef.current = p;
      setError(null);
      setApplied(null);
      setPreview(p);
    },
    onError: fail,
  });
  const commit = useMutation({
    mutationFn: async (batchId: string) => {
      committingRef.current = true;
      try {
        return await api.commitBasisImport(batchId);
      } finally {
        committingRef.current = false;
      }
    },
    onSuccess: async (r) => {
      previewRef.current = null;
      if (!closedRef.current) {
        setPreview(null);
        setApplied(r.applied_rows);
      }
      await queryClient.invalidateQueries();
    },
    onError: fail,
  });

  function close() {
    if (committingRef.current || closedRef.current) return;
    closedRef.current = true;
    if (previewRef.current)
      void api.discardBasisImport(previewRef.current.batch_id).catch(() => undefined);
    previewRef.current = null;
    onClose();
  }

  return (
    <div className="drawer-backdrop" onClick={close}>
      <div
        className="dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="import-title"
        onClick={(e) => e.stopPropagation()}
      >
        <header className="drawer-head">
          <h2 id="import-title">{t("import.title")}</h2>
          <div className="toolbar-spacer" />
          <button
            ref={closeRef}
            className="btn btn-ghost"
            onClick={close}
            disabled={commit.isPending}
          >
            {t("common.close")}
          </button>
        </header>
        <div className="drawer-body">
          <p className="meta">{t("import.intro")}</p>
          <label className="field">
            <span>{t("import.file")}</span>
            <input
              type="file"
              accept=".csv,text/csv"
              disabled={load.isPending || commit.isPending}
              onChange={(e) => {
                const f = e.target.files?.[0];
                if (f) load.mutate({ file: f });
                e.target.value = "";
              }}
            />
          </label>
          {load.isPending && <p className="meta">{t("import.reading")}</p>}
          {error && (
            <p className="field-error" role="alert">
              {error}
            </p>
          )}
          {applied !== null && (
            <p className="notice" role="status">
              {t("import.applied", { count: applied })}
            </p>
          )}
          {preview && (
            <PreviewView
              preview={preview}
              busy={commit.isPending || load.isPending}
              onRemap={(field, header) =>
                load.mutate({ mapping: { ...preview.mapping, [field]: header } })
              }
              onCommit={() => commit.mutate(preview.batch_id)}
              onDiscard={() => {
                void api.discardBasisImport(preview.batch_id);
                previewRef.current = null;
                setPreview(null);
              }}
            />
          )}
        </div>
      </div>
    </div>
  );
}

function PreviewView({
  preview: p,
  busy,
  onCommit,
  onDiscard,
  onRemap,
}: {
  preview: ImportPreview;
  busy: boolean;
  onRemap: (field: string, header: string) => void;
  onCommit: () => void;
  onDiscard: () => void;
}) {
  const { t } = useTranslation();
  return (
    <section aria-label={t("import.preview")}>
      <p role="status">
        {t("import.counts", {
          ok: p.ok_count,
          errors: p.error_count,
          duplicates: p.duplicate_count,
        })}
      </p>
      {p.duplicate_file && <p className="notice">{t("import.duplicateFile")}</p>}
      {p.missing_required.length > 0 && (
        <p className="field-error">
          {t("import.missingColumns", { columns: p.missing_required.join(", ") })}
        </p>
      )}
      <details className="mapping" open={p.missing_required.length > 0}>
        <summary>{t("import.mapping")}</summary>
        <div className="mapping-grid">
          {FIELDS.map((field) => (
            <label key={field} className="field">
              <span>
                <code>{field}</code>
              </span>
              <select
                className="select"
                value={p.mapping[field] ?? ""}
                disabled={busy}
                onChange={(e) => onRemap(field, e.target.value)}
              >
                <option value="">{t("import.unmapped")}</option>
                {p.columns.map((c) => (
                  <option key={c} value={c}>
                    {c}
                  </option>
                ))}
              </select>
            </label>
          ))}
        </div>
      </details>
      <Recalc before={p.before} after={p.after} />
      <div className="table-scroll">
        <table className="table table-compact">
          <thead>
            <tr>
              <th>{t("import.colRow")}</th>
              <th>{t("import.colStatus")}</th>
              <th>{t("import.colQuantity")}</th>
              <th className="right">{t("import.colBasis")}</th>
              <th>{t("import.colMessages")}</th>
            </tr>
          </thead>
          <tbody>
            {p.rows.map((r) => (
              <tr key={r.row}>
                <td className="num">
                  {r.row}
                  {r.external_row_id && <span className="meta"> · {r.external_row_id}</span>}
                </td>
                <td>
                  <span
                    className={
                      r.status === "ok" ? "chip" : r.status === "error" ? "chip chip-error" : "chip"
                    }
                  >
                    {t(`import.status.${r.status}`)}
                  </span>
                  {r.opening && <span className="meta"> {t("import.opening")}</span>}
                </td>
                <td className="num">{r.quantity ?? "—"}</td>
                <td className="right">
                  <Usd value={r.total_basis_usd} />
                </td>
                <td className="meta wrap">{r.messages.join("; ")}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <div className="row gap-top">
        <button className="btn btn-primary" disabled={!p.can_commit || busy} onClick={onCommit}>
          {t("import.apply", { count: p.ok_count })}
        </button>
        <button className="btn btn-ghost" onClick={onDiscard}>
          {t("import.discard")}
        </button>
        {!p.can_commit && <span className="meta">{t("import.cannotApply")}</span>}
      </div>
    </section>
  );
}

function Recalc({ before, after }: { before: RecalcSummary; after: RecalcSummary }) {
  const { t } = useTranslation();
  const rows: Array<[string, React.ReactNode, React.ReactNode]> = [
    [
      t("import.realized"),
      <Usd key="b" value={before.realized_known_usd} />,
      <Usd key="a" value={after.realized_known_usd} />,
    ],
    [
      t("import.knownBasis"),
      <Usd key="b" value={before.remaining_known_basis_usd} />,
      <Usd key="a" value={after.remaining_known_basis_usd} />,
    ],
    [t("import.unknownLots"), before.unknown_basis_lots, after.unknown_basis_lots],
    [t("import.reviewItems"), before.review_items, after.review_items],
    [t("import.gaps"), before.inventory_gaps, after.inventory_gaps],
  ];
  return (
    <table className="table table-compact" aria-label={t("import.recalc")}>
      <thead>
        <tr>
          <th>{t("import.recalc")}</th>
          <th className="right">{t("import.before")}</th>
          <th className="right">{t("import.after")}</th>
        </tr>
      </thead>
      <tbody>
        {rows.map(([label, b, a]) => (
          <tr key={label}>
            <td>{label}</td>
            <td className="right num">{b}</td>
            <td className="right num">{a}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
