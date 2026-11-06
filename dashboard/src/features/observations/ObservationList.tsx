import { createMemo, Show, type Accessor } from "solid-js";
import { useLab } from "../../app/context";
import {
  createRecordPage,
  createSubscription,
} from "../../shared/transport/subscription";
import type { QueryReply } from "../../shared/transport/client";
import { object, short } from "../../shared/lib/format";
import { VirtualList } from "../../shared/ui/VirtualList";
import { Empty, ErrorNotice } from "../../shared/ui/primitives";
import { appliedKinds, groupEpisodes } from "./model/episodes";
export function ObservationList(props: { active: Accessor<boolean> }) {
  const lab = useLab();
  const eligibleKinds = createMemo(() => {
    const selected = lab.route().scope.kind;
    return selected?.length
      ? appliedKinds.filter((kind) => selected.includes(kind))
      : appliedKinds;
  });
  const faults = createRecordPage(lab.api, () => ({
    operation: "timeline",
    scope: { ...lab.route().scope, kind: eligibleKinds() },
    snapshot: lab.route().snapshot,
    active: props.active() && lab.authorized() && eligibleKinds().length > 0,
    limit: 100,
  }));
  const findings = createSubscription<QueryReply>(lab.api, () =>
    props.active() && lab.authorized()
      ? {
          path: "api/investigate",
          body: {
            contract_version: 1,
            request_id: crypto.randomUUID(),
            lab_id: lab.api.labId,
            operation: "investigate.findings",
            arguments: {
              scope: lab.route().scope,
              snapshot_seq: lab.route().snapshot,
              limit: 100,
              order: "desc",
              fields: [],
            },
          },
        }
      : undefined,
  );
  const episodes = createMemo(() => {
    if (!eligibleKinds().length) return [];
    const f = faults.value(),
      r = findings.value();
    const mark =
      f && r
        ? BigInt(f.view.snapshot_seq) < BigInt(r.view.snapshot_seq)
          ? f.view.snapshot_seq
          : r.view.snapshot_seq
        : undefined;
    const rows = (f?.result.items ?? []).filter(
      (x) =>
        mark === undefined ||
        BigInt(String(object(x.event_ref).event_seq)) <= BigInt(mark),
    );
    // Fault applications define the page. Known Findings outside that page must
    // not reintroduce recent episodes while browsing older applications.
    const selectedFindings = (r?.result.items ?? []).filter((finding) => {
      const known =
        /^(tx-response-lost\.|rpc.lookup-view\.|sim.confirmation-delay\.|provider-timeout-burst\.|recent-blockhash\.)/.test(
          String(finding.rule_id),
        );
      return (
        !known ||
        (Array.isArray(finding.supporting_events) &&
          finding.supporting_events.some((s) =>
            rows.some(
              (row) =>
                String(object(object(s).event_ref).event_seq) ===
                String(object(row.event_ref).event_seq),
            ),
          ))
      );
    });
    return groupEpisodes(rows, selectedFindings);
  });
  const states = () => object(findings.value()?.result.analysis).rules;
  return (
    <div class="feature-list">
      <div class="list-caption">
        <span>{episodes().length} episodes in this view</span>
        <span class="live-label">{lab.live() ? "Live" : "Retained"}</span>
      </div>
      <ErrorNotice message={faults.error() || findings.error()} />
      <Show
        when={episodes().length}
        fallback={
          <Empty
            icon="observations"
            title={
              faults.busy()
                ? "Reading observations…"
                : "No fault episodes in this view"
            }
          >
            <p>Enable a Case and use your backend through the RPC URL.</p>
            <Show
              when={
                Array.isArray(states()) &&
                (states() as unknown[]).some(
                  (x) => object(x).state === "unsupported",
                )
              }
            >
              <p>
                Some automatic analysis is unavailable. This does not establish
                absence of faults.
              </p>
            </Show>
          </Empty>
        }
      >
        <VirtualList
          items={episodes()}
          height={84}
          label="Observation episodes"
        >
          {(ep) => (
            <button
              class="observation-row"
              classList={{
                selected:
                  String(
                    lab.route().target?.episode ??
                      lab.route().target?.finding_id,
                  ) === ep.id,
              }}
              onClick={() =>
                lab.select(
                  ep.anchor
                    ? {
                        episode: object(ep.anchor.event_ref).event_seq as
                          number | string,
                      }
                    : { finding_id: ep.id },
                )
              }
            >
              <div class="row-heading">
                <span class="fault-dot" />{" "}
                <strong>
                  {ep.title.length > 85
                    ? ep.title.slice(0, 85) + "…"
                    : ep.title}
                </strong>
              </div>
              <div class="row-meta">
                <time>{ep.at.slice(11, 19) || "Recorded Finding"}</time>
                <span>
                  {ep.signature ? short(ep.signature) : "RPC episode"}
                </span>
              </div>
              <small>
                {ep.findings.length
                  ? `${ep.findings.length} linked Finding${ep.findings.length === 1 ? "" : "s"}`
                  : "Fault applied"}{" "}
                <span class="open-path">Inspect path ›</span>
              </small>
            </button>
          )}
        </VirtualList>
      </Show>
      <footer class="list-footer">
        <Show when={faults.hasNewer()}>
          <button onClick={() => faults.newer()}>Newer episodes</button>
        </Show>
        <Show when={faults.value()?.page.next_cursor}>
          <button onClick={() => faults.older()}>Older episodes</button>
        </Show>
        <Show when={findings.value()?.page.next_cursor}>
          <span class="caption">
            Bounded episode view · exact Findings remain available by bookmark
          </span>
        </Show>
      </footer>
    </div>
  );
}
