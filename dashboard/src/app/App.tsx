import {
  batch,
  createEffect,
  createMemo,
  createSignal,
  onCleanup,
  onMount,
  Show,
} from "solid-js";
import {
  Api,
  bootstrap,
  type Scope,
  type Status,
  type QueryReply,
} from "../shared/transport/client";
import { createSubscription } from "../shared/transport/subscription";
import { Copy, ErrorNotice, Icon } from "../shared/ui/primitives";
import { Lab, type LabContext } from "./context";
import {
  readRoute,
  routeUrl,
  type Route,
  type Target,
  type Page,
} from "./route";
import { Workspace } from "../features/workspace/Workspace";
import { RpcRates } from "../features/traffic/RpcRates";
import { Traffic } from "../features/traffic/Traffic";
import { ObservationList } from "../features/observations/ObservationList";
import { Inspector } from "../features/inspector/Inspector";
import { Cases } from "../features/cases/Cases";
import { LabPage } from "../features/lab/LabPage";
import { Filters } from "./Filters";
export function App() {
  const labId = decodeURIComponent(location.pathname.split("/")[2] ?? ""),
    api = new Api(labId);
  const [route, setRoute] = createSignal<Route>(readRoute(location.search)),
    [authorized, setAuthorized] = createSignal(false),
    [authError, setAuthError] = createSignal(""),
    [related, setRelated] = createSignal<Scope>(),
    [previous, setPrevious] = createSignal<Target>();
  function update(next: Route, push = false) {
    const url = new URL(routeUrl(labId, next));
    history[push ? "pushState" : "replaceState"](
      null,
      "",
      url.pathname + url.search,
    );
    setRoute(next);
  }
  async function authorize() {
    try {
      await bootstrap(api);
      batch(() => {
        setAuthorized(true);
        setAuthError("");
      });
    } catch (e) {
      setAuthError(String(e));
    }
  }
  onMount(() => {
    void authorize();
    const pop = () => setRoute(readRoute(location.search)),
      hash = () => {
        if (location.hash.includes("ticket=")) void authorize();
      };
    window.addEventListener("popstate", pop);
    window.addEventListener("hashchange", hash);
    onCleanup(() => {
      window.removeEventListener("popstate", pop);
      window.removeEventListener("hashchange", hash);
      api.dispose();
    });
  });
  const status = createSubscription<Status>(api, () =>
    authorized() ? { path: "api/status" } : undefined,
  );
  const scope = createMemo(() => route().scope),
    snapshot = createMemo(() => route().snapshot);
  const summary = createSubscription<QueryReply>(api, () =>
    authorized() && !route().error
      ? {
          path: "api/investigate",
          body: {
            contract_version: 1,
            request_id: "workspace-summary",
            lab_id: labId,
            operation: "investigate.summary",
            arguments: {
              scope: scope(),
              snapshot_seq: snapshot(),
              fields: ["counts", "traffic_state"],
              order: "desc",
            },
          },
        }
      : undefined,
  );
  const live = () =>
      snapshot() === undefined &&
      ["ready", "starting", "stopping"].includes(status.value()?.state ?? ""),
    watermark = () => snapshot() ?? summary.value()?.view.snapshot_seq;
  const context: LabContext = {
    api,
    status: status.value,
    summary: summary.value,
    route,
    authorized,
    live,
    snapshot: watermark,
    update,
    navigate(page: Page) {
      update({ ...route(), page }, true);
    },
    scope(next) {
      setRelated(undefined);
      update({ ...route(), scope: next }, true);
    },
    select(target) {
      const current = route().target;
      if (current?.episode !== undefined || current?.finding_id)
        setPrevious(current);
      if (target.episode !== undefined || target.finding_id)
        setPrevious(target);
      update({ ...route(), target }, true);
    },
    back() {
      update({ ...route(), target: previous() }, true);
    },
  };
  const evidenceLink = () =>
    routeUrl(labId, { ...route(), snapshot: watermark() });
  createEffect(() => {
    document.title = `Ronchi · ${status.value()?.name ?? "Workspace"}`;
  });
  return (
    <Lab.Provider value={context}>
      <div class="app">
        <a class="skip-link" href="#main">
          Skip to content
        </a>
        <header class="topbar">
          <div class="brand">
            <svg width="27" height="24" viewBox="0 0 30 24" aria-hidden="true">
              <path
                d="M1 4h9l5-3 5 3h9M1 9h9l5-3 5 3h9M1 14h9l5-3 5 3h9M1 19h9l5-3 5 3h9"
                fill="none"
                stroke="currentColor"
                stroke-width="1.4"
              />
            </svg>
            <strong>Ronchi</strong>
            <span class="top-divider">/</span>
            <span>{status.value()?.name ?? "Connecting"}</span>
            <span class="mode-label">
              {status.value()?.configuration.mode === "surfpool"
                ? "Surfpool"
                : "Simulator"}
            </span>
          </div>
          <div class="top-actions">
            <Show when={status.value()?.rpc_address}>
              <code class="rpc-url">http://{status.value()?.rpc_address}</code>
              <Copy
                value={`http://${status.value()?.rpc_address}`}
                label="Copy RPC"
                copiedLabel="RPC URL copied"
                iconOnly
              />
            </Show>
            <span
              classList={{
                "connection-state": true,
                "is-live": live() && !status.error(),
              }}
            >
              <i />
              {status.error()
                ? "Reconnecting"
                : live()
                  ? "Live"
                  : snapshot() !== undefined
                    ? "Paused"
                    : "Retained"}
            </span>
            <Show
              when={live()}
              fallback={
                <Show
                  when={
                    snapshot() !== undefined &&
                    status.value()?.state === "ready"
                  }
                >
                  <button
                    onClick={() => update({ ...route(), snapshot: undefined })}
                  >
                    Resume live
                  </button>
                </Show>
              }
            >
              <button
                class="subtle"
                onClick={() => update({ ...route(), snapshot: watermark() })}
              >
                <Icon name="pause" size={12} /> Pause
              </button>
            </Show>
          </div>
        </header>
        <div class="navigation-bar">
          <nav aria-label="Main navigation">
            {(
              [
                ["workspace", "Workspace"],
                ["journal", "Journal"],
                ["lab", "Lab"],
              ] as [Page, string][]
            ).map(([id, title]) => (
              <button
                aria-current={route().page === id ? "page" : undefined}
                onClick={() => context.navigate(id)}
              >
                {title}
              </button>
            ))}
          </nav>
          <div class="runtime-metrics">
            <RpcRates
              traffic={status.value()?.rpc_traffic}
              unavailable={!!status.error()}
            />
            <span
              class="lab-health-card"
              classList={{
                "is-healthy":
                  status.value()?.health === "healthy" && !status.error(),
                "is-failed": status.value()?.health === "failed",
              }}
            >
              <i aria-hidden="true" />
              <span>{status.value()?.health ?? "Connecting"}</span>
            </span>
          </div>
        </div>
        <main id="main">
          <ErrorNotice
            message={
              route().error || authError() || status.error() || summary.error()
            }
          />
          <Show when={status.error() || summary.error()}>
            <details class="recovery">
              <summary>Open retained history after this Lab stops</summary>
              <p>
                Run this command in a terminal. It starts a read-only viewer
                without restarting the Lab.
              </p>
              <pre>{`ronchi --project '${(status.value()?.configuration.project ?? ".").replaceAll("'", "'\\''")}' --lab-id ${labId} dashboard --serve --selection '${new URL(evidenceLink()).search.slice(1).replaceAll("'", "'\\''")}'`}</pre>
            </details>
          </Show>
          <Show when={authError()}>
            <button onClick={() => void authorize()}>Retry connection</button>
            <p class="caption">
              Open this Lab's dashboard launch link to authorize this browser.
            </p>
          </Show>
          <Show when={authorized()}>
            <div class="scope-bar">
              <Filters />
            </div>
            <Workspace
              active={route().page === "workspace"}
              render={(id, active) =>
                id === "cases" ? (
                  <Cases active={active} />
                ) : id === "traffic" ? (
                  <Traffic active={active} related={related} />
                ) : id === "observations" ? (
                  <ObservationList active={active} />
                ) : (
                  <Inspector active={active} related={setRelated} />
                )
              }
            />
            <section class="journal-page" hidden={route().page !== "journal"}>
              <div class="journal-traffic">
                <Traffic journal active={() => route().page === "journal"} />
              </div>
              <aside class="journal-inspector">
                <Inspector
                  active={() => route().page === "journal"}
                  related={setRelated}
                />
              </aside>
            </section>
            <LabPage active={route().page === "lab"} />
          </Show>
        </main>
        <footer class="app-footer">
          <span>Make hidden failures visible.</span>
          <span>Local Lab · {live() ? "recording" : "retained evidence"}</span>
        </footer>
      </div>
    </Lab.Provider>
  );
}
