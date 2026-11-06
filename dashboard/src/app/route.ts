import { coordinate, type Sequence } from "../shared/protocol/wire";
import type { Scope } from "../shared/transport/client";
export type Page = "workspace" | "journal" | "lab";
export type Target = {
  episode?: Sequence;
  finding_id?: string;
  event_seq?: Sequence;
  request_id?: string;
};
export interface Route {
  page: Page;
  scope: Scope;
  snapshot?: Sequence;
  target?: Target;
  error?: string;
}
export function readRoute(search: string): Route {
  const p = new URLSearchParams(search),
    scope: Scope = {};
  const old: Record<string, Page> = {
    overview: "workspace",
    traffic: "workspace",
    observations: "workspace",
    case: "workspace",
  };
  const name = p.get("page") ?? "workspace";
  const page = old[name] ?? name;
  let error: string | undefined;
  if (
    !["workspace", "journal", "lab"].includes(page) ||
    p.getAll("page").length > 1
  )
    error = "Unknown dashboard page.";
  for (const k of ["signature", "rpc_method", "request_id", "kind"] as const) {
    const values = p.getAll(k);
    if (values.length) scope[k] = values;
  }
  for (const k of ["from_seq", "to_seq", "snapshot", "event", "episode"]) {
    const values = p.getAll(k);
    if (values.length > 1 || values.some((v) => coordinate(v) === undefined))
      error = `Invalid ${k}. Use an exact Journal coordinate.`;
  }
  for (const k of ["from_seq", "to_seq"] as const) {
    const v = p.get(k);
    if (v) scope[k] = coordinate(v);
  }
  for (const k of ["since", "until"] as const) {
    const v = p.get(k);
    if (v) scope[k] = v;
  }
  const activations = p.getAll("case_activation_id");
  if (activations.length) {
    if (activations.some((v) => coordinate(v) === undefined))
      error = "Invalid Case activation.";
    else scope.case_activation_id = activations.map((v) => coordinate(v)!);
  }
  const finding = p.get("finding"),
    event = p.get("event"),
    episode = p.get("episode");
  if (finding && !/^f1-[a-f0-9]{64}$/.test(finding))
    error = "Invalid Finding reference.";
  if ([finding, event, episode].filter(Boolean).length > 1)
    error = "Select one detail reference.";
  const target = finding
    ? { finding_id: finding }
    : event
      ? { event_seq: coordinate(event) }
      : episode
        ? { episode: coordinate(episode) }
        : undefined;
  return {
    page: page as Page,
    scope,
    snapshot: p.get("snapshot") ? coordinate(p.get("snapshot")!) : undefined,
    target,
    error,
  };
}
export function routeUrl(lab: string, route: Route) {
  const p = new URLSearchParams({ page: route.page });
  for (const [k, v] of Object.entries(route.scope)) {
    if (v !== undefined && !["finding_id", "event_refs"].includes(k))
      for (const x of Array.isArray(v) ? v : [v]) p.append(k, String(x));
  }
  if (route.snapshot !== undefined) p.set("snapshot", String(route.snapshot));
  if (route.target?.episode !== undefined)
    p.set("episode", String(route.target.episode));
  if (route.target?.event_seq !== undefined)
    p.set("event", String(route.target.event_seq));
  if (route.target?.finding_id) p.set("finding", route.target.finding_id);
  return `${location.origin}/ui/${encodeURIComponent(lab)}/?${p}`;
}
