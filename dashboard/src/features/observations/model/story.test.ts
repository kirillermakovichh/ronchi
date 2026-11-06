import { expect, it } from "vitest";
import { observationStory, mergeRecords } from "./story";
import type { Item } from "../../../shared/lib/format";
const record = (
  seq: number,
  kind: string,
  request = "r1",
  method = "sendTransaction",
  elapsed = 0,
  data: Item = {},
  source = "ronchi",
): Item => ({
  event_ref: { lab_id: "L1", event_seq: seq },
  schema_version: 1,
  kind,
  source,
  request_id: request,
  rpc_method: method,
  elapsed_ns: String(BigInt(elapsed) * 1000000n),
  data,
  execution_mode: "surfpool",
});
const finding = (
  rule: string,
  support: Record<string, number>,
  scope: Item = { signature: "S", accepted_request_id: "r1" },
): Item => ({
  finding_id: "F1",
  lab_id: "L1",
  rule_id: rule,
  scope,
  supporting_events: Object.entries(support).map(([role, event_seq]) => ({
    role,
    event_ref: { lab_id: "L1", event_seq },
  })),
});
const base = finding("tx-response-lost.surfpool-response-withheld", {
  submission: 548,
  acceptance: 554,
  fault_selection: 555,
  observed_withholding: 556,
});
const example = [
  record(548, "rpc.registered"),
  record(549, "transaction.signed_attempt"),
  record(
    551,
    "rpc.backend",
    "r1",
    "sendTransaction",
    200,
    { source_outcome: "result" },
    "surfpool",
  ),
  record(
    554,
    "transaction.accepted",
    "r1",
    "sendTransaction",
    215,
    {},
    "surfpool",
  ),
  record(555, "fault.response_loss_selected", "r1", "sendTransaction", 215, {
    policy: { case_id: "tx-response-lost", hold_ms: 20000 },
  }),
  record(556, "fault.withholding_started", "r1", "sendTransaction", 230),
  record(557, "rpc.response_prepared"),
  record(559, "rpc.registered", "r2", "getSignatureStatuses", 15000),
  record(562, "rpc.response_prepared", "r2", "getSignatureStatuses"),
  record(563, "rpc.boundary", "r2", "getSignatureStatuses", 15050, {
    rpc_response: "result",
    transport: "http_write_completed",
  }),
  record(589, "rpc.registered", "r3", "getSignatureStatuses", 15500),
  record(594, "rpc.registered", "r4", "getTransaction", 15600),
  record(629, "rpc.boundary", "r1", "sendTransaction", 20230, {
    rpc_response: "no_transport_response",
    transport: "withholding_deadline",
  }),
];
it("explains the supplied five-step pattern without counting transport stages as application actions", () => {
  const story = observationStory(base, example, true, 629);
  expect(story.counts).toEqual({
    submissions: 1,
    statuses: 2,
    transactions: 1,
    other: 0,
  });
  expect(story.steps.map((s) => s.title)).toEqual([
    "Surfpool returned the transaction signature.",
    "Ronchi withheld the response with a 20 s limit.",
    "A status check for the same signature arrived 15 s after the original request.",
    "Later activity: 1 status check, 1 transaction lookup.",
    "The original HTTP request ended without a transport response.",
  ]);
  expect(story.outcome).toContain("No repeat submission");
  expect(story.unknown).toContain("require separate application evidence");
  for (const step of story.steps)
    expect(step.records.length).toBeGreaterThan(0);
  expect(story.steps.at(-1)?.records[0].event_ref).toEqual({
    lab_id: "L1",
    event_seq: 629,
  });
});
it.each([
  "tx-response-lost.lookup-after-withholding",
  "tx-response-lost.submission-after-withholding",
])("explains %s with actual observed requests", (rule) => {
  const rows = [
    ...example,
    record(620, "rpc.registered", "r5", "sendTransaction", 17000),
  ];
  const story = observationStory({ ...base, rule_id: rule }, rows, true, 629);
  expect(story.counts.submissions).toBe(2);
  expect(story.after.submissions).toBe(1);
  expect(story.outcome).not.toContain("No repeat submission");
});
it.each([false, true])(
  "explains hidden/stale results with evidence, stale=%s",
  (stale) => {
    const rows = [
      record(
        1,
        "transaction.accepted",
        "r1",
        "sendTransaction",
        0,
        {},
        "surfpool",
      ),
      record(2, "rpc.lookup_source", "r2", "getSignatureStatuses", 0, {
        source_available: true,
      }),
      record(3, "fault.lookup_changed", "r2", "getSignatureStatuses", 0, {
        returned_view: stale ? { confirmationStatus: "processed" } : null,
      }),
      record(4, "rpc.response_prepared", "r2", "getSignatureStatuses"),
      record(5, "rpc.boundary", "r2", "getSignatureStatuses", 0, {
        rpc_response: "result",
        transport: "http_write_completed",
      }),
    ];
    const story = observationStory(
      finding("rpc.lookup-view.changed", {
        acceptance: 1,
        authoritative_source: 2,
        selected_lookup_change: 3,
        prepared_lookup: 4,
        rpc_boundary: 5,
      }),
      rows,
      true,
      5,
    );
    expect(
      story.steps.some((s) =>
        s.title.includes(stale ? "earlier transaction" : "prepared null"),
      ),
    ).toBe(true);
    expect(
      story.steps.some((s) =>
        s.records.some((r) => r.event_ref === rows[2].event_ref),
      ),
    ).toBe(true);
  },
);
it("explains simulator recovery using simulated time instead of real elapsed time", () => {
  const rows = [
    record(
      1,
      "transaction.accepted",
      "r1",
      "sendTransaction",
      1,
      {},
      "rpc_simulator",
    ),
    record(2, "fault.confirmation_schedule_assigned"),
    record(3, "transaction.transition", "r1", "sendTransaction", 300, {
      stage: "processed",
      scheduled_transition_ns: 2000000000,
    }),
    record(4, "transaction.transition", "r1", "sendTransaction", 400, {
      stage: "confirmed",
      scheduled_transition_ns: 7000000000,
    }),
  ];
  const story = observationStory(
    finding("sim.confirmation-delay.observed", {
      acceptance: 1,
      schedule_assignment: 2,
      held_commitment: 3,
      recovery: 4,
    }),
    rows,
    true,
    4,
  );
  expect(
    story.steps.find((s) => s.title.includes("later recorded"))?.detail,
  ).toContain("5 s of simulated time");
  expect(story.unknown).toContain("does not establish program execution");
});
it("explains pre-dispatch withholding and a later RPC attempt without inventing a retry", () => {
  const rows = [
    record(1, "rpc.registered", "r1", "getHealth"),
    record(2, "rpc.dispatch_prevented", "r1", "getHealth"),
    record(3, "fault.withholding_started", "r1", "getHealth", 0, {
      hold_ms: 20000,
    }),
    record(4, "rpc.boundary", "r1", "getHealth", 20000, {
      rpc_response: "no_transport_response",
    }),
    record(5, "rpc.registered", "r2", "getHealth", 21000),
  ];
  const story = observationStory(
    finding(
      "provider-timeout-burst.later-rpc-attempt",
      {
        dispatch_prevented: 2,
        observed_withholding: 3,
        transport_boundary: 4,
        later_rpc_attempt: 5,
      },
      { request_id: "r1", rpc_method: "getHealth" },
    ),
    rows,
    true,
    5,
  );
  expect(story.counts.other).toBe(2);
  expect(story.steps[0].title).toContain("prevented");
  expect(
    story.steps.some((s) => s.title.includes("Another getHealth request")),
  ).toBe(true);
  expect(story.unknown).toContain("Client timeout");
});
it.each(["surfpool", "simulator"])(
  "explains expired blockhash evidence in %s without claiming non-execution",
  (mode) => {
    const rows = [
      record(1, "rpc.blockhash_source"),
      record(2, "rpc.blockhash_source"),
      record(3, "transaction.signed_attempt"),
      record(4, "rpc.admission_result", "r1", "sendTransaction", 0, {
        admission: "error",
      }),
      record(
        5,
        mode === "simulator"
          ? "transaction.processing_prevented"
          : "rpc.backend",
      ),
    ];
    const story = observationStory(
      finding("recent-blockhash.expired-submission", {
        original_tuple: 1,
        expiry_height: 2,
        signed_submission: 3,
        admission_result: 4,
        processing_or_status: 5,
      }),
      rows,
      true,
      5,
    );
    expect(story.steps.some((s) => s.title.includes("validity"))).toBe(true);
    expect(
      story.steps.some((s) => s.title.includes("beyond its observed validity")),
    ).toBe(true);
    expect(story.unknown).toContain("does not prove non-execution");
  },
);
it("explains Custom lookup rules by their supported template and leaves unknown rules explicit", () => {
  const f = finding(
    "author.my-rule",
    { authoritative_lookup: 1, lookup_hidden: 2, later_lookup_received: 3 },
    { signature: "S", request_id: "r1" },
  );
  const rows = [
    record(1, "rpc.lookup_source"),
    record(2, "fault.custom_lookup_changed", "r1", "getTransaction", 0, {
      returned_available: false,
    }),
    record(3, "rpc.registered", "r2", "getTransaction"),
  ];
  const story = observationStory(f, rows, true, 3);
  expect(story.steps.some((s) => s.title.includes("prepared null"))).toBe(true);
  expect(story.unknown).toContain("Custom rule");
  const unknown = finding("future.rule", { evidence: 1 });
  expect(
    observationStory(unknown, [record(1, "future.fact")], false, 1).steps[0]
      .title,
  ).toContain("supporting records");
});
it("deduplicates request counts, pins references and never concludes absence from partial or missing records", () => {
  const rows = [
    ...example,
    example[0],
    record(700, "rpc.registered", "later"),
    {
      ...record(5, "rpc.registered", "foreign"),
      event_ref: { lab_id: "other", event_seq: 5 },
    },
  ];
  const story = observationStory(base, rows, false, 629);
  expect(story.counts.submissions).toBe(1);
  expect(story.outcome).not.toContain("No repeat submission");
  expect(story.outcome).toContain("lower bounds");
  expect(mergeRecords(example, example).length).toBe(example.length);
  const missing = observationStory(
    base,
    example.filter((r) => r.kind !== "rpc.boundary"),
    false,
    629,
  );
  expect(missing.steps.some((s) => s.title.includes("ended without"))).toBe(
    false,
  );
  const early = observationStory(base, example, true, 556);
  expect(early.counts.statuses).toBe(0);
  expect(early.steps.some((s) => s.title.includes("15 s"))).toBe(false);
});
it("keeps nanosecond arithmetic exact above JavaScript's safe integer range", () => {
  const offset = 9007199254740993000n;
  const rows = example.map((r) => ({
    ...r,
    elapsed_ns: String(BigInt(String(r.elapsed_ns)) + offset),
  }));
  expect(observationStory(base, rows, true, 629).steps[2].title).toContain(
    "15 s",
  );
});
