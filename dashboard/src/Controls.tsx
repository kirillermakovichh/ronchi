import type { Sequence } from "./wire";
import { useRef, useState } from "react";
import { Api, type Status, type Json } from "./api";
interface Props {
  api: Api;
  status: Status;
  snapshot?: Sequence;
  changed: () => void;
}
interface CaseCatalog {
  result: { case_id: string; modes: string[] }[];
}
export function Controls({ api, status, snapshot, changed }: Props) {
  const [inspection, setInspection] = useState<unknown>();
  const [catalog, setCatalog] = useState<CaseCatalog>(),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false);
  const [selectedCase, setSelectedCase] = useState("tx-response-lost");
  const [catalogVisible, setCatalogVisible] = useState(false);
  const [catalogLoading,setCatalogLoading]=useState(false);
  const [catalogError,setCatalogError]=useState("");
  const catalogPending=useRef(false);
  async function loadCatalog(show=false){
    if(show)setCatalogVisible(true);
    if(catalogPending.current)return;
    catalogPending.current=true;setCatalogLoading(true);setCatalogError("");
    try{setCatalog(await api.call<CaseCatalog>("api/cases",undefined));}
    catch(error){setCatalogError(String(error));}
    finally{catalogPending.current=false;setCatalogLoading(false);}
  }
  const [customYaml,setCustomYaml]=useState("");
  const [customPlan,setCustomPlan]=useState<{yaml:string;mode:string;plan:unknown}>();
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
  const cases = catalog?.result ?? [];
  const supported = cases.some(
    (entry) => entry.case_id === selectedCase && entry.modes.includes(status.configuration.mode),
  );
  return (
    <section className="controls" aria-label="Case and Check controls">
      <details
        onToggle={(event) => {
          if (event.currentTarget.open && live && catalog === undefined) {
            void loadCatalog();
          }
        }}
      >
        <summary>
          Case controls {live ? "" : "· read-only retained Journal"}
        </summary>
        <p>Replacing a Case keeps the Journal and previously held effects.</p>
        <button
          disabled={!live || busy || catalogLoading}
          onClick={() => void loadCatalog(true)}
        >
          List Cases
        </button>
        {catalogVisible && catalog !== undefined && <pre>{JSON.stringify(catalog, null, 2)}</pre>}
        <label>
          Case
          <select
            value={catalog === undefined ? "" : selectedCase}
            disabled={!live || busy || catalog === undefined}
            onChange={(event) => setSelectedCase(event.target.value)}
          >
            {catalog === undefined && <option value="">Case catalog not loaded</option>}
            {cases.map((entry) => (
              <option
                key={entry.case_id}
                value={entry.case_id}
                disabled={!entry.modes.includes(status.configuration.mode)}
              >
                {entry.case_id}
                {entry.modes.includes(status.configuration.mode) ? "" : " · unsupported in this mode"}
              </option>
            ))}
          </select>
        </label>
        <button
          disabled={!live || busy || !supported}
          onClick={() =>
            void run(async () => {
              await api.call("api/case/arm", { case_id: selectedCase });
              changed();
            })
          }
        >
          Arm / replace {selectedCase}
        </button>
        <label>
          Custom Case YAML
          <textarea value={customYaml} maxLength={65536} disabled={!live || busy}
            onChange={(event)=>{setCustomYaml(event.target.value);setCustomPlan(undefined);}} />
        </label>
        <button disabled={!live || busy || customYaml.length===0}
          onClick={()=>void run(async()=>{
            setCustomPlan(undefined);
            const reply=await api.call<{result:unknown}>("api/case/validate",{yaml:customYaml});
            setCustomPlan({yaml:customYaml,mode:status.configuration.mode,plan:reply.result});
          })}>Validate Custom Case</button>
        {customPlan!==undefined && <pre aria-label="Resolved Custom Case plan">{JSON.stringify(customPlan.plan,null,2)}</pre>}
        <button disabled={!live || busy || customPlan===undefined || customPlan.yaml!==customYaml || customPlan.mode!==status.configuration.mode}
          onClick={()=>void run(async()=>{
            await api.call("api/case/arm",{case_id:"custom",custom_yaml:customYaml});
            changed();
          })}>Activate Custom Case</button>
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
              const collectionId = crypto.randomUUID();
              setCollection({
                collection_id: collectionId,
                status: "outcome_unknown",
                snapshot_seq: snapshot ?? 0,
              });
              void run(async () => {
                const reply = await api.call<{
                  result: NonNullable<typeof collection>;
                }>("api/checks/observe", {
                  contract_version: 1,
                  operation: "check.observe",
                  request_id: crypto.randomUUID(),
                  lab_id: api.labId,
                  arguments: {
                    collection_id: collectionId,
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
      {inspection !== undefined && (
        <details open>
          <summary>Current public Lab status</summary>
          <pre>{JSON.stringify(inspection, null, 2)}</pre>
        </details>
      )}
      {error && (
        <button
          disabled={busy}
          onClick={() =>
            void run(async () =>
              setInspection(await api.call("api/status", undefined)),
            )
          }
        >
          Inspect Lab status
        </button>
      )}
      {(error || catalogError) && (
        <p role="alert">
          {error || catalogError}. Inspect current state before repeating a mutation with
          unknown effects.
        </p>
      )}
    </section>
  );
}
