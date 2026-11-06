import {
  createEffect,
  createMemo,
  createSignal,
  For,
  Show,
  onCleanup,
  type Accessor,
} from "solid-js";
import { useLab } from "../../app/context";
import { createAction } from "../../shared/transport/action";
import { caseInfo, object, text } from "../../shared/lib/format";
import { ErrorNotice, Raw } from "../../shared/ui/primitives";
import type { Json } from "../../shared/transport/client";
interface Catalog {
  result: { case_id: string; modes: string[] }[];
}
export function Cases(props: { active: Accessor<boolean> }) {
  const lab = useLab(),
    action = createAction(),
    [catalog, setCatalog] = createSignal<Catalog>(),
    [catalogError, setCatalogError] = createSignal(""),
    [selected, setSelected] = createSignal("tx-response-lost"),
    [notice, setNotice] = createSignal(""),
    [yaml, setYaml] = createSignal(""),
    [plan, setPlan] = createSignal<{
      yaml: string;
      mode: string;
      value: unknown;
    }>();
  let catalogPending = false;
  const live = () => lab.status()?.state === "ready",
    mode = () => lab.status()?.configuration.mode ?? "";
  const current = createMemo(() => caseInfo(lab.status()?.active_case));
  const policy = () => object(lab.status()?.active_case),
    hits = () => {
      const activity = lab.status()?.case_activity;
      if (
        !activity ||
        String(activity.policy_revision) !== String(policy().revision)
      )
        return undefined;
      return /^\d+$/.test(String(activity.applied_records))
        ? activity.applied_records
        : undefined;
    };
  const supported = () =>
    catalog()?.result.some(
      (c) => c.case_id === selected() && c.modes.includes(mode()),
    );
  async function load() {
    if (catalogPending) return;
    catalogPending = true;
    setCatalogError("");
    try {
      setCatalog(await lab.api.call<Catalog>("api/cases", undefined));
    } catch (e) {
      setCatalogError(String(e));
    } finally {
      catalogPending = false;
    }
  }
  createEffect(() => {
    if (props.active() && lab.authorized() && !catalog() && !catalogError())
      void load();
  });
  let mounted = true;
  onCleanup(() => {
    mounted = false;
  });
  async function arm() {
    setNotice("");
    await action.run(async (signal) => {
      const reply = await lab.api.call<{ result?: Json }>(
        "api/case/arm",
        { case_id: selected() },
        signal,
      );
      if (mounted)
        setNotice(
          `Case enabled. Activated at (UTC): ${text(object(object(reply.result).policy).activated_at, "confirmed by Ronchi")}.`,
        );
    });
  }
  return (
    <div class="case-controls">
      <div class="case-state">
        <span classList={{ "status-dot": true, enabled: !!current().id }} />
        <strong>
          {current().id
            ? hits() !== undefined && BigInt(hits()!) > 0n
              ? "Fault applied"
              : hits() === undefined
                ? "Enabled · activity unavailable"
                : "Enabled · waiting for a match"
            : "No Case enabled"}
        </strong>
        <Show when={current().id}>
          <span class="hit-count">
            {hits() === undefined ? "Count unavailable" : `${hits()} applied`}
          </span>
        </Show>
      </div>
      <Show when={current().id}>
        <p class="caption">{current().title}</p>
        <p class="caption">
          Activated{" "}
          {text(policy().activated_at, "time unavailable").slice(11, 19)} UTC
        </p>
      </Show>
      <label class="field">
        Case
        <select
          aria-label="Case"
          value={selected()}
          onChange={(e) => setSelected(e.currentTarget.value)}
        >
          <Show
            when={catalog()}
            fallback={
              <option value="tx-response-lost">Loading catalog…</option>
            }
          >
            <For each={catalog()?.result}>
              {(entry) => (
                <option
                  value={entry.case_id}
                  selected={entry.case_id === selected()}
                  disabled={!entry.modes.includes(mode())}
                >
                  {caseInfo(entry).title}
                  {entry.modes.includes(mode()) ? "" : " · unsupported"}
                </option>
              )}
            </For>
          </Show>
        </select>
      </label>

      <div class="actions">
        <button
          class="primary"
          disabled={!live() || action.busy() || !supported()}
          onClick={() => void arm()}
        >
          Arm Case
        </button>
        <button
          disabled={!live() || action.busy() || !current().id}
          onClick={() =>
            void action.run(async (signal) => {
              await lab.api.call("api/case/disarm", {}, signal);
              setNotice(
                "Case disabled for future requests. Existing held requests keep their deadlines.",
              );
            })
          }
        >
          Disarm
        </button>
      </div>
      <details class="policy-description">
        <summary>Policy details</summary>
        <p class="case-description">
          {caseInfo({ case_id: selected() }).description}
        </p>
      </details>
      <Show when={!live()}>
        <p class="caption">Retained Lab · controls are read-only.</p>
      </Show>
      <Show when={notice()}>
        <p role="status" class="confirmation">
          {notice()}
        </p>
      </Show>
      <ErrorNotice message={action.error() || catalogError()} />
      <Show when={catalogError()}>
        <button onClick={() => void load()}>Retry Case catalog</button>
      </Show>
      <details class="custom-editor">
        <summary>Custom Case</summary>
        <label class="field">
          Custom Case YAML
          <textarea
            value={yaml()}
            maxLength={65536}
            spellcheck={false}
            onInput={(e) => {
              setYaml(e.currentTarget.value);
              setPlan(undefined);
            }}
          />
        </label>
        <div class="actions">
          <button
            disabled={!live() || action.busy() || !yaml()}
            onClick={() =>
              void action.run(async (signal) => {
                const value = await lab.api.call<{ result: unknown }>(
                  "api/case/validate",
                  { yaml: yaml() },
                  signal,
                );
                setPlan({ yaml: yaml(), mode: mode(), value: value.result });
              })
            }
          >
            Validate Custom Case
          </button>
          <button
            disabled={
              !live() ||
              action.busy() ||
              !plan() ||
              plan()?.yaml !== yaml() ||
              plan()?.mode !== mode()
            }
            onClick={() =>
              void action.run(async (signal) => {
                await lab.api.call(
                  "api/case/arm",
                  { case_id: "custom", custom_yaml: yaml() },
                  signal,
                );
                setNotice("Custom Case enabled.");
              })
            }
          >
            Activate Custom Case
          </button>
        </div>
        <Show when={plan()}>
          <Raw value={plan()?.value} label="Resolved Custom Case plan" open />
        </Show>
      </details>
    </div>
  );
}
