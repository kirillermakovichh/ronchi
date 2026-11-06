import { render, screen, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { expect, it, vi } from "vitest";
import { createSubscription } from "./subscription";
import { Api } from "./client";
it("keeps an unchanged subscription through unrelated selection changes and cancels a replaced scope", async () => {
  const api = new Api("L1"),
    stop = vi.fn(),
    callbacks: ((value: any) => void)[] = [];
  const subscribe = vi
    .spyOn(api, "subscribe")
    .mockImplementation((_p, _b, value) => {
      callbacks.push(value);
      return stop;
    });
  const [signature, setSignature] = createSignal("a"),
    [irrelevant, setIrrelevant] = createSignal(0);
  render(() => {
    const data = createSubscription<{ label: string }>(api, () => {
      irrelevant();
      return {
        path: "api/investigate",
        body: {
          request_id: crypto.randomUUID(),
          arguments: { scope: { signature: [signature()] } },
        },
      };
    });
    return <p>{data.value()?.label ?? "loading"}</p>;
  });
  await waitFor(() => expect(subscribe).toHaveBeenCalledTimes(1));
  callbacks[0]({ label: "first" });
  expect(screen.getByText("first")).toBeTruthy();
  setIrrelevant(1);
  expect(subscribe).toHaveBeenCalledTimes(1);
  setSignature("b");
  await waitFor(() => expect(subscribe).toHaveBeenCalledTimes(2));
  expect(stop).toHaveBeenCalledTimes(1);
  expect(screen.queryByText("first")).toBeNull();
  callbacks[1]({ label: "second" });
  expect(screen.getByText("second")).toBeTruthy();
});
