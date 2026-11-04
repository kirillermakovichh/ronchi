import { useEffect, useRef, useState, type ReactNode } from "react";
import { Api, type QueryReply, type Scope } from "./api";
import type { Sequence } from "./wire";
import {
  boundaryResult,
  duration,
  eventTitle,
  object,
  short,
  signature,
  sourceLabel,
  text,
  type Item,
} from "./presentation";
export function Traffic({
  api,
  scope,
  snapshot,
  active,
  journal,
  select,
  pin,
  changeView,
  filter,
  detail,
  selected,
}: {
  api: Api;
  scope: Scope;
  snapshot?: Sequence;
  active: boolean;
  journal: boolean;
  select: (item: Item) => void;
  pin: () => void;
  changeView: (journal: boolean) => void;
  filter: (scope: Scope) => void;
  detail: ReactNode;
  selected?: Sequence;
}) {
  const [page, setPage] = useState<QueryReply>(),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false);
  const [cursors, setCursors] = useState<(string | null)[]>([null]);
  const request = useRef<AbortController | undefined>(undefined),
    loaded = useRef("");
  const key = JSON.stringify([snapshot, scope, journal]);
  const operation = journal ? "timeline" : "requests";
  useEffect(() => {
    if (!active || snapshot === undefined) return;
    if (loaded.current === key) {
      setBusy(false);
      return () => request.current?.abort();
    }
    const controller = new AbortController();
    request.current?.abort();
    request.current = controller;
    setBusy(true);
    setError("");
    void api
      .query(operation, scope, snapshot, undefined, {}, controller.signal)
      .then((result) => {
        if (controller.signal.aborted) return;
        loaded.current = key;
        setPage(result);
        setCursors([null]);
        setBusy(false);
      })
      .catch((e) => {
        if (!controller.signal.aborted) {
          setError(String(e));
          setBusy(false);
        }
      });
    return () => request.current?.abort();
  }, [api, active, key]);
  async function go(cursor: string | null, previous = false) {
    if (busy || snapshot === undefined) return;
    pin();
    const controller = new AbortController();
    request.current?.abort();
    request.current = controller;
    setBusy(true);
    try {
      const result = await api.query(
        operation,
        scope,
        snapshot,
        cursor,
        {},
        controller.signal,
      );
      if (!controller.signal.aborted) {
        setPage(result);
        setCursors((old) =>
          previous ? old.slice(0, -1) : [...old.slice(-63), cursor],
        );
        setError("");
      }
    } catch (e) {
      if (!controller.signal.aborted) setError(String(e));
    } finally {
      if (!controller.signal.aborted) setBusy(false);
    }
  }
  const current = loaded.current === key;
  return (
    <>
      <div className="traffic-tools">
        <div className="segmented" aria-label="Traffic representation">
          <button aria-pressed={!journal} onClick={() => changeView(false)}>
            RPC requests
          </button>
          <button aria-pressed={journal} onClick={() => changeView(true)}>
            Journal records
          </button>
        </div>
        {!journal && (
          <div className="quick-filters" aria-label="Quick method filters">
            {[
              ["", "All methods"],
              ["sendTransaction", "Submissions"],
              ["getSignatureStatuses", "Status lookups"],
            ].map(([method, label]) => (
              <button
                key={method}
                aria-pressed={
                  method
                    ? scope.rpc_method?.length === 1 &&
                      scope.rpc_method[0] === method
                    : !scope.rpc_method?.length
                }
                onClick={() => {
                  const next = { ...scope };
                  if (method) next.rpc_method = [method];
                  else delete next.rpc_method;
                  filter(next);
                }}
              >
                {label}
              </button>
            ))}
          </div>
        )}
      </div>
      {error && (
        <p role="alert" className="error">
          {error}. Previous records remain available. Retry or narrow the
          selection.
        </p>
      )}
      {busy && (
        <p role="status" className="caption">
          Loading bounded page…
        </p>
      )}
      <div className="inspector-grid">
        <section
          className="request-list"
          aria-label={journal ? "Journal records" : "RPC requests"}
          aria-busy={busy}
        >
          {!journal && (
            <div className="request-columns" aria-hidden="true">
              <span>Time (UTC)</span>
              <span>Method / signature</span>
              <span>Observed at RPC boundary</span>
            </div>
          )}
          {page && !page.result.items?.length && (
            <div className="empty-state">
              <h2>
                {journal
                  ? "No records in this selection"
                  : "No RPC requests in this selection"}
              </h2>
              <p>
                Use your backend through the RPC URL or widen the filters. An
                enabled Case needs matching activity.
              </p>
              <button onClick={() => filter({})}>Clear filters</button>
            </div>
          )}
          {page?.result.items?.map((item) => {
            const seq = object(item.event_ref).event_seq,
              result = boundaryResult(item),
              sig = signature(item),
              elapsed = duration(item);
            return (
              <button
                key={JSON.stringify(item.event_ref)}
                disabled={!current}
                className={`${journal ? "event" : "request-row"} ${String(seq) === String(selected) ? "selected" : ""} ${result.fault || String(item.kind).startsWith("fault.") ? "fault" : ""}`}
                onClick={() => select(item)}
              >
                {journal ? (
                  <>
                    <span className="seq">#{text(seq)}</span>
                    <span>
                      <strong>{eventTitle(item)}</strong>
                      <small>
                        {text(item.kind)} · {sourceLabel(item.source)}
                      </small>
                    </span>
                  </>
                ) : (
                  <>
                    <time title={text(item.recorded_at)}>
                      {text(item.recorded_at, "").slice(11, 19) || "—"}
                    </time>
                    <span>
                      <strong>
                        <code>{text(item.rpc_method, "Unknown method")}</code>
                      </strong>
                      <small>
                        {sig ? short(sig) : "No signature recorded"}
                      </small>
                    </span>
                    <span className={result.fault ? "signal-text" : ""}>
                      {result.label}
                      <small>
                        {elapsed
                          ? `Registered → boundary: ${elapsed}`
                          : "Client receipt unknown"}
                      </small>
                    </span>
                  </>
                )}
              </button>
            );
          })}
          {page && (
            <div className="pagination">
              <span className="caption">
                {page.result.items?.length ?? 0}{" "}
                {journal ? "records" : "requests"} on this page · snapshot{" "}
                {page.view.snapshot_seq}
              </span>
              <div className="actions">
                {cursors.length > 1 && (
                  <button
                    disabled={busy || !current}
                    onClick={() => void go(cursors[cursors.length - 2], true)}
                  >
                    Previous page
                  </button>
                )}
                {page.page.next_cursor && (
                  <button
                    disabled={busy || !current}
                    onClick={() => void go(page.page.next_cursor)}
                  >
                    Next 50 {journal ? "events" : "requests"}
                  </button>
                )}
              </div>
              {page.page.truncated && (
                <p className="caption">
                  Response size limit reached. Continue to the next bounded
                  page.
                </p>
              )}
            </div>
          )}
        </section>
        {detail}
      </div>
    </>
  );
}
