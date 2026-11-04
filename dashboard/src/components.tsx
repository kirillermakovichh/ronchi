import { useState } from "react";
import { text } from "./presentation";
export function CopyButton({
  value,
  children = "Copy",
  label,
}: {
  value: string;
  children?: React.ReactNode;
  label?: string;
}) {
  const [message, setMessage] = useState("");
  return (
    <span className="copy-control">
      <button
        aria-label={label}
        onClick={async () => {
          try {
            await navigator.clipboard.writeText(value);
            setMessage("Copied");
          } catch {
            setMessage("Select and copy the text manually.");
          }
        }}
      >
        {children}
      </button>
      {message && <small role="status">{message}</small>}
    </span>
  );
}
export function Raw({
  value,
  label = "Technical details and JSON",
}: {
  value: unknown;
  label?: string;
}) {
  return (
    <details className="raw">
      <summary>{label}</summary>
      <pre>{JSON.stringify(value, null, 2)}</pre>
    </details>
  );
}
function valueLabel(value: unknown): string {
  if (value === null) return "null";
  if (typeof value === "boolean") return value ? "true" : "false";
  return text(value, "Unavailable");
}
export function Fields({
  value,
  depth = 0,
}: {
  value: unknown;
  depth?: number;
}) {
  if (value === null || typeof value !== "object")
    return <span className="fact-value">{valueLabel(value)}</span>;
  const entries = Object.entries(value);
  if (!entries.length)
    return (
      <span className="muted">
        {Array.isArray(value) ? "Empty list" : "No retained fields"}
      </span>
    );
  if (depth >= 3)
    return (
      <span className="muted">
        {entries.length} fields · open technical details for exact data
      </span>
    );
  return (
    <dl className="facts-tree">
      {entries.slice(0, 12).map(([key, child]) => (
        <div key={key}>
          <dt>{key.replaceAll("_", " ")}</dt>
          <dd>
            {typeof child === "string" && child.length > 400 ? (
              `${child.slice(0, 400)}… (display shortened)`
            ) : (
              <Fields value={child} depth={depth + 1} />
            )}
          </dd>
        </div>
      ))}
      {entries.length > 12 && (
        <div>
          <dt>More fields</dt>
          <dd>{entries.length - 12} additional fields in technical details</dd>
        </div>
      )}
    </dl>
  );
}
export function GapList({
  gaps,
}: {
  gaps: { code: string; message: string }[];
}) {
  if (!gaps.length) return null;
  return (
    <section className="knowledge-gap">
      <h3>What this evidence cannot establish</h3>
      {gaps.map((g) => (
        <p key={g.code}>{g.message}</p>
      ))}
      <details>
        <summary>Evidence Gap identifiers</summary>
        <ul>
          {gaps.map((g) => (
            <li key={g.code}>
              <code>{g.code}</code>
            </li>
          ))}
        </ul>
      </details>
    </section>
  );
}
export function TabBar({
  tabs,
  selected,
  choose,
}: {
  tabs: [string, string][];
  selected: string;
  choose: (value: string) => void;
}) {
  return (
    <div className="tabs" role="tablist" aria-label="Detail sections">
      {tabs.map(([id, label], index) => (
        <button
          key={id}
          role="tab"
          aria-selected={id === selected}
          tabIndex={id === selected ? 0 : -1}
          onClick={() => choose(id)}
          onKeyDown={(event) => {
            const next =
              event.key === "ArrowRight"
                ? (index + 1) % tabs.length
                : event.key === "ArrowLeft"
                  ? (index + tabs.length - 1) % tabs.length
                  : event.key === "Home"
                    ? 0
                    : event.key === "End"
                      ? tabs.length - 1
                      : undefined;
            if (next === undefined) return;
            event.preventDefault();
            choose(tabs[next][0]);
            (
              event.currentTarget.parentElement?.children[
                next
              ] as HTMLButtonElement
            ).focus();
          }}
        >
          {label}
        </button>
      ))}
    </div>
  );
}
