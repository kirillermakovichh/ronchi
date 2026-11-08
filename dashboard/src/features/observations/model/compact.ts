import { object, text, type Item } from "../../../shared/lib/format";
import { sequence, type Story } from "./story";
import { type LookupReply, type Tone } from "./reply";
export interface TimelineRow {
  id: string;
  time: string;
  label: string;
  icon: string;
  tone: Tone;
  records: Item[];
  at: bigint;
  note?: string;
}
export function compactObservation(
  finding: Item,
  story: Story,
  records: Item[],
  replies: LookupReply[],
) {
  const scope = object(finding.scope),
    original = scope.accepted_request_id ?? scope.request_id;
  const origin = records.find(
    (r) =>
      typeof original === "string" &&
      r.kind === "rpc.registered" &&
      r.request_id === original,
  );
  const relative = (r: Item | undefined) => {
    const a = String(origin?.elapsed_ns),
      b = String(r?.elapsed_ns);
    if (!/^\d+$/.test(a) || !/^\d+$/.test(b)) return "—";
    const delta = BigInt(b) - BigInt(a),
      seconds = ((delta < 0n ? -delta : delta) + 500000000n) / 1000000000n;
    return `${delta < 0n && seconds > 0n ? "−" : ""}${seconds} s`;
  };
  const timeline: TimelineRow[] = [];
  const add = (
    label: string,
    icon: string,
    tone: Tone,
    at: Item,
    facts: Item[],
    id: string,
    note?: string,
  ) => {
    const row: TimelineRow = {
      id,
      time: relative(at),
      label,
      icon,
      tone,
      records: facts,
      at: sequence(at),
      note,
    };
    timeline.push(row);
    return row;
  };
  // Keep semantic Fault/source/recovery steps. Lookup requests and replies get
  // their own paired rows so an authoritative state cannot masquerade as a reply.
  for (const step of story.steps) {
    const anchor =
      records.find((r) => sequence(r) === step.at) ?? step.records[0];
    if (
      anchor?.kind === "rpc.registered" &&
      replies.some((r) => r.request.request_id === anchor.request_id)
    )
      continue;
    if (step.title.startsWith("Later activity:")) continue;
    let label = step.title.replace(/\.$/, ""),
      icon = "•",
      tone: Tone = "neutral";
    const kind = String(anchor?.kind);
    if (kind === "transaction.accepted") {
      label =
        anchor.source === "surfpool"
          ? "Surfpool returned a signature"
          : "Simulator accepted the transaction";
      icon = "✓";
    } else if (kind === "fault.withholding_started") {
      const selection = step.records.find(
        (r) => r.kind === "fault.response_loss_selected",
      );
      const ms =
        object(object(selection?.data).policy).hold_ms ??
        object(anchor.policy_ref).hold_ms ??
        object(anchor.data).hold_ms;
      label = `Ronchi withheld the response${typeof ms === "number" ? ` · ${ms / 1000} s limit` : ""}`;
      icon = "⏸";
      tone = "injected";
    } else if (kind === "rpc.dispatch_prevented") {
      label = "Ronchi prevented dispatch";
      icon = "⏸";
      tone = "injected";
    } else if (kind.startsWith("fault.")) {
      icon = "⏸";
      tone = "injected";
      label = label
        .replace(
          "Ronchi substituted an earlier transaction lookup view",
          "Ronchi returned an earlier status",
        )
        .replace(
          "Ronchi hid the available result and prepared null instead",
          "Ronchi replaced the result with null",
        );
    } else if (kind === "rpc.boundary") {
      icon = "⏹";
      label =
        object(anchor.data).rpc_response === "no_transport_response"
          ? "Original request ended without a response"
          : "Original HTTP request ended";
    } else if (kind === "transaction.transition") {
      icon = "→";
      label = `Simulator: ${text(object(anchor.data).stage, "state changed")}`;
    } else if (kind === "rpc.registered") {
      icon = "→";
      label = `Application sent ${text(anchor.rpc_method, "an RPC request")}`;
    }
    if (
      tone === "neutral" &&
      kind === "transaction.transition" &&
      step.records.some(
        (r) => r.kind === "fault.confirmation_schedule_assigned",
      )
    ) {
      tone = "injected";
      icon = "⏸";
    }
    add(
      label,
      icon,
      tone,
      anchor,
      step.records,
      `step:${kind}:${step.at}`,
      step.detail,
    );
  }
  const shownRequests = new Set(
    timeline.flatMap((row) =>
      row.records
        .filter((r) => r.kind === "rpc.registered")
        .map((r) => String(r.request_id)),
    ),
  );
  const otherRequests = records.filter(
    (r) =>
      r.kind === "rpc.registered" &&
      r.source === "ronchi" &&
      r.request_id !== original &&
      !["getSignatureStatuses", "getTransaction"].includes(
        String(r.rpc_method),
      ) &&
      !shownRequests.has(String(r.request_id)),
  );
  for (const request of otherRequests.slice(-4))
    add(
      request.rpc_method === "sendTransaction"
        ? "Application submitted the transaction again"
        : `Application sent ${text(request.rpc_method, "an RPC request")}`,
      "→",
      "neutral",
      request,
      [request],
      `activity:${request.request_id}`,
    );
  const expiredReturns = new Map<string, Item>();
  for (const step of story.steps)
    for (const record of step.records)
      if (
        record.kind === "fault.blockhash_returned" &&
        object(record.data).expiry_observed_elapsed_ns != null
      )
        expiredReturns.set(String(sequence(record)), record);
  for (const record of expiredReturns.values())
    add(
      "Ronchi returned an expired blockhash",
      "⏸",
      "injected",
      record,
      [record],
      `expired:${sequence(record)}`,
    );
  const span = (a: Item, b: Item) => {
    const first = relative(a),
      last = relative(b);
    return first === last
      ? first
      : first === "—" || last === "—"
        ? "—"
        : `${first.replace(/ s$/, " ").trim()}–${last}`;
  };
  const groups: { items: LookupReply[]; key: string }[] = [];
  for (const reply of replies) {
    const key = JSON.stringify([
      reply.request.rpc_method,
      reply.label,
      reply.tone,
      reply.err,
    ]);
    const last = groups.at(-1);
    const boundary = records.find(
      (r) => r.kind === "rpc.boundary" && r.request_id === original,
    );
    const crosses =
      boundary &&
      last &&
      sequence(boundary) > sequence(last.items.at(-1)!.request) &&
      sequence(boundary) < sequence(reply.request);
    if (last?.key === key && !crosses) last.items.push(reply);
    else groups.push({ key, items: [reply] });
  }
  const shown = groups.length > 6 ? [groups[0], ...groups.slice(-5)] : groups;
  for (const group of shown) {
    const r = group.items[0],
      count = group.items.length,
      method = r.request.rpc_method;
    const suffix = count > 1 ? ` ×${count}` : "";
    const requestRow = add(
      `${method === "getSignatureStatuses" ? "Application checked status" : "Application requested the transaction"}${suffix}`,
      "→",
      "neutral",
      r.request,
      group.items.map((r) => r.request),
      `request:${r.request.request_id}`,
    );
    requestRow.time = span(r.request, group.items.at(-1)!.request);
    const simulated =
      r.request.execution_mode === "simulator" && r.status
        ? "Simulated RPC reply: "
        : "RPC reply: ";
    const responseRow = add(
      `${simulated}${r.label}${suffix}`,
      r.tone === "success"
        ? "✓"
        : r.tone === "failure"
          ? "✕"
          : r.tone === "pending"
            ? "◷"
            : "○",
      r.tone,
      r.at,
      group.items.flatMap((r) => r.records),
      `reply:${r.request.request_id}`,
      r.reason,
    );
    responseRow.time = span(r.at, group.items.at(-1)!.at);
  }
  timeline.sort((a, b) =>
    a.at < b.at ? -1 : a.at > b.at ? 1 : a.id.startsWith("request") ? -1 : 1,
  );
  const statuses = replies.filter(
    (r) => r.request.rpc_method === "getSignatureStatuses",
  );
  const latest = statuses.at(-1) ?? replies.at(-1);
  const laterFailure = replies
    .filter(
      (r) =>
        r.tone === "failure" &&
        (!latest || sequence(r.request) >= sequence(latest.request)),
    )
    .at(-1);
  const result = laterFailure ?? latest;
  const rule = text(finding.rule_id, "");
  const fault = rule.startsWith("tx-response-lost.")
    ? "Response withheld"
    : rule === "rpc.lookup-view.changed"
      ? "Lookup changed"
      : rule === "sim.confirmation-delay.observed"
        ? "Confirmation delayed"
        : rule === "provider-timeout-burst.later-rpc-attempt"
          ? "RPC request held"
          : rule === "recent-blockhash.expired-submission"
            ? "Expired blockhash"
            : "RPC observation";
  const checked = replies.length ? " → transaction checked" : "";
  const resultLabel = result?.status
    ? `${result.request.execution_mode === "simulator" ? "simulated " : ""}${result.status}`
    : result?.tone === "failure"
      ? "execution error"
      : "Result unknown";
  const headline = `${fault}${checked} → ${resultLabel}`;
  const c = story.counts,
    prefix = story.complete ? "" : "≥ ";
  const counts = [
    `${prefix}${c.submissions} submission${c.submissions === 1 ? "" : "s"}`,
    `${prefix}${c.statuses} status check${c.statuses === 1 ? "" : "s"}`,
    c.transactions
      ? `${prefix}${c.transactions} transaction lookup${c.transactions === 1 ? "" : "s"}`
      : "",
    c.other
      ? `${prefix}${c.other} other RPC call${c.other === 1 ? "" : "s"}`
      : "",
    story.complete &&
    c.submissions === 1 &&
    rule.startsWith("tx-response-lost.")
      ? "no repeat submission observed"
      : "",
  ]
    .filter(Boolean)
    .join(" · ");
  let followUp: string | undefined;
  if (
    rule.startsWith("tx-response-lost.") &&
    records.some(
      (r) =>
        r.kind === "fault.withholding_started" && r.request_id === original,
    )
  ) {
    if (!story.complete) {
      followUp =
        "Not enough data about subsequent RPC activity for this signature.";
    } else {
      const actions = [
        [story.after.statuses, "status check"],
        [story.after.transactions, "transaction lookup"],
        [story.after.submissions, "repeat submission"],
      ] as const;
      const observed = actions
        .filter(([count]) => count > 0)
        .map(([count, name]) => `${count} ${name}${count === 1 ? "" : "s"}`);
      const absent = actions
        .filter(([count]) => count === 0)
        .map(([, name]) => `${name}s`);
      const list = (items: string[], conjunction: string) =>
        items.length < 2
          ? items.join("")
          : `${items.slice(0, -1).join(", ")} ${conjunction} ${items.at(-1)}`;
      followUp = [
        observed.length
          ? `Observed ${list(observed, "and")} for this signature.`
          : "",
        absent.length
          ? `No ${list(absent, "or")} observed in this history.`
          : "",
      ]
        .filter(Boolean)
        .join(" ");
    }
  }
  return {
    timeline,
    headline,
    counts,
    result,
    followUp,
    omittedGroups:
      Math.max(0, groups.length - shown.length) +
      Math.max(0, otherRequests.length - 4),
  };
}
