import { render, screen, fireEvent } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createSignal } from "solid-js";
import { Copy } from "./primitives";
let descriptor: PropertyDescriptor | undefined;
const writeText = vi.fn();
beforeEach(() => {
  vi.useFakeTimers();
  descriptor = Object.getOwnPropertyDescriptor(navigator, "clipboard");
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: { writeText },
  });
  writeText.mockReset();
});
afterEach(() => {
  if (descriptor) Object.defineProperty(navigator, "clipboard", descriptor);
  else Reflect.deleteProperty(navigator, "clipboard");
  vi.useRealTimers();
});
it("copies the exact signature and shows a check for one second after success", async () => {
  writeText.mockResolvedValue(undefined);
  render(() => (
    <Copy value="exact-signature" label="Copy signature" iconOnly />
  ));
  const button = screen.getByRole("button", { name: "Copy signature" });
  expect(button.textContent).toBe("");
  await fireEvent.click(button);
  expect(writeText).toHaveBeenCalledWith("exact-signature");
  expect(screen.getByRole("button", { name: "Signature copied" })).toBe(button);
  vi.advanceTimersByTime(999);
  expect(screen.getByRole("button", { name: "Signature copied" })).toBe(button);
  vi.advanceTimersByTime(1);
  expect(screen.getByRole("button", { name: "Copy signature" })).toBe(button);
});
it("does not confirm a rejected clipboard write", async () => {
  writeText.mockRejectedValue(new Error("Permission denied"));
  render(() => <Copy value="signature" label="Copy signature" iconOnly />);
  await fireEvent.click(screen.getByRole("button", { name: "Copy signature" }));
  expect(screen.queryByRole("button", { name: "Signature copied" })).toBeNull();
  expect(screen.getByRole("alert").textContent).toContain(
    "Select and copy manually",
  );
});
it("restarts the one-second confirmation and clears the timer on unmount", async () => {
  writeText.mockResolvedValue(undefined);
  const view = render(() => (
    <Copy value="signature" label="Copy signature" iconOnly />
  ));
  await fireEvent.click(screen.getByRole("button", { name: "Copy signature" }));
  vi.advanceTimersByTime(800);
  await fireEvent.click(
    screen.getByRole("button", { name: "Signature copied" }),
  );
  vi.advanceTimersByTime(999);
  expect(screen.getByRole("button", { name: "Signature copied" })).toBeTruthy();
  view.unmount();
  expect(vi.getTimerCount()).toBe(0);
});

it("keeps confirmation during live updates of the same signature", async () => {
  writeText.mockResolvedValue(undefined);
  const [record, setRecord] = createSignal({
    signature: "same-signature",
    revision: 1,
  });
  render(() => (
    <Copy value={record().signature} label="Copy signature" iconOnly />
  ));
  await fireEvent.click(screen.getByRole("button", { name: "Copy signature" }));
  vi.advanceTimersByTime(500);
  setRecord({ signature: "same-signature", revision: 2 });
  expect(screen.getByRole("button", { name: "Signature copied" })).toBeTruthy();
  vi.advanceTimersByTime(499);
  expect(screen.getByRole("button", { name: "Signature copied" })).toBeTruthy();
  vi.advanceTimersByTime(1);
  expect(screen.getByRole("button", { name: "Copy signature" })).toBeTruthy();
});
