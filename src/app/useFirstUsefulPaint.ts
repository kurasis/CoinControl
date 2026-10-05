import { useEffect } from "react";

/** Data-ready frame timing only; the mark contains no portfolio values. */
export function useFirstUsefulPaint(ready: boolean) {
  useEffect(() => {
    if (
      !ready ||
      typeof performance.mark !== "function" ||
      typeof performance.getEntriesByName !== "function" ||
      typeof requestAnimationFrame !== "function" ||
      performance.getEntriesByName("portfolio-first-useful").length
    )
      return;
    let painted = 0;
    const frame = requestAnimationFrame(() => {
      painted = requestAnimationFrame(() => {
        if (!performance.getEntriesByName("portfolio-first-useful").length)
          performance.mark("portfolio-first-useful");
      });
    });
    return () => {
      cancelAnimationFrame(frame);
      cancelAnimationFrame(painted);
    };
  }, [ready]);
}
