import { object, text, type Item } from "../../../shared/lib/format";
import { sequence } from "./story";
import type { Api, Json } from "../../../shared/transport/client";
import { parseEvidence } from "../../../shared/protocol/json";
export type Tone = "success" | "failure" | "pending" | "neutral" | "injected";
export interface LookupReply {
  request: Item;
  at: Item;
  records: Item[];
  label: string;
  tone: Tone;
  status?: string;
  err?: Json;
  raw?: unknown;
  reason?: string;
  retryable?: boolean;
}
export const lookupRequests = (records: Item[]) =>
  records.filter(
    (r) =>
      r.kind === "rpc.registered" &&
      r.source === "ronchi" &&
      ["getSignatureStatuses", "getTransaction"].includes(String(r.rpc_method)),
  );
function unknown(
  request: Item,
  records: Item[],
  label = "Reply unavailable",
  reason?: string,
): LookupReply {
  return {
    request,
    at:
      records.find((r) => r.kind === "rpc.boundary") ??
      records.find((r) => r.kind === "rpc.response_prepared") ??
      request,
    records,
    label,
    tone: "neutral",
    reason,
  };
}
function briefError(value: unknown): string {
  const e = object(value);
  if (Array.isArray(e.InstructionError)) {
    const [index, error] = e.InstructionError;
    const custom = object(error).Custom;
    return `Instruction ${index}: ${custom !== undefined ? `custom error ${custom}` : text(error, JSON.stringify(error))}`.slice(
      0,
      140,
    );
  }
  return (
    typeof value === "string"
      ? value.replace(/([a-z])([A-Z])/g, "$1 $2")
      : JSON.stringify(value)
  ).slice(0, 140);
}
export function classifyReply(
  request: Item,
  records: Item[],
  value: unknown,
): LookupReply {
  const base = unknown(request, records);
  base.at = records.find((r) => r.kind === "rpc.boundary") ?? base.at;
  base.raw = value;
  const body = object(value);
  if (Object.hasOwn(body, "error"))
    return {
      ...base,
      label: `RPC error: ${text(object(body.error).code, "unknown")}`,
      reason: text(object(body.error).message, "RPC request failed"),
      raw: value,
    };
  if (!Object.hasOwn(body, "result"))
    return { ...base, reason: "No retained RPC result" };
  const result = body.result;
  const method = String(request.rpc_method);
  const target = text(object(request).selected_signature, "");
  const signatures = Array.isArray(request.transaction_signatures)
    ? request.transaction_signatures
    : [];
  const index = signatures.indexOf(target);
  let entry: unknown;
  if (method === "getSignatureStatuses") {
    const entries = object(result).value;
    if (
      index < 0 ||
      !Array.isArray(entries) ||
      entries.length !== signatures.length
    )
      return {
        ...base,
        reason: "Signature position or response array is unavailable",
      };
    entry = entries[index];
  } else {
    if (!target || signatures.length !== 1 || signatures[0] !== target)
      return {
        ...base,
        reason: "Transaction signature correlation is unavailable",
      };
    entry = result;
  }
  if (entry === null)
    return {
      ...base,
      label:
        method === "getSignatureStatuses"
          ? "Status not found (null)"
          : "Transaction not found (null)",
    };
  const data = object(entry),
    error = method === "getTransaction" ? object(data.meta) : data;
  if (!Object.hasOwn(error, "err"))
    return { ...base, reason: "Execution error field is unavailable" };
  if (error.err !== null)
    return {
      ...base,
      label: `Execution error: ${briefError(error.err)}`,
      tone: "failure",
      err: error.err,
    };
  if (method === "getTransaction")
    return {
      ...base,
      label: "Transaction returned · err: null",
      err: null,
      reason: "The transaction reply does not specify a confirmation level",
    };
  const status = data.confirmationStatus;
  if (status === "confirmed" || status === "finalized")
    return {
      ...base,
      label: `${status} · err: null`,
      status,
      tone: "success",
      err: null,
    };
  if (status === "processed")
    return {
      ...base,
      label: "processed · not yet confirmed",
      status,
      tone: "pending",
      err: null,
    };
  return {
    ...base,
    label: "Confirmation level unavailable · err: null",
    err: null,
  };
}
export function replyKey(request: Item, records: Item[]): string {
  const related = records.filter(
    (r) =>
      r.request_id === request.request_id &&
      [
        "rpc.response_prepared",
        "rpc.boundary",
        "rpc.backend",
        "rpc.upstream",
        "fault.lookup_changed",
        "fault.custom_lookup_changed",
      ].includes(String(r.kind)),
  );
  return JSON.stringify([
    request.request_id,
    request.selected_signature,
    related.map((r) => r.event_ref),
  ]);
}
async function digest(value: unknown) {
  // serde_json retains sorted object keys. Unsafe integer spellings that were
  // converted for display fail digest comparison rather than inventing a match.
  const sort = (v: unknown): unknown =>
    Array.isArray(v)
      ? v.map(sort)
      : v !== null && typeof v === "object"
        ? Object.fromEntries(
            Object.entries(v)
              .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
              .map(([k, v]) => [k, sort(v)]),
          )
        : v;
  const bytes = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(JSON.stringify(sort(value))),
  );
  return [...new Uint8Array(bytes)]
    .map((n) => n.toString(16).padStart(2, "0"))
    .join("");
}
export async function resolveReply(
  api: Api,
  request: Item,
  all: Item[],
  snapshot: number | string,
  signal: AbortSignal,
): Promise<LookupReply> {
  if (typeof request.request_id !== "string" || !request.request_id)
    return unknown(
      request,
      [],
      "Reply unavailable",
      "Request identity is missing",
    );
  const records = all
    .filter((r) => r.request_id === request.request_id)
    .sort((a, b) => (sequence(a) < sequence(b) ? -1 : 1));
  const boundary = records.find((r) => r.kind === "rpc.boundary");
  const prepared = records.find((r) => r.kind === "rpc.response_prepared");
  if (
    boundary?.source === "ronchi" &&
    boundary.schema_version === 1 &&
    object(boundary.data).rpc_response === "no_transport_response"
  )
    return unknown(
      request,
      records,
      "No RPC reply",
      "Ronchi recorded an HTTP ending without a transport response",
    );
  if (!boundary || object(boundary.data).transport !== "http_write_completed")
    return unknown(
      request,
      records,
      boundary ? "Delivery unconfirmed" : "Response pending",
      "A completed HTTP server write is not recorded. Client receipt is unknown.",
    );
  if (
    object(request.event_ref).lab_id !== api.labId ||
    request.source !== "ronchi" ||
    request.schema_version !== 1 ||
    boundary.source !== "ronchi" ||
    boundary.schema_version !== 1 ||
    prepared?.source !== "ronchi" ||
    prepared.schema_version !== 1
  )
    return unknown(
      request,
      records,
      "Reply unavailable",
      "RPC correlation schema or source is unsupported",
    );
  const response = object(boundary.data).rpc_response;
  const p = object(prepared?.data);
  if (response === "error" && Object.hasOwn(p, "error_code"))
    return {
      ...unknown(request, records),
      at: boundary,
      label: `RPC error: ${text(p.error_code)}`,
      reason: "An RPC error is separate from a transaction execution error",
    };
  if (response !== "result" || !prepared)
    return unknown(
      request,
      records,
      response === "no_transport_response"
        ? "No RPC reply"
        : "Reply unavailable",
    );
  if (
    prepared.payload_status === "redacted" ||
    prepared.payload_status === "omitted_by_policy"
  )
    return unknown(
      request,
      records,
      "Reply unavailable",
      "Prepared result is redacted or omitted",
    );
  if (Object.hasOwn(p, "result"))
    return classifyReply(request, records, { result: p.result });
  const target = text(request.selected_signature, "");
  const signatures = Array.isArray(request.transaction_signatures)
    ? request.transaction_signatures
    : [];
  const index = signatures.indexOf(target);
  if (
    index < 0 ||
    (request.rpc_method === "getTransaction" && signatures.length !== 1)
  )
    return unknown(
      request,
      records,
      "Reply unavailable",
      "Signature correlation is unavailable",
    );
  const digests = p.lookup_boundary_digests;
  const effects = records.filter(
    (r) =>
      ["fault.lookup_changed", "fault.custom_lookup_changed"].includes(
        String(r.kind),
      ) &&
      object(r.data).entry_index === index &&
      sequence(r) < sequence(prepared),
  );
  const latest = effects.at(-1);
  if (latest && Array.isArray(digests) && typeof digests[index] === "string") {
    const effect = object(latest.data);
    const candidate = Object.hasOwn(effect, "returned_view")
      ? effect.returned_view
      : effect.returned_available === false
        ? null
        : undefined;
    if (
      candidate !== undefined &&
      (await digest(candidate)) === digests[index]
    ) {
      const result =
        request.rpc_method === "getSignatureStatuses"
          ? {
              value: signatures.map((_, i) => (i === index ? candidate : null)),
            }
          : candidate;
      return {
        ...classifyReply(request, records, { result }),
        raw: {
          availability: "verified_entry_only",
          entry_index: index,
          returned_view: candidate,
          prepared_lookup_digest: digests[index],
        },
      };
    }
    return unknown(
      request,
      records,
      "Reply unavailable",
      "The application-facing replacement could not be verified",
    );
  }
  const sourceKind = object(p.source_result).kind;
  if (sourceKind !== "rpc.backend" && sourceKind !== "rpc.upstream")
    return unknown(request, records);
  if (typeof p.source_identity !== "string" || !p.source_identity)
    return unknown(
      request,
      records,
      "Reply unavailable",
      "RPC source identity is unavailable",
    );
  const chunks = records.filter(
    (r) => r.kind === sourceKind && sequence(r) < sequence(prepared),
  );
  const meta = object(object(chunks[0]?.data).source_result);
  const expected = Number(meta.chunks);
  if (
    !Number.isInteger(expected) ||
    expected < 1 ||
    expected > 2 ||
    chunks.length !== expected ||
    Number(meta.bytes) > 32768
  )
    return unknown(
      request,
      records,
      "Reply unavailable",
      "Retained response exceeds the 32 KiB automatic preview budget, or chunks are missing",
    );
  const buffers: Uint8Array[] = [];
  let total = 0;
  for (let i = 0; i < chunks.length; i++) {
    const record = chunks[i],
      data = object(record.data),
      m = object(data.source_result);
    if (
      record.payload_status !== "available" ||
      data.chunk_schema_version !== 2 ||
      m.chunk !== i ||
      m.chunks !== expected ||
      m.encoding !== "base64-json" ||
      m.redacted !== false ||
      m.bytes !== meta.bytes ||
      data.source_identity !== p.source_identity
    )
      return unknown(
        request,
        records,
        "Reply unavailable",
        "Source response provenance is missing, redacted or inconsistent",
      );
    let encoded = "",
      offset = 0;
    for (let part = 0; part < 6; part++) {
      const reply = await api.query(
        "evidence",
        {
          event_refs: [
            record.event_ref as { lab_id: string; event_seq: number | string },
          ],
        },
        snapshot,
        undefined,
        { include_payload: true, payload_offset: offset, limit: 1 },
        signal,
      );
      const item = reply.result.items?.[0];
      const payload = object(item?.payload);
      if (
        object(item?.event_ref).lab_id !== object(record.event_ref).lab_id ||
        String(object(item?.event_ref).event_seq) !==
          String(object(record.event_ref).event_seq) ||
        payload.offset !== offset ||
        typeof payload.preview !== "string" ||
        Number(payload.retained_bytes) > 24000
      )
        return unknown(
          request,
          records,
          "Reply unavailable",
          "The retained response chunk is unavailable",
        );
      encoded += payload.preview;
      if (encoded.length > 24000) return unknown(request, records);
      if (payload.next_offset === null) break;
      if (
        typeof payload.next_offset !== "number" ||
        payload.next_offset <= offset ||
        part === 5
      )
        return unknown(request, records);
      offset = payload.next_offset;
    }
    let bytes: Uint8Array;
    try {
      bytes = Uint8Array.from(atob(encoded), (c) => c.charCodeAt(0));
    } catch {
      return unknown(request, records);
    }
    total += bytes.length;
    if (total > 32768) return unknown(request, records);
    buffers.push(bytes);
  }
  if (total !== meta.bytes)
    return unknown(
      request,
      records,
      "Reply unavailable",
      "The retained response is incomplete",
    );
  const bytes = new Uint8Array(total);
  let at = 0;
  for (const b of buffers) {
    bytes.set(b, at);
    at += b.length;
  }
  let raw: unknown;
  try {
    raw = parseEvidence(
      new TextDecoder("utf-8", { fatal: true }).decode(bytes),
    );
  } catch {
    return unknown(request, records);
  }
  if (Array.isArray(digests)) {
    const r = object(raw).result;
    const value =
      request.rpc_method === "getSignatureStatuses" &&
      index >= 0 &&
      Array.isArray(object(r).value)
        ? (object(r).value as Json[])[index]
        : r;
    if (
      typeof digests[index] != "string" ||
      (await digest(value)) !== digests[index]
    )
      return unknown(
        request,
        records,
        "Reply unavailable",
        "The source result could not be verified against the prepared reply",
      );
  }
  return classifyReply(request, records, raw);
}
