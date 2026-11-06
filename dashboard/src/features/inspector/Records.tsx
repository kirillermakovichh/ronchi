import {
  createEffect,
  createSignal,
  For,
  onCleanup,
  Show,
  type Accessor,
} from "solid-js";
import { useLab } from "../../app/context";
import type { QueryReply, Scope } from "../../shared/transport/client";
import {
  object,
  text,
  eventTitle,
  sourceLabel,
  type Item,
} from "../../shared/lib/format";
import { ErrorNotice, Fields, Raw } from "../../shared/ui/primitives";
export function Records(props: {
  records: Item[];
  scope: Scope;
  snapshot: number | string;
  active: Accessor<boolean>;
  more?: boolean;
}) {
  const lab = useLab(),
    [page, setPage] = createSignal<QueryReply>(),
    [raw, setRaw] = createSignal<QueryReply>(),
    [error, setError] = createSignal(""),
    [busy, setBusy] = createSignal(false);
  let pending: AbortController | undefined;
  createEffect(() => {
    if (!props.active()) pending?.abort();
  });
  onCleanup(() => pending?.abort());
  const items = () => page()?.result.items ?? props.records;
  async function readPayload(item: Item, offset = 0) {
    if (busy()) return;
    pending?.abort();
    const controller = new AbortController();
    pending = controller;
    setBusy(true);
    setError("");
    try {
      const result = await lab.api.query(
        "evidence",
        {
          event_refs: [
            item.event_ref as { lab_id: string; event_seq: number | string },
          ],
        },
        props.snapshot,
        undefined,
        { include_payload: true, payload_offset: offset },
        controller.signal,
      );
      if (!controller.signal.aborted) setRaw(result);
    } catch (e) {
      if (!controller.signal.aborted) setError(String(e));
    } finally {
      if (!controller.signal.aborted) setBusy(false);
    }
  }
  async function next() {
    const last = items().at(-1);
    if (!last) return;
    pending?.abort();
    const c = new AbortController();
    pending = c;
    setBusy(true);
    try {
      const result = await lab.api.query(
        "evidence",
        {
          ...props.scope,
          from_seq: (
            BigInt(String(object(last.event_ref).event_seq)) + 1n
          ).toString(),
        },
        props.snapshot,
        undefined,
        { limit: 100, order: "asc" },
        c.signal,
      );
      if (!c.signal.aborted) setPage(result);
    } catch (e) {
      if (!c.signal.aborted) setError(String(e));
    } finally {
      if (!c.signal.aborted) setBusy(false);
    }
  }
  const payload = () => object(raw()?.result.items?.[0]?.payload);
  return (
    <section class="records" aria-label="Supporting evidence">
      <div class="section-line">
        <h3>Supporting evidence</h3>
        <span class="caption">{items().length} records in this page</span>
      </div>
      <For each={items()}>
        {(item) => (
          <article class="record">
            <details>
              <summary>
                <span class="record-seq">
                  #{text(object(item.event_ref).event_seq)}
                </span>{" "}
                {eventTitle(item)}
              </summary>
              <p class="caption">
                {sourceLabel(item.source)} · {text(item.recorded_at)}
              </p>
              <Fields value={item.data} />
              <Raw value={item} label="Exact record JSON" />
              <Show when={item.payload_status === "available"}>
                <button
                  disabled={busy()}
                  onClick={() => void readPayload(item)}
                >
                  Load payload #{text(object(item.event_ref).event_seq)}
                </button>
              </Show>
            </details>
          </article>
        )}
      </For>
      <div class="actions">
        <Show when={page()?.page.next_cursor ?? props.more}>
          <button disabled={busy()} onClick={() => void next()}>
            Next evidence page
          </button>
        </Show>
        <Show when={page()}>
          <button onClick={() => setPage(undefined)}>
            First evidence page
          </button>
        </Show>
        <button
          disabled={busy() || !items().length}
          onClick={() =>
            void readPayload(
              items().find((i) => i.payload_status === "available") ??
                items()[0],
            )
          }
        >
          Load raw payload preview
        </button>
        <Show when={busy()}>
          <button
            onClick={() => {
              pending?.abort();
              setBusy(false);
            }}
          >
            Cancel read
          </button>
        </Show>
      </div>
      <ErrorNotice message={error()} />
      <Show when={raw()}>
        <section class="payload" aria-label="Raw payload preview">
          <h3>
            Retained payload · record #
            {text(object(raw()?.result.items?.[0]?.event_ref).event_seq)}
          </h3>
          <Show
            when={raw()?.result.items?.[0]?.payload}
            fallback={
              <p>
                Payload is{" "}
                {text(raw()?.result.items?.[0]?.payload_status, "unavailable")}.
                No original wire body is implied.
              </p>
            }
          >
            <p class="caption">
              Byte offset {text(payload().offset)} ·{" "}
              {text(payload().retained_bytes)} retained bytes ·{" "}
              {payload().payload_truncated
                ? "partial preview"
                : "end of retained payload"}
            </p>
            <pre>{text(payload().preview, "")}</pre>
            <Show when={typeof payload().next_offset === "number"}>
              <button
                disabled={busy()}
                onClick={() =>
                  void readPayload(
                    raw()!.result.items![0],
                    payload().next_offset as number,
                  )
                }
              >
                Next payload chunk
              </button>
            </Show>
          </Show>
          <Raw value={raw()?.result} label="Payload response JSON" />
        </section>
      </Show>
    </section>
  );
}
