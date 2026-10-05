import { beforeEach, describe, expect, it } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import "./i18n";
import i18n from "./i18n";
import { App } from "./App";
import { resetMock } from "./ipc/mock";
import { api } from "./ipc/client";

function renderApp() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <App />
    </QueryClientProvider>,
  );
}

beforeEach(async () => {
  resetMock();
  window.location.hash = "#/";
  await i18n.changeLanguage("en");
});

describe("first launch", () => {
  it("opens with no keys and offers setup paths and a labeled demo", async () => {
    renderApp();
    expect(await screen.findByText("Track your crypto, read-only")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Add address" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Configure data sources" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Explore demo portfolio" })).toBeInTheDocument();
    // No fake balances on an empty profile.
    expect(screen.queryByText("$0.00")).not.toBeInTheDocument();
  });
});

describe("demo portfolio", () => {
  it("shows required columns, a dash for missing quotes, and masks values in privacy mode", async () => {
    const user = userEvent.setup();
    renderApp();
    await user.click(await screen.findByRole("button", { name: "Explore demo portfolio" }));
    expect(await screen.findByText("Demo data. Not your portfolio.")).toBeInTheDocument();

    const assets = await screen.findByRole("region", { name: "Assets" });
    const table = await within(assets).findByRole("table");
    for (const col of ["Asset", "Price (USD)", "Balance", "Value (USD)", "24h"]) {
      expect(
        within(table).getByRole("columnheader", { name: new RegExp(col.replace(/[()]/g, "\\$&")) }),
      ).toBeInTheDocument();
    }
    const unpriced = within(table).getByText("Unpriced demo token").closest("tr")!;
    expect(within(unpriced).getAllByText("—").length).toBeGreaterThanOrEqual(2);
    expect(screen.getByText(/Partial: 1 asset without a price/)).toBeInTheDocument();

    const total = screen.getByText("Total balance").parentElement!;
    expect(within(total).getByText(/^\$[\d,]+\.\d{2}$/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Hide balances and addresses" }));
    expect(within(total).queryByText(/^\$[\d,]+\.\d{2}$/)).not.toBeInTheDocument();
    expect(within(total).getByText("•••••")).toBeInTheDocument();
  });
});

describe("localization", () => {
  it("renders the Russian interface", async () => {
    const settings = await api.getSettings();
    await api.updateSettings({ ...settings, language: "ru" });
    renderApp();
    expect(
      await screen.findByText("Отслеживайте криптовалюту только для чтения"),
    ).toBeInTheDocument();
  });

  it("has the same keys in English and Russian", async () => {
    const en = (await import("./i18n/en.json")).default;
    const ru = (await import("./i18n/ru.json")).default;
    const keys = (o: object, p = ""): string[] =>
      Object.entries(o).flatMap(([k, v]) =>
        typeof v === "object"
          ? keys(v as object, `${p}${k}.`)
          : [`${p}${k.replace(/_(one|few|many|other)$/, "")}`],
      );
    expect(new Set(keys(ru))).toEqual(new Set(keys(en)));
  });
});

describe("synchronization", () => {
  it("shows sync state for a new address and runs a manual sync", async () => {
    const user = userEvent.setup();
    const wallet = await api.createWallet("Savings");
    await api.addAccount(wallet.id, "bitcoin", "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa");
    window.location.hash = "#/wallets";
    renderApp();
    expect(await screen.findByText("Waiting for the first synchronization")).toBeInTheDocument();
    await user.click(await screen.findByRole("button", { name: "Sync now" }));
    expect(await screen.findByText("Synchronization finished")).toBeInTheDocument();
  });

  it("offers no sync in the demo profile", async () => {
    await api.switchProfile("demo");
    window.location.hash = "#/wallets";
    renderApp();
    expect(await screen.findAllByText(/History complete/)).not.toHaveLength(0);
    expect(screen.queryByRole("button", { name: "Sync now" })).not.toBeInTheDocument();
  });
});

describe("network coverage", () => {
  it("reports what each required network reads and its limitations", async () => {
    window.location.hash = "#/settings/networks";
    renderApp();
    const table = await screen.findByRole("table", { name: "Networks" });
    for (const name of ["Bitcoin", "Base", "Arbitrum One", "Optimism", "Polygon PoS"]) {
      expect(await within(table).findByText(name)).toBeInTheDocument();
    }
    expect(within(table).getByText("BNB Smart Chain")).toBeInTheDocument();
    expect(within(table).getByText("TronGrid")).toBeInTheDocument();
    expect(within(table).getByText("TonAPI")).toBeInTheDocument();
    expect(
      screen.getByText(/Staked TRX \(frozen, delegated or unfreezing\) counts/),
    ).toBeInTheDocument();
    expect(screen.getByText(/TRC-10 tokens are not tracked/)).toBeInTheDocument();
  });
});

describe("accounting", () => {
  it("shows lifetime accounting metrics and the review entry point", async () => {
    await api.switchProfile("demo");
    renderApp();
    const metrics = await screen.findByRole("region", { name: "Performance summary" });
    for (const label of [
      "Unrealized P&L",
      "Period gain / return",
      "Realized P&L",
      "Income",
      "Fees and expenses",
      "Total P&L",
    ]) {
      expect(within(metrics).getByText(label)).toBeInTheDocument();
    }
    // Incomplete parts are never shown as complete totals.
    expect(await within(metrics).findByText("Partial")).toBeInTheDocument();
    expect(within(metrics).getByText("Available once every part is complete")).toBeInTheDocument();
    expect(
      await within(metrics).findByText("Some transfers are not classified yet"),
    ).toBeInTheDocument();
    expect(screen.getByText(/2 movements need your input/)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Review missing data" })).toHaveAttribute(
      "href",
      "#/review",
    );
  });

  it("resolves a movement in the review drawer and keeps an audit version", async () => {
    const user = userEvent.setup();
    await api.switchProfile("demo");
    window.location.hash = "#/review";
    renderApp();
    const items = await screen.findByRole("region", { name: /Needs your input/ });
    const row = (
      await within(items).findByText("Unclassified outgoing", { selector: ".chip" })
    ).closest("tr")!;
    await user.click(within(row).getByRole("button", { name: "Resolve" }));

    const drawer = await screen.findByRole("dialog", { name: "Movement details" });
    expect(
      await within(drawer).findByText(/This asset left your tracked accounts/),
    ).toBeInTheDocument();
    await user.selectOptions(within(drawer).getByLabelText("Classification"), "withdrawal");
    await user.click(within(drawer).getByRole("button", { name: "Save and recalculate" }));
    expect(await within(drawer).findByText("Decision history")).toBeInTheDocument();
    expect(within(drawer).getByText(/Version 1/)).toBeInTheDocument();

    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(
      await within(items).findByText("Unknown cost basis", { selector: ".chip" }),
    ).toBeInTheDocument();
    await waitFor(() =>
      expect(
        within(items).queryByText("Unclassified outgoing", { selector: ".chip" }),
      ).not.toBeInTheDocument(),
    );
  });

  it("rejects a non-decimal acquisition cost before saving", async () => {
    const user = userEvent.setup();
    await api.switchProfile("demo");
    window.location.hash = "#/review";
    renderApp();
    const row = (await screen.findByText("Unknown cost basis", { selector: ".chip" })).closest(
      "tr",
    )!;
    await user.click(within(row).getByRole("button", { name: "Resolve" }));
    const drawer = await screen.findByRole("dialog", { name: "Movement details" });
    await user.click(await within(drawer).findByLabelText("Enter acquisition lots"));
    await user.type(within(drawer).getByLabelText("Total cost (USD)"), "1,250");
    await user.click(within(drawer).getByRole("button", { name: "Save and recalculate" }));
    expect(await within(drawer).findByRole("alert")).toHaveTextContent(
      "Total cost (USD) must be a plain decimal number",
    );
  });

  it("previews a CSV import, blocks errors, and applies a clean file", async () => {
    const user = userEvent.setup();
    await api.switchProfile("demo");
    window.location.hash = "#/review";
    renderApp();
    await user.click(await screen.findByRole("button", { name: "Import cost basis CSV" }));
    const dialog = await screen.findByRole("dialog", { name: "Import cost basis from CSV" });
    const input = within(dialog).getByLabelText("CSV file");
    const header = "external_row_id,network_id,account_address,quantity,total_basis_usd";
    await user.upload(
      input,
      new File([`${header}\nr1,ethereum,0xabc,1.2,2400\nr2,ethereum,0xabc,bad,1\n`], "b.csv", {
        type: "text/csv",
      }),
    );
    expect(await within(dialog).findByText(/Ready: 1 · errors: 1/)).toBeInTheDocument();
    expect(
      within(dialog).getByRole("table", { name: "Recalculation preview" }),
    ).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Apply 1 row" })).toBeDisabled();

    await user.upload(
      input,
      new File([`${header}\nr1,ethereum,0xabc,1.2,2400\n`], "good.csv", { type: "text/csv" }),
    );
    expect(await within(dialog).findByText(/Ready: 1 · errors: 0/)).toBeInTheDocument();
    const apply = within(dialog).getByRole("button", { name: "Apply 1 row" });
    expect(apply).toBeEnabled();
    await user.click(apply);
    expect(await within(dialog).findByText(/Applied 1 row/)).toBeInTheDocument();
  });

  it("opens asset detail with separate price and holdings charts", async () => {
    const user = userEvent.setup();
    await api.switchProfile("demo");
    renderApp();
    const assets = await screen.findByRole("region", { name: "Assets" });
    await user.click(await within(assets).findByRole("link", { name: "Bitcoin" }));
    expect(await screen.findByRole("heading", { name: "Bitcoin", level: 1 })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Price" })).toHaveAttribute("aria-selected", "true");
    await user.click(screen.getByRole("tab", { name: "Your holdings value" }));
    expect(screen.getByRole("tab", { name: "Your holdings value" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(await screen.findByRole("heading", { name: "Open lots (FIFO)" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Activity for this asset" })).toBeInTheDocument();
  });
});
