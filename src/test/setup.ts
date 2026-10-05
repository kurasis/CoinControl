import "@testing-library/jest-dom/vitest";
import { vi } from "vitest";

// jsdom has no canvas or ResizeObserver; charts are verified visually and via their text summary.
vi.mock("echarts/core", async (importOriginal) => {
  const actual = await importOriginal<typeof import("echarts/core")>();
  return {
    ...actual,
    init: () => ({ setOption: () => {}, resize: () => {}, dispose: () => {} }),
  };
});

globalThis.ResizeObserver ??= class {
  observe() {}
  unobserve() {}
  disconnect() {}
} as unknown as typeof ResizeObserver;
