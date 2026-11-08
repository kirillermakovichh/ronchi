import { createEffect, createMemo, createSignal, onCleanup } from "solid-js";
import type { RpcTraffic } from "../../shared/transport/client";
import { rpcRates } from "./rates";
import { Tooltip } from "../../shared/ui/Tooltip";
import "./rates.css";

function formatRate(rate: number | undefined) {
  return rate !== undefined && rate > 0 && rate < 0.1
    ? "<0.1"
    : (rate?.toFixed(1) ?? "—");
}

export function RpcRates(props: {
  traffic?: RpcTraffic;
  unavailable?: boolean;
}) {
  const [age, setAge] = createSignal(0);
  createEffect(() => {
    const traffic = props.traffic;
    const unavailable = props.unavailable;
    setAge(0);
    if (!traffic?.running || unavailable || BigInt(traffic.requests) === 0n)
      return;
    const received = performance.now();
    // A local display clock. This never requests data or sends socket frames.
    const timer = setInterval(() => setAge(performance.now() - received), 1000);
    onCleanup(() => clearInterval(timer));
  });
  const rates = createMemo(() =>
    props.traffic && !props.unavailable
      ? rpcRates(props.traffic, age())
      : undefined,
  );
  const boundary = () =>
    props.unavailable
      ? "Connection unavailable; waiting for fresh Lab counters."
      : !props.traffic
        ? "RPC counters unavailable."
        : props.traffic.running
          ? "Whole Lab, up to the current time."
          : props.traffic.end_known
            ? "Whole Lab, fixed at Lab shutdown."
            : "Partial observation: fixed at the last recorded event; shutdown time is unknown.";
  return (
    <div class="rpc-rates" aria-label="Lab RPC rates">
      <Tooltip content="All recorded RPC calls since this Lab started, including errors and withheld responses. Independent of Traffic filters.">
        <span class="rpc-rate">
          <span class="caption">RPC calls</span>
          <strong data-testid="rpc-calls">
            {props.traffic && !props.unavailable
              ? BigInt(props.traffic.requests)
                  .toString()
                  .replace(/\B(?=(\d{3})+(?!\d))/g, " ")
              : "—"}
          </strong>
        </span>
      </Tooltip>
      <Tooltip content="Average RPC requests per second from the first call until now or Lab stop. Includes idle time.">
        <span class="rpc-rate">
          <span class="caption">RPS avg</span>
          <strong data-testid="rps-avg">{formatRate(rates()?.avg)}</strong>
        </span>
      </Tooltip>
      <Tooltip content="Average RPC requests per second during active intervals. Excludes gaps of 10 seconds or more.">
        <span class="rpc-rate">
          <span class="caption">RPS active</span>
          <strong data-testid="rps-active">
            {formatRate(rates()?.active)}
          </strong>
        </span>
      </Tooltip>
      {!props.unavailable &&
      props.traffic &&
      !props.traffic.running &&
      !props.traffic.end_known ? (
        <span class="caption" title={boundary()}>
          last recorded
        </span>
      ) : null}
    </div>
  );
}
