import { act, renderHook } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { useFirstUsefulPaint } from "./useFirstUsefulPaint";

afterEach(() => vi.unstubAllGlobals());

it("releases optional work after two data-ready frames and marks the useful screen once", () => {
  const entries: { name: string }[] = [];
  const mark = vi.fn((name: string) => entries.push({ name }));
  const frames: FrameRequestCallback[] = [];
  vi.stubGlobal("performance", { now: () => 0, mark, getEntriesByName: () => entries });
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => frames.push(callback));
  vi.stubGlobal("cancelAnimationFrame", vi.fn());
  const { result, rerender } = renderHook(({ ready }) => useFirstUsefulPaint(ready), {
    initialProps: { ready: false },
  });
  expect(result.current).toBe(false);
  expect(frames).toHaveLength(0);
  rerender({ ready: true });
  act(() => frames.shift()!(0));
  expect(result.current).toBe(false);
  expect(mark).not.toHaveBeenCalled();
  act(() => frames.shift()!(16));
  expect(result.current).toBe(true);
  expect(mark).toHaveBeenCalledExactlyOnceWith("portfolio-first-useful");
  rerender({ ready: false });
  expect(result.current).toBe(false);
  rerender({ ready: true });
  expect(result.current).toBe(true);
  expect(mark).toHaveBeenCalledTimes(1);
});
