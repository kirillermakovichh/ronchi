// @vitest-environment jsdom
import { it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Controls } from "./Controls";
import { Api, type Status } from "./api";
it("retained controls expose read-only state and never offer a mutation", async () => {
  const status: Status = {
    lab_id: "L1",
    name: "old",
    state: "stopped",
    health: "healthy",
    journal_state: "finalized",
    rpc_address: null,
    active_case: null,
    configuration: { mode: "simulator" },
    snapshot_seq: 9,
  };
  render(
    <Controls
      api={new Api("L1")}
      status={status}
      snapshot={9}
      changed={() => {
        throw new Error("Unexpected mutation");
      }}
    />,
  );
  await userEvent.click(screen.getByText(/Case controls/));
  expect(
    (screen.getByRole("button", { name: "Disarm Case" }) as HTMLButtonElement)
      .disabled,
  ).toBe(true);
  await userEvent.click(screen.getByText("Application Checks"));
  expect(screen.getByText(/No configured Check source/)).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Collect observations" })).toBe(
    null,
  );
  expect(screen.queryByLabelText("Explicit Check evaluation")).toBe(null);
});
