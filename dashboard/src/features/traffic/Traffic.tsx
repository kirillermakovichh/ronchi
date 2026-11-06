import { createMemo, createSignal, Show, type Accessor } from "solid-js";
import { useLab } from "../../app/context";
import type { Scope } from "../../shared/transport/client";
import { createRecordPage } from "../../shared/transport/subscription";
import {
  boundaryResult,
  duration,
  eventTitle,
  object,
  short,
  signature,
  text,
  type Item,
} from "../../shared/lib/format";
import { VirtualList } from "../../shared/ui/VirtualList";
import { Empty, ErrorNotice } from "../../shared/ui/primitives";
export function Traffic(props: {
  active: Accessor<boolean>;
  journal?: boolean;
  related?: Accessor<Scope | undefined>;
}) {
  const lab = useLab(),
    [method, setMethod] = createSignal(""),
    [only, setOnly] = createSignal(false);
  const scope = createMemo(() => ({
    ...lab.route().scope,
    ...(only() ? props.related?.() : {}),
    ...(method() ? { rpc_method: [method()] } : {}),
  }));
  const page = createRecordPage(lab.api, () => ({
    operation: props.journal ? "timeline" : "requests",
    scope: scope(),
    snapshot: lab.route().snapshot,
    active: props.active() && lab.authorized(),
    limit: 100,
  }));
  const rows = () => page.value()?.result.items ?? [];
  function related(item: Item) {
    const s = props.related?.();
    return (
      !!s &&
      (s.signature?.includes(signature(item) ?? "") ||
        s.request_id?.includes(text(item.request_id, "")))
    );
  }
  return (
    <div class="feature-list">
      <div class="traffic-toolbar">
        <label>
          <span class="sr-only">RPC method filter</span>
          <select
            aria-label="RPC method filter"
            value={method()}
            onChange={(e) => setMethod(e.currentTarget.value)}
          >
            <option value="">All methods</option>
            <option value="sendTransaction">Submissions</option>
            <option value="getSignatureStatuses">Status lookups</option>
            <option value="getTransaction">Transaction lookups</option>
          </select>
        </label>
        <Show when={props.related?.()}>
          <button
            classList={{ pressed: only() }}
            aria-pressed={only()}
            onClick={() => setOnly((v) => !v)}
          >
            Only related
          </button>
        </Show>
        <span class="caption">
          {rows().length} {props.journal ? "records" : "requests"}
        </span>
      </div>
      <ErrorNotice message={page.error()} />
      <Show
        when={rows().length}
        fallback={
          <Empty
            icon="traffic"
            title={page.busy() ? "Reading traffic…" : "No matching traffic"}
          >
            Connect your backend to the RPC URL, or clear the filters.
          </Empty>
        }
      >
        <Show when={!props.journal}>
          <div class="traffic-columns">
            <span>Time</span>
            <span>Method / signature</span>
            <span>RPC boundary</span>
          </div>
        </Show>
        <VirtualList
          items={rows()}
          height={props.journal ? 58 : 66}
          label={props.journal ? "Journal records" : "RPC requests"}
        >
          {(item) => {
            const seq = () =>
              object(item.event_ref).event_seq as number | string;
            const result = () => boundaryResult(item);
            return (
              <button
                class="traffic-row"
                classList={{
                  selected:
                    String(lab.route().target?.event_seq) === String(seq()),
                  related: related(item),
                  fault: result().fault,
                }}
                onClick={() =>
                  lab.select({
                    event_seq: seq(),
                    request_id: props.journal
                      ? undefined
                      : text(item.request_id, "") || undefined,
                  })
                }
              >
                <Show
                  when={props.journal}
                  fallback={
                    <>
                      <time>
                        {text(item.recorded_at, "").slice(11, 19) || "—"}
                      </time>
                      <span class="method">
                        <strong>
                          {text(item.rpc_method, "Unknown method")}
                        </strong>
                        <small>
                          {signature(item)
                            ? short(signature(item)!)
                            : text(item.request_id, "No identity")}
                        </small>
                      </span>
                      <span class="boundary">
                        <span>{result().label}</span>
                        <small>
                          {duration(item) ?? "Client receipt unknown"}
                        </small>
                      </span>
                    </>
                  }
                >
                  <span class="record-seq">#{seq()}</span>
                  <span>
                    <strong>{eventTitle(item)}</strong>
                    <small>{text(item.kind)}</small>
                  </span>
                </Show>
              </button>
            );
          }}
        </VirtualList>
      </Show>
      <footer class="list-footer">
        <Show when={page.hasNewer()}>
          <button onClick={() => page.newer()}>Newer records</button>
          <button onClick={() => page.latest()}>Latest traffic</button>
        </Show>
        <Show when={page.value()?.page.next_cursor}>
          <button disabled={page.busy()} onClick={() => page.older()}>
            Older {props.journal ? "events" : "requests"}
          </button>
        </Show>
        <Show when={page.value()?.page.truncated}>
          <span class="caption">Bounded page</span>
        </Show>
      </footer>
    </div>
  );
}
