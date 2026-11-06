import {
  createEffect,
  createMemo,
  createSignal,
  onCleanup,
  type Accessor,
} from "solid-js";
import type { Api, QueryReply, Scope } from "./client";
import type { Sequence } from "../protocol/wire";
export function createSubscription<T>(
  api: Api,
  source: Accessor<{ path: string; body?: unknown } | undefined>,
) {
  const [value, setValue] = createSignal<T>(),
    [error, setError] = createSignal("");
  const selected = createMemo(source, undefined, {
    equals: (a, b) =>
      JSON.stringify(
        a && {
          ...a,
          body:
            a.body && typeof a.body === "object"
              ? { ...a.body, request_id: undefined }
              : a.body,
        },
      ) ===
      JSON.stringify(
        b && {
          ...b,
          body:
            b.body && typeof b.body === "object"
              ? { ...b.body, request_id: undefined }
              : b.body,
        },
      ),
  });
  let lastSelection = "";
  createEffect(() => {
    const request = selected();
    if (!request) return;
    const identity = JSON.stringify({
      ...request,
      body:
        request.body && typeof request.body === "object"
          ? { ...request.body, request_id: undefined }
          : request.body,
    });
    if (lastSelection && identity !== lastSelection) setValue(undefined);
    lastSelection = identity;
    setError("");
    onCleanup(
      api.subscribe<T>(
        request.path,
        request.body,
        (result) => {
          setValue(() => result);
          setError("");
        },
        (e) => setError(String(e)),
      ),
    );
  });
  return { value, error };
}
export function createRecordPage(
  api: Api,
  options: Accessor<{
    operation: string;
    scope: Scope;
    snapshot?: Sequence;
    active: boolean;
    limit?: number;
  }>,
) {
  const [value, setValue] = createSignal<QueryReply>(),
    [error, setError] = createSignal(""),
    [busy, setBusy] = createSignal(false),
    [anchors, setAnchors] = createSignal<(Sequence | undefined)[]>([undefined]);
  const selected = createMemo(options, undefined, {
    equals: (a, b) => JSON.stringify(a) === JSON.stringify(b),
  });
  let selection = "";
  createEffect(() => {
    const o = selected(),
      key = JSON.stringify([o.operation, o.scope]);
    if (key !== selection) {
      selection = key;
      setValue(undefined);
      setError("");
      setAnchors([undefined]);
    }
  });
  createEffect(() => {
    const o = selected(),
      anchor = anchors().at(-1);
    if (!o.active) return;
    setBusy(true);
    const scope = { ...o.scope };
    if (
      anchor !== undefined &&
      (scope.to_seq === undefined || BigInt(anchor) < BigInt(scope.to_seq))
    )
      scope.to_seq = anchor;
    onCleanup(
      api.subscribeQuery(
        o.operation,
        scope,
        o.snapshot,
        { order: "desc", limit: o.limit ?? 100 },
        (r) => {
          setValue(r);
          setError("");
          setBusy(false);
        },
        (e) => {
          setError(String(e));
          setBusy(false);
        },
      ),
    );
  });
  return {
    value,
    error,
    busy,
    hasNewer: () => anchors().length > 1,
    older() {
      const rows = value()?.result.items ?? [],
        seq = rows.at(-1)?.event_ref;
      const n =
        seq && typeof seq === "object" && !Array.isArray(seq)
          ? seq.event_seq
          : undefined;
      if (!busy() && n !== undefined && BigInt(String(n)) > 1n)
        setAnchors((a) => [
          ...a.slice(-63),
          (BigInt(String(n)) - 1n).toString(),
        ]);
    },
    newer() {
      setAnchors((a) => (a.length > 1 ? a.slice(0, -1) : a));
    },
    latest() {
      setAnchors([undefined]);
    },
  };
}
