import {
  createEffect,
  createMemo,
  createSignal,
  onCleanup,
  For,
  Show,
  type JSX,
} from "solid-js";
import { CopyIcon } from "./CopyIcon";
import { object, text } from "../lib/format";
export function Icon(props: { name: string; size?: number }) {
  const paths: Record<string, JSX.Element> = {
    traffic: (
      <>
        <path d="M3 7h14m-4-4 4 4-4 4M21 17H7m4-4-4 4 4 4" />
      </>
    ),
    cases: (
      <>
        <path d="m9 3-6 9h8l-2 9 12-13h-9l2-5Z" />
      </>
    ),
    observations: (
      <>
        <path d="M4 4h16v13H9l-5 4V4Z" />
        <path d="M8 8h8M8 12h5" />
      </>
    ),
    inspector: (
      <>
        <circle cx="10" cy="10" r="6" />
        <path d="m15 15 6 6M10 7v6M7 10h6" />
      </>
    ),
    grip: (
      <>
        <path d="M8 5h.01M8 12h.01M8 19h.01M15 5h.01M15 12h.01M15 19h.01" />
      </>
    ),
    pause: <path d="M8 5v14M16 5v14" />,
  };
  return (
    <svg
      width={props.size ?? 16}
      height={props.size ?? 16}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="1.6"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    >
      {paths[props.name] ?? paths.inspector}
    </svg>
  );
}
export function Empty(props: {
  title: string;
  children?: JSX.Element;
  icon?: string;
}) {
  return (
    <div class="empty">
      <Icon name={props.icon ?? "inspector"} size={25} />
      <strong>{props.title}</strong>
      <div>{props.children}</div>
    </div>
  );
}
export function ErrorNotice(props: { message?: string }) {
  return (
    <Show when={props.message}>
      <p class="error" role="alert">
        {props.message}
      </p>
    </Show>
  );
}
export function Copy(props: {
  value: string;
  label?: string;
  iconOnly?: boolean;
}) {
  const value = createMemo(() => props.value);
  const [copied, setCopied] = createSignal(false),
    [notice, setNotice] = createSignal("");
  let timer: ReturnType<typeof setTimeout> | undefined,
    disposed = false,
    attempt = 0;
  createEffect(() => {
    value();
    attempt++;
    clearTimeout(timer);
    setCopied(false);
    setNotice("");
  });
  onCleanup(() => {
    disposed = true;
    attempt++;
    clearTimeout(timer);
  });
  async function copy() {
    const current = ++attempt;
    try {
      await navigator.clipboard.writeText(value());
      if (disposed || current !== attempt) return;
      clearTimeout(timer);
      setNotice("");
      setCopied(true);
      timer = setTimeout(() => setCopied(false), 1000);
    } catch {
      if (disposed || current !== attempt) return;
      clearTimeout(timer);
      setCopied(false);
      setNotice("Select and copy manually");
    }
  }
  return (
    <span class="copy">
      <button
        type="button"
        class={props.iconOnly ? "copy-icon-button" : "subtle"}
        classList={{ copied: copied() }}
        aria-label={
          props.iconOnly
            ? copied()
              ? "Signature copied"
              : (props.label ?? "Copy")
            : undefined
        }
        title={
          props.iconOnly
            ? copied()
              ? "Copied"
              : (props.label ?? "Copy")
            : props.value
        }
        onClick={() => void copy()}
      >
        {props.iconOnly ? (
          <CopyIcon copied={copied()} />
        ) : (
          (props.label ?? "Copy")
        )}
      </button>
      <Show when={copied()}>
        <small role="status" class={props.iconOnly ? "sr-only" : undefined}>
          Copied
        </small>
      </Show>
      <Show when={notice()}>
        <small role="alert">{notice()}</small>
      </Show>
    </span>
  );
}
export function Raw(props: { value: unknown; label?: string; open?: boolean }) {
  const [open, setOpen] = createSignal(props.open ?? false);
  return (
    <details
      class="raw"
      open={props.open}
      onToggle={(e) => setOpen(e.currentTarget.open)}
    >
      <summary>{props.label ?? "Exact JSON"}</summary>
      <Show when={open()}>
        <pre>{JSON.stringify(props.value, null, 2)}</pre>
      </Show>
    </details>
  );
}
export function Fields(props: { value: unknown; depth?: number }) {
  const entries = () => Object.entries(object(props.value));
  return (
    <Show
      when={props.value !== null && typeof props.value === "object"}
      fallback={
        <span>
          {typeof props.value === "boolean"
            ? String(props.value)
            : props.value === null
              ? "null"
              : text(props.value, "Unavailable")}
        </span>
      }
    >
      <Show
        when={(props.depth ?? 0) < 3}
        fallback={<span class="muted">Open exact JSON for nested fields</span>}
      >
        <dl class="fields">
          <For each={entries().slice(0, 16)}>
            {([key, value]) => (
              <div>
                <dt>{key.replaceAll("_", " ")}</dt>
                <dd>
                  <Show
                    when={typeof value !== "string" || value.length <= 400}
                    fallback={<span>{String(value).slice(0, 400)}…</span>}
                  >
                    <Fields value={value} depth={(props.depth ?? 0) + 1} />
                  </Show>
                </dd>
              </div>
            )}
          </For>
        </dl>
        <Show when={entries().length > 16}>
          <p class="muted">More fields in exact JSON.</p>
        </Show>
      </Show>
    </Show>
  );
}
export function Tabs(props: {
  items: readonly (readonly [string, string])[];
  value: string;
  onChange: (value: string) => void;
  label?: string;
}) {
  return (
    <div
      class="tabs"
      role="tablist"
      aria-label={props.label ?? "Detail sections"}
    >
      <For each={props.items}>
        {([id, label], index) => (
          <button
            role="tab"
            aria-selected={props.value === id}
            tabIndex={props.value === id ? 0 : -1}
            onClick={() => props.onChange(id)}
            onKeyDown={(e) => {
              const n = props.items.length,
                i = index();
              const next =
                e.key === "ArrowRight"
                  ? (i + 1) % n
                  : e.key === "ArrowLeft"
                    ? (i + n - 1) % n
                    : e.key === "Home"
                      ? 0
                      : e.key === "End"
                        ? n - 1
                        : undefined;
              if (next === undefined) return;
              e.preventDefault();
              props.onChange(props.items[next][0]);
              (
                e.currentTarget.parentElement?.children[next] as HTMLElement
              )?.focus();
            }}
          >
            {label}
          </button>
        )}
      </For>
    </div>
  );
}
