import { useEffect, useState } from "react";
import {
  Api,
  ApiError,
  bootstrap,
  type QueryReply,
  type Scope,
  type Status,
} from "./api";
import type { Sequence } from "./wire";
export function useLab(
  api: Api,
  scope: Scope,
  snapshot: Sequence | undefined,
  refresh: number,
  invalid?: string,
) {
  const [authorized, setAuthorized] = useState(false),
    [error, setError] = useState(""),
    [statusError, setStatusError] = useState("");
  const [authorization, setAuthorization] = useState(0);
  const [status, setStatus] = useState<Status>(),
    [summary, setSummary] = useState<QueryReply>();
  const [committedScope, setCommittedScope] = useState(scope),
    [loading, setLoading] = useState(true);
  useEffect(() => {
    let active = true;
    if (!api.labId) {
      setLoading(false);
      return;
    }
    async function authorize() {
      setAuthorized(false);
      try {
        await bootstrap(api);
        if (active) {
          setAuthorized(true);
          setAuthorization((n) => n + 1);
        }
      } catch (e) {
        if (active) {
          setError(String(e));
          setLoading(false);
        }
      }
    }
    const reopen = () => {
      if (new URLSearchParams(location.hash.slice(1)).get("ticket"))
        void authorize();
    };
    void authorize();
    window.addEventListener("hashchange", reopen);
    return () => {
      active = false;
      window.removeEventListener("hashchange", reopen);
    };
  }, [api]);

  useEffect(() => {
    if (!authorized || invalid) return;
    const controller = new AbortController();
    let timer: ReturnType<typeof setTimeout>;
    setLoading(true);
    async function load() {
      if (document.hidden) {
        timer = setTimeout(load, 2500);
        return;
      }
      try {
        const [current, result] = await Promise.all([
          api.call<Status>("api/status", undefined, controller.signal),
          api.query(
            "summary",
            scope,
            snapshot,
            undefined,
            {},
            controller.signal,
          ),
        ]);
        if (controller.signal.aborted) return;
        setStatus(current);
        setStatusError("");
        setSummary(result);
        setCommittedScope(scope);
        setError("");
        setLoading(false);
        if (snapshot === undefined && current.state === "ready")
          timer = setTimeout(load, 2500);
      } catch (e) {
        if (controller.signal.aborted) return;
        setError(
          e instanceof ApiError
            ? `Operational query error: ${String(e)}. Previous evidence remains available.`
            : `Connection lost: ${String(e)}. Previous evidence remains available.`,
        );
        setLoading(false);
        if (snapshot === undefined) timer = setTimeout(load, 2500);
      }
    }
    void load();
    return () => {
      controller.abort();
      clearTimeout(timer);
    };
  }, [api, authorized, authorization, scope, snapshot, refresh, invalid]);
  useEffect(() => {
    if (!authorized || snapshot === undefined || status?.state !== "ready")
      return;
    const controller = new AbortController();
    let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      if (!document.hidden) {
        try {
          const current = await api.call<Status>(
            "api/status",
            undefined,
            controller.signal,
          );
          if (!controller.signal.aborted) {
            setStatus(current);
            setStatusError("");
          }
        } catch (e) {
          if (!controller.signal.aborted)
            setStatusError(
              `Connection lost. Last known Lab status is retained. ${String(e)}`,
            );
        }
      }
      if (!controller.signal.aborted) timer = setTimeout(poll, 2500);
    }
    timer = setTimeout(poll, 2500);
    return () => {
      controller.abort();
      clearTimeout(timer);
    };
  }, [api, authorized, authorization, snapshot, status?.state, refresh]);
  return {
    authorized,
    error: invalid ?? (error || statusError),
    status,
    summary,
    committedScope,
    loading,
  };
}
