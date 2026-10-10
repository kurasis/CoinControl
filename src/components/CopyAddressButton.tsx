import { useState } from "react";
import { useTranslation } from "react-i18next";

/** Clipboard failures are visible, including WebViews without the Clipboard API. */
export function CopyAddressButton({
  address,
  compact = false,
}: {
  address: string;
  compact?: boolean;
}) {
  const { t } = useTranslation();
  const [status, setStatus] = useState<"copied" | "copyFailed" | null>(null);
  const [pending, setPending] = useState(false);
  return (
    <span className="stack">
      <button
        className={compact ? "btn btn-ghost" : "btn"}
        disabled={pending}
        aria-label={t("wallets.copyAddress")}
        onClick={async () => {
          setPending(true);
          setStatus(null);
          try {
            if (!navigator.clipboard) throw new Error("Clipboard unavailable");
            await navigator.clipboard.writeText(address);
            setStatus("copied");
          } catch {
            setStatus("copyFailed");
          } finally {
            setPending(false);
          }
        }}
      >
        {t(compact ? "wallets.copy" : "wallets.copyAddress")}
      </button>
      <span className="meta" role="status">
        {status && t(`common.${status}`)}
      </span>
    </span>
  );
}
