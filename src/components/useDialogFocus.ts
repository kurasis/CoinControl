import { useEffect, useEffectEvent, type RefObject } from "react";

/** Keep modal keyboard navigation inside the panel and restore its opener on close. */
export function useDialogFocus(ref: RefObject<HTMLElement | null>, onClose: () => void) {
  const close = useEffectEvent(onClose);
  useEffect(() => {
    const panel = ref.current;
    if (!panel) return;
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const focusable = () =>
      [
        ...panel.querySelectorAll<HTMLElement>(
          "button, a[href], input, select, textarea, summary, [tabindex]",
        ),
      ].filter(
        (element) =>
          element.tabIndex >= 0 &&
          !element.matches(":disabled") &&
          !element.closest("[hidden], [inert]") &&
          (element.checkVisibility?.() ?? true),
      );
    (focusable()[0] ?? panel).focus();
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        close();
      } else if (event.key === "Tab") {
        const elements = focusable();
        const first = elements[0];
        const last = elements.at(-1);
        const active = document.activeElement;
        if (
          !first ||
          !panel.contains(active) ||
          (event.shiftKey
            ? active === first || active === panel
            : active === last || active === panel)
        ) {
          event.preventDefault();
          (event.shiftKey ? (last ?? panel) : (first ?? panel)).focus();
        }
      }
    };
    const onFocus = (event: FocusEvent) => {
      if (!panel.contains(event.target as Node)) (focusable()[0] ?? panel).focus();
    };
    document.addEventListener("keydown", onKey);
    document.addEventListener("focusin", onFocus);
    return () => {
      document.removeEventListener("keydown", onKey);
      document.removeEventListener("focusin", onFocus);
      if (opener?.isConnected) opener.focus();
    };
  }, [ref]);
}
