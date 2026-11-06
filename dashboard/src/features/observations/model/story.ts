import { object, text, type Item } from "../../../shared/lib/format";
import type { Sequence } from "../../../shared/protocol/wire";
export interface StoryStep {
  title: string;
  detail?: string;
  records: Item[];
  at: bigint;
}
export interface Story {
  steps: StoryStep[];
  counts: {
    submissions: number;
    statuses: number;
    transactions: number;
    other: number;
  };
  after: {
    submissions: number;
    statuses: number;
    transactions: number;
    other: number;
  };
  complete: boolean;
  outcome: string;
  unknown: string;
}
export const sequence = (item: Item) =>
  integer(object(item.event_ref).event_seq);
function integer(value: unknown): bigint {
  return /^\d+$/.test(String(value)) ? BigInt(String(value)) : -1n;
}
export function mergeRecords(...pages: Item[][]): Item[] {
  const unique = new Map<string, Item>();
  for (const page of pages)
    for (const item of page) {
      if (sequence(item) >= 0n)
        unique.set(JSON.stringify(item.event_ref), item);
    }
  return [...unique.values()].sort((a, b) =>
    sequence(a) < sequence(b) ? -1 : sequence(a) > sequence(b) ? 1 : 0,
  );
}
function interval(
  start: Item | undefined,
  end: Item | undefined,
  field = "elapsed_ns",
) {
  if (!start || !end) return;
  const a = integer(
    field === "elapsed_ns" ? start[field] : object(start.data)[field],
  );
  const b = integer(
    field === "elapsed_ns" ? end[field] : object(end.data)[field],
  );
  if (a < 0n || b < a) return;
  const ms = (b - a) / 1000000n;
  return ms < 1000n ? `${ms} ms` : `${Number((ms + 50n) / 100n) / 10} s`;
}
function tally(records: Item[]) {
  const counts = { submissions: 0, statuses: 0, transactions: 0, other: 0 };
  const seen = new Set<string>();
  for (const item of records) {
    if (item.kind !== "rpc.registered" || item.source !== "ronchi") continue;
    const identity =
      typeof item.request_id === "string"
        ? item.request_id
        : String(sequence(item));
    if (seen.has(identity)) continue;
    seen.add(identity);
    if (item.rpc_method === "sendTransaction") counts.submissions++;
    else if (item.rpc_method === "getSignatureStatuses") counts.statuses++;
    else if (item.rpc_method === "getTransaction") counts.transactions++;
    else counts.other++;
  }
  return counts;
}
function actions(counts: Story["counts"]) {
  return [
    [counts.submissions, "submission"],
    [counts.statuses, "status check"],
    [counts.transactions, "transaction lookup"],
    [counts.other, "other RPC request"],
  ]
    .filter(([n]) => Number(n) > 0)
    .map(([n, label]) => `${n} ${label}${n === 1 ? "" : "s"}`)
    .join(", ");
}
export function observationStory(
  finding: Item,
  records: Item[],
  complete: boolean,
  snapshot: Sequence,
): Story {
  const lab = text(finding.lab_id, "");
  const rows = mergeRecords(records).filter(
    (r) =>
      sequence(r) <= integer(snapshot) &&
      (!lab || object(r.event_ref).lab_id === lab),
  );
  const supports = Array.isArray(finding.supporting_events)
    ? finding.supporting_events.map(object)
    : [];
  const role = (name: string) =>
    rows.find((r) =>
      supports.some(
        (s) =>
          s.role === name &&
          object(s.event_ref).lab_id === object(r.event_ref).lab_id &&
          String(object(s.event_ref).event_seq) === String(sequence(r)),
      ),
    );
  const supported = new Set(
    supports.map((s) => String(object(s.event_ref).event_seq)),
  );
  const incident = rows.filter((r) => supported.has(String(sequence(r))));
  const kind = (name: string) => incident.find((r) => r.kind === name);
  const original = text(
    object(finding.scope).accepted_request_id,
    text(object(finding.scope).request_id, ""),
  );
  const originalRows = rows.filter(
    (r) => original && r.request_id === original,
  );
  const acceptance = role("acceptance") ?? kind("transaction.accepted");
  const registration = originalRows.find((r) => r.kind === "rpc.registered");
  const hold = kind("fault.withholding_started");
  const effect =
    kind("fault.lookup_changed") ?? kind("fault.custom_lookup_changed");
  const assignment = kind("fault.confirmation_schedule_assigned");
  const fault =
    hold ??
    effect ??
    assignment ??
    role("signed_submission") ??
    incident.find((r) => String(r.kind).startsWith("fault."));
  const counts = tally(rows);
  const follow = rows.filter(
    (r) =>
      r.kind === "rpc.registered" &&
      r.source === "ronchi" &&
      fault &&
      sequence(r) > sequence(fault) &&
      r.request_id !== original,
  );
  const after = tally(follow);
  const steps: StoryStep[] = [];
  const add = (
    title: string,
    facts: Item[],
    detail?: string,
    anchor?: Item,
  ) => {
    const records = mergeRecords(facts);
    if (records.length)
      steps.push({
        title,
        detail,
        records,
        at: sequence(anchor ?? records[0]),
      });
  };
  const rule = text(finding.rule_id, "");
  const custom = supports.some((s) => s.role === "lookup_hidden");
  const lost = rule.startsWith("tx-response-lost.");
  const timeout = rule === "provider-timeout-burst.later-rpc-attempt";
  const expired = rule === "recent-blockhash.expired-submission";
  const delayed = rule === "sim.confirmation-delay.observed";
  if (acceptance && !expired)
    add(
      acceptance.source === "surfpool"
        ? "Surfpool returned the transaction signature."
        : "The simulator accepted the transaction.",
      originalRows.filter((r) =>
        [
          "rpc.registered",
          "transaction.signed_attempt",
          "rpc.backend",
          "rpc.admission_result",
          "transaction.submission",
          "transaction.accepted",
        ].includes(String(r.kind)),
      ),
      acceptance.source === "surfpool"
        ? "A returned signature does not establish execution or finality."
        : "Acceptance does not execute programs or change balances.",
      acceptance,
    );
  if (timeout) {
    const dispatch = kind("rpc.dispatch_prevented");
    if (dispatch)
      add(
        "Ronchi prevented this request from reaching the RPC execution source.",
        [dispatch],
      );
  }
  if (hold) {
    const selection =
      role("fault_selection") ?? kind("fault.response_loss_selected");
    const policy = object(object(selection?.data).policy);
    const holdMs =
      object(hold.data).hold_ms ??
      policy.hold_ms ??
      object(hold.policy_ref).hold_ms;
    const caseId = text(
      policy.case_id,
      text(
        object(hold.policy_ref).case_id,
        timeout ? "provider-timeout-burst" : "tx-response-lost",
      ),
    );
    add(
      `Ronchi withheld the response${typeof holdMs === "number" ? ` with a ${holdMs / 1000} s limit` : ""}.`,
      [...(selection ? [selection] : []), hold],
      `Case ${caseId}. A withholding limit is not evidence of a client timeout.`,
      hold,
    );
  }
  if (effect) {
    const data = object(effect.data);
    const hidden =
      data.returned_available === false ||
      (Object.hasOwn(data, "returned_view") && data.returned_view === null);
    const source = role("authoritative_lookup") ?? role("authoritative_source");
    if (source)
      add("The RPC source had a transaction lookup result available.", [
        source,
      ]);
    add(
      hidden
        ? "Ronchi hid the available result and prepared null instead."
        : "Ronchi substituted an earlier transaction lookup view.",
      [
        effect,
        ...incident.filter((r) =>
          ["prepared_lookup", "rpc_boundary"].some((name) => role(name) === r),
        ),
      ],
      custom
        ? "The replacement belongs to the Custom Case's declared sequence."
        : undefined,
      effect,
    );
  }
  if (delayed) {
    const held = role("held_commitment"),
      recovery = role("recovery"),
      stage = text(object(held?.data).stage, "the selected stage");
    if (held)
      add(
        `The simulator held transaction progress at ${stage}.`,
        [...(assignment ? [assignment] : []), held],
        undefined,
        held,
      );
    if (recovery) {
      const duration = interval(held, recovery, "scheduled_transition_ns");
      add(
        `The simulator later recorded ${text(object(recovery.data).stage, "the next stage")}.`,
        [recovery],
        duration
          ? `${duration} of simulated time between the recorded stages.`
          : undefined,
      );
    }
  }
  if (expired) {
    const validity = incident.filter((r) =>
      [
        "original_tuple",
        "expiry_height",
        "stale_tuple_view",
        "stale_read_boundary",
      ].some((name) => role(name) === r),
    );
    add(
      "The RPC source reported the blockhash validity and a later chain height.",
      validity,
    );
    const signed = role("signed_submission");
    if (signed)
      add(
        "A signed transaction used the blockhash beyond its observed validity bound.",
        [signed],
      );
    const prevented = incident.find(
      (r) => r.kind === "transaction.processing_prevented",
    );
    const admitted = role("admission_result");
    if (prevented)
      add(
        "The simulator prevented processing of this transaction.",
        [prevented],
        "This is a local simulator observation.",
      );
    else if (admitted)
      add(
        `The RPC execution source recorded submission result: ${text(object(admitted.data).admission, "unknown")}.`,
        [admitted],
        "A submission result alone does not establish an economic outcome.",
      );
  }
  if (follow.length) {
    const first = follow[0],
      method = text(first.rpc_method, "RPC");
    const delay = interval(registration, first);
    const label =
      method === "getSignatureStatuses"
        ? "A status check for the same signature arrived"
        : method === "getTransaction"
          ? "A transaction lookup for the same signature arrived"
          : method === "sendTransaction"
            ? "Another submission of the same signature arrived"
            : `Another ${method} request arrived`;
    add(
      `${label}${delay ? ` ${delay} after the original request` : ""}.`,
      [first, ...(registration ? [registration] : [])],
      "The request is observed; its cause and the client's internal state are not.",
      first,
    );
    if (follow.length > 1)
      add(
        `Later activity: ${actions(tally(follow.slice(1)))}.`,
        follow.slice(1),
        "Each count is a distinct received RPC request.",
        follow[1],
      );
  }
  // For unknown rules, explain the received actions without inventing a Fault.
  if (!steps.length) {
    const requests = rows.filter(
      (r) => r.kind === "rpc.registered" && r.source === "ronchi",
    );
    if (requests.length)
      add(`Ronchi received ${actions(tally(requests))}.`, requests);
    if (incident.length)
      add(
        "Ronchi linked these supporting records to the observation.",
        incident,
        "The exact rule statement is available in Technical details.",
      );
  }
  const boundary = originalRows.find((r) => r.kind === "rpc.boundary");
  if (boundary) {
    const data = object(boundary.data);
    if (data.rpc_response === "no_transport_response")
      add(
        "The original HTTP request ended without a transport response.",
        [boundary],
        data.transport === "withholding_deadline"
          ? "The withholding deadline was reached. Client interpretation remains unknown."
          : "The recorded HTTP ending does not establish the client's own timeout.",
      );
    else if (data.transport === "http_write_completed")
      add(
        "Ronchi completed the server write for the original request.",
        [boundary],
        "A completed server write does not prove client receipt.",
      );
    else
      add(
        "The original HTTP request has a recorded transport ending.",
        [boundary],
        "Its exact outcome is available in Technical details; client receipt remains unconfirmed.",
      );
  }
  if (
    rows.some(
      (r) =>
        r.kind === "rpc.registered" &&
        (r.schema_version !== 1 || r.source !== "ronchi"),
    )
  )
    complete = false;
  let outcome = actions(counts)
    ? `${actions(counts)} recorded in the selected history.`
    : "No received RPC requests are present in the available records.";
  if ((lost || timeout) && actions(after))
    outcome += ` After withholding: ${actions(after)}.`;
  if (lost && counts.submissions === 1 && complete)
    outcome +=
      " No repeat submission of this signature was recorded in this selected history.";
  if (!complete)
    outcome +=
      " This history is incomplete; counts are lower bounds and cannot establish absence of other actions.";
  const missingEnding =
    (lost || timeout) && !boundary
      ? "The available records do not establish how the original HTTP request ended. "
      : "";
  const unknown = expired
    ? "Client receipt, application recovery and business completion remain unconfirmed. An error or null status does not prove non-execution."
    : delayed
      ? "Simulated progress does not establish program execution or balance changes. Application completion requires separate evidence."
      : "Client timeout, reasons for later requests and business completion require separate application evidence. RPC activity alone does not establish a transfer or application failure.";
  return {
    steps: steps.sort((a, b) => (a.at < b.at ? -1 : a.at > b.at ? 1 : 0)),
    counts,
    after,
    complete,
    outcome,
    unknown: custom
      ? `${missingEnding}${unknown} This Custom rule is not a maintainer-verified Coverage Claim.`
      : missingEnding + unknown,
  };
}
