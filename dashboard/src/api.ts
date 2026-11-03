import { requestJson, type Sequence } from "./wire";
import { parseEvidence } from "./json";
export type Json =
  null | boolean | number | string | Json[] | { [key: string]: Json };
export interface Scope {
  signature?: string[];
  rpc_method?: string[];
  kind?: string[];
  case_activation_id?: Sequence[];
  from_seq?: Sequence;
  to_seq?: Sequence;
  since?: string;
  until?: string;
  finding_id?: string;
  event_refs?: { lab_id: string; event_seq: Sequence }[];
}
export interface QueryReply {
  contract_version: number;
  lab_id: string;
  result: {
    items?: Record<string, Json>[];
    findings?: Record<string, Json>[];
    [key: string]: Json | undefined;
  };
  view: {
    snapshot_seq: Sequence;
    journal_state: string;
    observation_boundary: string[];
    evidence_gaps: { code: string; message: string }[];
    selection_complete: boolean;
  };
  page: {
    next_cursor: string | null;
    truncated: boolean;
    reason: string | null;
  };
}
export interface Status {
  lab_id: string;
  name: string;
  state: string;
  health: string;
  journal_state: string;
  rpc_address: string | null;
  active_case: Json;
  configuration: {
    mode: string;
    checks?: { sources: { check_id: string; version: number }[] };
  };
  snapshot_seq: Sequence;
}
export class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message);
  }
}
export class Api {
  constructor(readonly labId: string) {}
  async call<T>(path: string, body: unknown, signal?: AbortSignal): Promise<T> {
    const response = await fetch(
      `/ui/${encodeURIComponent(this.labId)}/${path}`,
      {
        method: body === undefined ? "GET" : "POST",
        credentials: "same-origin",
        headers:
          body === undefined ? {} : { "content-type": "application/json" },
        body: body === undefined ? undefined : requestJson(body),
        signal,
      },
    );
    const reader = response.body?.getReader();
    const chunks: Uint8Array[] = [];
    let size = 0;
    if (reader) {
      try {
        while (true) {
          const { done, value } = await reader.read();
          if (done) break;
          size += value.byteLength;
          if (size > 262144) {
            await reader.cancel();
            throw new ApiError(
              response.status,
              "Response exceeds the browser limit. Use a smaller selection.",
            );
          }
          chunks.push(value);
        }
      } finally {
        reader.releaseLock();
      }
    }
    const bytes = new Uint8Array(size);
    let offset = 0;
    for (const chunk of chunks) {
      bytes.set(chunk, offset);
      offset += chunk.byteLength;
    }
    const text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
    let value: unknown;
    try {
      value = parseEvidence(text);
    } catch {
      throw new ApiError(
        response.status,
        `HTTP ${response.status}. Open a new dashboard launch link if the session expired.`,
      );
    }
    if (!response.ok) {
      const error = (
        value as { error?: { code?:string;message?: string; effects?: string } }
      ).error;
      throw new ApiError(
        response.status,
        `${error?.code??"operation_error"}: ${error?.message ?? `HTTP ${response.status}`} · effects: ${error?.effects ?? "unknown"}`,
      );
    }
    return value as T;
  }
  query(
    operation: string,
    scope: Scope,
    snapshot?: Sequence,
    cursor?: string | null,
    extra: Record<string, unknown> = {},
    signal?: AbortSignal,
  ) {
    return this.call<QueryReply>(
      "api/investigate",
      {
        contract_version: 1,
        request_id: crypto.randomUUID(),
        lab_id: this.labId,
        operation: `investigate.${operation}`,
        arguments: {
          scope,
          limit: 50,
          snapshot_seq: snapshot,
          cursor,
          fields: operation === "summary" ? ["counts", "traffic_state"] : [],
          ...extra,
        },
      },
      signal,
    );
  }
}
export async function bootstrap(api: Api) {
  const ticket = new URLSearchParams(location.hash.slice(1)).get("ticket");
  if (ticket) {
    history.replaceState(null, "", location.pathname + location.search);
    await api.call("session", { ticket });
  }
}
