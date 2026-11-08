import { render, screen } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createSignal } from "solid-js";
import { RpcRates } from "./RpcRates";
import type { RpcTraffic } from "../../shared/transport/client";
const traffic: RpcTraffic = {
  requests: 10,
  first_elapsed_ns: 0,
  last_rpc_elapsed_ns: 1e9,
  active_start_elapsed_ns: 0,
  active_completed_ns: 0,
  observed_elapsed_ns: 1e9,
  running: true,
  end_known: false,
};
beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  vi.restoreAllMocks();
  vi.useRealTimers();
});
it("updates locally during idle, resynchronizes on push and clears timers", () => {
  let clock = 0;
  vi.spyOn(performance, "now").mockImplementation(() => clock);
  const [value, setValue] = createSignal(traffic);
  const [unavailable, setUnavailable] = createSignal(false);
  const view = render(() => (
    <RpcRates traffic={value()} unavailable={unavailable()} />
  ));
  expect(screen.getByTestId("rps-avg").textContent).toBe("10.0");
  expect(screen.getByTestId("rpc-calls").textContent).toBe("10");
  clock = 1000;
  vi.advanceTimersByTime(1000);
  expect(screen.getByTestId("rps-avg").textContent).toBe("5.0");
  expect(screen.getByTestId("rps-active").textContent).toBe("10.0");
  setValue({ ...traffic, requests: 20, observed_elapsed_ns: 2e9 });
  expect(screen.getByTestId("rpc-calls").textContent).toBe("20");
  expect(screen.getByTestId("rps-avg").textContent).toBe("10.0");
  setUnavailable(true);
  expect(screen.getByTestId("rpc-calls").textContent).toBe("—");
  expect(screen.getByTestId("rps-avg").textContent).toBe("—");
  expect(vi.getTimerCount()).toBe(0);
  setUnavailable(false);
  expect(vi.getTimerCount()).toBe(1);
  view.unmount();
  expect(vi.getTimerCount()).toBe(0);
});
it("labels interrupted bounds and creates no timer for retained or empty history", () => {
  const [value, setValue] = createSignal({ ...traffic, running: false });
  render(() => <RpcRates traffic={value()} />);
  expect(screen.getByText("last recorded").getAttribute("title")).toContain(
    "shutdown time is unknown",
  );
  expect(vi.getTimerCount()).toBe(0);
  setValue({ ...traffic, requests: 0 });
  expect(screen.getByTestId("rpc-calls").textContent).toBe("0");
  expect(screen.getByTestId("rps-avg").textContent).toBe("0.0");
  expect(vi.getTimerCount()).toBe(0);
});

it("distinguishes positive sub-tenth rates from zero and unavailable counters", () => {
  const [value, setValue] = createSignal<RpcTraffic>({
    ...traffic,
    requests: 1,
    last_rpc_elapsed_ns: 30e9,
    observed_elapsed_ns: 100e9,
    running: false,
    end_known: true,
  });
  render(() => <RpcRates traffic={value()} />);
  expect(screen.getByTestId("rps-avg").textContent).toBe("<0.1");
  expect(screen.getByTestId("rps-active").textContent).toBe("<0.1");
  setValue({
    ...value(),
    last_rpc_elapsed_ns: 10e9,
    observed_elapsed_ns: 10e9,
  });
  expect(screen.getByTestId("rps-avg").textContent).toBe("0.1");
  setValue({ ...value(), requests: 0 });
  expect(screen.getByTestId("rps-avg").textContent).toBe("0.0");
  setValue({ ...value(), requests: 1, first_elapsed_ns: null });
  expect(screen.getByTestId("rps-avg").textContent).toBe("—");
});

it("switches to the sub-tenth label during idle without receiving new counters", () => {
  let clock = 0;
  vi.spyOn(performance, "now").mockImplementation(() => clock);
  render(() => <RpcRates traffic={traffic} />);
  clock = 100000;
  vi.advanceTimersByTime(100000);
  expect(screen.getByTestId("rps-avg").textContent).toBe("<0.1");
  expect(screen.getByTestId("rps-active").textContent).toBe("10.0");
});

it("displays exact large RPC totals with space-separated thousands", () => {
  render(() => (
    <RpcRates
      traffic={{ ...traffic, requests: "9007199254740993", running: false }}
    />
  ));
  expect(screen.getByTestId("rpc-calls").textContent).toBe(
    "9 007 199 254 740 993",
  );
});
