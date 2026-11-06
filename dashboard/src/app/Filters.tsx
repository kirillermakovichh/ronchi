import { createEffect, createSignal, For, Show } from "solid-js";
import { useLab } from "./context";
import type { Scope } from "../shared/transport/client";
import { coordinate } from "../shared/protocol/wire";
import { ErrorNotice } from "../shared/ui/primitives";
export function Filters() {
  const lab = useLab(),
    [open, setOpen] = createSignal(false),
    [draft, setDraft] = createSignal<Scope>({}),
    [error, setError] = createSignal("");
  createEffect(() => setDraft({ ...lab.route().scope }));
  const keys = [
    "signature",
    "rpc_method",
    "request_id",
    "kind",
    "case_activation_id",
    "from_seq",
    "to_seq",
    "since",
    "until",
  ] as const;
  return (
    <div class="filters">
      <button aria-expanded={open()} onClick={() => setOpen((v) => !v)}>
        Filters{" "}
        <Show when={Object.keys(lab.route().scope).length}>
          <span class="count-badge">
            {Object.keys(lab.route().scope).length}
          </span>
        </Show>
      </button>
      <Show when={Object.keys(lab.route().scope).length}>
        <button class="subtle" onClick={() => lab.scope({})}>
          Clear filters
        </button>
      </Show>
      <Show when={open()}>
        <form
          class="filter-popover"
          onSubmit={(e) => {
            e.preventDefault();
            setError("");
            lab.scope(draft());
            setOpen(false);
          }}
        >
          <div class="filter-grid">
            <For each={keys}>
              {(key) => (
                <label class="field">
                  {key.replaceAll("_", " ")}
                  <input
                    value={
                      Array.isArray(draft()[key])
                        ? (draft()[key] as unknown[]).join(", ")
                        : String(draft()[key] ?? "")
                    }
                    onInput={(e) => {
                      const value = e.currentTarget.value.trim(),
                        next = { ...draft() };
                      if (!value) delete next[key];
                      else if (key === "from_seq" || key === "to_seq") {
                        const c = coordinate(value);
                        if (c === undefined) {
                          setError("Use an exact Journal coordinate.");
                          return;
                        }
                        next[key] = c;
                      } else if (key === "case_activation_id") {
                        const c = value
                          .split(",")
                          .map((x) => coordinate(x.trim()));
                        if (c.some((x) => x === undefined)) {
                          setError("Use exact activation coordinates.");
                          return;
                        }
                        next[key] = c as (number | string)[];
                      } else if (key === "since" || key === "until")
                        next[key] = value;
                      else
                        next[key] = value
                          .split(",")
                          .map((x) => x.trim())
                          .filter(Boolean);
                      setError("");
                      setDraft(next);
                    }}
                  />
                </label>
              )}
            </For>
          </div>
          <ErrorNotice message={error()} />
          <div class="actions">
            <button class="primary" disabled={!!error()}>
              Apply filters
            </button>
            <button type="button" onClick={() => setOpen(false)}>
              Close
            </button>
          </div>
        </form>
      </Show>
    </div>
  );
}
