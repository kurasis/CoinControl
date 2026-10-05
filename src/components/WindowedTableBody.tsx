import { useEffect, useState, type ReactNode } from "react";
import { defaultRangeExtractor, useVirtualizer } from "@tanstack/react-virtual";

/** Keep long tables bounded while preserving native table semantics and keyboard focus. */
export function WindowedTableBody<T>({
  rows,
  columns,
  rowKey,
  children,
}: {
  rows: T[];
  columns: number;
  rowKey: (row: T) => string | number;
  children: (row: T) => ReactNode;
}) {
  "use no memo"; // TanStack Virtual owns mutable measurements; do not compiler-memoize them.
  const [body, setBody] = useState<HTMLTableSectionElement | null>(null);
  const [focusedKey, setFocusedKey] = useState<string | number | null>(null);
  const focused = focusedKey === null ? -1 : rows.findIndex((row) => rowKey(row) === focusedKey);
  const enabled = rows.length > 100;
  const scroller = body?.closest<HTMLElement>(".table-scroll") ?? null;
  // Mutable measurements stay inside this non-memoized component (see the directive above).
  // eslint-disable-next-line react-hooks/incompatible-library
  const virtual = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scroller,
    getItemKey: (index) => rowKey(rows[index]!),
    estimateSize: () => 64,
    overscan: 6,
    enabled,
    initialRect: { width: 800, height: 480 },
    rangeExtractor: (range) => {
      const indices = defaultRangeExtractor(range);
      if (focused >= 0 && !indices.includes(focused)) {
        indices.push(focused);
        indices.sort((a, b) => a - b);
      }
      return indices;
    },
  });

  useEffect(() => {
    if (!scroller || !body) return;
    scroller.classList.toggle("table-windowed", enabled);
    scroller.tabIndex = enabled ? 0 : -1;
    body.closest("table")?.setAttribute("aria-rowcount", String(rows.length + 1));
    return () => {
      scroller.classList.remove("table-windowed");
      scroller.removeAttribute("tabindex");
    };
  }, [body, scroller, enabled, rows.length]);

  const items = enabled ? virtual.getVirtualItems() : rows.map((_, index) => ({ index }));
  const spacer = (height: number, key: string) =>
    height > 0 && (
      <tr key={key} aria-hidden="true" className="table-spacer">
        <td colSpan={columns} style={{ height }} />
      </tr>
    );
  return (
    <tbody ref={setBody} data-row-count={rows.length} data-windowed={enabled}>
      {items.flatMap((item, position) => {
        const measured = enabled ? virtual.getVirtualItems()[position] : undefined;
        const previous = enabled ? virtual.getVirtualItems()[position - 1] : undefined;
        return [
          measured && spacer(measured.start - (previous?.end ?? 0), `gap-${item.index}`),
          <tr
            key={`row-${rowKey(rows[item.index]!)}`}
            data-index={item.index}
            aria-rowindex={item.index + 2}
            ref={enabled ? virtual.measureElement : undefined}
            onFocus={() => setFocusedKey(rowKey(rows[item.index]!))}
            onBlur={(event) => {
              if (!event.currentTarget.contains(event.relatedTarget)) setFocusedKey(null);
            }}
          >
            {children(rows[item.index]!)}
          </tr>,
        ];
      })}
      {enabled &&
        spacer(virtual.getTotalSize() - (virtual.getVirtualItems().at(-1)?.end ?? 0), "tail")}
    </tbody>
  );
}
