import { expect, it } from "vitest";
import type { RpcTraffic } from "../../shared/transport/client";
import { rpcRates } from "./rates";
const traffic: RpcTraffic = {
  requests: 300,
  first_elapsed_ns: 5e9,
  last_rpc_elapsed_ns: 35e9,
  active_start_elapsed_ns: 5e9,
  active_completed_ns: 0,
  observed_elapsed_ns: 75e9,
  end_known: false,
  running: true,
};
it("includes idle time in avg and excludes the idle tail from active", () => {
  expect(rpcRates(traffic)?.avg).toBeCloseTo(4.285714);
  expect(rpcRates(traffic)?.active).toBe(10);
  expect(rpcRates(traffic, 10000)?.avg).toBe(3.75);
  expect(rpcRates(traffic, 10000)?.active).toBe(10);
});
it("sums closed intervals, gives a singleton one second and freezes stopped history", () => {
  const stopped = {
    ...traffic,
    requests: 4,
    active_completed_ns: 9e9,
    first_elapsed_ns: 5e9,
    last_rpc_elapsed_ns: 24e9,
    active_start_elapsed_ns: 24e9,
    observed_elapsed_ns: 70e9,
    running: false,
    end_known: true,
  };
  expect(rpcRates(stopped, 1e6)).toEqual(rpcRates(stopped, 0));
  expect(rpcRates(stopped)?.active).toBe(0.4);
  expect(rpcRates(stopped)?.avg).toBeCloseTo(4 / 65);
});
it("handles no traffic, immediate calls and missing counters without infinity", () => {
  expect(
    rpcRates({ ...traffic, requests: "0", first_elapsed_ns: null }),
  ).toEqual({ avg: 0, active: 0 });
  expect(rpcRates({ ...traffic, first_elapsed_ns: null })).toBeUndefined();
  expect(
    rpcRates({
      ...traffic,
      requests: 1,
      first_elapsed_ns: 5e9,
      last_rpc_elapsed_ns: 5e9,
      observed_elapsed_ns: 5e9,
    }),
  ).toEqual({ avg: 1, active: 1 });
});
it("subtracts exact nanoseconds before converting to a display number", () => {
  const offset = 900719925474099300n;
  expect(
    rpcRates({
      ...traffic,
      first_elapsed_ns: offset.toString(),
      last_rpc_elapsed_ns: (offset + 30_000_000_000n).toString(),
      active_start_elapsed_ns: offset.toString(),
      observed_elapsed_ns: (offset + 70_000_000_000n).toString(),
    }),
  ).toEqual(rpcRates(traffic));
});
