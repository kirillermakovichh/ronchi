import { object, signature, text, type Item } from "../../../shared/lib/format";
import { sequence } from "./story";
export const appliedKinds = [
  "fault.withholding_started",
  "fault.lookup_changed",
  "fault.custom_lookup_changed",
  "fault.confirmation_schedule_assigned",
  "fault.blockhash_returned",
];
export interface Episode {
  id: string;
  anchor?: Item;
  findings: Item[];
  title: string;
  at: string;
  signature?: string;
}
export function faultTitle(item: Item) {
  const kind = String(item.kind),
    policy = object(item.policy_ref),
    caseId = String(
      policy.case_id ?? object(object(item.data).policy).case_id ?? "",
    );
  return kind === "fault.withholding_started"
    ? caseId === "provider-timeout-burst"
      ? "RPC request held"
      : "Response withheld"
    : kind === "fault.lookup_changed" || kind === "fault.custom_lookup_changed"
      ? "Transaction lookup changed"
      : kind === "fault.confirmation_schedule_assigned"
        ? "Confirmation policy applied"
        : kind === "fault.blockhash_returned"
          ? "Cached blockhash returned"
          : "RPC observation";
}
export function groupEpisodes(records: Item[], findings: Item[]): Episode[] {
  const episodes: Episode[] = records
    .filter((r) => appliedKinds.includes(String(r.kind)))
    .map((anchor) => ({
      id: String(sequence(anchor)),
      anchor,
      findings: [] as Item[],
      title: faultTitle(anchor),
      at: text(anchor.recorded_at, ""),
      signature: signature(anchor),
    }));
  for (const finding of findings) {
    const refs = Array.isArray(finding.supporting_events)
      ? finding.supporting_events.map((s) => object(object(s).event_ref))
      : [];
    const matching = episodes.filter((ep) =>
      refs.some(
        (r) =>
          String(r.lab_id) === String(object(ep.anchor?.event_ref).lab_id) &&
          String(r.event_seq) === ep.id,
      ),
    );
    if (matching.length === 1) matching[0].findings.push(finding);
    else
      episodes.push({
        id: String(finding.finding_id),
        anchor: undefined,
        findings: [finding],
        title: text(finding.statement, "RPC observation"),
        at: "",
        signature: text(object(finding.scope).signature, "") || undefined,
      });
  }
  return episodes.sort((a, b) => {
    const x = a.anchor
      ? sequence(a.anchor)
      : BigInt(String(object(a.findings[0]?.scope).from_seq ?? 0));
    const y = b.anchor
      ? sequence(b.anchor)
      : BigInt(String(object(b.findings[0]?.scope).from_seq ?? 0));
    return x < y ? 1 : x > y ? -1 : a.id.localeCompare(b.id);
  });
}
export function episodeFinding(anchor: Item, records: Item[]): Item {
  const request =
    anchor.request_id ?? object(object(anchor.data).request).request_id;
  const original = records.filter((r) => r.request_id === request),
    sig = signature(anchor) ?? original.map(signature).find(Boolean);
  const kind = String(anchor.kind);
  const rule =
    kind === "fault.withholding_started"
      ? original.some((r) => r.kind === "rpc.dispatch_prevented")
        ? "provider-timeout-burst.later-rpc-attempt"
        : "tx-response-lost.lookup-after-withholding"
      : kind === "fault.lookup_changed"
        ? "rpc.lookup-view.changed"
        : kind === "fault.custom_lookup_changed"
          ? "custom.lookup-hidden"
          : kind === "fault.confirmation_schedule_assigned"
            ? "sim.confirmation-delay.observed"
            : kind === "fault.blockhash_returned"
              ? "ui.cached-blockhash"
              : "unknown";
  const role = (r: Item) =>
    r.kind === "transaction.accepted"
      ? "acceptance"
      : r.kind === "rpc.registered"
        ? "submission"
        : r.kind === "fault.withholding_started"
          ? "observed_withholding"
          : r.kind === "fault.custom_lookup_changed"
            ? "lookup_hidden"
            : r.kind === "fault.confirmation_schedule_assigned"
              ? "assignment"
              : "support";
  return {
    lab_id: object(anchor.event_ref).lab_id ?? "",
    rule_id: rule,
    scope: {
      accepted_request_id: request ?? "",
      signature: sig ?? "",
      from_seq: object(anchor.event_ref).event_seq ?? 0,
    },
    supporting_events: [
      ...original.filter((r) => sequence(r) <= sequence(anchor)),
      anchor,
    ].map((r) => ({ event_ref: r.event_ref, role: role(r) })),
    statement: faultTitle(anchor),
  };
}
export function episodeWindow(
  records: Item[],
  anchor: Item,
): { records: Item[]; next?: Item } {
  const original =
      anchor.request_id ?? object(object(anchor.data).request).request_id,
    seq = sequence(anchor);
  const start = records.find(
    (r) => r.kind === "rpc.registered" && r.request_id === original,
  );
  const next = records.find(
    (r) =>
      sequence(r) > seq &&
      appliedKinds.includes(String(r.kind)) &&
      r.request_id !== original,
  );
  return {
    next,
    records: records.filter(
      (r) =>
        r.request_id === original ||
        (sequence(r) >= sequence(start ?? anchor) &&
          (!next || sequence(r) < sequence(next))),
    ),
  };
}
