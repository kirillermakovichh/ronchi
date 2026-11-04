import { useEffect, useRef, useState } from "react";
import { Api, type QueryReply, type Scope } from "./api";
import type { DetailSelection } from "./selection";
import type { Sequence } from "./wire";
import {
  boundaryResult,
  eventTitle,
  findingTitle,
  object,
  short,
  signature,
  sourceLabel,
  text,
  type Item,
} from "./presentation";
import { CopyButton, Fields, GapList, Raw, TabBar } from "./components";
export function EvidenceDetail({
  api,
  target,
  snapshot,
  scope,
  finding: suppliedFinding,
  active,
  inspector,
  filterSignature,
  back,
}: {
  api: Api;
  target?: DetailSelection;
  snapshot?: Sequence;
  scope: Scope;
  finding?: Item;
  active: boolean;
  inspector: boolean;
  filterSignature: (signature: string) => void;
  back: () => void;
}) {
  const [page, setPage] = useState<QueryReply>(),
    [overview, setOverview] = useState<Item[]>([]),
    [anchor, setAnchor] = useState<Item>();
  const [finding, setFinding] = useState<Item>(),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false),
    [tab, setTab] = useState("overview");
  const [raw, setRaw] = useState<QueryReply>(),
    [rawError, setRawError] = useState(""),
    [rawBusy, setRawBusy] = useState(false);
  const request = useRef<AbortController | undefined>(undefined),
    rawRequest = useRef<AbortController | undefined>(undefined);
  const loaded = useRef("");
  const heading = useRef<HTMLHeadingElement>(null),
    focused = useRef("");
  const selectionKey = JSON.stringify([target, snapshot, scope, inspector]);
  const selectedScope = useRef<Scope>({});
  useEffect(() => {
    if (!active || !target || snapshot === undefined) return;
    if (loaded.current === selectionKey) {
      setBusy(false);
      setRawBusy(false);
      return;
    }
    const controller = new AbortController();
    request.current?.abort();
    request.current = controller;
    rawRequest.current?.abort();
    setBusy(true);
    setError("");
    setPage(undefined);
    setOverview([]);
    setAnchor(undefined);
    setRaw(undefined);
    setRawError("");
    setRawBusy(false);
    setTab("overview");
    setFinding(suppliedFinding);
    async function load() {
      try {
        let selected: Scope = target!.finding_id
          ? { ...scope, finding_id: target!.finding_id }
          : {
              ...scope,
              event_refs: [
                { lab_id: api.labId, event_seq: target!.event_seq! },
              ],
            };
        let response = await api.query(
          "evidence",
          selected,
          snapshot,
          undefined,
          {},
          controller.signal,
        );
        const first = response.result.items?.[0];
        if (!first)
          throw new Error(
            "Detail reference is unavailable in the selected Lab and snapshot.",
          );
        if (inspector && typeof first.request_id === "string") {
          selected = { request_id: [first.request_id] };
          response = await api.query(
            "evidence",
            selected,
            snapshot,
            undefined,
            {},
            controller.signal,
          );
        }
        let metadata = suppliedFinding;
        if (target!.finding_id && !metadata?.statement) {
          const findings = await api.query(
            "findings",
            {
              ...scope,
              event_refs: response.result.items?.map(
                (i) => i.event_ref as { lab_id: string; event_seq: Sequence },
              ),
            },
            snapshot,
            undefined,
            { limit: 200 },
            controller.signal,
          );
          metadata = findings.result.items?.find(
            (i) => i.finding_id === target!.finding_id,
          );
        }
        if (controller.signal.aborted) return;
        selectedScope.current = selected;
        loaded.current = selectionKey;
        setAnchor(first);
        setPage(response);
        setOverview(response.result.items ?? []);
        setFinding(metadata);
        setBusy(false);
      } catch (e) {
        if (!controller.signal.aborted) {
          setError(String(e));
          setBusy(false);
        }
      }
    }
    void load();
    return () => {
      controller.abort();
      rawRequest.current?.abort();
    };
  }, [api, active, selectionKey]);
  useEffect(
    () => () => {
      request.current?.abort();
      rawRequest.current?.abort();
    },
    [],
  );
  async function next() {
    if (!page?.page.next_cursor || busy) return;
    const controller = new AbortController();
    request.current?.abort();
    request.current = controller;
    setBusy(true);
    try {
      const result = await api.query(
        "evidence",
        selectedScope.current,
        snapshot,
        page.page.next_cursor,
        {},
        controller.signal,
      );
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
  useEffect(() => {
    if (!active || !page || busy || focused.current === selectionKey) return;
    focused.current = selectionKey;
    heading.current?.focus({ preventScroll: true });
    if (window.matchMedia("(max-width: 1150px)").matches)
      heading.current?.scrollIntoView({ block: "start" });
  }, [active, page, busy, selectionKey]);
  const rawPending = useRef(false);
  async function payload(item: Item, offset = 0) {
    if (rawPending.current || snapshot === undefined) return;
    rawPending.current = true;
    const controller = new AbortController();
    rawRequest.current?.abort();
    rawRequest.current = controller;
    setRawBusy(true);
    try {
      const response = await api.query(
        "evidence",
        {
          event_refs: [
            item.event_ref as { lab_id: string; event_seq: Sequence },
          ],
        },
        snapshot,
        undefined,
        { include_payload: true, payload_offset: offset },
        controller.signal,
      );
      if (!controller.signal.aborted) {
        setRaw(response);
        setRawError("");
      }
    } catch (e) {
      if (!controller.signal.aborted) setRawError(String(e));
    } finally {
      rawPending.current = false;
      if (!controller.signal.aborted) setRawBusy(false);
    }
  }
  if (!target)
    return (
      <div className="empty-state">
        <h2>Select a request or record</h2>
        <p>Its evidence will stay pinned while the Lab continues recording.</p>
      </div>
    );
  const items = page?.result.items ?? [],
    sig = signature(anchor ?? {}) ?? overview.map(signature).find(Boolean);
  const exchange = Object.fromEntries(
    [
      ["boundary", "rpc.boundary"],
      ["prepared", "rpc.response_prepared"],
      ["acceptance", "transaction.accepted"],
      ["withholding", "fault.withholding_started"],
      ["dispatch", "rpc.dispatch_prevented"],
    ].map(([key, kind]) => [
      key,
      overview.find((i) => i.kind === kind) ?? null,
    ]),
  );
  const row = { ...anchor, exchange },
    result = boundaryResult(row),
    prepared = object(object(exchange.prepared).data),
    boundary = object(object(exchange.boundary).data);
  const title = target.finding_id
    ? findingTitle(finding ?? {})
    : inspector
      ? text(anchor?.rpc_method, "RPC request")
      : anchor
        ? eventTitle(anchor)
        : "Journal record";
  const rawItem = raw?.result.items?.[0],
    rawPayload = object(rawItem?.payload);
  return (
    <aside
      className="evidence-detail"
      aria-label="Supporting evidence"
      aria-busy={busy}
    >
      {target.finding_id && (
        <button className="back-link" onClick={back}>
          ← All observations
        </button>
      )}
      <span className="badge">
        {target.finding_id
          ? "Evidence-linked Finding"
          : inspector
            ? "Received RPC request"
            : "Retained Journal record"}
      </span>
      <h2 className="detail-title" ref={heading} tabIndex={-1}>
        {title}
      </h2>
      {busy && !page && <p role="status">Loading pinned evidence…</p>}
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      {sig && (
        <div className="signature-row">
          <code title={sig}>{short(sig)}</code>
          <CopyButton value={sig} label="Copy exact signature">
            Copy signature
          </CopyButton>
          <button onClick={() => filterSignature(sig)}>
            Inspect this signature
          </button>
        </div>
      )}
      {page && (
        <p className="caption">
          Pinned view · snapshot {page.view.snapshot_seq} ·{" "}
          {page.view.journal_state}
        </p>
      )}
      {page && inspector && (
        <p className="caption">
          Related records use this exact request ID at the pinned snapshot. They
          can include records outside the list filters.
        </p>
      )}
      {target.finding_id && page ? (
        <>
          <section className="detail-section">
            <h3>Observed statement</h3>
            <p>
              {text(
                finding?.statement,
                "The saved Finding was resolved, but its readable metadata is unavailable in this bounded page. Inspect the supporting records.",
              )}
            </p>
          </section>
          <section className="detail-section">
            <h3>What Ronchi observed</h3>
            <ol className="observation-chain">
              {overview.map((item) => (
                <li
                  key={JSON.stringify(item.event_ref)}
                  className={
                    String(item.kind).startsWith("fault.") ? "fault" : ""
                  }
                >
                  <strong>{eventTitle(item)}</strong>
                  <small>
                    Record #{text(object(item.event_ref).event_seq)} ·{" "}
                    {sourceLabel(item.source)}
                  </small>
                </li>
              ))}
            </ol>
            <p className="caption">
              Record order establishes observation order. It does not establish
              the application's internal decisions.
            </p>
          </section>
          <section className="knowledge-gap">
            <h3>What remains unknown</h3>
            <p>
              Client interpretation and business completion require separate
              application evidence. Two signatures alone do not establish one
              business operation or two transfers.
            </p>
          </section>
          <section className="detail-section">
            <h3>Developer guidance</h3>
            <p>
              {text(
                finding?.guidance,
                "Inspect the supporting records and the corresponding application behavior.",
              )}
            </p>
          </section>
          {Array.isArray(finding?.limitations) && (
            <details>
              <summary>Observation limits</summary>
              <ul>
                {finding.limitations.map((v, i) => (
                  <li key={i}>{text(v)}</li>
                ))}
              </ul>
            </details>
          )}
          <Raw
            value={finding ?? { finding_id: target.finding_id }}
            label="Finding identifiers and JSON"
          />
        </>
      ) : page && inspector ? (
        <>
          <TabBar
            tabs={[
              ["overview", "Overview"],
              ["request", "Request"],
              ["response", "Response"],
              ["records", "Records"],
            ]}
            selected={tab}
            choose={setTab}
          />
          <div role="tabpanel" aria-label={`${tab} details`}>
            {tab === "overview" && (
              <>
                <dl className="fact-grid">
                  <div>
                    <dt>Observed boundary result</dt>
                    <dd className={result.fault ? "signal-text" : ""}>
                      {result.label}
                    </dd>
                  </div>
                  <div>
                    <dt>Request identity</dt>
                    <dd>
                      <code>{text(anchor?.request_id)}</code>
                    </dd>
                  </div>
                  <div>
                    <dt>Execution mode</dt>
                    <dd>{text(anchor?.execution_mode)}</dd>
                  </div>
                  <div>
                    <dt>Local acceptance</dt>
                    <dd>
                      {exchange.acceptance
                        ? "Acceptance recorded"
                        : "No acceptance record in this page"}
                    </dd>
                  </div>
                  <div>
                    <dt>Client receipt</dt>
                    <dd>{text(boundary.client_receipt, "Unknown")}</dd>
                  </div>
                </dl>
                <p className="caption">
                  These are server-side observations. Prepared responses and
                  completed server writes do not prove client receipt.
                </p>
              </>
            )}
            {tab === "request" && (
              <>
                <h3>Retained request facts</h3>
                <p>
                  Ronchi does not retain the original RPC request body. The
                  fields below describe the received request and its
                  registration.
                </p>
                <Fields
                  value={{
                    method: anchor?.rpc_method ?? null,
                    request_id: anchor?.request_id ?? null,
                    batch_id: anchor?.batch_id ?? null,
                    signatures: anchor?.transaction_signatures ?? [],
                    registration: anchor?.data ?? null,
                  }}
                />
                <Raw value={anchor} label="Registration metadata JSON" />
              </>
            )}
            {tab === "response" && (
              <>
                <h3>{result.label}</h3>
                <p className="caption">
                  Prepared response content and transport observations are
                  separate facts.
                </p>
                {exchange.prepared ? (
                  <>
                    <h4>Prepared response facts</h4>
                    <Fields value={prepared} />
                    <Raw
                      value={exchange.prepared}
                      label="Prepared response metadata JSON"
                    />
                  </>
                ) : (
                  <p>No prepared response is recorded in this bounded page.</p>
                )}
                {exchange.boundary ? (
                  <>
                    <h4>RPC boundary</h4>
                    <Fields value={boundary} />
                  </>
                ) : (
                  <p>
                    No boundary result is recorded at this snapshot. The request
                    may still be running, or evidence may be missing.
                  </p>
                )}
              </>
            )}
            {tab === "records" && (
              <p>
                Related records use the exact request ID, including records
                outside the list filters. Evidence is shown below.
              </p>
            )}
          </div>
        </>
      ) : (
        page &&
        anchor && (
          <section className="detail-section">
            <h3>Recorded facts</h3>
            <p className="caption">
              {sourceLabel(anchor.source)} · {text(anchor.recorded_at)}
            </p>
            <Fields value={anchor.data} />
          </section>
        )
      )}
      {page && (
        <details
          className="supporting-records"
          open={!target.finding_id && (!inspector || tab === "records")}
        >
          <summary>
            Supporting evidence · {items.length} records on this page
            {page.page.next_cursor ? " · more available" : ""}
          </summary>
          {items.map((item) => (
            <article className="record" key={JSON.stringify(item.event_ref)}>
              <h4>
                <span>#{text(object(item.event_ref).event_seq)}</span>{" "}
                {eventTitle(item)}
              </h4>
              <small>
                {sourceLabel(item.source)} · {text(item.recorded_at)}
              </small>
              <details>
                <summary>Retained fields</summary>
                <Fields value={item.data} />
              </details>
              <Raw value={item} label="Exact record JSON" />
              {item.payload_status === "available" && (
                <button disabled={rawBusy} onClick={() => void payload(item)}>
                  Load payload #{text(object(item.event_ref).event_seq)}
                </button>
              )}
            </article>
          ))}
          {page.page.next_cursor && (
            <button disabled={busy} onClick={() => void next()}>
              Next evidence page
            </button>
          )}
          {!page.view.selection_complete && (
            <p className="caption">
              This page is incomplete. Continue to more retained records or
              inspect the Evidence Gaps.
            </p>
          )}
        </details>
      )}
      {page && (
        <>
          <GapList gaps={page.view.evidence_gaps} />
          <button
            disabled={rawBusy || items.length === 0}
            onClick={() =>
              void payload(
                items.find((i) => i.payload_status === "available") ?? items[0],
              )
            }
          >
            Load raw payload preview
          </button>
        </>
      )}
      {rawError && (
        <p role="alert">{rawError}. The previous payload remains available.</p>
      )}
      {raw && (
        <section className="payload-preview" aria-label="Raw payload preview">
          <h3>
            Retained payload · record #
            {text(object(rawItem?.event_ref).event_seq)}
          </h3>
          {rawItem?.payload ? (
            <>
              <p className="caption">
                Byte offset {text(rawPayload.offset)} ·{" "}
                {text(rawPayload.retained_bytes)} retained bytes ·{" "}
                {rawPayload.payload_truncated
                  ? "partial preview"
                  : "end of retained payload"}
              </p>
              <pre>{text(rawPayload.preview, "")}</pre>
            </>
          ) : (
            <p>
              Payload is {text(rawItem?.payload_status, "unavailable")}. No
              original wire body is implied.
            </p>
          )}
          {typeof rawPayload.next_offset === "number" && rawItem && (
            <button
              disabled={rawBusy}
              onClick={() =>
                void payload(rawItem, rawPayload.next_offset as number)
              }
            >
              Next payload chunk
            </button>
          )}
          <details>
            <summary>Payload response JSON</summary>
            <pre aria-label="Payload response JSON">
              {JSON.stringify(raw.result, null, 2)}
            </pre>
          </details>
        </section>
      )}
    </aside>
  );
}
