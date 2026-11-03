// @vitest-environment jsdom
import { afterEach, it, expect } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Controls } from "./Controls";
import { Api, type Status } from "./api";
afterEach(cleanup);
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

it("arms the selected maintained Case from the core catalog", async () => {
  const { vi } = await import("vitest");
  const api = new Api("L2");
  const call = vi.spyOn(api, "call")
    .mockResolvedValueOnce({ result: [
      { case_id: "tx-response-lost", modes: ["simulator"] },
      { case_id: "tx-confirmation-stalls", modes: ["simulator"] },
    ] })
    .mockResolvedValueOnce({});
  const changed = vi.fn();
  render(<Controls api={api} status={{lab_id:"L2",name:"live",state:"ready",health:"healthy",journal_state:"open",rpc_address:null,active_case:null,configuration:{mode:"simulator"},snapshot_seq:1}} changed={changed} />);
  await userEvent.click(screen.getByRole("button", {name:"List Cases"}));
  await userEvent.selectOptions(screen.getByRole("combobox", {name:"Case"}), "tx-confirmation-stalls");
  await userEvent.click(screen.getByRole("button", {name:"Arm / replace tx-confirmation-stalls"}));
  expect(call).toHaveBeenLastCalledWith("api/case/arm", {case_id:"tx-confirmation-stalls"});
  expect(changed).toHaveBeenCalledOnce();
});
