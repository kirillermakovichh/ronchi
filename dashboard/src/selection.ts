import type { Scope } from "./api";
export function selection(search: string): {
  scope: Scope;
  snapshot?: number;
  error?: string;
} {
  const params = new URLSearchParams(search),
    scope: Scope = {};
  for (const key of ["from_seq", "to_seq", "case_activation_id", "snapshot"]) {
    const value = params.get(key);
    if (
      value !== null &&
      (!/^\d+$/.test(value) || !Number.isSafeInteger(Number(value)))
    )
      return {
        scope: {},
        error: `Invalid ${key}. Use an exact safe integer for browser navigation.`,
      };
  }
  for (const key of ["signature", "rpc_method", "kind"] as const) {
    const value = params.get(key);
    if (value) scope[key] = [value];
  }
  for (const key of ["from_seq", "to_seq"] as const) {
    const value = params.get(key);
    if (value && /^\d+$/.test(value)) scope[key] = Number(value);
  }
  for (const key of ["since", "until"] as const) {
    const value = params.get(key);
    if (value) scope[key] = value;
  }
  const activation = params.get("case_activation_id");
  if (activation && /^\d+$/.test(activation))
    scope.case_activation_id = [Number(activation)];
  const snapshot = params.get("snapshot");
  return {
    scope,
    snapshot: snapshot && /^\d+$/.test(snapshot) ? Number(snapshot) : undefined,
  };
}
export function bookmark(
  labId: string,
  scope: Scope,
  snapshot: number,
): string {
  const params = new URLSearchParams({ snapshot: String(snapshot) });
  for (const [key, value] of Object.entries(scope)) {
    if (value !== undefined && key !== "event_refs")
      params.set(key, String(Array.isArray(value) ? value[0] : value));
  }
  return `${location.origin}/ui/${encodeURIComponent(labId)}/?${params}`;
}
