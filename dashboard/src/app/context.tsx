import { createContext, useContext, type Accessor } from "solid-js";
import type {
  Api,
  QueryReply,
  Status,
  Scope,
} from "../shared/transport/client";
import type { Sequence } from "../shared/protocol/wire";
import type { Route, Target, Page } from "./route";
export interface LabContext {
  api: Api;
  status: Accessor<Status | undefined>;
  summary: Accessor<QueryReply | undefined>;
  route: Accessor<Route>;
  authorized: Accessor<boolean>;
  live: Accessor<boolean>;
  snapshot: Accessor<Sequence | undefined>;
  navigate: (page: Page) => void;
  scope: (scope: Scope) => void;
  select: (target: Target) => void;
  back: () => void;
  update: (route: Route, push?: boolean) => void;
}
export const Lab = createContext<LabContext>();
export function useLab() {
  const value = useContext(Lab);
  if (!value) throw new Error("Missing Lab context");
  return value;
}
