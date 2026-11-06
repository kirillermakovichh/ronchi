import type { RpcTraffic } from "../../shared/transport/client";
const SECOND = 1_000_000_000n;
const floorSecond = (ns: bigint) => (ns > SECOND ? ns : SECOND);
/** Subtract exact integer timestamps before converting durations for display. */
export function rpcRates(traffic: RpcTraffic, sinceReceiptMs = 0) {
  if (traffic.requests === 0 || BigInt(traffic.requests) === 0n)
    return { avg: 0, active: 0 };
  if (traffic.first_elapsed_ns === null || traffic.last_rpc_elapsed_ns === null)
    return undefined;
  const extension = traffic.running
    ? BigInt(Math.floor(Math.max(0, sinceReceiptMs) * 1_000_000))
    : 0n;
  const total = floorSecond(
    BigInt(traffic.observed_elapsed_ns) +
      extension -
      BigInt(traffic.first_elapsed_ns),
  );
  const active =
    BigInt(traffic.active_completed_ns) +
    floorSecond(
      BigInt(traffic.last_rpc_elapsed_ns) -
        BigInt(traffic.active_start_elapsed_ns),
    );
  return {
    avg: (Number(traffic.requests) * 1e9) / Number(total),
    active: (Number(traffic.requests) * 1e9) / Number(active),
  };
}
