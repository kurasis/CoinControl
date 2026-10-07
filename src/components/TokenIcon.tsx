/** Local monogram icon; remote token-provided images are never requested. */
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
