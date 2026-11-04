import { useEffect, useState } from "react";
import type { Scope } from "./api";
import { selection } from "./selection";
const names: Record<string, string> = {
  signature: "Signature",
  rpc_method: "RPC method",
  request_id: "Request ID",
  kind: "Record type",
  case_activation_id: "Case activation",
  from_seq: "First record",
  to_seq: "Last record",
  since: "From time (RFC 3339)",
  until: "Until time (RFC 3339)",
};
export function Filters({
  scope,
  apply,
}: {
  scope: Scope;
  apply: (scope: Scope) => void;
}) {
  const [error, setError] = useState("");
  function values(current: Scope): Record<string, string> {
    return Object.fromEntries(
      Object.keys(names).map((key) => {
        const value = current[key as keyof Scope];
        return [
          key,
          Array.isArray(value) ? value.join(", ") : String(value ?? ""),
        ];
      }),
    );
  }
  const [draft, setDraft] = useState(() => values(scope));
  useEffect(() => setDraft(values(scope)), [scope]);
  return (
    <details className="filter-panel">
      <summary>
        Filter this investigation
        {Object.values(scope).some((v) =>
          Array.isArray(v) ? v.length : v !== undefined,
        )
          ? " · filters applied"
          : ""}
      </summary>
      <form
        className="filters"
        onSubmit={(event) => {
          event.preventDefault();
          const params = new URLSearchParams();
          for (const [key, value] of new FormData(event.currentTarget)) {
            const values = [
              "signature",
              "rpc_method",
              "request_id",
              "kind",
              "case_activation_id",
            ].includes(key)
              ? String(value).split(",")
              : [String(value)];
            for (const entry of values)
              if (entry.trim()) params.append(key, entry.trim());
          }
          const result = selection("?" + params);
          if (result.error) {
            setError(result.error);
            return;
          }
          setError("");
          apply(result.scope);
        }}
      >
        <p className="form-intro">
          Filters apply across Observations, Traffic and Journal. Separate
          multiple values with commas.
        </p>
        <div className="filter-fields">
          {["signature", "rpc_method"].map((key) => (
            <label key={key}>
              {names[key]}
              <input
                name={key}
                value={draft[key] ?? ""}
                onChange={(event) =>
                  setDraft((old) => ({ ...old, [key]: event.target.value }))
                }
              />
            </label>
          ))}
        </div>
        <details>
          <summary>More filters</summary>
          <div className="filter-fields">
            {[
              "request_id",
              "kind",
              "case_activation_id",
              "from_seq",
              "to_seq",
              "since",
              "until",
            ].map((key) => (
              <label key={key}>
                {names[key]}
                <input
                  aria-label={key === "kind" ? "kind" : undefined}
                  name={key}
                  value={draft[key] ?? ""}
                  onChange={(event) =>
                    setDraft((old) => ({ ...old, [key]: event.target.value }))
                  }
                />
              </label>
            ))}
          </div>
        </details>
        <div className="actions">
          <button className="primary">Apply filters</button>
          <button
            type="button"
            onClick={() => {
              setError("");
              apply({});
            }}
          >
            Clear filters
          </button>
        </div>
        {error && <p role="alert">{error}</p>}
      </form>
    </details>
  );
}
export function FilterChips({
  scope,
  apply,
}: {
  scope: Scope;
  apply: (scope: Scope) => void;
}) {
  return (
    <div className="filter-chips" aria-label="Applied filters">
      {Object.entries(scope)
        .filter(([key]) => key in names)
        .flatMap(([key, value]) =>
          (Array.isArray(value)
            ? value
            : value === undefined
              ? []
              : [value]
          ).map((v, index) => (
            <button
              key={`${key}:${index}`}
              title={`Remove ${names[key]} ${v}`}
              onClick={() => {
                const next = { ...scope };
                if (Array.isArray(value)) {
                  const left = value.filter((_, i) => i !== index);
                  if (left.length) Object.assign(next, { [key]: left });
                  else delete next[key as keyof Scope];
                } else delete next[key as keyof Scope];
                apply(next);
              }}
            >
              {names[key]}: <span>{String(v)}</span>
              <span aria-hidden="true"> ×</span>
            </button>
          )),
        )}
    </div>
  );
}
