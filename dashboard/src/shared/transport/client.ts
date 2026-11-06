import { requestJson, type Sequence } from "../protocol/wire";
import { parseEvidence } from "../protocol/json";
export type Json =
  null | boolean | number | string | Json[] | { [key: string]: Json };
export interface Scope {
  signature?: string[];
  rpc_method?: string[];
  request_id?: string[];
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
export interface RpcTraffic {
  requests: Sequence;
  first_elapsed_ns: Sequence | null;
  last_rpc_elapsed_ns: Sequence | null;
  active_start_elapsed_ns: Sequence;
  active_completed_ns: Sequence;
  observed_elapsed_ns: Sequence;
  end_known: boolean;
  running: boolean;
}
export interface Status {
  lab_id: string;
  name: string;
  state: string;
  health: string;
  journal_state: string;
  rpc_address: string | null;
  active_case: Json;
  case_activity?: { policy_revision: Sequence; applied_records: Sequence };
  configuration: {
    mode: string;
    project?: string;
  };
  snapshot_seq: Sequence;
  rpc_traffic?: RpcTraffic;
}
export class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message);
  }
}
type Subscription<T = unknown> = {
  path: string;
  body: unknown;
  value: (value: T) => void;
  error: (error: Error) => void;
};
export class Api {
  private socket?: WebSocket;
  private ready = false;
  private updates = new Map<string, unknown>();
  private paint?: number;
  private flushUpdates() {
    if (this.paint !== undefined) return;
    const schedule =
      window.requestAnimationFrame?.bind(window) ??
      ((callback: FrameRequestCallback) =>
        window.setTimeout(() => callback(performance.now()), 0));
    this.paint = schedule(() => {
      this.paint = undefined;
      const updates = this.updates;
      this.updates = new Map();
      for (const [id, value] of updates)
        this.subscriptions.get(id)?.value(value);
    });
  }
  private stopped = false;
  private retries = 0;
  private timer?: ReturnType<typeof setTimeout>;
  private subscriptions = new Map<string, Subscription<any>>();
  private pending = new Map<
    string,
    {
      path: string;
      body: unknown;
      sent: boolean;
      resolve: (value: any) => void;
      reject: (error: Error) => void;
      cleanup: () => void;
    }
  >();
  constructor(readonly labId: string) {}
  private visibility = () =>
    this.send({ type: "visibility", visible: !document.hidden });
  start() {
    this.stopped = false;
    document.removeEventListener("visibilitychange", this.visibility);
    document.addEventListener("visibilitychange", this.visibility);
    this.connect();
  }
  dispose() {
    this.stopped = true;
    clearTimeout(this.timer);
    document.removeEventListener("visibilitychange", this.visibility);
    this.socket?.close();
    this.socket = undefined;
    this.ready = false;
    this.updates.clear();
    if (this.paint !== undefined)
      (
        window.cancelAnimationFrame?.bind(window) ??
        window.clearTimeout.bind(window)
      )(this.paint);
    this.paint = undefined;
    this.rejectPending();
  }
  private send(value: unknown) {
    if (!this.ready || this.socket?.readyState !== WebSocket.OPEN) return;
    if (this.socket.bufferedAmount > 512 * 1024) {
      this.socket.close();
      return;
    }
    this.socket.send(requestJson(value));
  }
  private rejectPending() {
    for (const p of this.pending.values()) {
      p.cleanup();
      p.reject(
        new ApiError(
          0,
          `Connection lost. Operation effects: ${p.sent ? "unknown" : "none"}. Inspect current state before repeating a command.`,
        ),
      );
    }
    this.pending.clear();
  }
  private connect() {
    if (this.stopped || this.socket) return;
    const socket = new WebSocket(
      `${location.protocol === "https:" ? "wss:" : "ws:"}//${location.host}/ui/${encodeURIComponent(this.labId)}/socket`,
    );
    this.socket = socket;
    socket.onmessage = (event) => {
      if (this.socket !== socket || typeof event.data !== "string") return;
      if (new TextEncoder().encode(event.data).length > 320 * 1024) {
        socket.close();
        return;
      }
      let frame: any;
      try {
        frame = parseEvidence(event.data);
      } catch {
        socket.close();
        return;
      }
      if (frame.type === "ready") {
        if (frame.lab_id !== this.labId || frame.contract_version !== 1) {
          socket.close();
          return;
        }
        this.ready = true;
        this.retries = 0;
        this.visibility();
        for (const [id, s] of this.subscriptions)
          this.send({ type: "subscribe", id, path: s.path, body: s.body });
        for (const [id, p] of this.pending) {
          p.sent = true;
          this.send({ type: "call", id, path: p.path, body: p.body });
        }
        return;
      }
      if (frame.type !== "reply" && frame.type !== "update") return;
      const error =
        frame.status >= 200 && frame.status < 300
          ? undefined
          : this.failure(frame.status, frame.value);
      if (frame.type === "update") {
        const s = this.subscriptions.get(frame.id);
        if (s) {
          if (error) {
            this.updates.delete(frame.id);
            s.error(error);
          } else {
            this.updates.set(frame.id, frame.value);
            this.flushUpdates();
          }
        }
      } else {
        const p = this.pending.get(frame.id);
        if (!p) return;
        this.pending.delete(frame.id);
        p.cleanup();
        if (error) p.reject(error);
        else p.resolve(frame.value);
      }
    };
    socket.onclose = () => {
      if (this.socket !== socket) return;
      this.socket = undefined;
      this.ready = false;
      this.updates.clear();
      this.rejectPending();
      if (this.stopped) return;
      for (const s of this.subscriptions.values())
        s.error(
          new ApiError(
            0,
            "Connection lost. Reconnecting; previous evidence remains available.",
          ),
        );
      this.timer = setTimeout(
        () => this.connect(),
        Math.min(1000 * 2 ** this.retries++, 10000),
      );
    };
    socket.onerror = () => socket.close();
  }
  private failure(status: number, value: any) {
    const e = value?.error;
    return new ApiError(
      status,
      `${e?.code ?? e?.category ?? "operation_error"}: ${e?.path ? e.path + ": " : ""}${e?.message ?? "Browser operation failed"} · effects: ${e?.effects ?? (e?.category ? "none" : "unknown")}`,
    );
  }
  call<T>(path: string, body: unknown, signal?: AbortSignal): Promise<T> {
    if (signal?.aborted)
      return Promise.reject(new DOMException("Aborted", "AbortError"));
    if (this.pending.size >= 4)
      return Promise.reject(
        new ApiError(429, "Browser command capacity reached · effects: none"),
      );
    if (new TextEncoder().encode(requestJson(body ?? null)).length > 400 * 1024)
      return Promise.reject(
        new ApiError(413, "Request exceeds browser limit · effects: none"),
      );
    this.start();
    return new Promise<T>((resolve, reject) => {
      const id = crypto.randomUUID();
      const abort = () => {
        const p = this.pending.get(id);
        if (!p) return;
        this.pending.delete(id);
        p.cleanup();
        this.send({ type: "cancel", id });
        reject(
          new ApiError(
            0,
            `Operation cancelled · effects: ${p.sent ? "unknown" : "none"}`,
          ),
        );
      };
      const timer = setTimeout(abort, 30000);
      const cleanup = () => {
        clearTimeout(timer);
        signal?.removeEventListener("abort", abort);
      };
      this.pending.set(id, {
        path,
        body: body ?? null,
        sent: this.ready,
        resolve,
        reject,
        cleanup,
      });
      signal?.addEventListener("abort", abort, { once: true });
      this.send({ type: "call", id, path, body: body ?? null });
    });
  }
  subscribe<T>(
    path: string,
    body: unknown,
    value: (value: T) => void,
    error: (error: Error) => void,
  ) {
    const id = crypto.randomUUID();
    if (this.subscriptions.size >= 16) {
      error(new ApiError(429, "Browser subscription capacity reached"));
      return () => {};
    }
    this.subscriptions.set(id, { path, body: body ?? null, value, error });
    this.start();
    this.send({ type: "subscribe", id, path, body: body ?? null });
    return () => {
      this.subscriptions.delete(id);
      this.updates.delete(id);
      this.send({ type: "unsubscribe", id });
    };
  }
  private request(
    operation: string,
    scope: Scope,
    snapshot?: Sequence,
    cursor?: string | null,
    extra: Record<string, unknown> = {},
  ) {
    return {
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
    };
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
      this.request(operation, scope, snapshot, cursor, extra),
      signal,
    );
  }
  subscribeQuery(
    operation: string,
    scope: Scope,
    snapshot: Sequence | undefined,
    extra: Record<string, unknown>,
    value: (value: QueryReply) => void,
    error: (error: Error) => void,
  ) {
    return this.subscribe<QueryReply>(
      "api/investigate",
      this.request(operation, scope, snapshot, undefined, extra),
      value,
      error,
    );
  }
}
export async function bootstrap(api: Api) {
  const ticket = new URLSearchParams(location.hash.slice(1)).get("ticket");
  if (ticket) {
    history.replaceState(null, "", location.pathname + location.search);
    const response = await fetch(
      `/ui/${encodeURIComponent(api.labId)}/session`,
      {
        method: "POST",
        credentials: "same-origin",
        headers: { "content-type": "application/json" },
        body: requestJson({ ticket }),
      },
    );
    if (!response.ok)
      throw new ApiError(
        response.status,
        "Open this Lab's dashboard link to reconnect.",
      );
  }
  api.start();
}
