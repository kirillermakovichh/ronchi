// @vitest-environment jsdom
import { it, expect, afterEach } from "vitest";
import { render, screen, cleanup } from "@testing-library/react";
import { StateNotice } from "./StateNotice";
import type { Status } from "./api";
afterEach(cleanup);
it("shows a concrete no-selection startup action without claiming capture", () => {
  render(<StateNotice labId="" />);
  expect(screen.getByRole("status").textContent).toContain("ronchi up");
});
it("shows starting from a controlled status DTO without changing lifecycle", () => {
  render(<StateNotice labId="L1" status={{ state: "starting" } as Status} />);
  expect(screen.getByText("Lab starting")).toBeTruthy();
});
it("labels bounded analysis as incomplete rather than a resilience pass", () => {
  render(
    <StateNotice labId="L1" analysis={{ findings: "operational_limit" }} />,
  );
  expect(screen.getByRole("alert").textContent).toContain("incomplete");
});
it("separates unsupported capability from injected Faults", () => {
  render(
    <StateNotice
      labId="L1"
      error="unsupported_capability: source not implemented"
    />,
  );
  expect(screen.getByRole("alert").textContent).toContain(
    "Unsupported capability",
  );
});
it("labels a core failure separately from an injected Fault", () => {
  render(<StateNotice labId="L1" status={{ health: "failed" } as Status} />);
  expect(screen.getByRole("alert").textContent).toContain(
    "Ronchi core failure",
  );
});
