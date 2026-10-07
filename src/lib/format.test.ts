import { describe, expect, it } from "vitest";
import { compareDecimal } from "../components/AssetTable";
import { DASH, formatPercent, formatQuantity, formatUsd, signOf } from "./format";

describe("formatting", () => {
  it("renders missing values as a dash, never $0", () => {
    expect(formatUsd(null, "en-US")).toBe(DASH);
    expect(formatPercent(undefined, "en-US")).toBe(DASH);
  });

  it("keeps exact digits beyond JavaScript's safe integer range", () => {
    expect(formatQuantity("9007199254740993", "en-US")).toBe("9,007,199,254,740,993");
    expect(formatUsd("90071992547409.935", "en-US")).toBe("$90,071,992,547,409.94");
  });

  it("rounds USD half-even for display", () => {
    expect(formatUsd("0.125", "en-US")).toBe("$0.12");
    expect(formatUsd("0.135", "en-US")).toBe("$0.14");
  });

  it("marks tiny non-zero balances instead of showing zero", () => {
    expect(formatQuantity("0.000000001", "en-US")).toBe("<0.00000001");
    expect(formatQuantity("0", "en-US")).toBe("0");
    expect(formatQuantity("0.1", "en-US", 0)).toBe("<1");
    expect(formatQuantity("0", "en-US", 0)).toBe("0");
  });

  it("shows signs on percentages and localizes them", () => {
    expect(formatPercent("5", "en-US")).toBe("+5.00%");
    expect(formatPercent("-20", "en-US")).toBe("-20.00%");
    expect(formatPercent("1.5", "ru-RU")).toBe("+1,50 %");
    expect(signOf("-0.1")).toBe(-1);
    expect(signOf("0.00")).toBe(0);
    expect(signOf("-0.00")).toBe(0);
  });
});

describe("compareDecimal", () => {
  it("orders exact decimal strings without float conversion", () => {
    const values = ["10", "-2.5", "0.0001", "9007199254740993", "9007199254740992.5", "-10"];
    expect([...values].sort(compareDecimal)).toEqual([
      "-10",
      "-2.5",
      "0.0001",
      "10",
      "9007199254740992.5",
      "9007199254740993",
    ]);
    expect(compareDecimal("1.50", "1.5")).toBe(0);
  });
});
