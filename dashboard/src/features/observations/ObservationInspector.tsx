import {
  createEffect,
  createMemo,
  onCleanup,
  createSignal,
  For,
  Show,
  type Accessor,
} from "solid-js";
import { useLab } from "../../app/context";
import type { QueryReply, Scope } from "../../shared/transport/client";
import { object, signature, text, type Item } from "../../shared/lib/format";
import { createSubscription } from "../../shared/transport/subscription";
import { Copy, ErrorNotice, Raw } from "../../shared/ui/primitives";
import { episodeFinding, episodeWindow } from "./model/episodes";
import { mergeRecords, observationStory, sequence } from "./model/story";
import { compactObservation } from "./model/compact";
import { createReplies } from "./replies";
import { Records } from "../inspector/Records";
import { readView, saveView } from "./viewState";
export function ObservationInspector(props: {
  anchor?: Item;
  finding?: Item;
  supports: Item[];
  active: Accessor<boolean>;
  related: (scope: Scope | undefined) => void;
}) {
  const lab = useLab();
  const viewKey = JSON.stringify([
    lab.api.labId,
    lab.route().target,
    lab.route().scope,
  ]);
  const cached = readView(viewKey);
  let element!: HTMLDivElement,
    restored = false,
    paint: number | undefined;
  const [expanded, setExpanded] = createSignal(
      new Set<string>(cached?.expanded ?? []),
    ),
    [technical, setTechnical] = createSignal(cached?.technical ?? false);
  const sig = createMemo(() =>
    props.anchor
      ? (signature(props.anchor) ?? props.supports.map(signature).find(Boolean))
      : text(object(props.finding?.scope).signature, "") ||
        props.supports.map(signature).find(Boolean),
  );
  const original = () =>
    text(
      props.anchor?.request_id,
      text(
        object(props.finding?.scope).accepted_request_id,
        text(object(props.finding?.scope).request_id, ""),
      ),
    );
  const selection = createMemo<Scope>(() => {
    const scope = { ...lab.route().scope };
    delete scope.finding_id;
    delete scope.event_refs;
    const registration = props.supports.find(
      (r) => r.kind === "rpc.registered" && r.request_id === original(),
    );
    const from = object(registration?.event_ref).event_seq;
    if (
      from !== undefined &&
      (scope.from_seq === undefined ||
        BigInt(String(from)) > BigInt(scope.from_seq))
    )
      scope.from_seq = from as number | string;
    if (sig()) scope.signature = [sig()!];
    else if (original()) {
      const request = props.supports.find(
        (r) => r.kind === "rpc.registered" && r.request_id === original(),
      );
      if (
        props.supports.some((r) => r.kind === "rpc.dispatch_prevented") &&
        typeof request?.rpc_method === "string"
      )
        scope.rpc_method = [request.rpc_method];
      else scope.request_id = [original()];
    }
    return scope;
  });
  const history = createSubscription<QueryReply>(lab.api, () =>
    props.active() && (sig() || original())
      ? {
          path: "api/investigate",
          body: {
            contract_version: 1,
            request_id: crypto.randomUUID(),
            lab_id: lab.api.labId,
            operation: "investigate.evidence",
            arguments: {
              scope: selection(),
              snapshot_seq: lab.route().snapshot,
              limit: 200,
              order: "asc",
              fields: [],
            },
          },
        }
      : undefined,
  );
  const watermark = () => {
    const a = history.value()?.view.snapshot_seq,
      b = lab.snapshot();
    return a !== undefined && b !== undefined
      ? BigInt(a) < BigInt(b)
        ? a
        : b
      : (a ?? b ?? 0);
  };
  const full = createMemo(() =>
    mergeRecords(
      props.supports,
      props.anchor ? [props.anchor] : [],
      history.value()?.result.items ?? [],
    ).filter((r) => sequence(r) <= BigInt(watermark())),
  );
  const window = createMemo(() =>
    props.anchor
      ? episodeWindow(full(), props.anchor)
      : { records: full(), next: undefined },
  );
  const finding = createMemo(
    () => props.finding ?? episodeFinding(props.anchor!, full()),
  );
  const restricted = () => {
    const s = lab.route().scope;
    return !!(s.kind?.length || s.rpc_method?.length || s.request_id?.length);
  };
  const complete = () => {
    const r = history.value();
    return (
      !!sig() &&
      !!r &&
      !r.page.next_cursor &&
      !r.page.truncated &&
      r.view.selection_complete &&
      !restricted() &&
      !r.view.evidence_gaps.some(
        (g) => g.code !== "application_coverage_unknown",
      )
    );
  };
  const story = createMemo(() =>
    observationStory(finding(), window().records, complete(), watermark()),
  );
  const replies = createReplies(lab.api, () => ({
    records: window().records,
    signature: sig(),
    snapshot: watermark(),
    active: props.active(),
    selection: JSON.stringify([
      lab.route().target,
      selection(),
      lab.route().snapshot,
    ]),
    epoch: history.value(),
  }));
  const compact = createMemo(() => {
    const c = compactObservation(
      finding(),
      story(),
      window().records,
      replies.items(),
    );
    if (
      props.anchor &&
      !c.timeline.some((row) =>
        row.records.some((r) => sequence(r) === sequence(props.anchor!)),
      )
    )
      c.timeline.push({
        id: `applied:${sequence(props.anchor)}`,
        label: text(finding().statement, "Fault applied"),
        time: "0 s",
        icon: "⏸",
        tone: "injected",
        at: sequence(props.anchor),
        records: [props.anchor],
      });
    c.timeline.sort((a, b) => (a.at < b.at ? -1 : a.at > b.at ? 1 : 0));
    return c;
  });
  const linked = createSubscription<QueryReply>(lab.api, () =>
    props.active() && props.anchor
      ? {
          path: "api/investigate",
          body: {
            contract_version: 1,
            request_id: crypto.randomUUID(),
            lab_id: lab.api.labId,
            operation: "investigate.findings",
            arguments: {
              scope: sig() ? { signature: [sig()!] } : {},
              snapshot_seq: lab.route().snapshot,
              limit: 100,
              order: "desc",
              fields: [],
            },
          },
        }
      : undefined,
  );
  const findings = () =>
    props.finding
      ? [props.finding]
      : (linked.value()?.result.items ?? []).filter(
          (f) =>
            Array.isArray(f.supporting_events) &&
            f.supporting_events.some(
              (s) =>
                String(object(object(s).event_ref).event_seq) ===
                  String(sequence(props.anchor!)) &&
                object(object(s).event_ref).lab_id ===
                  object(props.anchor!.event_ref).lab_id,
            ),
        );
  function setStep(id: string, open: boolean) {
    setExpanded((prev) => {
      if (prev.has(id) === open) return prev;
      const next = new Set(prev);
      if (open) next.add(id);
      else next.delete(id);
      return next;
    });
  }
  createEffect(() => {
    if (!history.value() || restored) return;
    restored = true;
    paint = requestAnimationFrame(() => {
      const container = element.closest<HTMLElement>(
        ".widget-body,.journal-inspector",
      );
      if (container) container.scrollTop = cached?.scroll ?? 0;
    });
  });
  onCleanup(() => {
    if (paint !== undefined) cancelAnimationFrame(paint);
    saveView(viewKey, {
      expanded: [...expanded()],
      technical: technical(),
      scroll:
        element?.closest<HTMLElement>(".widget-body,.journal-inspector")
          ?.scrollTop ?? 0,
    });
  });
  return (
    <div class="observation-inspector" ref={element}>
      <div
        class={`observed-result tone-${compact().result?.tone ?? "neutral"}`}
      >
        <span class="eyeline">Observed RPC path</span>
        <h2>{compact().headline}</h2>
        <p class="counts" aria-label="Observed RPC actions">
          {history.value() ? compact().counts : "Reading observed actions…"}
        </p>
      </div>
      <div class="history-bound">
        Selected history only · snapshot {watermark()}
        <Show when={!complete()}> · partial evidence</Show>
        <Show when={window().next}> · before next fault application</Show>
        <Show when={replies.loading()}> · reading replies</Show>
      </div>
      <ErrorNotice message={history.error()} />
      <ol class="path" aria-label="What happened">
        <For each={compact().timeline}>
          {(row) => (
            <li class={`tone-${row.tone}`}>
              <time>{row.time}</time>
              <span class="path-icon" aria-hidden="true">
                {row.icon}
              </span>
              <details
                open={expanded().has(row.id)}
                onToggle={(e) => setStep(row.id, e.currentTarget.open)}
              >
                <summary>
                  {row.label}
                  <Show when={row.tone === "injected"}>
                    <small class="fault-tag">Ronchi fault</small>
                  </Show>
                </summary>
                <div class="step-detail">
                  <Show when={row.note}>
                    <p>{row.note}</p>
                  </Show>
                  <For each={row.records}>
                    {(record) => (
                      <button
                        class="source-link"
                        onClick={() =>
                          lab.select({
                            event_seq: object(record.event_ref).event_seq as
                              number | string,
                            request_id:
                              text(record.request_id, "") || undefined,
                          })
                        }
                      >
                        Record #{text(object(record.event_ref).event_seq)}
                      </button>
                    )}
                  </For>
                  <For
                    each={replies
                      .items()
                      .filter((r) =>
                        row.records.some(
                          (record) =>
                            record.request_id === r.request.request_id,
                        ),
                      )}
                  >
                    {(reply) => (
                      <>
                        <p class={`tone-${reply.tone}`}>{reply.label}</p>
                        <Raw
                          value={{
                            request_id: reply.request.request_id,
                            signature: sig(),
                            reply: reply.raw ?? null,
                            reason: reply.reason,
                          }}
                          label="Full retained RPC reply"
                        />
                      </>
                    )}
                  </For>
                  <button
                    class="subtle"
                    onClick={() =>
                      props.related(
                        sig()
                          ? { signature: [sig()!] }
                          : original()
                            ? { request_id: [original()] }
                            : undefined,
                      )
                    }
                  >
                    Highlight related traffic
                  </button>
                </div>
              </details>
            </li>
          )}
        </For>
      </ol>
      <Show when={!compact().timeline.length}>
        <p>Result unknown.</p>
      </Show>
      <Show when={compact().followUp}>
        <section
          class="follow-up-notice"
          aria-label="Activity after response loss"
          aria-live="polite"
          aria-atomic="true"
        >
          <span class="follow-up-icon" aria-hidden="true">
            ◷
          </span>
          <div>
            <h3>After response loss</h3>
            <p>{compact().followUp}</p>
          </div>
        </section>
      </Show>
      <button
        class="technical-toggle"
        aria-label="Technical details"
        aria-expanded={technical()}
        onClick={() => setTechnical((v) => !v)}
      >
        Technical details <span>{technical() ? "−" : "+"}</span>
      </button>
      <Show when={technical()}>
        <section class="technical">
          <Show when={sig()}>
            <div class="signature">
              <code>{sig()}</code>
              <Copy value={sig()!} label="Copy signature" iconOnly />
            </div>
          </Show>
          <p>{story().outcome}</p>
          <p>{story().unknown}</p>
          <p class="caption">
            A completed server write does not prove client receipt. RPC finality
            does not establish business completion. Simulated state does not
            establish program execution.
          </p>
          <Show when={replies.omitted() || compact().omittedGroups}>
            <p class="caption">
              Additional activity is available in the paginated technical
              history. Automatic previews inspect at most 16 recent lookups.
            </p>
          </Show>
          <For each={findings()}>
            {(f) => <Raw value={f} label={`Finding · ${text(f.rule_id)}`} />}
          </For>
          <Raw
            value={props.anchor ?? props.finding}
            label="Episode anchor and identifiers"
          />
          <Records
            records={window().records}
            scope={selection()}
            snapshot={watermark()}
            active={() => props.active() && technical()}
            more={!!history.value()?.page.next_cursor}
          />
        </section>
      </Show>
    </div>
  );
}
