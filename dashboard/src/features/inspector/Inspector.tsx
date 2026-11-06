import {
  createEffect,
  createMemo,
  createSignal,
  Show,
  type Accessor,
} from "solid-js";
import { useLab } from "../../app/context";
import type { QueryReply, Scope } from "../../shared/transport/client";
import { createSubscription } from "../../shared/transport/subscription";
import {
  object,
  signature,
  text,
  eventTitle,
  boundaryResult,
} from "../../shared/lib/format";
import {
  Empty,
  ErrorNotice,
  Fields,
  Raw,
  Tabs,
  Copy,
} from "../../shared/ui/primitives";
import { ObservationInspector } from "../observations/ObservationInspector";
import { Records } from "./Records";
export function Inspector(props: {
  active: Accessor<boolean>;
  related: (scope: Scope | undefined) => void;
}) {
  const lab = useLab(),
    key = createMemo(() => JSON.stringify(lab.route().target));
  return (
    <div class="inspector-content">
      <Show
        when={lab.route().target}
        fallback={
          <Empty title="Select an Observation or RPC request">
            Inspect its path, returned results and source evidence here. New
            traffic continues live.
          </Empty>
        }
      >
        <Show keyed when={key()}>
          {(_key) => <Selected active={props.active} related={props.related} />}
        </Show>
      </Show>
    </div>
  );
}
function Selected(props: {
  active: Accessor<boolean>;
  related: (scope: Scope | undefined) => void;
}) {
  const lab = useLab(),
    target = lab.route().target!,
    [tab, setTab] = createSignal("overview");
  const observation = target.episode !== undefined || !!target.finding_id;
  const finding = createSubscription<QueryReply>(lab.api, () =>
    props.active() && target.finding_id
      ? {
          path: "api/investigate",
          body: {
            contract_version: 1,
            lab_id: lab.api.labId,
            request_id: crypto.randomUUID(),
            operation: "investigate.findings",
            arguments: {
              scope: { finding_id: target.finding_id },
              snapshot_seq: lab.route().snapshot,
              limit: 1,
              fields: [],
            },
          },
        }
      : undefined,
  );
  const anchor = createSubscription<QueryReply>(lab.api, () =>
    props.active() &&
    (target.episode !== undefined || target.event_seq !== undefined)
      ? {
          path: "api/investigate",
          body: {
            contract_version: 1,
            lab_id: lab.api.labId,
            request_id: crypto.randomUUID(),
            operation: "investigate.evidence",
            arguments: {
              scope: {
                event_refs: [
                  {
                    lab_id: lab.api.labId,
                    event_seq: target.episode ?? target.event_seq,
                  },
                ],
              },
              snapshot_seq: lab.route().snapshot,
              limit: 1,
              fields: [],
            },
          },
        }
      : undefined,
  );
  const first = () => anchor.value()?.result.items?.[0];
  const scope = createMemo<Scope>(() =>
    target.finding_id
      ? { finding_id: target.finding_id }
      : first()?.request_id &&
          (target.episode !== undefined ||
            target.request_id ||
            (first()?.kind === "rpc.registered" &&
              lab.route().page === "workspace"))
        ? { request_id: [text(first()?.request_id)] }
        : {
            event_refs: [
              {
                lab_id: lab.api.labId,
                event_seq: target.episode ?? target.event_seq!,
              },
            ],
          },
  );
  const evidence = createSubscription<QueryReply>(lab.api, () =>
    props.active()
      ? {
          path: "api/investigate",
          body: {
            contract_version: 1,
            lab_id: lab.api.labId,
            request_id: crypto.randomUUID(),
            operation: "investigate.evidence",
            arguments: {
              scope: scope(),
              snapshot_seq: lab.route().snapshot,
              limit: 200,
              order: "asc",
              fields: [],
            },
          },
        }
      : undefined,
  );
  const records = () => evidence.value()?.result.items ?? [];
  createEffect(() => {
    if (!observation) return;
    const sig =
      signature(first() ?? {}) ??
      text(object(finding.value()?.result.items?.[0]?.scope).signature, "");
    const req = text(first()?.request_id, "");
    props.related(
      sig ? { signature: [sig] } : req ? { request_id: [req] } : undefined,
    );
  });
  const exchange = () =>
    Object.fromEntries(
      [
        ["boundary", "rpc.boundary"],
        ["prepared", "rpc.response_prepared"],
        ["acceptance", "transaction.accepted"],
        ["withholding", "fault.withholding_started"],
        ["dispatch", "rpc.dispatch_prevented"],
      ].map(([name, kind]) => [
        name,
        records().find((r) => r.kind === kind) ?? null,
      ]),
    );
  const result = () => boundaryResult({ ...first(), exchange: exchange() });
  return (
    <>
      <div class="inspector-toolbar">
        <button class="subtle" onClick={() => lab.back()}>
          ← Back to Observation
        </button>
        <span class="caption">
          {lab.live() ? "Live detail" : "Retained detail"}
        </span>
      </div>
      <ErrorNotice
        message={anchor.error() || evidence.error() || finding.error()}
      />
      <Show
        when={observation}
        fallback={
          <>
            <h2 class="request-title">
              {first()
                ? first()!.kind === "rpc.registered"
                  ? text(first()!.rpc_method)
                  : eventTitle(first()!)
                : "Reading evidence…"}
            </h2>
            <Show when={first()}>
              <div class="history-bound">
                Selected history only · snapshot{" "}
                {evidence.value()?.view.snapshot_seq ?? lab.snapshot()}
              </div>
              <Tabs
                items={[
                  ["overview", "Overview"],
                  ["request", "Request"],
                  ["response", "Response"],
                  ["records", "Records"],
                ]}
                value={tab()}
                onChange={setTab}
              />
              <Show when={tab() === "overview"}>
                <div class="observed-result">
                  <h3>{result().label}</h3>
                  <p>Server-side observation · client receipt unknown</p>
                </div>
                <Fields
                  value={{
                    request_id: first()?.request_id,
                    source: first()?.source,
                    mode: first()?.execution_mode,
                    recorded_at: first()?.recorded_at,
                  }}
                />
                <Show when={signature(first()!)}>
                  <div class="signature">
                    <code>{signature(first()!)}</code>
                    <Copy
                      value={signature(first()!)!}
                      label="Copy signature"
                      iconOnly
                    />
                  </div>
                </Show>
              </Show>
              <Show when={tab() === "request"}>
                <p class="caption">
                  Ronchi does not retain the original RPC request body. These
                  are recorded registration facts.
                </p>
                <Fields value={first()?.data} />
                <Raw value={first()} label="Registration metadata JSON" />
              </Show>
              <Show when={tab() === "response"}>
                <h3>{result().label}</h3>
                <h4>Prepared response facts</h4>
                <Fields value={object(exchange().prepared).data} />
                <Raw
                  value={exchange().prepared}
                  label="Prepared response metadata JSON"
                />
                <h4>RPC boundary</h4>
                <Fields value={object(exchange().boundary).data} />
                <p class="caption">
                  A completed server write does not prove client receipt.
                </p>
              </Show>
              <Show when={tab() === "records"}>
                <Records
                  records={records()}
                  scope={scope()}
                  snapshot={
                    evidence.value()?.view.snapshot_seq ?? lab.snapshot() ?? 0
                  }
                  active={props.active}
                  more={!!evidence.value()?.page.next_cursor}
                />
              </Show>
              <Show when={tab() !== "records"}>
                <details class="technical">
                  <summary>Technical details</summary>
                  <Records
                    records={records()}
                    scope={scope()}
                    snapshot={
                      evidence.value()?.view.snapshot_seq ?? lab.snapshot() ?? 0
                    }
                    active={props.active}
                    more={!!evidence.value()?.page.next_cursor}
                  />
                </details>
              </Show>
            </Show>
          </>
        }
      >
        <Show
          when={
            target.finding_id ? finding.value()?.result.items?.[0] : first()
          }
          fallback={
            <Empty title="Reading Observation…">
              If evidence is unavailable, widen the selected history.
            </Empty>
          }
        >
          <ObservationInspector
            anchor={target.episode !== undefined ? first() : undefined}
            finding={
              target.finding_id ? finding.value()?.result.items?.[0] : undefined
            }
            supports={records()}
            active={props.active}
            related={props.related}
          />
        </Show>
      </Show>
    </>
  );
}
