import type { Sequence } from "./wire";
import { useEffect, useRef, useState } from "react";
import { Api, type Scope, type Json, type QueryReply } from "./api";
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
  const request = useRef<AbortController | undefined>(undefined);
  useEffect(() => {
    request.current?.abort();
    setPage(undefined);
    setError("");
    setBusy(false);
    return () => request.current?.abort();
  }, [api, scope, snapshot]);
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
          <button key={String(item.finding_id)} onClick={() => select(item)}>
            {String(item.rule_id)} · {String(item.finding_id).slice(0, 11)} ↗
          </button>
        ))}
      </div>
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
