import { useEffect, useMemo, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import {
  Api,
  bootstrap,
  type Json,
  type QueryReply,
  type Scope,
  type Status,
} from "./api";
import { selection, bookmark } from "./selection";
import "./style.css";
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
    [snapshot, setSnapshot] = useState<number | undefined>(initial.snapshot);
  const [summary, setSummary] = useState<QueryReply>(),
    [timeline, setTimeline] = useState<QueryReply>(),
    [detail, setDetail] = useState<QueryReply>(),
    [finding, setFinding] = useState<Record<string, Json>>();
  const evidenceRequest = useRef<AbortController | undefined>(undefined);
  useEffect(() => () => evidenceRequest.current?.abort(), [api, scope]);
  async function loadEvidence(
    selected: Scope,
    seq: number,
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
    if (!controller.signal.aborted) setDetail(response);
  }
  const [busy, setBusy] = useState(false),
    [refresh, setRefresh] = useState(0);
  useEffect(() => {
    let active = true;
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
        setTimeline(events);
        if (current.state !== "ready" && snapshot === undefined)
          setSnapshot(result.view.snapshot_seq);
        setError("");
      } catch (e) {
        if (!controller.signal.aborted)
          setError(
            `Connection or core failure: ${String(e)}. Last pinned evidence is retained on screen.`,
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
  async function act(work: () => Promise<void>) {
    setBusy(true);
    try {
      await work();
      setError("");
    } catch (e) {
      if (!(e instanceof DOMException && e.name === "AbortError"))
        setError(String(e));
    } finally {
      setBusy(false);
    }
  }
  async function evidence(item: Record<string, Json>, isFinding = false) {
    const seq = summary?.view.snapshot_seq;
    if (seq === undefined) return;
    setSnapshot(seq);
    setFinding(isFinding ? item : undefined);
    const ref = item.event_ref as
      { lab_id: string; event_seq: number } | undefined;
    const selected: Scope = isFinding
      ? { ...scope, finding_id: String(item.finding_id) }
      : { event_refs: ref ? [ref] : [] };
    await loadEvidence(selected, seq);
  }
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
          {status?.state ?? "Connecting"} · {status?.health ?? "unknown health"}
        </div>
      </header>
      <main>
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
        {error && (
          <div role="alert" className="error">
            {error}
            <p>
              Use <code>ronchi dashboard --lab-id {labId}</code> for a new
              session. A lost connection does not prove that the Lab stopped.
            </p>
          </div>
        )}
        {!authorized && (
          <p>
            Exchange the single-use launch ticket to open this Investigation.
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
                  setSnapshot(undefined);
                  setDetail(undefined);
                  setFinding(undefined);
                  setRefresh((x) => x + 1);
                }}
              >
                Refresh to latest snapshot
              </button>
              <button
                onClick={() =>
                  void navigator.clipboard.writeText(
                    bookmark(labId, scope, summary.view.snapshot_seq),
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
            <dl className="counts">{Object.entries((summary.result.counts ?? {}) as Record<string,Json>).map(([key,value])=><div key={key}><dt>{key.replaceAll('_',' ')}</dt><dd>{String(value)}</dd></div>)}</dl>
            <p className="boundary">
              Observation Boundary:{" "}
              {summary.view.observation_boundary.join(" · ")}
            </p>
            <details>
              <summary>Analysis state</summary>
              <Facts value={summary.result.analysis} />
            </details>
            {summary.view.evidence_gaps.map((g) => (
              <p className="gap" key={g.code}>
                <strong>Evidence Gap / {g.code}</strong> — {g.message}
              </p>
            ))}
            <div className="findings">
              {summary.result.findings?.map((item) => (
                <button
                  key={String(item.finding_id)}
                  onClick={() => void act(() => evidence(item, true))}
                >
                  {String(item.rule_id)} ↗
                </button>
              ))}
            </div>
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
            setSelectionValid(true);
            setScope(next.scope);
            setDetail(undefined);
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
                        scope,
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
            {!detail && (
              <p>
                Select a Finding or event. Its evidence will remain pinned while
                the Lab continues.
              </p>
            )}
            {finding && (
              <>
                <h3>Observed statement</h3>
                <Facts value={finding.statement ?? finding} />
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
                          i.event_ref as { lab_id: string; event_seq: number },
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
                      key={JSON.stringify(item.event_ref)}
                      onClick={() =>
                        void act(async () => {
                          await loadEvidence(
                            {
                              event_refs: [
                                item.event_ref as {
                                  lab_id: string;
                                  event_seq: number;
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
