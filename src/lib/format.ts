// Display formatting. Amounts arrive as exact decimal strings and are passed to
// Intl.NumberFormat as strings, so no float conversion happens before rounding.

export const DASH = "—";
export const MASK = "•••••";

function asIntl(value: string): Intl.StringNumericLiteral {
  return value as Intl.StringNumericLiteral;
}

export function formatUsd(value: string | null | undefined, locale: string): string {
  if (value === null || value === undefined) return DASH;
  return new Intl.NumberFormat(locale, {
    style: "currency",
    currency: "USD",
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
    roundingMode: "halfEven",
  }).format(asIntl(value));
}

/** Unit prices keep more precision for sub-dollar assets. */
export function formatPrice(value: string | null | undefined, locale: string): string {
  if (value === null || value === undefined) return DASH;
  const small = value.replace("-", "").startsWith("0.");
  return new Intl.NumberFormat(locale, {
    style: "currency",
    currency: "USD",
    minimumFractionDigits: 2,
    maximumFractionDigits: small ? 6 : 2,
    roundingMode: "halfEven",
  }).format(asIntl(value));
}

export function formatQuantity(value: string, locale: string, maxFraction = 8): string {
  const formatted = new Intl.NumberFormat(locale, {
    maximumFractionDigits: maxFraction,
    roundingMode: "trunc",
  }).format(asIntl(value));
  const isZero = /^-?0(\.0*)?$/.test(value);
  if (!isZero && /^-?0([.,]0*)?$/.test(formatted.replace(/\s/g, ""))) {
    const tiny = new Intl.NumberFormat(locale, { maximumFractionDigits: maxFraction }).format(
      asIntl(maxFraction === 0 ? "1" : `0.${"0".repeat(maxFraction - 1)}1`),
    );
    return `<${tiny}`;
  }
  return formatted;
}

export function formatPercent(value: string | null | undefined, locale: string): string {
  if (value === null || value === undefined) return DASH;
  const n = new Intl.NumberFormat(locale, {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
    signDisplay: "exceptZero",
    roundingMode: "halfEven",
  }).format(asIntl(value));
  return locale.startsWith("ru") ? `${n}\u00a0%` : `${n}%`;
}

export function signOf(value: string | null | undefined): -1 | 0 | 1 {
  if (!value) return 0;
  if (!/[1-9]/.test(value)) return 0;
  if (value.startsWith("-")) return -1;
  return 1;
}

function dateFormatter(
  locale: string,
  options: Intl.DateTimeFormatOptions,
  timeZone?: string | null,
): Intl.DateTimeFormat {
  try {
    return new Intl.DateTimeFormat(locale, { ...options, timeZone: timeZone ?? undefined });
  } catch (error) {
    // Older backups or a WebView with an older timezone database can contain
    // an unsupported override. Retry with the OS zone, keeping locale/style.
    if (!(error instanceof RangeError) || timeZone == null) throw error;
    return new Intl.DateTimeFormat(locale, options);
  }
}

export function formatDateTime(
  unixSeconds: number,
  locale: string,
  timeZone?: string | null,
): string {
  return dateFormatter(locale, { dateStyle: "medium", timeStyle: "short" }, timeZone).format(
    new Date(unixSeconds * 1000),
  );
}

export function formatDate(unixSeconds: number, locale: string, timeZone?: string | null): string {
  return dateFormatter(locale, { dateStyle: "medium" }, timeZone).format(
    new Date(unixSeconds * 1000),
  );
}

export function truncateAddress(address: string): string {
  return address.length > 20 ? `${address.slice(0, 8)}…${address.slice(-6)}` : address;
}
