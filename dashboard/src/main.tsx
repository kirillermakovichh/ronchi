import type { Sequence } from "./wire";
import { useEffect, useMemo, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import {
  Api,
  ApiError,
  bootstrap,
  type Json,
  type QueryReply,
  type Scope,
  type Status,
} from "./api";
import { selection, bookmark, type DetailSelection } from "./selection";
import "./style.css";
import { StateNotice } from "./StateNotice";
import { Findings } from "./Findings";
import { Controls } from "./Controls";
const initial = selection(location.search);
function Facts({ value }: { value: unknown }) {
  return <pre>{JSON.stringify(value, null, 2)}</pre>;
}
function App() {
  const labId = decodeURIComponent(location.pathname.split("/")[2] ?? "");
  const api = useMemo(() => new Api(labId), [labId]);
  const [authorized, setAuthorized] = useState(false),
    [error, setError] = useState(initial.error ?? "");
  const [selectionValid, setSelectionValid] = useState(!initial.error);
  const [status, setStatus] = useState<Status>(),
    [scope, setScope] = useState<Scope>(initial.scope),
    [snapshot, setSnapshot] = useState<Sequence | undefined>(initial.snapshot);
  const [committedScope, setCommittedScope] = useState<Scope>(initial.scope);
  const [summary, setSummary] = useState<QueryReply>(),
    [timeline, setTimeline] = useState<QueryReply>(),
    [detail, setDetail] = useState<QueryReply>(),
    [finding, setFinding] = useState<Record<string, Json>>();
  const [detailError, setDetailError] = useState("");
  const [selectedDetail, setSelectedDetail] = useState<
    DetailSelection | undefined
  >(initial.detail);
  const evidenceRequest = useRef<AbortController | undefined>(undefined);
  useEffect(() => () => evidenceRequest.current?.abort(), [api]);
  async function loadEvidence(
    selected: Scope,
    seq: Sequence,
    extra: Record<string, unknown> = {},
  ) {
    evidenceRequest.current?.abort();
    const controller = new AbortController();
    evidenceRequest.current = controller;
    const response = await api.query(
      "evidence",
      selected,
      seq,
      undefined,
      extra,
      controller.signal,
    );
    if (controller.signal.aborted)
      throw new DOMException("Aborted", "AbortError");
    if (!controller.signal.aborted) {
      if (!response.result.items?.length)
        throw new Error(
          "Detail reference is unavailable in the selected Lab and snapshot.",
        );
      setDetail(response);
      setDetailError("");
    }
    return response;
  }
  const [busy, setBusy] = useState(false),
    [refresh, setRefresh] = useState(0);
  useEffect(() => {
    let active = true;
    if (!labId) return;
    bootstrap(api)
      .then(() => {
        if (active) setAuthorized(true);
      })
      .catch((e) => {
        if (active) setError(String(e));
      });
    return () => {
      active = false;
    };
  }, [api]);
  useEffect(() => {
    if (
      !authorized ||
      !selectionValid ||
      !initial.detail ||
      initial.snapshot === undefined
    )
      return;
    const target = initial.detail;
    if (target.finding_id) setFinding({ finding_id: target.finding_id });
    const selected: Scope = target.finding_id
      ? { ...initial.scope, finding_id: target.finding_id }
      : {
          ...initial.scope,
          event_refs: [{ lab_id: labId, event_seq: target.event_seq! }],
        };
    void loadEvidence(selected, initial.snapshot)
      .then(async (response) => {
        if (!target.finding_id) return;
        const controller = evidenceRequest.current;
        const metadata = await api.query(
          "findings",
          {
            ...initial.scope,
            event_refs: response.result.items?.map(
              (item) =>
                item.event_ref as { lab_id: string; event_seq: Sequence },
            ),
          },
          initial.snapshot,
          undefined,
          { limit: 50, fields: ["statement", "guidance"] },
          controller?.signal,
        );
        if (!controller?.signal.aborted)
          setFinding(
            metadata.result.items?.find(
              (item) => item.finding_id === target.finding_id,
            ) ?? { finding_id: target.finding_id },
          );
      })
      .catch((e) => {
        if (!(e instanceof DOMException && e.name === "AbortError"))
          setDetailError(
            `Invalid or unavailable detail bookmark: ${String(e)}`,
          );
      });
  }, [authorized, api]);
  useEffect(() => {
    if (!authorized || !selectionValid) return;
    const controller = new AbortController();
    let timer: ReturnType<typeof setTimeout>;
    async function load() {
      if (document.hidden && snapshot === undefined) {
        timer = setTimeout(load, 2000);
        return;
      }
      try {
        const current = await api.call<Status>(
          "api/status",
          undefined,
          controller.signal,
        );
        if (controller.signal.aborted) return;
        setStatus(current);
        const result = await api.query(
          "summary",
          scope,
          snapshot,
          undefined,
          {},
          controller.signal,
        );
        const events = await api.query(
          "timeline",
          scope,
          result.view.snapshot_seq,
          undefined,
          {},
          controller.signal,
        );
        if (controller.signal.aborted) return;
        setSummary(result);
        setCommittedScope(scope);
        setTimeline(events);
        if (current.state !== "ready" && snapshot === undefined)
          setSnapshot(result.view.snapshot_seq);
        setError("");
      } catch (e) {
        if (!controller.signal.aborted)
          setError(
            e instanceof ApiError
              ? `Operational query error: ${String(e)}. Last pinned evidence is retained on screen.`
              : `Connection lost: ${String(e)}. Last pinned evidence is retained on screen.`,
          );
      }
      if (!controller.signal.aborted && snapshot === undefined)
        timer = setTimeout(load, 2500);
    }
    void load();
    return () => {
      controller.abort();
      clearTimeout(timer);
    };
  }, [api, authorized, scope, snapshot, refresh, selectionValid]);
  useEffect(() => {
    if (!authorized || snapshot === undefined || status?.state !== "ready")
      return;
    const controller = new AbortController();
    let timer: ReturnType<typeof setTimeout>;
    async function update() {
      if (!document.hidden) {
        try {
          const current = await api.call<Status>(
            "api/status",
            undefined,
            controller.signal,
          );
          if (!controller.signal.aborted) setStatus(current);
        } catch (e) {
          if (!controller.signal.aborted)
            setError(
              `Connection lost. Last known Lab status is retained. ${String(e)}`,
            );
        }
      }
      if (!controller.signal.aborted) timer = setTimeout(update, 2500);
    }
    timer = setTimeout(update, 2500);
    return () => {
      controller.abort();
      clearTimeout(timer);
    };
  }, [api, authorized, snapshot, status?.state]);
  const actionPending = useRef(false);
  async function act(work: () => Promise<void>) {
    if (actionPending.current) return;
    actionPending.current = true;
    setBusy(true);
    try {
      await work();
      setError("");
    } catch (e) {
      if (!(e instanceof DOMException && e.name === "AbortError"))
        setError(String(e));
    } finally {
      actionPending.current = false;
      setBusy(false);
    }
  }
  function saveLocation(
    selected: Scope,
    seq?: Sequence,
    target?: DetailSelection,
  ) {
    const url = new URL(bookmark(labId, selected, seq ?? 0, target));
    if (seq === undefined) url.searchParams.delete("snapshot");
    history.replaceState(null, "", url.pathname + url.search);
  }
  async function evidence(item: Record<string, Json>, isFinding = false) {
    const seq = summary?.view.snapshot_seq;
    if (seq === undefined) return;
    setSnapshot(seq);
    setScope(committedScope);
    setFinding(isFinding ? item : undefined);
    const ref = item.event_ref as
      { lab_id: string; event_seq: Sequence } | undefined;
    const selected: Scope = isFinding
      ? { ...committedScope, finding_id: String(item.finding_id) }
      : { event_refs: ref ? [ref] : [] };
    setSelectedDetail(
      isFinding
        ? { finding_id: String(item.finding_id) }
        : { event_seq: ref?.event_seq },
    );
    saveLocation(
      committedScope,
      seq,
      isFinding
        ? { finding_id: String(item.finding_id) }
        : { event_seq: ref?.event_seq },
    );
    await loadEvidence(selected, seq);
  }
  const commandPrefix = status?.configuration.project
    ? `ronchi --project '${status.configuration.project.replaceAll("'", "'\\''")}'`
    : "ronchi";
  return (
    <>
      <header>
        <div className="brand">
          <svg width="44" height="30" viewBox="0 0 44 30" aria-hidden="true">
            {[4, 10, 16, 22, 28].map((y) => (
              <path key={y} d={`M0 ${y} H17 l5 -3 l5 3 H44`} />
            ))}
          </svg>
          <strong>RONCHI</strong>
          <span>Investigation</span>
        </div>
        <div className="live">
          {error.startsWith("Connection") ? "Last known · " : ""}
          {!labId ? "No Lab selected" : (status?.state ?? "Connecting")} ·{" "}
          {status?.health ?? "unknown health"}
        </div>
      </header>
      <main>
        <StateNotice
          labId={labId}
          status={status}
          analysis={summary?.result.analysis}
          error={error}
        />
        <section className="lab-head">
          <div>
            <p className="eyebrow">
              LOCAL LAB / {status?.configuration.mode ?? "unknown mode"}
            </p>
            <h1>{status?.name ?? "Lab"}</h1>
            <button
              className="identity"
              onClick={() => void navigator.clipboard.writeText(labId)}
              title="Copy exact Lab ID"
            >
              {labId || "No Lab selected"}
            </button>
          </div>
          <dl>
            <dt>
              {status?.state === "ready"
                ? "RPC endpoint"
                : "Recorded RPC endpoint (not live)"}
            </dt>
            <dd>{status?.rpc_address ?? "Unavailable"}</dd>
            <dt>Journal / active Case</dt>
            <dd>
              {status?.journal_state ?? "unknown"} /{" "}
              {JSON.stringify(status?.active_case ?? "none")}
            </dd>
          </dl>
        </section>
        {summary && (
          <details>
            <summary>Open this pinned selection after Lab stop</summary>
            <p>
              Start a read-only viewer for the exact Lab. Its new endpoint
              replaces the old live endpoint.
            </p>
            <code>
              {commandPrefix} dashboard --serve --lab-id {labId} --selection '
              {new URL(
                bookmark(
                  labId,
                  committedScope,
                  summary.view.snapshot_seq,
                  selectedDetail,
                ),
              ).search.slice(1)}
              '
            </code>
          </details>
        )}
        {error && (
          <div role="alert" className="error">
            {error}
            <p>
              Use{" "}
              <code>
                {commandPrefix} dashboard --lab-id {labId}
              </code>{" "}
              to print this Lab’s link. A lost connection does not prove that the Lab
              stopped.
            </p>
          </div>
        )}
        {labId && !authorized && (
          <p>
            Open this Lab’s dashboard link to access this Investigation.
          </p>
        )}
        {summary && (
          <section className="summary" aria-label="Investigation summary">
            <div className="section-title">
              <h2>Investigation</h2>
              <span>
                {snapshot === undefined ? "Live view" : "Pinned view"} ·
                snapshot {summary.view.snapshot_seq} ·{" "}
                {summary.view.journal_state}
              </span>
              <button
                onClick={() => {
                  evidenceRequest.current?.abort();
                  saveLocation(scope);
                  setSnapshot(undefined);
                  setDetail(undefined);
                  setSelectedDetail(undefined);
                  setDetailError("");
                  setFinding(undefined);
                  setRefresh((x) => x + 1);
                }}
              >
                Refresh to latest snapshot
              </button>
              <button
                onClick={() =>
                  void navigator.clipboard.writeText(
                    bookmark(
                      labId,
                      committedScope,
                      summary.view.snapshot_seq,
                      selectedDetail,
                    ),
                  )
                }
              >
                Copy evidence link
              </button>
            </div>
            <p>
              {String(
                (summary.result.lab_context as { case_state?: string })
                  ?.case_state === "armed" &&
                  summary.result.traffic_state === "no_rpc_traffic"
                  ? "waiting_for_match"
                  : (summary.result.traffic_state ?? "Evidence available"),
              )}
              . No Finding alone does not prove resilience.
            </p>
            <dl className="counts">
              {Object.entries(
                (summary.result.counts ?? {}) as Record<string, Json>,
              ).map(([key, value]) => (
                <div key={key}>
                  <dt>{key.replaceAll("_", " ")}</dt>
                  <dd>{String(value)}</dd>
                </div>
              ))}
            </dl>
            <p className="boundary">
              Observation Boundary:{" "}
              {summary.view.observation_boundary.join(" · ")}
            </p>
            <details>
              <summary>Committed scope</summary>
              <Facts value={committedScope} />
            </details>
            <details>
              <summary>Analysis state</summary>
              <Facts value={summary.result.analysis} />
            </details>
            {summary.view.evidence_gaps.map((g) => (
              <p className="gap" key={g.code}>
                <strong>Evidence Gap / {g.code}</strong> — {g.message}
              </p>
            ))}
            <Findings
              key={`${summary.view.snapshot_seq}:${JSON.stringify(committedScope)}`}
              api={api}
              scope={committedScope}
              snapshot={summary.view.snapshot_seq}
              items={summary.result.findings ?? []}
              more={summary.result.findings_more === true}
              pin={() => {
                setScope(committedScope);
                setSnapshot(summary.view.snapshot_seq);
                saveLocation(
                  committedScope,
                  summary.view.snapshot_seq,
                  selectedDetail,
                );
              }}
              select={(item) => void act(() => evidence(item, true))}
            />
          </section>
        )}
        {status && (
          <Controls
            api={api}
            status={status}
            snapshot={summary?.view.snapshot_seq}
            changed={() => setRefresh((x) => x + 1)}
          />
        )}
        <form
          className="filters"
          onSubmit={(event) => {
            event.preventDefault();
            const data = new FormData(event.currentTarget),
              p = new URLSearchParams();
            for (const [key, value] of data)
              if (value) p.set(key, String(value));
            const next = selection("?" + p);
            if (next.error) {
              setError(next.error);
              return;
            }
            evidenceRequest.current?.abort();
            setSelectionValid(true);
            saveLocation(next.scope, snapshot);
            setScope(next.scope);
            setDetail(undefined);
            setSelectedDetail(undefined);
            setDetailError("");
            setFinding(undefined);
          }}
        >
          <h2>Selection</h2>
          {(
            [
              "signature",
              "rpc_method",
              "kind",
              "case_activation_id",
              "from_seq",
              "to_seq",
              "since",
              "until",
            ] as const
          ).map((key) => (
            <label key={key}>
              {key.replaceAll("_", " ")}
              <input name={key} defaultValue={String(scope[key] ?? "")} />
            </label>
          ))}
          <button>Apply filters</button>
        </form>
        <div className="workspace">
          <section className="timeline">
            <div className="section-title">
              <h2>Timeline</h2>
              <span>event_seq order</span>
            </div>
            {!timeline?.result.items?.length && (
              <p>
                Drive your independently running backend through the RPC URL. If
                a Case is armed, wait for matching activity.
              </p>
            )}
            {timeline?.result.items?.map((item) => (
              <button
                className={`event ${String(item.kind).startsWith("fault.") ? "fault" : ""} ${detail?.result.items?.some((s) => JSON.stringify(s.event_ref) === JSON.stringify(item.event_ref)) ? "supporting" : ""}`}
                key={JSON.stringify(item.event_ref)}
                onClick={() => void act(() => evidence(item))}
              >
                <span className="seq">
                  #
                  {String(
                    (item.event_ref as Record<string, Json>)?.event_seq ?? "?",
                  )}
                </span>
                <span>
                  <strong>{String(item.kind ?? "Event")}</strong>
                  <small>
                    {String(item.source ?? "unknown source")} ·{" "}
                    {String(item.recorded_at ?? "")}
                  </small>
                </span>
              </button>
            ))}
            {timeline?.page.next_cursor && (
              <button
                disabled={busy}
                onClick={() =>
                  void act(async () => {
                    setSnapshot(timeline.view.snapshot_seq);
                    setTimeline(
                      await api.query(
                        "timeline",
                        committedScope,
                        timeline.view.snapshot_seq,
                        timeline.page.next_cursor,
                      ),
                    );
                  })
                }
              >
                Next 50 events
              </button>
            )}
            {timeline?.page.truncated && (
              <p>
                Bounded page ·{" "}
                {timeline.page.reason ?? "continue to inspect more events"}
              </p>
            )}
          </section>
          <aside aria-label="Supporting evidence">
            <h2>Supporting evidence</h2>
            {detailError && <p role="alert">{detailError}</p>}
            {!detail && (
              <p>
                Select a Finding or event. Its evidence will remain pinned while
                the Lab continues.
              </p>
            )}
            {finding && (
              <>
                <h3>Observed statement</h3>
                <Facts
                  value={
                    finding.statement ??
                    "Statement is not loaded in this bookmarked Evidence Slice."
                  }
                />
                <h3>Developer guidance</h3>
                <Facts
                  value={
                    finding.guidance ??
                    "Inspect supporting evidence and application behavior."
                  }
                />
              </>
            )}
            {detail && (
              <>
                <p>
                  Snapshot {detail.view.snapshot_seq} ·{" "}
                  {detail.view.journal_state}
                </p>
                <Facts value={detail.result} />
                {detail.result.items?.map(
                  (item) =>
                    typeof item.transaction_signature === "string" && (
                      <button
                        key={String(item.transaction_signature)}
                        onClick={() =>
                          void navigator.clipboard.writeText(
                            String(item.transaction_signature),
                          )
                        }
                      >
                        Copy exact signature{" "}
                        {String(item.transaction_signature).slice(0, 10)}…
                      </button>
                    ),
                )}
                <button
                  disabled={busy}
                  onClick={() =>
                    void act(async () => {
                      const refs = detail.result.items?.map(
                        (i) =>
                          i.event_ref as {
                            lab_id: string;
                            event_seq: Sequence;
                          },
                      );
                      await loadEvidence(
                        { event_refs: refs },
                        detail.view.snapshot_seq,
                        { include_payload: true },
                      );
                    })
                  }
                >
                  Load raw payload preview
                </button>
                {detail.result.items?.map((item) => {
                  const payload = item.payload as
                    { next_offset?: number } | undefined;
                  if (payload?.next_offset == null) return null;
                  return (
                    <button
                      disabled={busy}
                      key={JSON.stringify(item.event_ref)}
                      onClick={() =>
                        void act(async () => {
                          await loadEvidence(
                            {
                              event_refs: [
                                item.event_ref as {
                                  lab_id: string;
                                  event_seq: Sequence;
                                },
                              ],
                            },
                            detail.view.snapshot_seq,
                            {
                              include_payload: true,
                              payload_offset: payload.next_offset,
                            },
                          );
                        })
                      }
                    >
                      Next payload chunk
                    </button>
                  );
                })}
              </>
            )}
          </aside>
        </div>
        <footer>
          Simulator observations do not prove program execution or economic
          effects. Closing this page does not stop or disarm the Lab.
        </footer>
      </main>
    </>
  );
}
createRoot(document.getElementById("root")!).render(<App />);
