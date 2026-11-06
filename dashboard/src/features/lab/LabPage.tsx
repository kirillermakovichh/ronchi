import { Show } from "solid-js";
import { useLab } from "../../app/context";
import { Copy, Fields, Raw } from "../../shared/ui/primitives";
export function LabPage(props: { active: boolean }) {
  const lab = useLab();
  return (
    <section class="page-content" hidden={!props.active}>
      <div class="page-title">
        <h1>Lab</h1>
        <p>Configuration, execution authority and runtime diagnostics.</p>
      </div>
      <div class="lab-grid">
        <section class="panel-form">
          <h2>Identity and connection</h2>
          <Fields
            value={{
              lab_id: lab.api.labId,
              name: lab.status()?.name,
              state: lab.status()?.state,
              health: lab.status()?.health,
              journal: lab.status()?.journal_state,
              mode: lab.status()?.configuration.mode,
            }}
          />
          <Show when={lab.status()?.rpc_address}>
            <p>
              {lab.status()?.state === "ready" ? "RPC" : "Recorded RPC"}:{" "}
              <code>http://{lab.status()?.rpc_address}</code>
            </p>
            <Copy
              value={`http://${lab.status()?.rpc_address}`}
              label="Copy RPC URL"
            />
          </Show>
          <Copy value={lab.api.labId} label="Copy Lab ID" />
        </section>
        <section class="panel-form">
          <h2>Execution boundary</h2>
          <p>
            {lab.status()?.configuration.mode === "surfpool"
              ? "All blockchain reads and writes use the owned local Surfpool. The public datasource hydrates state; submitted transactions remain local."
              : "The simulator models RPC-visible transaction state. It does not execute programs or change balances."}
          </p>
          <p>
            Your backend runs independently. Stopping this viewer does not stop
            your backend.
          </p>
          <Raw value={lab.status()?.configuration} label="Lab configuration" />
          <Raw value={lab.status()} label="Public runtime status" />
        </section>
      </div>
      <section class="panel-form">
        <h2>Investigation coverage</h2>
        <Fields value={lab.summary()?.view} />
        <Raw
          value={lab.summary()?.result.analysis}
          label="Finding analysis metadata"
        />
      </section>
    </section>
  );
}
