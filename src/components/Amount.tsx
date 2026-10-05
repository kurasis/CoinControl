import { useApp } from "../app/AppContext";
import {
  DASH,
  MASK,
  formatPercent,
  formatPrice,
  formatQuantity,
  formatUsd,
  signOf,
} from "../lib/format";

/** USD amount. `null` renders as an em dash, never as $0. Masked in privacy mode. */
export function Usd({
  value,
  sensitive = true,
}: {
  value: string | null | undefined;
  sensitive?: boolean;
}) {
  const { locale, privacy } = useApp();
  if (sensitive && privacy && value != null)
    return (
      <span className="num" aria-label="hidden">
        {MASK}
      </span>
    );
  return <span className="num">{formatUsd(value, locale)}</span>;
}

/** Market unit price: public data, not masked. */
export function Price({ value }: { value: string | null | undefined }) {
  const { locale } = useApp();
  return <span className="num">{formatPrice(value, locale)}</span>;
}

export function Quantity({ value, symbol }: { value: string; symbol?: string | null }) {
  const { locale, privacy } = useApp();
  if (privacy) return <span className="num">{MASK}</span>;
  return (
    <span className="num">
      {formatQuantity(value, locale)}
      {symbol ? ` ${symbol}` : ""}
    </span>
  );
}

/** Signed percentage with sign text and color; color is never the only cue. */
export function Percent({
  value,
  sensitive = false,
}: {
  value: string | null | undefined;
  sensitive?: boolean;
}) {
  const { locale, privacy } = useApp();
  if (value == null) return <span className="num muted">{DASH}</span>;
  if (sensitive && privacy) return <span className="num">{MASK}</span>;
  const sign = signOf(value);
  return (
    <span className={`num ${sign > 0 ? "positive" : sign < 0 ? "negative" : ""}`}>
      {formatPercent(value, locale)}
    </span>
  );
}
