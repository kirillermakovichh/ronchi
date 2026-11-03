import type { Status } from "./api";
export function StateNotice({
  labId,
  status,
  analysis,
  error = "",
}: {
  labId: string;
  status?: Status;
  analysis?: unknown;
  error?: string;
}) {
  if (!labId)
    return (
      <section role="status">
        <h2>No Lab selected</h2>
        <p>
          Start a simulator Lab with <code>ronchi up</code>. Open its exact
          dashboard launch link with <code>ronchi dashboard --lab-id ID</code>.
        </p>
      </section>
    );
  if (status?.state === "starting")
    return (
      <section role="status">
        <h2>Lab starting</h2>
        <p>
          Wait for native readiness. Inspect{" "}
          <code>ronchi status --lab-id {labId}</code> if startup does not
          finish. The dashboard does not start another Lab.
        </p>
      </section>
    );
  if (error.includes("unsupported_capability"))
    return (
      <section role="alert">
        <h2>Unsupported capability</h2>
        <p>{error}</p>
        <p>
          Select a capability listed by this Lab. Existing evidence remains
          available.
        </p>
      </section>
    );
  if ((analysis as { findings?: string })?.findings === "operational_limit")
    return (
      <section role="alert">
        <h2>Analysis limit</h2>
        <p>
          The bounded analysis is incomplete. Narrow the signature or sequence
          interval. No completed analysis or Verdict is implied.
        </p>
      </section>
    );
  if (status?.health === "failed" || status?.state === "failed")
    return (
      <section role="alert">
        <h2>Ronchi core failure</h2>
        <p>
          Inspect the exact Lab status and retained Journal. This is separate
          from an injected Fault.
        </p>
      </section>
    );
  return null;
}
