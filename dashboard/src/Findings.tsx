import type { Sequence } from "./wire";
import { useEffect, useRef, useState } from "react";
import { Api, type Scope, type Json, type QueryReply } from "./api";
import { findingTitle, object, short, text } from "./presentation";
interface Props {
  api: Api;
  scope: Scope;
  snapshot: Sequence;
  items: Record<string, Json>[];
  more: boolean;
  pin: () => void;
  select: (item: Record<string, Json>) => void;
}
const projection = ["statement", "guidance"];
export function Findings({
  api,
  scope,
  snapshot,
  items,
  more,
  pin,
  select,
}: Props) {
  const [page, setPage] = useState<QueryReply>(),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  const [cursors, setCursors] = useState<(string | null)[]>([null]);
  const scopeKey = JSON.stringify(scope);
  const request = useRef<AbortController | undefined>(undefined);
  useEffect(() => {
    request.current?.abort();
    setPage(undefined);
    setError("");
    setBusy(false);
    setCursors([null]);
    return () => request.current?.abort();
  }, [api, scopeKey, snapshot]);
  async function next() {
    if (busy) return;
    pin();
    const controller = new AbortController();
    request.current?.abort();
    request.current = controller;
    setBusy(true);
    try {
      // Summary has no cursor. Resolve the same first five once to acquire the
      // server cursor, then explicitly request the next bounded page.
      const first =
        page ??
        (await api.query(
          "findings",
          scope,
          snapshot,
          undefined,
          { limit: 5, fields: projection },
          controller.signal,
        ));
      const result = first.page.next_cursor
        ? await api.query(
            "findings",
            scope,
            snapshot,
            first.page.next_cursor,
            { limit: 5, fields: projection },
            controller.signal,
          )
        : first;
      if (!controller.signal.aborted) {
        if (first.page.next_cursor)
          setCursors((old) => [...old, first.page.next_cursor]);
        setPage(result);
        setError("");
      }
    } catch (e) {
      if (!controller.signal.aborted) setError(String(e));
    } finally {
      if (!controller.signal.aborted) setBusy(false);
    }
  }
  return (
    <div aria-label="Findings">
      <div className="findings">
        {(page?.result.items ?? items).map((item) => (
          <button
            className="observation"
            key={String(item.finding_id)}
            onClick={() => select(item)}
          >
            <span className="badge signal">RPC observation</span>
            <strong>{findingTitle(item)}</strong>
            <span className="description">
              {text(
                item.statement,
                "Open the supporting evidence for this observation.",
              )}
            </span>
            <small>
              {typeof object(item.scope).signature === "string"
                ? short(String(object(item.scope).signature)) + " · "
                : ""}
              {Array.isArray(item.supporting_events)
                ? `${item.supporting_events.length} supporting records`
                : "Evidence-linked Finding"}
            </small>
            <span className="open-action">Investigate observation →</span>
          </button>
        ))}
      </div>
      {!page && items.length === 0 && (
        <div className="empty-state">
          <h3>No Finding in this selection</h3>
          <p>
            Inspect the RPC traffic or widen your filters. Absence of a Finding
            does not establish resilience or application success.
          </p>
        </div>
      )}
      {page && cursors.length > 1 && (
        <button
          disabled={busy}
          onClick={async () => {
            if (busy) return;
            pin();
            if (cursors.length === 2) {
              setPage(undefined);
              setCursors([null]);
              return;
            }
            const controller = new AbortController();
            request.current?.abort();
            request.current = controller;
            setBusy(true);
            try {
              const result = await api.query(
                "findings",
                scope,
                snapshot,
                cursors[cursors.length - 2],
                { limit: 5, fields: projection },
                controller.signal,
              );
              if (!controller.signal.aborted) {
                setPage(result);
                setCursors((old) => old.slice(0, -1));
                setError("");
              }
            } catch (e) {
              if (!controller.signal.aborted) setError(String(e));
            } finally {
              if (!controller.signal.aborted) setBusy(false);
            }
          }}
        >
          Previous Findings
        </button>
      )}
      {(page ? page.page.next_cursor : more) && (
        <button disabled={busy} onClick={() => void next()}>
          {busy ? "Loading Findings…" : "Next Findings"}
        </button>
      )}
      {page && (
        <p>
          Bounded Findings page · snapshot {page.view.snapshot_seq}.{" "}
          {page.page.next_cursor ? "More Findings remain." : "End of Findings."}
        </p>
      )}
      {error && (
        <p role="alert">{error}. The previous Findings page remains visible.</p>
      )}
    </div>
  );
}
