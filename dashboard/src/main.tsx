import { useEffect, useMemo, useState } from "react";
import { createRoot } from "react-dom/client";
import { Api, type Scope } from "./api";
import {
  selection,
  bookmark,
  type DetailSelection,
  type Page,
} from "./selection";
import { useLab } from "./useLab";
import { caseInfo, object, short, type Item } from "./presentation";
import { CopyButton, GapList, Raw } from "./components";
import { Filters, FilterChips } from "./Filters";
import { Overview } from "./Overview";
import { EvidenceDetail } from "./EvidenceDetail";
import { Traffic } from "./Traffic";
import { StateNotice } from "./StateNotice";
import { Findings } from "./Findings";
import { Controls } from "./Controls";
import "./style.css";
type Route = ReturnType<typeof selection>;
function App() {
  const labId = decodeURIComponent(location.pathname.split("/")[2] ?? "");
  const api = useMemo(() => new Api(labId), [labId]);
  const [route, setRoute] = useState<Route>(() => selection(location.search)),
    [refresh, setRefresh] = useState(0),
    [chosenFinding, setChosenFinding] = useState<Item>();
  const lab = useLab(api, route.scope, route.snapshot, refresh, route.error);
  const { status, summary, committedScope } = lab;
  const page = route.page ?? "overview",
    trafficPage = page === "traffic" || page === "journal";
  const currentCase = caseInfo(status?.active_case),
    live = route.snapshot === undefined && status?.state === "ready";
  const snapshot = route.snapshot ?? summary?.view.snapshot_seq;
  function update(next: Route, push = false) {
    const url = new URL(
      bookmark(labId, next.scope, next.snapshot ?? 0, next.detail, next.page),
    );
    if (next.snapshot === undefined) url.searchParams.delete("snapshot");
    if (push) history.pushState(null, "", url.pathname + url.search);
    else history.replaceState(null, "", url.pathname + url.search);
    setRoute(next);
  }
  useEffect(() => {
    const pop = () => setRoute(selection(location.search));
    window.addEventListener("popstate", pop);
    return () => window.removeEventListener("popstate", pop);
  }, []);
  function navigate(next: Page) {
    update({ ...route, page: next }, true);
  }
  function apply(scope: Scope) {
    setChosenFinding(undefined);
    update({ scope, snapshot: route.snapshot, page }, true);
  }
  function pin() {
    if (summary && route.snapshot === undefined)
      update({
        ...route,
        scope: committedScope,
        snapshot: summary.view.snapshot_seq,
      });
  }
  function select(item: Item, isFinding = false) {
    if (!summary) return;
    const target: DetailSelection = isFinding
      ? { finding_id: String(item.finding_id) }
      : { event_seq: object(item.event_ref).event_seq as number | string };
    setChosenFinding(isFinding ? item : undefined);
    update(
      {
        scope: committedScope,
        snapshot: summary.view.snapshot_seq,
        detail: target,
        page: isFinding
          ? "observations"
          : page === "journal"
            ? "journal"
            : "traffic",
      },
      true,
    );
  }
  function latest() {
    setChosenFinding(undefined);
    update({ scope: route.scope, page });
    setRefresh((n) => n + 1);
  }
  const commandPrefix = status?.configuration.project
    ? `ronchi --project '${status.configuration.project.replaceAll("'", "'\\''")}'`
    : "ronchi";
  const savedLink = summary
    ? bookmark(
        labId,
        committedScope,
        summary.view.snapshot_seq,
        route.detail,
        page,
      )
    : "";
  const detail = (
    <EvidenceDetail
      api={api}
      target={
        route.detail?.event_seq !== undefined || page === "observations"
          ? route.detail
          : undefined
      }
      snapshot={snapshot}
      scope={committedScope}
      finding={
        chosenFinding?.finding_id === route.detail?.finding_id
          ? chosenFinding
          : undefined
      }
      active={lab.authorized && (trafficPage || page === "observations")}
      inspector={page === "traffic"}
      filterSignature={(sig) =>
        update({ scope: { signature: [sig] }, snapshot, page: "traffic" }, true)
      }
      back={() =>
        update({ ...route, detail: undefined, page: "observations" }, true)
      }
    />
  );
  function findings(preview = false) {
    return (
      summary && (
        <Findings
          key={`${summary.view.snapshot_seq}:${JSON.stringify(committedScope)}:${preview}`}
          api={api}
          scope={committedScope}
          snapshot={summary.view.snapshot_seq}
          items={(summary.result.findings ?? []).slice(0, preview ? 3 : 5)}
          more={!preview && summary.result.findings_more === true}
          pin={pin}
          select={(item) => select(item, true)}
        />
      )
    );
  }
  const onboarding =
    page === "overview" &&
    summary?.result.traffic_state === "no_rpc_traffic" &&
    Object.keys(committedScope).length === 0 &&
    status?.state === "ready";
  const headings: Record<Page, [string, string]> = {
    overview: [
      "Lab overview",
      status?.state === "ready"
        ? "Connect your backend, apply a Case and inspect what Ronchi observes."
        : "Explore the retained history of this Lab.",
    ],
    observations: [
      "Observations",
      "Evidence-linked Findings explain supported RPC behavior. Each observation has a path to its source records.",
    ],
    traffic: [
      "RPC traffic",
      "Select a received request to inspect its boundary result and related records.",
    ],
    journal: [
      "Journal",
      "The retained sequence of Lab, RPC, Case and application observations.",
    ],
    case: [
      "Case",
      "One continuing policy applies to matching calls until disabled or replaced.",
    ],
    checks: [
      "Application Checks",
      "Collect explicit application observations, then evaluate configured requirements.",
    ],
  };
  const activeCount =
    (summary?.result.findings?.length ?? 0) +
    (summary?.result.findings_more ? "+" : "");
  return (
    <div className="app-shell">
      <a className="skip-link" href="#main">
        Skip to content
      </a>
      <aside className="sidebar">
        <div className="brand">
          <svg width="33" height="28" viewBox="0 0 44 30" aria-hidden="true">
            {[4, 10, 16, 22, 28].map((y) => (
              <path key={y} d={`M0 ${y} H17 l5 -3 l5 3 H44`} />
            ))}
          </svg>
          <strong>RONCHI</strong>
        </div>
        <div className="lab-name">
          <small>Current Lab</small>
          <strong>{status?.name ?? "Connecting…"}</strong>
          <span>
            {status
              ? status.configuration.mode === "surfpool"
                ? "Local Surfpool"
                : "RPC simulator"
              : "Mode unavailable"}
          </span>
        </div>
        <nav aria-label="Main navigation">
          {(
            [
              ["overview", "Overview"],
              ["observations", "Observations"],
              ["traffic", "Traffic"],
              ["case", "Case"],
              ["checks", "Checks"],
            ] as [Page, string][]
          ).map(([id, title]) => (
            <button
              key={id}
              aria-current={
                (id === "traffic" ? trafficPage : page === id)
                  ? "page"
                  : undefined
              }
              onClick={() => navigate(id)}
            >
              {title}
              {id === "observations" && summary && <small>{activeCount}</small>}
            </button>
          ))}
        </nav>
        <div className="sidebar-foot">
          <span>
            {status
              ? status.configuration.mode === "surfpool"
                ? "Owned local execution backend"
                : "Simulator · no program execution"
              : "Execution mode unavailable"}
          </span>
          <details>
            <summary>Lab identity</summary>
            <code title={labId}>{short(labId)}</code>
            <CopyButton value={labId} label={labId}>
              Copy Lab ID
            </CopyButton>
          </details>
        </div>
      </aside>
      <div className="main-column">
        <header className="lab-bar">
          <span>
            {status?.name ?? "Lab"}{" "}
            <span className="muted">/ local Solana</span>
          </span>
          <div className="lab-actions">
            <span className="badge">
              {lab.error.startsWith("Connection") ? "Last known · " : ""}
              {status ? `${status.state} · ${status.health}` : "Connecting"}
            </span>
            <button onClick={() => navigate("case")} title={currentCase.title}>
              {status?.state === "ready"
                ? currentCase.id
                  ? "Case enabled"
                  : "Choose a Case"
                : "Case history"}
            </button>
          </div>
        </header>
        <main id="main">
          <StateNotice
            labId={labId}
            status={status}
            analysis={summary?.result.analysis}
            error={lab.error}
          />
          {lab.error && (
            <div role="alert" className="error">
              <strong>Investigation unavailable</strong>
              <p>{lab.error}</p>
              <p>
                Open this Lab’s launch link to reconnect. A lost connection does
                not prove the Lab stopped.
              </p>
              <code>
                {commandPrefix} dashboard --lab-id {labId}
              </code>
              <button onClick={() => setRefresh((n) => n + 1)}>
                Retry connection
              </button>
            </div>
          )}
          {!lab.authorized && labId && (
            <p>Open this Lab’s dashboard launch link to access its history.</p>
          )}
          {labId && (
            <div className="page-heading">
              <h1>{headings[page][0]}</h1>
              <p>{headings[page][1]}</p>
            </div>
          )}
          {summary && (
            <section
              hidden={onboarding}
              className="snapshot-bar summary"
              aria-label="Investigation summary"
            >
              <div className="section-title">
                <span>
                  {live ? "Live view" : "Pinned view"} · snapshot{" "}
                  {summary.view.snapshot_seq} · {summary.view.journal_state}
                </span>
                <div className="actions">
                  {live && <button onClick={pin}>Pin this view</button>}
                  <button onClick={latest}>Refresh to latest snapshot</button>
                  <CopyButton value={savedLink}>Copy evidence link</CopyButton>
                </div>
              </div>
              {!live && status?.state === "ready" && (
                <p className="caption">
                  Lab keeps recording.{" "}
                  {String(status.snapshot_seq) !==
                  String(summary.view.snapshot_seq)
                    ? "New Journal records may be available. "
                    : ""}
                  Refresh explicitly to update this investigation.
                </p>
              )}
              <FilterChips scope={committedScope} apply={apply} />
              <details className="investigation-context">
                <summary>Investigation context and recovery</summary>
                <p>
                  Observation sources:{" "}
                  {summary.view.observation_boundary
                    .map((source) => source.replaceAll("_", " "))
                    .join(", ")}
                </p>
                <details>
                  <summary>Exact Lab identity</summary>
                  <code>{labId}</code>
                  <CopyButton value={labId}>Copy Lab ID</CopyButton>
                </details>
                <Raw value={committedScope} label="Committed scope" />
                <Raw value={summary.result.analysis} label="Analysis state" />
                <details>
                  <summary>Open this pinned selection after Lab stop</summary>
                  <p>
                    Start a read-only viewer for this exact Lab. It uses a new
                    local endpoint.
                  </p>
                  <code>
                    {commandPrefix} dashboard --serve --lab-id {labId}{" "}
                    --selection '{new URL(savedLink).search.slice(1)}'
                  </code>
                </details>
              </details>
            </section>
          )}
          {lab.loading && (
            <p className="loading" role="status">
              {summary ? "Updating selection…" : "Loading Lab…"}
            </p>
          )}
          {lab.authorized && (
            <div hidden={onboarding}>
              <Filters scope={route.scope} apply={apply} />
            </div>
          )}
          <section hidden={page !== "overview" || !summary}>
            <Overview
              status={status}
              summary={summary}
              navigate={navigate}
              findings={findings(true)}
            />
          </section>
          <section hidden={page !== "observations"}>
            <div hidden={Boolean(route.detail?.finding_id)}>
              {findings()}
              <GapList gaps={summary?.view.evidence_gaps ?? []} />
            </div>
            {route.detail?.finding_id && detail}
          </section>
          <section hidden={!trafficPage}>
            <Traffic
              api={api}
              scope={committedScope}
              snapshot={summary?.view.snapshot_seq}
              active={trafficPage && lab.authorized}
              journal={page === "journal"}
              selected={route.detail?.event_seq}
              select={(item) => select(item)}
              pin={pin}
              changeView={(journal) =>
                navigate(journal ? "journal" : "traffic")
              }
              filter={apply}
              detail={trafficPage ? detail : null}
            />
          </section>
          <section hidden={page !== "case" && page !== "checks"}>
            {page === "case" && (
              <section className="current-case">
                <span className="badge">
                  {status?.state === "ready"
                    ? "Current runtime policy"
                    : "Retained Lab · read-only"}
                </span>
                <h2>{currentCase.title}</h2>
                <p>{currentCase.description}</p>
                {currentCase.id && <code>{currentCase.id}</code>}
                <p className="caption">
                  Changing the Case preserves the Lab and Journal. Effects
                  already in progress follow their existing policy.
                </p>
              </section>
            )}
            {status && (
              <Controls
                api={api}
                status={status}
                snapshot={summary?.view.snapshot_seq}
                changed={() => setRefresh((n) => n + 1)}
                view={page === "checks" ? "checks" : "case"}
                active={
                  lab.authorized && (page === "case" || page === "checks")
                }
              />
            )}
          </section>
          <footer>
            {status?.configuration.mode === "surfpool"
              ? "Evidence describes the owned local Execution Backend."
              : "Simulator observations do not prove program execution or economic effects."}{" "}
            Closing this page does not stop the Lab.
          </footer>
        </main>
      </div>
    </div>
  );
}
createRoot(document.getElementById("root")!).render(<App />);
