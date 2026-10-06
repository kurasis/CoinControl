import { expect, it } from "vitest";
import { InfiniteQueryObserver, QueryClient } from "@tanstack/react-query";
import { refreshCachedViews } from "./refreshCachedViews";

it("retains the pending activity page and refreshes earlier pages after a background write", async () => {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: Infinity } },
  });
  let release!: () => void;
  const pending = new Promise<void>((resolve) => (release = resolve));
  let delayed = false;
  let revision = 0;
  let nextSignal: AbortSignal | undefined;
  const options = {
    queryKey: ["activity", "concurrent-write"],
    initialPageParam: 0,
    queryFn: async ({ pageParam, signal }: { pageParam: number; signal: AbortSignal }) => {
      if (pageParam === 1 && !delayed) {
        delayed = true;
        nextSignal = signal;
        await pending;
      }
      return {
        page: pageParam,
        revision,
        rows: Array.from({ length: 50 }, (_, i) => pageParam * 50 + i),
      };
    },
    getNextPageParam: (last: { page: number }) => last.page + 1,
  };
  await client.ensureInfiniteQueryData(options);
  const observer = new InfiniteQueryObserver(client, options);
  const unsubscribe = observer.subscribe(() => {});
  try {
    const next = observer.fetchNextPage();
    expect(nextSignal).toBeDefined();
    revision = 1;
    const refresh = refreshCachedViews(client);
    expect(nextSignal?.aborted).toBe(false);
    release();
    await Promise.all([next, refresh]);
    const pages = observer.getCurrentResult().data!.pages;
    expect(pages.map((page) => page.revision)).toEqual([1, 1]);
    expect(pages.flatMap((page) => page.rows)).toEqual(Array.from({ length: 100 }, (_, i) => i));
    await observer.fetchNextPage();
    expect(observer.getCurrentResult().data!.pages.flatMap((page) => page.rows)).toEqual(
      Array.from({ length: 150 }, (_, i) => i),
    );
  } finally {
    release();
    unsubscribe();
    client.clear();
  }
});
