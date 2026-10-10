import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactNode } from "react";
import "../i18n";
import { api } from "../ipc/client";
import { resetMock } from "../ipc/mock";
import { AppProvider } from "../app/AppContext";
import { AssetPolicyControls } from "./AssetPolicyControls";
import { CopyAddressButton } from "./CopyAddressButton";
import { ProviderQuotaControls } from "./ProviderQuotaControls";

beforeEach(() => resetMock());
afterEach(() => vi.restoreAllMocks());
function mount(child: ReactNode) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <AppProvider>{child}</AppProvider>
    </QueryClientProvider>,
  );
}

it("shows and saves Ankr's existing daily credit budget", async () => {
  const user = userEvent.setup();
  const provider = (await api.listProviders()).find((p) => p.id === "ankr")!;
  const save = vi.spyOn(api, "setProviderQuota");
  mount(<ProviderQuotaControls provider={provider} />);
  await user.click(screen.getByText("Local provider budgets"));
  const credits = await screen.findByRole("spinbutton", { name: "Estimated credits per UTC day" });
  await user.clear(credits);
  await user.type(credits, "500000");
  await user.click(screen.getByRole("button", { name: "Save budgets" }));
  expect(save).toHaveBeenCalledWith("ankr", expect.objectContaining({ daily_credits: 500000 }));
});

it("bounds asset policy controls and searches assets outside the first page", async () => {
  const user = userEvent.setup();
  vi.spyOn(api, "listAssetPolicies").mockResolvedValue(
    Array.from({ length: 123 }, (_, i) => ({
      asset_id: `ethereum:token:${i}`,
      symbol: `TOKEN${i}`,
      name: null,
      verification: "verified",
      hidden: false,
      exclude_override: null,
    })),
  );
  mount(<AssetPolicyControls />);
  const section = screen.getByRole("region", { name: "Token visibility and accounting" });
  await within(section).findByText(/TOKEN0 ·/);
  expect(within(section).getAllByRole("checkbox")).toHaveLength(50);
  await user.click(within(section).getByRole("button", { name: "Next" }));
  expect(within(section).getAllByRole("checkbox")).toHaveLength(50);
  await user.click(within(section).getByRole("button", { name: "Next" }));
  expect(within(section).getAllByRole("checkbox")).toHaveLength(23);
  await user.type(within(section).getByRole("searchbox"), "TOKEN122");
  expect(within(section).getAllByRole("checkbox")).toHaveLength(1);
  expect(within(section).getByText(/TOKEN122 ·/)).toBeInTheDocument();
});

it("reports clipboard success and rejection without an unhandled promise", async () => {
  const user = userEvent.setup();
  const write = vi
    .spyOn(navigator.clipboard, "writeText")
    .mockRejectedValueOnce(new Error("denied"))
    .mockResolvedValueOnce(undefined);
  render(<CopyAddressButton address="public-address" />);
  const button = screen.getByRole("button", { name: "Copy full address" });
  await user.click(button);
  expect(await screen.findByRole("status")).toHaveTextContent("Could not copy");
  await user.click(button);
  expect(screen.getByRole("status")).toHaveTextContent("Address copied");
  expect(write).toHaveBeenLastCalledWith("public-address");
});
