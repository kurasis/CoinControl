/** Monogram fallback icon. Token-provided images are not loaded in this stage. */
export function TokenIcon({ symbol }: { symbol: string | null }) {
  const text =
    (symbol ?? "?")
      .replace(/[^\p{L}\p{N}]/gu, "")
      .slice(0, 3)
      .toUpperCase() || "?";
  return (
    <span className="token-icon" aria-hidden="true">
      {text}
    </span>
  );
}
