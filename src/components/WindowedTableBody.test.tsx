import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { WindowedTableBody } from "./WindowedTableBody";

const rows = Array.from({ length: 10000 }, (_, index) => ({ id: index, name: `Row ${index}` }));
function Table({ data = rows }: { data?: typeof rows }) {
  return (
    <div className="table-scroll" data-testid="scroll">
      <table>
        <thead>
          <tr>
            <th>Name</th>
          </tr>
        </thead>
        <WindowedTableBody rows={data} columns={1} rowKey={(row) => row.id}>
          {(row) => (
            <td>
              <button>{row.name}</button>
            </td>
          )}
        </WindowedTableBody>
      </table>
    </div>
  );
}

beforeEach(() => {
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (
    this: HTMLElement,
  ) {
    return (this as HTMLElement).classList.contains("table-scroll") ? 480 : 64;
  });
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (
    this: HTMLElement,
  ) {
    const height = (this as HTMLElement).classList.contains("table-scroll") ? 480 : 64;
    return {
      x: 0,
      y: 0,
      top: 0,
      left: 0,
      right: 800,
      bottom: height,
      width: 800,
      height,
      toJSON() {},
    };
  });
});
afterEach(() => vi.restoreAllMocks());

describe("long table navigation", () => {
  it("bounds DOM rows and reaches the last of 10000 rows by scrolling", async () => {
    render(<Table />);
    const scroll = screen.getByTestId("scroll");
    expect(await screen.findByRole("button", { name: "Row 0" })).toBeInTheDocument();
    expect(screen.getAllByRole("button").length).toBeLessThan(40);
    expect(screen.getByRole("table")).toHaveAttribute("aria-rowcount", "10001");
    expect(scroll).toHaveAttribute("tabindex", "0");
    scroll.scrollTop = 640000 - 480;
    fireEvent.scroll(scroll);
    expect(await screen.findByRole("button", { name: "Row 9999" })).toBeInTheDocument();
    expect(screen.getAllByRole("button").length).toBeLessThan(40);
    expect(screen.getByRole("button", { name: "Row 9999" }).closest("tr")).toHaveAttribute(
      "aria-rowindex",
      "10001",
    );
  });

  it("retains a focused row across scrolling and handles a shorter filtered result", async () => {
    const view = render(<Table />);
    const first = await screen.findByRole("button", { name: "Row 0" });
    first.focus();
    const scroll = screen.getByTestId("scroll");
    scroll.scrollTop = 320000;
    fireEvent.scroll(scroll);
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: "Row 5000" })).toBeInTheDocument(),
    );
    expect(first).toHaveFocus();
    expect(first).toBeInTheDocument();
    view.rerender(<Table data={[...rows.slice(1), rows[0]!]} />);
    expect(first).toBeInTheDocument();
    expect(first).toHaveFocus();
    view.rerender(<Table data={rows.slice(0, 3)} />);
    expect(screen.getAllByRole("button")).toHaveLength(3);
    expect(scroll).not.toHaveClass("table-windowed");
    expect(screen.getByRole("table")).toHaveAttribute("aria-rowcount", "4");
  });
});

it("keeps short, horizontally scrollable tables reachable from the keyboard", () => {
  render(<Table data={rows.slice(0, 2)} />);
  expect(screen.getByTestId("scroll")).toHaveAttribute("tabindex", "0");
  expect(screen.getAllByRole("button")).toHaveLength(2);
});
