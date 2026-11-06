import { render, screen, waitFor } from "@solidjs/testing-library";
import userEvent from "@testing-library/user-event";
import { createSignal } from "solid-js";
import { expect, it, vi } from "vitest";
import { Cases } from "./Cases";
import { Lab, type LabContext } from "../../app/context";
import { Api, type Status } from "../../shared/transport/client";
it("loads the catalog, acknowledges explicit activation and preserves a Custom draft", async () => {
  const api = new Api("L1"),
    call = vi.spyOn(api, "call").mockImplementation(async (path) =>
      path === "api/cases"
        ? { result: [{ case_id: "tx-response-lost", modes: ["simulator"] }] }
        : ({
            result: { policy: { activated_at: "2026-11-06T00:00:00Z" } },
          } as any),
    );
  const [active, setActive] = createSignal(true);
  const status: Status = {
    lab_id: "L1",
    name: "default",
    state: "ready",
    health: "healthy",
    journal_state: "open",
    rpc_address: "127.0.0.1:1",
    active_case: null,
    configuration: { mode: "simulator" },
    snapshot_seq: 1,
  };
  const context = {
    api,
    status: () => status,
    authorized: () => true,
  } as unknown as LabContext;
  render(() => (
    <Lab.Provider value={context}>
      <Cases active={active} />
    </Lab.Provider>
  ));
  await waitFor(() =>
    expect(
      screen.getByRole("button", { name: "Arm Case" }).hasAttribute("disabled"),
    ).toBe(false),
  );
  expect(
    (screen.getByRole("combobox", { name: "Case" }) as HTMLSelectElement).value,
  ).toBe("tx-response-lost");
  await userEvent.click(screen.getByRole("button", { name: "Arm Case" }));
  await screen.findByText(/Case enabled\. Activated/);
  expect(
    call.mock.calls.filter(([path]) => path === "api/case/arm"),
  ).toHaveLength(1);
  await userEvent.click(screen.getByText("Custom Case", { exact: true }));
  await userEvent.type(screen.getByLabelText("Custom Case YAML"), "draft");
  setActive(false);
  setActive(true);
  expect(
    (screen.getByLabelText("Custom Case YAML") as HTMLTextAreaElement).value,
  ).toBe("draft");
});
it("keeps Custom validation available when catalog loading fails", async () => {
  const api = new Api("L1");
  vi.spyOn(api, "call").mockImplementation(async (path) => {
    if (path === "api/cases") throw new Error("Catalog unavailable");
    return { result: { valid: true } } as any;
  });
  const context = {
    api,
    authorized: () => true,
    status: () => ({
      state: "ready",
      configuration: { mode: "simulator" },
      active_case: null,
    }),
  } as unknown as LabContext;
  render(() => (
    <Lab.Provider value={context}>
      <Cases active={() => true} />
    </Lab.Provider>
  ));
  await screen.findByText(/Catalog unavailable/);
  await userEvent.click(screen.getByText("Custom Case", { exact: true }));
  await userEvent.type(screen.getByLabelText("Custom Case YAML"), "yaml");
  await userEvent.click(
    screen.getByRole("button", { name: "Validate Custom Case" }),
  );
  await screen.findByText("Resolved Custom Case plan");
  expect(
    (
      screen.getByRole("button", {
        name: "Activate Custom Case",
      }) as HTMLButtonElement
    ).disabled,
  ).toBe(false);
});
