import { beforeEach, expect, it, vi } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "../i18n";
import { ChartDataTable } from "./ChartDataTable";
import { MASK } from "../lib/format";

const app = vi.hoisted(() => ({ privacy: false, locale: "en-US", timeZone: "UTC" }));
vi.mock("../app/AppContext", () => ({ useApp: () => app }));
const points = [
  { t: 1704067200, value: "9007199254740993.17", estimated: false },
  { t: 1704153600, value: null, estimated: false },
  { t: 1704240000, value: "0", estimated: true, partial: true },
];

beforeEach(async () => {
  app.privacy = false;
  app.locale = "en-US";
  await i18n.changeLanguage("en");
});

it("shows exact large decimals, zero and a missing observation distinctly with coverage labels", async () => {
  const user = userEvent.setup();
  render(<ChartDataTable points={points} kind="value" label="Portfolio value" />);
  const toggle = screen.getByText("View chart data: Portfolio value");
  // jsdom does not implement the native summary keyboard default action;
  // Enter/Space expansion is covered in the actual Chromium/WebView2 harness.
  await user.click(toggle);
  expect(toggle.closest("details")).toHaveAttribute("open");
  const table = screen.getByRole("table", { name: "Portfolio value observations" });
  expect(within(table).getByText("$9,007,199,254,740,993.17")).toBeInTheDocument();
  expect(within(table).getByText("$0.00")).toBeInTheDocument();
  expect(within(table).getByText("—")).toBeInTheDocument();
  expect(within(table).getByText("No data")).toBeInTheDocument();
  expect(
    within(table).getByText("Estimated from daily prices · Some assets have no price"),
  ).toBeInTheDocument();
});

it("removes private values from rendered text when privacy changes, while market prices stay public", async () => {
  const user = userEvent.setup();
  const view = render(<ChartDataTable points={points} kind="value" label="Portfolio value" />);
  await user.click(screen.getByText("View chart data: Portfolio value"));
  app.privacy = true;
  view.rerender(<ChartDataTable points={points} kind="value" label="Portfolio value" />);
  expect(view.container.textContent).not.toContain("9,007,199,254,740,993.17");
  expect(screen.getAllByText(MASK)).toHaveLength(2);
  expect(screen.getByText("—")).toBeInTheDocument();
  view.rerender(<ChartDataTable points={points} kind="price" label="Market price" />);
  expect(screen.getByText("$9,007,199,254,740,993.17")).toBeInTheDocument();
});

it("keeps a long series bounded and reaches the last observation with keyboard pagination", async () => {
  const user = userEvent.setup();
  const long = Array.from({ length: 102 }, (_, i) => ({
    t: 1704067200 + i * 86400,
    value: String(i),
    estimated: false,
  }));
  render(<ChartDataTable points={long} kind="value" label="Portfolio value" />);
  await user.click(screen.getByText("View chart data: Portfolio value"));
  expect(screen.getAllByRole("row")).toHaveLength(51);
  const next = screen.getByRole("button", { name: "Next page" });
  next.focus();
  await user.keyboard("{Enter}{Enter}");
  expect(screen.getByRole("status")).toHaveTextContent("Page 3 of 3");
  expect(screen.getAllByRole("row")).toHaveLength(3);
  expect(screen.getByText("$101.00")).toBeInTheDocument();
  expect(next).toBeDisabled();
});

it("localizes dates and coverage in Russian", async () => {
  app.locale = "ru-RU";
  await i18n.changeLanguage("ru");
  const user = userEvent.setup();
  render(<ChartDataTable points={points} kind="value" label="Стоимость портфеля" />);
  await user.click(screen.getByText("Данные графика: Стоимость портфеля"));
  expect(screen.getByRole("columnheader", { name: "Дата и время" })).toBeInTheDocument();
  expect(screen.getByText("Нет данных")).toBeInTheDocument();
  expect(screen.getByText(/1 янв\. 2024/)).toBeInTheDocument();
});
