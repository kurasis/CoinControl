import { useEffect, useState } from "react";

/** Data-ready frame timing only; the mark contains no portfolio values. */
export function useFirstUsefulPaint(ready: boolean) {
  const [painted, setPainted] = useState(
    () =>
      typeof requestAnimationFrame !== "function" ||
      (typeof performance.getEntriesByName === "function" &&
        performance.getEntriesByName("portfolio-first-useful").length > 0),
  );
  useEffect(() => {
    if (!ready || painted || typeof requestAnimationFrame !== "function") return;
    let secondFrame = 0;
    const frame = requestAnimationFrame(() => {
      secondFrame = requestAnimationFrame(() => {
        if (
          typeof performance.mark === "function" &&
          typeof performance.getEntriesByName === "function" &&
          !performance.getEntriesByName("portfolio-first-useful").length
        )
          performance.mark("portfolio-first-useful");
        setPainted(true);
      });
    });
    return () => {
      cancelAnimationFrame(frame);
      cancelAnimationFrame(secondFrame);
    };
  }, [ready, painted]);
  return ready && painted;
}
