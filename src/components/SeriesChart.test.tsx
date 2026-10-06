import { expect, it, vi } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "../i18n";
import { SeriesChart } from "./SeriesChart";
import { MASK } from "../lib/format";

const app = vi.hoisted(() => ({ privacy: false, locale: "en-US", timeZone: "UTC" }));
const pendingRenderer = vi.hoisted(() => new Promise<never>(() => {}));
vi.mock("../app/AppContext", () => ({ useApp: () => app }));
vi.mock("./SeriesChartCanvas", () => ({
  default: () => {
    throw pendingRenderer;
  },
}));

it("keeps observations usable and privacy reactive while the canvas renderer is still loading", async () => {
  await i18n.changeLanguage("en");
  const user = userEvent.setup();
  const points = [{ t: 1704067200, value: "123.45", estimated: false }];
  const view = render(<SeriesChart points={points} kind="value" label="Portfolio value" />);
  expect(screen.getByRole("img")).toHaveAttribute("aria-busy", "true");
  await user.click(screen.getByText("View chart data: Portfolio value"));
  const table = screen.getByRole("table", { name: "Portfolio value observations" });
  expect(within(table).getByText("$123.45")).toBeInTheDocument();

  app.privacy = true;
  view.rerender(<SeriesChart points={points} kind="value" label="Portfolio value" />);
  expect(view.container.textContent).not.toContain("123.45");
  expect(within(table).getByText(MASK)).toBeInTheDocument();
  expect(screen.getByRole("img")).toHaveAttribute("aria-busy", "true");

  view.rerender(<SeriesChart points={points} kind="price" label="Market price" />);
  expect(screen.getByRole("table", { name: "Market price observations" })).toHaveTextContent(
    "$123.45",
  );
});
