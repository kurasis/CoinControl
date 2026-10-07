import { afterEach, expect, it, vi } from "vitest";
import { fireEvent, render, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { api } from "../ipc/client";
import { TokenIcon } from "./TokenIcon";

afterEach(() => vi.restoreAllMocks());

function draw(symbol: string | null, assetId?: string) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <TokenIcon symbol={symbol} assetId={assetId} />
      <TokenIcon symbol={symbol} assetId={assetId} />
    </QueryClientProvider>,
  );
}

it("keeps a text fallback without identity and never loads by symbol", () => {
  const request = vi.spyOn(api, "assetIcon");
  const view = draw("<svg>eth</svg>");
  expect(view.container.textContent).toBe("SVGSVG");
  expect(request).not.toHaveBeenCalled();
  expect(view.container.querySelector("svg")).toBeNull();
});

it("coalesces identities and falls back when a sanitized image cannot display", async () => {
  const request = vi.spyOn(api, "assetIcon").mockResolvedValue("data:image/png;base64,AA==");
  const view = draw("ETH", "asset:ethereum:native");
  await waitFor(() => expect(view.container.querySelectorAll("img")).toHaveLength(2));
  expect(request).toHaveBeenCalledOnce();
  fireEvent.error(view.container.querySelector("img")!);
  expect(view.container.textContent).toBe("ETH");
  expect(view.container.querySelectorAll("img")).toHaveLength(1);
});

it.each([null, "https://untrusted.example/logo.png", "data:image/svg+xml,<svg/>"])(
  "uses the monogram for missing or non-PNG results: %s",
  async (result) => {
    const request = vi.spyOn(api, "assetIcon").mockResolvedValue(result);
    const view = draw("BTC", "asset:bitcoin:native");
    await waitFor(() => expect(request).toHaveBeenCalledOnce());
    expect(view.container.querySelector("img")).toBeNull();
    expect(view.container.textContent).toBe("BTCBTC");
  },
);

it("does not turn an optional IPC failure into an asset view failure", async () => {
  const request = vi.spyOn(api, "assetIcon").mockRejectedValue(new Error("offline"));
  const view = draw(null, "asset:bitcoin:native");
  await waitFor(() => expect(request).toHaveBeenCalledOnce());
  expect(view.container.textContent).toBe("??");
});
