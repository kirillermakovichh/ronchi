import type { Sequence } from "./wire";
import { useState } from "react";
import { Api, type Status, type Json } from "./api";
interface Props {
  api: Api;
  status: Status;
  snapshot?: Sequence;
  changed: () => void;
}
export function Controls({ api, status, snapshot, changed }: Props) {
  const [catalog, setCatalog] = useState<unknown>(),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false);
  const [collection, setCollection] = useState<{
      collection_id: string;
      status: string;
      snapshot_seq: Sequence;
      [key: string]: Json;
    }>(),
    [evaluation, setEvaluation] = useState<unknown>();
  async function run(work: () => Promise<void>) {
    setBusy(true);
    try {
      await work();
      setError("");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }
  const live = status.state === "ready";
  const sources = status.configuration.checks?.sources ?? [];
  return (
    <section className="controls" aria-label="Case and Check controls">
      <details>
        <summary>
          Case controls {live ? "" : "· read-only retained Journal"}
        </summary>
        <p>Replacing a Case keeps the Journal and previously held effects.</p>
        <button
          disabled={!live || busy}
          onClick={() =>
            void run(async () =>
              setCatalog(await api.call("api/cases", undefined)),
            )
          }
        >
          List Cases
        </button>
        {catalog !== undefined && <pre>{JSON.stringify(catalog, null, 2)}</pre>}
        <button
          disabled={!live || busy || status.configuration.mode !== "simulator"}
          onClick={() =>
            void run(async () => {
              await api.call("api/case/arm", { case_id: "tx-response-lost" });
              changed();
            })
          }
        >
          Arm / replace tx-response-lost
        </button>
        <button
          disabled={!live || busy}
          onClick={() =>
            void run(async () => {
              await api.call("api/case/disarm", {});
              changed();
            })
          }
        >
          Disarm Case
        </button>
      </details>
      <details>
        <summary>Application Checks</summary>
        <p>
          Collection writes application-source observations to the Journal.
          Evaluation reads the pinned evidence only. HTTP sources only · 32 KiB
          response cap · one intent or signature.
        </p>
        {sources.length === 0 ? (
          <p>
            No configured Check source. Configure a named source before
            collection. RPC-only Investigation has no formal Verdict.
          </p>
        ) : (
          <form
            onSubmit={(event) => {
              event.preventDefault();
              const data = new FormData(event.currentTarget);
              const checkId = String(data.get("check")),
                intent = String(data.get("intent")).trim();
              if (!intent) {
                setError("Select exactly one intent.");
                return;
              }
              void run(async () => {
                const reply = await api.call<{
                  result: NonNullable<typeof collection>;
                }>("api/checks/observe", {
                  contract_version: 1,
                  operation: "check.observe",
                  request_id: crypto.randomUUID(),
                  lab_id: api.labId,
                  arguments: {
                    collection_id: crypto.randomUUID(),
                    check_id: checkId,
                    scope: { intent_ids: [intent], signatures: [] },
                    poll: data.get("poll") === "on",
                    horizon_seconds: data.get("poll") === "on" ? 60 : null,
                  },
                });
                setCollection(reply.result);
                changed();
              });
            }}
          >
            <label>
              Check
              <select name="check">
                {sources.map((source) => (
                  <option
                    key={`${source.check_id}:${source.version}`}
                    value={source.check_id}
                  >
                    {source.check_id} · definition {source.version}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Intent identity
              <input name="intent" required />
            </label>
            <label>
              <input name="poll" type="checkbox" />
              Explicit bounded polling (60 seconds)
            </label>
            <button disabled={!live || busy}>Collect observations</button>
            <button
              type="button"
              disabled={busy || snapshot === undefined}
              onClick={(event) => {
                const form = event.currentTarget.form;
                if (!form || snapshot === undefined) return;
                const data = new FormData(form),
                  checkId = String(data.get("check")),
                  intent = String(data.get("intent")).trim(),
                  source = sources.find((s) => s.check_id === checkId);
                if (!intent || !source) {
                  setError("Select one intent and a pinned definition.");
                  return;
                }
                void run(async () => {
                  const result = await api.call("api/checks/evaluate", {
                    contract_version: 1,
                    request_id: crypto.randomUUID(),
                    lab_id: api.labId,
                    operation: "check.evaluate",
                    arguments: {
                      snapshot_seq: snapshot,
                      evaluation: {
                        check_id: checkId,
                        definition_version: source.version,
                        scope: { intent_ids: [intent], signatures: [] },
                      },
                    },
                  });
                  setEvaluation(result);
                });
              }}
            >
              Evaluate pinned snapshot {snapshot ?? "unavailable"}
            </button>
          </form>
        )}
        {collection && (
          <>
            <pre>{JSON.stringify(collection, null, 2)}</pre>
            <button
              disabled={busy}
              onClick={() =>
                void run(async () => {
                  const reply = await api.call<{
                    result: NonNullable<typeof collection>;
                  }>(
                    `api/check-collections/${collection.collection_id}`,
                    undefined,
                  );
                  setCollection(reply.result);
                  changed();
                })
              }
            >
              Refresh collection status
            </button>
            <button
              disabled={!live || busy || collection.status !== "running"}
              onClick={() =>
                void run(async () => {
                  const reply = await api.call<{
                    result: NonNullable<typeof collection>;
                  }>(
                    `api/check-collections/${collection.collection_id}/cancel`,
                    {},
                  );
                  setCollection(reply.result);
                  changed();
                })
              }
            >
              Cancel collection
            </button>
          </>
        )}
        {evaluation !== undefined && (
          <div aria-label="Explicit Check evaluation">
            <pre>{JSON.stringify(evaluation, null, 2)}</pre>
          </div>
        )}
      </details>
      {error && (
        <p role="alert">
          {error}. Inspect current state before repeating a mutation with
          unknown effects.
        </p>
      )}
    </section>
  );
}
