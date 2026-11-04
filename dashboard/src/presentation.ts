import type { Json } from "./api";
export type Item = Record<string, Json>;
export const object = (value: unknown): Item =>
  value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as Item)
    : {};
export const text = (value: unknown, fallback = "Not recorded") =>
  typeof value === "string" || typeof value === "number"
    ? String(value)
    : fallback;
export const short = (value: string) =>
  value.length > 24 ? `${value.slice(0, 8)}…${value.slice(-6)}` : value;
export function signature(item: Item): string | undefined {
  const direct = item.transaction_signature;
  if (typeof direct === "string") return direct;
  const signatures = item.transaction_signatures;
  if (Array.isArray(signatures) && typeof signatures[0] === "string")
    return signatures[0];
  const accepted = object(
    object(item.exchange).acceptance,
  ).transaction_signature;
  return typeof accepted === "string" ? accepted : undefined;
}
const caseCopy: Record<string, { title: string; description: string }> = {
  "tx-response-lost": {
    title: "Lost transaction response",
    description:
      "Withhold the submission response after local acceptance. Inspect how the backend reconciles the original signature.",
  },
  "provider-timeout-burst": {
    title: "RPC timeout burst",
    description:
      "Hold matching requests without dispatching them. Inspect later RPC activity.",
  },
  "signature-status-null": {
    title: "Unavailable transaction status",
    description:
      "Hide supported transaction lookup results during the configured window.",
  },
  "stale-transaction-read-after-submission": {
    title: "Stale transaction status",
    description:
      "Return an earlier supported status view while the authoritative state can continue progressing.",
  },
  "tx-confirmation-stalls": {
    title: "Stalled confirmation",
    description:
      "Delay the supported confirmation progression and inspect subsequent status queries.",
  },
  "finality-delayed": {
    title: "Delayed finality",
    description:
      "Delay the supported local finality progression and inspect subsequent queries.",
  },
  "tx-blockhash-expired": {
    title: "Expired blockhash",
    description:
      "Expose the supported expired-blockhash scenario and inspect later submissions.",
  },
};
export function caseInfo(value: unknown) {
  const entry = object(value),
    id = text(entry.case_id, "");
  return {
    id,
    title: id ? (caseCopy[id]?.title ?? id) : "No Case enabled",
    description:
      caseCopy[id]?.description ??
      (id
        ? "A continuing policy applies to matching RPC calls until disabled or replaced."
        : "Choose a Case to apply a controlled RPC fault."),
  };
}
const findingTitles: Record<string, string> = {
  "tx-response-lost.lookup-after-withholding":
    "Original transaction queried after response withholding",
  "tx-response-lost.submission-after-withholding":
    "Later submission observed after response withholding",
  "rpc.lookup-view.changed": "Transaction lookup view changed",
  "sim.confirmation-delay.observed": "Confirmation delay observed",
  "provider-timeout-burst.later-rpc-attempt":
    "Later RPC attempt observed after withholding",
  "recent-blockhash.expired-submission": "Submission used an expired blockhash",
};
export function findingTitle(item: Item): string {
  const title = findingTitles[text(item.rule_id, "")];
  if (title) return title;
  return text(item.statement, "Saved RPC observation");
}
const eventTitles: Record<string, string> = {
  "lab.starting": "Lab startup began",
  "lab.ready": "Lab became ready",
  "lab.stopping": "Lab shutdown began",
  "lab.stopped": "Lab stopped",
  "case.changed": "Case policy changed",
  "rpc.registered": "RPC request received",
  "rpc.response_prepared": "RPC response prepared",
  "rpc.boundary": "RPC boundary recorded",
  "rpc.dispatch_prevented": "Request dispatch prevented",
  "rpc.admission_result": "Transaction admission result recorded",
  "transaction.accepted": "Transaction accepted locally",
  "transaction.submission": "Transaction submission recorded",
  "transaction.transition": "Local transaction state changed",
  "transaction.processing_prevented": "Transaction processing prevented",
  "fault.response_loss_selected": "Response withholding selected",
  "fault.withholding_started": "Response withholding started",
  "fault.withholding_ended": "Response withholding ended",
  "rpc.backend_started": "Local backend exchange started",
  "rpc.backend": "Local backend result recorded",
  "rpc.upstream_started": "Read Upstream exchange started",
  "rpc.upstream": "Read Upstream result recorded",
  "check.observation": "Application observation collected",
  "check.invocation": "Application source invoked",
  "custom.selection": "Custom Case step selected",
};
export function eventTitle(item: Item): string {
  const kind = text(item.kind, "");
  if (kind === "rpc.registered") {
    const method = text(item.rpc_method, "");
    const names: Record<string, string> = {
      sendTransaction: "Transaction submission request received",
      getSignatureStatuses: "Transaction status lookup received",
      getTransaction: "Transaction lookup received",
      getLatestBlockhash: "Latest blockhash requested",
    };
    if (names[method]) return names[method];
    if (method) return `${method} request received`;
  }
  return eventTitles[kind] ?? kind.replaceAll("_", " ").replaceAll(".", " · ");
}
export function sourceLabel(value: unknown): string {
  const source = text(value, "Unknown source");
  return (
    {
      ronchi: "Ronchi",
      rpc: "RPC boundary",
      simulator: "Local simulator",
      rpc_simulator: "Local simulator",
      execution_backend: "Local execution backend",
      application: "Application source",
    }[source] ?? source
  );
}
export function boundaryResult(item: Item): { label: string; fault: boolean } {
  const exchange = object(item.exchange),
    boundary = object(object(exchange.boundary).data);
  if (boundary.rpc_response === "no_transport_response")
    return { label: "No transport response", fault: true };
  if (boundary.rpc_response === "error")
    return { label: "RPC error prepared", fault: false };
  if (boundary.rpc_response === "notification_no_response")
    return { label: "Notification · no response", fault: false };
  if (boundary.rpc_response === "result")
    return {
      label:
        boundary.transport === "http_write_completed"
          ? "Server write completed"
          : "Result prepared",
      fault: false,
    };
  if (exchange.withholding)
    return { label: "Withholding started", fault: true };
  if (exchange.dispatch) return { label: "Dispatch prevented", fault: true };
  return { label: "Boundary not recorded", fault: false };
}
export function duration(item: Item): string | undefined {
  const end = object(object(item.exchange).boundary).elapsed_ns,
    start = item.elapsed_ns;
  if (!/^\d+$/.test(String(start)) || !/^\d+$/.test(String(end))) return;
  const ns = BigInt(String(end)) - BigInt(String(start));
  if (ns < 0n) return;
  return ns < 1000000000n
    ? `${ns / 1000000n} ms`
    : `${Number(ns / 1000000n) / 1000} s`;
}
