import type { QueryReply, Status } from "./api";
import type { Page } from "./selection";
import { caseInfo, object, text } from "./presentation";
import { CopyButton } from "./components";
import type { ReactNode } from "react";
export function Overview({
  status,
  summary,
  navigate,
  findings,
}: {
  status?: Status;
  summary?: QueryReply;
  navigate: (page: Page) => void;
  findings: ReactNode;
}) {
  const counts = object(summary?.result.counts),
    noTraffic = summary?.result.traffic_state === "no_rpc_traffic";
  const live = status?.state === "ready",
    currentCase = caseInfo(status?.active_case),
    historicalCase = caseInfo(
      object(object(summary?.result.lab_context).case_binding).policy,
    );
  const endpoint = status?.rpc_address
    ? `http://${status.rpc_address}`
    : undefined;
  return (
    <>
      {noTraffic && live ? (
        <div className="overview-grid">
          <section className="onboarding">
            <h2>Ronchi is ready. No RPC calls yet.</h2>
            <p>
              Point your backend at this RPC URL, then use your application as
              usual.
            </p>
            {endpoint && (
              <div className="endpoint">
                <code>{endpoint}</code>
                <CopyButton value={endpoint}>Copy RPC URL</CopyButton>
              </div>
            )}
            <p className="muted">
              Run your backend with your normal development tools. Ronchi
              records calls on this RPC path.
            </p>
            <details className="setup-help">
              <summary>
                {status?.configuration.mode === "surfpool"
                  ? "Local execution requirements"
                  : "Simulator connection requirements"}
              </summary>
              {status?.configuration.mode === "surfpool" ? (
                <p>
                  Prepare the programs, accounts and funded signers your backend
                  needs in the local Surfpool state. Ronchi does not create
                  missing application state or fund signers.
                </p>
              ) : (
                <p>
                  Use the Solana HTTP RPC path. Simulator submissions require{" "}
                  <code>skipPreflight: true</code>. Use Surfpool when your
                  backend needs program execution or full preflight. WebSocket
                  is not supported.
                </p>
              )}
            </details>
            <div className="actions">
              <button className="primary" onClick={() => navigate("case")}>
                {currentCase.id ? "Inspect active Case" : "Choose a Case"}
              </button>
              <button onClick={() => navigate("traffic")}>Open Traffic</button>
            </div>
          </section>
          <section className="setup-steps">
            <h2>Get your first observation</h2>
            <ol>
              <li>
                <strong>Connect your backend</strong>
                <p>Replace its Solana HTTP RPC URL with the address above.</p>
              </li>
              <li>
                <strong>
                  {currentCase.id ? "Case is enabled" : "Choose a Case"}
                </strong>
                <p>
                  {currentCase.id
                    ? currentCase.title + ". Waiting for matching calls."
                    : "One policy keeps applying to matching requests until disabled or replaced."}
                </p>
              </li>
              <li>
                <strong>Use your application</strong>
                <p>
                  Perform an action, then open Observations or inspect the RPC
                  traffic.
                </p>
              </li>
            </ol>
          </section>
        </div>
      ) : (
        <>
          <dl className="metrics">
            <div>
              <dt>RPC calls in this selection</dt>
              <dd>{text(counts.rpc_registrations, "—")}</dd>
            </div>
            <div>
              <dt>Accepted local transactions</dt>
              <dd>{text(counts.accepted_transactions, "—")}</dd>
            </div>
            <div>
              <dt>Journal records in this selection</dt>
              <dd>{text(counts.total, "—")}</dd>
            </div>
          </dl>
          <div className="overview-grid">
            <section>
              <div className="section-title">
                <h2>Observed behavior</h2>
                <button onClick={() => navigate("observations")}>
                  All observations
                </button>
              </div>
              {findings}
              <div className="next-action">
                <h3>Need to inspect the calls themselves?</h3>
                <p>
                  Traffic includes calls that have no Finding. Open a request to
                  see its retained facts.
                </p>
                <button onClick={() => navigate("traffic")}>
                  Open Traffic
                </button>
              </div>
            </section>
            <section className="context-panel">
              <h2>Case at this snapshot</h2>
              <h3>{historicalCase.title}</h3>
              <p>{historicalCase.description}</p>
              <button onClick={() => navigate("case")}>
                Manage current Case
              </button>
              <div className="next-action">
                <h3>Application outcome</h3>
                <p>
                  RPC evidence alone does not establish the result of a business
                  operation. Configured Checks can collect application
                  observations.
                </p>
                <button onClick={() => navigate("checks")}>Open Checks</button>
              </div>
              {endpoint && (
                <div className="next-action">
                  <h3>{live ? "RPC URL" : "Recorded RPC URL · not live"}</h3>
                  <code>{endpoint}</code>
                  {live && (
                    <CopyButton value={endpoint}>Copy RPC URL</CopyButton>
                  )}
                </div>
              )}
            </section>
          </div>
        </>
      )}
      {noTraffic && !live && (
        <p className="empty-state">
          No RPC calls were recorded. Open the Journal to inspect the Lab
          lifecycle.
        </p>
      )}
    </>
  );
}
