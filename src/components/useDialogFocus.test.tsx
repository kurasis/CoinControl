import { useRef } from "react";
import { expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useDialogFocus } from "./useDialogFocus";

function Panel({ onClose }: { onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  useDialogFocus(ref, onClose);
  return (
    <div role="dialog" ref={ref} tabIndex={-1}>
      <button onClick={onClose}>Close</button>
      <button disabled>Unavailable</button>
      <input aria-label="Last field" />
    </div>
  );
}

it("wraps Tab in both directions, keeps focus on rerender, uses the latest callback and restores its opener", async () => {
  const user = userEvent.setup();
  const opener = document.createElement("button");
  document.body.append(opener);
  opener.focus();
  const initial = vi.fn();
  const latest = vi.fn();
  const view = render(<Panel onClose={initial} />);
  const close = screen.getByRole("button", { name: "Close" });
  const last = screen.getByRole("textbox");
  expect(close).toHaveFocus();
  await user.tab({ shift: true });
  expect(last).toHaveFocus();
  view.rerender(<Panel onClose={latest} />);
  expect(last).toHaveFocus();
  await user.tab();
  expect(close).toHaveFocus();
  await user.keyboard("{Escape}");
  expect(initial).not.toHaveBeenCalled();
  expect(latest).toHaveBeenCalledOnce();
  view.unmount();
  expect(opener).toHaveFocus();
  opener.remove();
});
