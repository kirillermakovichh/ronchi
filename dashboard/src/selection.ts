import { coordinate, type Sequence } from "./wire";
import type { Scope } from "./api";
export interface DetailSelection {
  finding_id?: string;
  event_seq?: Sequence;
}
export function selection(search: string): {
  scope: Scope;
  snapshot?: Sequence;
  error?: string;
  detail?: DetailSelection;
} {
  const params = new URLSearchParams(search),
    scope: Scope = {};
  for (const key of [
    "from_seq",
    "to_seq",
    "case_activation_id",
    "snapshot",
    "event",
  ]) {
    const values = params.getAll(key);
    if (
      values.some((value) => coordinate(value) === undefined) ||
      (key !== "case_activation_id" && values.length > 1)
    )
      return {
        scope: {},
        error: `Invalid ${key}. Use a decimal integer within the Journal range.`,
      };
  }
  for (const key of ["signature", "rpc_method", "kind"] as const) {
    const values = params.getAll(key);
    if (values.length) scope[key] = values;
  }
  for (const key of ["from_seq", "to_seq"] as const) {
    const value = params.get(key);
    if (value && /^\d+$/.test(value)) scope[key] = coordinate(value);
  }
  for (const key of ["since", "until"] as const) {
    const value = params.get(key);
    if (value) scope[key] = value;
  }
  const activations = params.getAll("case_activation_id");
  if (activations.length)
    scope.case_activation_id = activations.map((value) => coordinate(value)!);
  const snapshot = params.get("snapshot");
  const finding = params.get("finding"),
    event = params.get("event");
  if ((finding || event) && !snapshot)
    return { scope, error: "Detail links require an explicit snapshot." };
  if (
    (finding && event) ||
    (finding && !/^f1-[0-9a-f]{64}$/.test(finding)) ||
    (event && BigInt(event) < 1n)
  )
    return { scope, error: "Invalid detail reference." };
  const detail: DetailSelection | undefined = finding
    ? { finding_id: finding }
    : event
      ? { event_seq: coordinate(event) }
      : undefined;
  return {
    scope,
    ...(detail ? { detail } : {}),
    snapshot: snapshot ? coordinate(snapshot) : undefined,
  };
}
export function bookmark(
  labId: string,
  scope: Scope,
  snapshot: Sequence,
  detail?: DetailSelection,
): string {
  const params = new URLSearchParams({ snapshot: String(snapshot) });
  for (const [key, value] of Object.entries(scope)) {
    if (value !== undefined && key !== "event_refs" && key !== "finding_id") {
      for (const entry of Array.isArray(value) ? value : [value])
        params.append(key, String(entry));
    }
  }
  if (detail?.finding_id) params.set("finding", detail.finding_id);
  if (detail?.event_seq !== undefined)
    params.set("event", String(detail.event_seq));
  return `${location.origin}/ui/${encodeURIComponent(labId)}/?${params}`;
}
