// Only protocol integer coordinates use raw decimal tokens. Fact strings remain
// strings. This avoids coercing Journal coordinates through JavaScript Number.
export type Sequence = number | string;
const integerFields = new Set([
  "snapshot_seq",
  "event_seq",
  "from_seq",
  "to_seq",
  "case_activation_id",
]);
export function requestJson(value: unknown, key = ""): string {
  if (
    typeof value === "string" &&
    integerFields.has(key) &&
    /^\d+$/.test(value)
  )
    return value;
  if (Array.isArray(value))
    return "[" + value.map((v) => requestJson(v, key)).join(",") + "]";
  if (value !== null && typeof value === "object")
    return (
      "{" +
      Object.entries(value)
        .filter(([, v]) => v !== undefined)
        .map(([k, v]) => JSON.stringify(k) + ":" + requestJson(v, k))
        .join(",") +
      "}"
    );
  return JSON.stringify(value) ?? "null";
}
export function coordinate(value: string): Sequence | undefined {
  if (
    value.length > 19 ||
    !/^\d+$/.test(value) ||
    BigInt(value) > 9223372036854775807n
  )
    return undefined;
  const exact = BigInt(value);
  return exact <= BigInt(Number.MAX_SAFE_INTEGER)
    ? Number(exact)
    : exact.toString();
}
