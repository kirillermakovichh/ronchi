import { it, expect } from "vitest";
import { observationStory } from "./story";
import { compactObservation } from "./compact";
import { classifyReply, type LookupReply } from "./reply";
import type { Item } from "../../../shared/lib/format";
const row = (
  seq: number,
  kind: string,
  at: number,
  request = "r1",
  method = "sendTransaction",
  data: Item = {},
): Item => ({
  event_ref: { lab_id: "L1", event_seq: seq },
  kind,
  elapsed_ns: at * 1000000000,
  source: kind === "transaction.accepted" ? "surfpool" : "ronchi",
  schema_version: 1,
  execution_mode: "surfpool",
  request_id: request,
  rpc_method: method,
  selected_signature: "S",
  transaction_signatures: ["S"],
  data,
});
const f: Item = {
  lab_id: "L1",
  rule_id: "tx-response-lost.surfpool-response-withheld",
  scope: { signature: "S", accepted_request_id: "r1" },
  supporting_events: Object.entries({
    submission: 1,
    acceptance: 2,
    fault_selection: 3,
    observed_withholding: 4,
  }).map(([role, event_seq]) => ({
    role,
    event_ref: { lab_id: "L1", event_seq },
  })),
};
const rows = [
  row(1, "rpc.registered", 0),
  row(2, "transaction.accepted", 0),
  row(3, "fault.response_loss_selected", 0, "r1", "sendTransaction", {
    policy: { hold_ms: 20000 },
  }),
  row(4, "fault.withholding_started", 0),
  row(5, "rpc.registered", 15, "r2", "getSignatureStatuses"),
  row(6, "rpc.boundary", 15, "r2", "getSignatureStatuses", {
    rpc_response: "result",
    transport: "http_write_completed",
  }),
  row(7, "rpc.boundary", 20, "r1", "sendTransaction", {
    rpc_response: "no_transport_response",
  }),
];
it("shows the requested five-line timeline with outcome first, counts and distinct injected color", () => {
  const reply = classifyReply(rows[4], [rows[5]], {
    result: { value: [{ confirmationStatus: "finalized", err: null }] },
  });
  const compact = compactObservation(
    f,
    observationStory(f, rows, true, 7),
    rows,
    [reply],
  );
  expect(compact.headline).toBe(
    "Response withheld → transaction checked → finalized",
  );
  expect(compact.counts).toContain("1 submission · 1 status check");
  expect(compact.timeline.map((r) => r.time)).toEqual([
    "0 s",
    "0 s",
    "15 s",
    "15 s",
    "20 s",
  ]);
  expect(compact.timeline[1].tone).toBe("injected");
  expect(compact.timeline[3].tone).toBe("success");
  expect(compact.timeline[3].label).toBe("RPC reply: finalized · err: null");
  expect(compact.timeline[4].label).toBe(
    "Original request ended without a response",
  );
  expect(compact.timeline.every((r) => r.records.length > 0)).toBe(true);
});
it("never substitutes simulated lifecycle progress for a missing RPC reply", () => {
  const simulated = rows.map((r) => ({ ...r, execution_mode: "simulator" }));
  const compact = compactObservation(
    f,
    observationStory(f, simulated, true, 7),
    simulated,
    [],
  );
  expect(compact.headline).toContain("Result unknown");
  const reply = classifyReply(
    { ...rows[4], execution_mode: "simulator" },
    [rows[5]],
    { result: { value: [{ confirmationStatus: "confirmed", err: null }] } },
  );
  expect(
    compactObservation(f, observationStory(f, rows, true, 7), rows, [reply])
      .headline,
  ).toContain("simulated confirmed");
});
it("keeps the latest status observation when a later getTransaction does not specify commitment", () => {
  const status = classifyReply(rows[4], [rows[5]], {
    result: { value: [{ confirmationStatus: "finalized", err: null }] },
  });
  const request = row(8, "rpc.registered", 21, "r3", "getTransaction");
  const transaction = classifyReply(
    request,
    [row(9, "rpc.boundary", 21, "r3", "getTransaction")],
    { result: { meta: { err: null } } },
  );
  expect(
    compactObservation(f, observationStory(f, rows, true, 9), rows, [
      status,
      transaction,
    ]).result?.status,
  ).toBe("finalized");
  const failed = {
    ...transaction,
    tone: "failure",
    label: "Execution error",
    err: "error",
  } as LookupReply;
  expect(
    compactObservation(f, observationStory(f, rows, true, 9), rows, [
      status,
      failed,
    ]).headline,
  ).toContain("execution error");
  const missing = classifyReply(
    {
      ...rows[4],
      request_id: "r4",
      event_ref: { lab_id: "L1", event_seq: 10 },
    },
    [],
    { result: { value: [null] } },
  );
  expect(
    compactObservation(f, observationStory(f, rows, true, 10), rows, [
      status,
      missing,
    ]).headline,
  ).toContain("Result unknown");
});

it("shows a time range for grouped lookups and keeps a Fault's actual time", () => {
  const first = classifyReply(rows[4], [rows[5]], {
    result: { value: [{ confirmationStatus: "finalized", err: null }] },
  });
  const laterRequest = row(
    10,
    "rpc.registered",
    100,
    "r3",
    "getSignatureStatuses",
  );
  const laterReply = classifyReply(
    laterRequest,
    [row(11, "rpc.boundary", 101, "r3", "getSignatureStatuses")],
    { result: { value: [{ confirmationStatus: "finalized", err: null }] } },
  );
  // No original boundary between the two requests in this history.
  const history = rows.filter(
    (r) => r.request_id !== "r1" || r.kind !== "rpc.boundary",
  );
  const view = compactObservation(
    f,
    observationStory(f, history, true, 11),
    history,
    [first, laterReply],
  );
  expect(
    view.timeline.find((r) => r.label === "Application checked status ×2")
      ?.time,
  ).toBe("15–100 s");
  const fault = row(
    3,
    "fault.blockhash_returned",
    5,
    "r1",
    "getLatestBlockhash",
    { expiry_observed_elapsed_ns: 1000 },
  );
  const expiryFinding = {
    ...f,
    rule_id: "recent-blockhash.expired-submission",
  };
  const expiryStory = {
    ...observationStory(expiryFinding, history, true, 11),
    steps: [{ title: "Validity observed", records: [rows[0], fault], at: 1n }],
  };
  const expiry = compactObservation(
    expiryFinding,
    expiryStory,
    [rows[0], fault],
    [],
  );
  expect(expiry.timeline.find((r) => r.at === 1n)?.tone).toBe("neutral");
  expect(
    expiry.timeline.find(
      (r) => r.label === "Ronchi returned an expired blockhash",
    )?.time,
  ).toBe("5 s");
});

it("keeps a repeat submission visible after an earlier status lookup", () => {
  const repeat = row(8, "rpc.registered", 21, "r3", "sendTransaction");
  const history = [...rows, repeat];
  const reply = classifyReply(rows[4], [rows[5]], {
    result: { value: [{ confirmationStatus: "finalized", err: null }] },
  });
  const view = compactObservation(
    { ...f, rule_id: "tx-response-lost.submission-after-withholding" },
    observationStory(f, history, true, 8),
    history,
    [reply],
  );
  expect(view.counts).toContain("2 submissions");
  expect(view.timeline.find((r) => r.id === "activity:r3")?.label).toBe(
    "Application submitted the transaction again",
  );
  expect(view.timeline.find((r) => r.id === "activity:r3")?.time).toBe("21 s");
});
