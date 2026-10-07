import { expect, it } from "vitest";
import { isCommandError } from "./client";

it("recognizes command errors only when code and message are strings", () => {
  expect(isCommandError({ code: "storage", message: "Failed", detail: null })).toBe(true);
  // The browser preview's errors also use this shape without a detail field.
  expect(isCommandError({ code: "preview", message: "Desktop only" })).toBe(true);
  for (const error of [
    null,
    undefined,
    "error",
    {},
    { code: 1, message: "Failed" },
    { code: "storage", message: { secret: "untrusted response" } },
  ]) {
    expect(isCommandError(error)).toBe(false);
  }
});
