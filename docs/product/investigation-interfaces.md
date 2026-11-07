# Investigation interfaces

Status: accepted initial implementation contract, version `1`; no released API compatibility promise.
This document owns shared query semantics, CLI/MCP operation names, query limits and machine responses.
The [dashboard contract](dashboard.md) owns browser presentation and live transport.
The [evidence contract](evidence-and-findings.md) owns facts, correlation, Findings, and persistence limits.
The [Lab lifecycle](lab-lifecycle.md) owns workspace resolution, process ownership, readiness, and mutation effects.
All three interfaces call the same core operations against one Lab Journal.

## Implemented Investigation surface

The Rust query core supports finite summary, RPC request, Timeline, Evidence Slice, and Findings queries.
Schema 6 stores indexed navigation metadata in the same transaction as each Journal event.
Signed bytes remain in the original event storage.
Queries use one read-only connection per page and release its SQLite snapshot before returning.
Cursors bind the immutable Lab, committed watermark, normalized selection, projection, payload choice, and page size.
Older Journals use a read-only SQL adapter with an explicit correlation gap.
The core limits concurrent readers to four and cancels SQLite VM work at its deadline.
Summary executes the versioned Findings rules and reports their evaluation states and Evidence Gaps.
CLI exposes `investigate summary|requests|timeline|evidence|findings` with `--signature`, `--rpc-method`, `--request-id`, `--case-activation`, `--kind`, `--event-seq`, `--event-ref LAB_ID:EVENT_SEQ`, `--from-seq`, `--to-seq`, `--since`, `--until`, `--fields`, `--limit`, and `--cursor`.
Each invocation returns one page and never follows a stream or fetches all pages.
Live CLI queries use authenticated `POST /v1/labs/{id}/investigate` with the typed core request.
Retained CLI queries open the exact Journal read-only without starting a Lab.
`schema investigate.summary|investigate.requests|investigate.timeline|investigate.evidence|investigate.findings` exposes shared request and success/error models offline.
JSON mode emits one envelope; text mode escapes terminal controls in untrusted values.
Evidence accepts `--include-payload` and returns a SQL-bounded preview of the selected retained payload.
The preview ceiling is 4096 UTF-8 bytes before JSON escaping; the core can reduce it further to fit the full envelope.
It never cuts a UTF-8 character or substitutes replacement characters.
`payload.retained_bytes` is the UTF-8 length of the retained representation; base64 counts encoded text bytes.
`payload.encoding` is `base64`, `text`, or `json`; a truncated JSON/base64 preview is a fragment, not a complete decoded value.
`payload.payload_truncated` describes the preview independently of item pagination.
Mandatory `payload_status` distinguishes `available`, `absent`, `redacted`, `omitted_by_policy`, and `lost` when the retained facts declare that state.
The query never reverses redaction or invents missing content.
Normal views omit signed bytes, raw bodies, and transaction payloads.
Ctrl-C cancels the query with exit 130 and keeps the Lab running.
The query deadline returns exit 124; stdout pipe closure cancels further reading and exits 0.
Native process checks verify these paths on macOS, including reader release and continued RPC writes.
`investigate findings` evaluates the full selected scope and returns a finite page of versioned Findings.
Summary evaluates the same scope and returns at most five Findings plus `findings_more`.
`analysis.findings` is `performed` or `operational_limit`; per-rule reports distinguish match, no match, insufficient evidence, unsupported evidence, and operational limits.
The [rule contract](evidence-and-findings.md#implemented-version-1-rules) defines requirements and resource limits.
`limit`, `cursor`, and `fields` change presentation, not the analyzed facts.
Findings always retain identity, semantic scope, supporting roles, material limitations, and `evidence_arguments`.
`fields` additionally selects `statement` and `guidance`.
`limitations` remains available as an explicit field path and is always retained.
Each `evidence_arguments` object selects the same scope, stable `finding_id`, and `snapshot_seq`.
Supply those arguments to `investigate.evidence` to resolve the rule’s supporting records.
The CLI equivalent uses `--finding-id <id> --snapshot-seq <seq>` with the original scope filters.
An unknown or out-of-scope ID returns `finding_unavailable` (exit 3); incomplete analysis that cannot resolve it returns `analysis_incomplete` (exit 5).
Finding evidence requires an explicit watermark and never silently selects a newer snapshot.
`investigate.findings` also accepts `scope.finding_id` with an explicit snapshot to resolve one exact Finding and its full selected observation window.
This avoids recomputing a bookmarked window from only the incident's supporting records.
All operations accept optional `snapshot_seq` (`--snapshot-seq`) to pin an existing committed watermark.
Findings cursors additionally bind the maintained rule definitions and versions.
The authenticated control operation uses the existing `POST /v1/labs/{id}/investigate` route.
`schema investigate.findings` works offline and includes the Finding and rule-definition schemas.
Its `maintained_case_rules` field publishes additional Case rule definitions and versions.
Dashboard and MCP use these shared query operations.
Follow streams and export have separate contracts.

## Operations and identity

| Core operation | CLI | MCP tool | Effect |
| --- | --- | --- | --- |
| `lab.up` | `ronchi up --detach` | `ronchi_lab_up` | Start a background Lab and wait for readiness within the startup deadline |
| `lab.status` | `ronchi status` | `ronchi_lab_status` | Inspect readiness, mode, endpoints, Journal state, health, and active Case |
| `lab.down` | `ronchi down` | `ronchi_lab_down` | Stop owned resources and finalize the Journal |
| `case.list` | `ronchi case list` | `ronchi_case_list` | List available built-in Cases and mode requirements |
| `case.arm` | `ronchi case arm <case>` / `ronchi case activate <file>` | `ronchi_case_arm` | Arm or replace a built-in or validated Custom Case |
| `case.validate` | `ronchi case validate <file> --mode <mode>` | `ronchi_case_validate` | Inspect a bounded Custom Case plan without effects |
| `case.disarm` | `ronchi case disarm` | `ronchi_case_disarm` | Stop selecting the policy for new requests |
| `investigate.summary` | `ronchi investigate summary` | `ronchi_investigate_summary` | Return compact counts, up to five Findings, gaps, and drill-down references |
| `investigate.requests` | `ronchi investigate requests` | `ronchi_investigate_requests` | Return a bounded page of received RPC requests with related exchange facts |
| `investigate.timeline` | `ronchi investigate timeline` | `ronchi_investigate_timeline` | Return a bounded Timeline page |
| `investigate.evidence` | `ronchi investigate evidence` | `ronchi_investigate_evidence` | Resolve selected evidence references or an explicit payload projection |
| `investigate.findings` | `ronchi investigate findings` | `ronchi_investigate_findings` | Return a bounded page of deterministic Findings |

Journal queries and mutations require an immutable `lab_id` after target resolution; the connection never selects an implicit Lab.
Bootstrap operations are exceptions: `lab.up` accepts a project-local name, `lab.status` can resolve a name, and the offline `case.list` needs no Lab.
Human CLI `ronchi up` stays in the terminal and stops its owned Lab on Ctrl-C.
`--detach` selects background lifetime independently of `--json`.
Foreground `--json` flushes one readiness envelope, then waits for shutdown without progress text.
Normal shutdown adds no stdout record.
If the Lab fails after readiness, the CLI emits a structured error on a subsequent JSON line and exits nonzero.
The `lab.up` schema describes core startup settings; `--detach` is a CLI lifetime option.

Schemas and help also need no Lab.
Bootstrap responses expose the resolved identity when one exists.
CLI `--lab <name>` resolves the human Lab name and returns the resolved `lab_id`.
CLI `--lab-id <id>` selects an exact retained Journal, including a stopped Lab.
Without an exact ID, queries select the current named Lab, or its latest stopped Lab; responses declare `selection_basis:current|latest|explicit`.
Mutations resolve only the current named Lab; an absent live target returns a state conflict instead of modifying history.
Every new Lab has a new `lab_id`; names may be reused without making old references ambiguous.
An event reference is `{lab_id,event_seq}`; a Finding reference adds its stable `finding_id`.
Links preserve the source Lab, selected scope, and snapshot so the dashboard and machine views agree.
A Finding scope uses `signature` and `accepted_request_id` for an accepted transaction.
A request-based scope uses `request_id`, `rpc_method`, and an optional known `signature` without asserting acceptance.
Existing accepted-transaction Findings preserve their wire fields and semantic IDs.
An Investigation has query state, not a separate start, capture, or stop lifecycle.

### Foreground terminal presentation

Interactive `up` uses an inline Ronchi panel with the parallel-band identity.
A compact Unicode Braille mark keeps four bands and their centered rise within two terminal rows.
The mark and secondary hints share the same yellow accent.
The runtime-details block uses normal-foreground labels and hints; the Lab name and RPC/dashboard addresses are yellow.
It retains scrollback and normal terminal input; it does not use raw mode or an alternate screen.
The panel shows the RPC URL and a short OSC 8 dashboard hyperlink.
The hyperlink remains reusable until the Lab stops.
`ronchi dashboard` prints the same Lab link; use the same project and Lab selectors for a non-default Lab.
Browser authorization has no 30-minute timeout; reopen that link if the browser discards its session cookie.
Live launch metadata reports `expires_in_seconds:null` and `expires_on:"lab_stop"`.
A retained viewer instead reports `expires_on:"viewer_stop"` and remains read-only.
Terminals without hyperlink support can use that command to print the full launch URL.

Startup and normal shutdown animate at most eight times per second.
Ready-state motion runs twice per second and indicates the running Lab, not RPC traffic or a Finding.
One authenticated status request at a time updates the Case, held RPC count, and committed Journal watermark.
Requests start no more than once per second and never evaluate Findings or scan Journal history.
The panel validates the returned Lab ID, process identity, and contract version.
Missing status appears as unavailable, with any old watermark marked as last observed.
Held RPC requests use the yellow signal accent; they do not imply an application failure or business correctness.

`up --no-animation` keeps a static styled panel and disables live polling.
It conflicts with `--detach`.
Nonempty `NO_COLOR` and `CLICOLOR=0` suppress color and emphasis.
JSON, redirected stdout, CI, and absent or `dumb` TERM use the existing plain renderer.
These presentation choices do not change foreground ownership or Ctrl-C behavior.
Normal terminal completion reports the verified finalized Journal watermark.
An interrupted or unconfirmed completion never receives a finalized success label.

Native PTY tests cover live Case changes, a paused status source, resize, actual Ctrl-C input,
external stop, static/color opt-outs, JSON/CI/dumb output, and abrupt daemon death.

### Implemented Case operations

`case list` works offline and lists maintained Cases with their supported modes.
The current simulator catalog includes `tx-response-lost`, `tx-confirmation-stalls`, `finality-delayed`, and `provider-timeout-burst`.
`provider-timeout-burst` also supports managed Surfpool.
The dashboard Case selector uses this core catalog and disables unsupported modes.
`case arm tx-response-lost` arms or replaces a continuing policy without restarting the Lab.
`case disarm` changes later registrations; existing holds retain their original policy and deadline.
Arm accepts `--hold-ms` (1..20000, default 20000), `--max-held` (1..16, default 16), and up to 16 repeated exact `--signature` filters.
Timeout arm additionally accepts repeated `--rpc-method`, `--outage-ms`, and `--recovery-ms` under the [Case contract](cases.md#implemented-timeout-coverage-claim).
Each successful arm/disarm commits a new `registration_seq`; arm uses that sequence as its immutable policy revision.
Repeated arm creates a new revision; it does not clear the identity ledger.
Invalid settings or unsupported Case IDs leave the current revision unchanged.
Mutations require an authenticated ready Lab; retained or stopping Labs return `state_conflict`.
A transport deadline can leave mutation effects unknown; inspect status before another mutation.

Authenticated control routes are `GET /v1/labs/{id}/cases`, `POST /v1/labs/{id}/case/arm`, and `POST /v1/labs/{id}/case/disarm`.
Arm's strict JSON model uses `case_id`, `hold_ms`, `max_held`, `signatures`, and optional typed `confirmation` settings.
The [confirmation-delay claim](cases.md#verified-simulator-confirmation-delay-claim) defines those settings and evidence limits.
Unknown arm keys return a structured `invalid_argument` without changing policy.
Disarm accepts an empty body or `{}` and rejects supplied settings.
Transient control-queue capacity returns `state_conflict`; Journal failure returns a product error and fails the Lab.
The CLI uses these routes and the same core mutation operation.
`schema case.list`, `schema case.arm`, `schema case.disarm`, and `schema case.validate` work offline.
The Custom Case catalog and retained plan contract live in [Custom Cases](custom-cases.md#prototype-authoring-and-inspection).
Status exposes `active_case`, `case_registration_seq`, and bounded `held_effects` with original policy revision and remaining real milliseconds.
MCP exposes these controls through the operation names above.

## Bounded query contract

Requests use `contract_version:1`, a transport-specific `request_id`, an operation, and typed arguments.
Lab-scoped requests include the resolved `lab_id`; bootstrap exceptions follow the identity rules above.
Investigation arguments contain `scope`, `limit`, `cursor`, `fields`, and `include_payload`; omitted scope means the selected Lab.
`scope` accepts event references, `finding_id`, transaction `signature`, `rpc_method`, `request_id`, `case_activation_id`, event `kind`, `from_seq`, `to_seq`, `since`, and `until`.
Lists within one filter use OR; different filters use AND.
Sequence bounds are inclusive; UTC RFC 3339 time bounds use `[since,until)` over recorded time.
Recorded time is a filter, not proof of causal order.
Default Timeline order is ascending `event_seq`; explicit causal links distinguish relationships from concurrent observations.
An initial query captures `snapshot_seq`, the highest committed Journal sequence.
Every page uses that watermark, even while new events arrive.
The opaque cursor binds Lab, snapshot, normalized filters, projection, and continuation position.
Changing those inputs with a cursor returns `invalid_cursor`; cursors are not durable bookmarks.
A fresh query captures a new snapshot; saved references retain exact identities and scope.
Queries read and project in the storage layer; renderers do not load the full Journal and trim it afterward.

Provisional tuning defaults: `limit=50`, maximum `200` items, maximum encoded core response `256 KiB`, query deadline `10 s`.
Requests are bounded to `64 KiB` and at most `200` aggregate filter values; over-budget input returns `invalid_argument` before query execution.
The same limits apply to the dashboard, CLI, and MCP; a request above a maximum returns `invalid_argument`.
Summary returns at most five Findings and counts at the snapshot; additional Findings use their paged operation.
`fields` selects documented typed paths; unknown paths return `invalid_field`.
Projection always retains identity, kind, source, necessary correlation references, and completeness metadata.
`include_payload` defaults to `false`; only the selected evidence operation can request retained payload fields.
If a payload cannot fit, return its bounded preview, retained byte length, and `payload_truncated:true`; never imply that preview is complete.
If one projected item still cannot fit, return `result_too_large` with a smaller projection or interval as the recovery step.
Exhaustive evidence retrieval is an explicit client traversal of bounded pages, not an unbounded response.

### Ordering

Shared queries accept optional `order:asc|desc` (`--order`).
Omitted order remains ascending for CLI/MCP compatibility.
Descending record cursors bind direction and snapshot.
Summary and Findings can order incidents by descending start sequence, with stable Finding-ID tie-breaking.
`investigate.findings` accepts an optional 0..4096 `offset` (`--offset`) inside the bounded evaluated result.
New live query results can shift incident positions; an explicit snapshot holds a fixed selection.
Existing response, preview, analysis and candidate limits apply.

### RPC request projection

`investigate.requests` returns one item for each selected `rpc.registered` record.
It uses the same ordering, scope, watermark, cursor, response budget and cancellation rules as Timeline.
`request_id` is an OR-list filter within the shared scope and intersects other filters.
The CLI spelling is `--request-id`.
Requests always select registered requests; a conflicting record-kind filter yields an empty page.

Default request fields include `exchange`.
Its `boundary`, `prepared`, `acceptance`, `withholding` and `dispatch` entries contain the latest retained record of each supported kind for that exact request ID through the watermark, or null.
Each entry retains its event reference, kind, source, elapsed coordinate, signature and compact kind-specific facts.
Boundary facts include transport, prepared-result class, shared withholding and client receipt.
Prepared content is excluded from the list projection and stays available through Evidence Slice.
The projection uses the existing request index and does not read original event bodies or create a new Journal schema.
Explicit field selection can omit `exchange` to reduce response size.
Missing records remain unknown, including incomplete legacy correlation and shared withholding not attached to a particular batch item.
These summaries do not infer client receipt, timeout, retries or business completion.
Use the Evidence Slice with the same `request_id` to page through all related retained records.

### Implemented projection paths

Summary accepts `counts`, `traffic_state`, `lab_context`, `analysis`, and `findings`.
`lab_context` preserves the Lab execution mode and Case binding at the watermark, independent of the record filters.
It reports unknown legacy Case context explicitly.
It always retains `lab_context`, Findings analysis, and the bounded Findings list.
Event queries accept `event_ref`, `kind`, `schema_version`, `source`, `recorded_at`, `elapsed_ns`, `execution_mode`, `rpc_method`, `transaction_signature`, `transaction_signatures`, `request_id`, `batch_id`, `registration_seq`, `policy_ref`, `cause_refs`, `data`, `simulated_time_ns`, `slot`, `block_height`, `payload_status`, and `payload`.
`data` means the payload-free typed event facts, not the original event body.
`payload` requires Evidence Slice and explicit `include_payload:true`.
An explicit payload request returns the preview even when `fields` omits `payload`.
Projection always retains event identity, kind, source, schema, execution mode, correlation references, and payload status.
The response always retains the selected scope, watermark, Journal state, gaps, and pagination metadata.
Unknown paths return `invalid_field`.

## Responses and completeness

The shared result envelope contains exactly one of `result` or `error`:

```json
{"contract_version":1,"request_id":"q1","lab_id":"L1","result":{"items":[]},"view":{"snapshot_seq":42,"scope":{},"observation_boundary":["rpc","simulator"],"evidence_gaps":[],"journal_state":"open","selection_complete":true},"page":{"next_cursor":null,"truncated":false,"reason":null}}
```

`view` describes the selected snapshot, not every observation that could exist outside Ronchi.
`selection_complete` means all matching retained records in this selection have been returned; it does not establish business correctness.
`journal_state` is `open`, `finalized`, or `interrupted`; an open Journal can still have a complete bounded selection.
`evidence_gaps` describes missing observations required by the requested conclusion, including interrupted or omitted evidence.
`page.next_cursor` identifies more retained matches; `page.truncated` identifies a byte-budget stop before the requested item limit.
`page.reason` is `response_bytes` for that stop; ordinary pagination uses `null`.
Payload previews have their own truncation flag even when the item selection is complete.
An empty selection, no Case match, missing Client Outcome, and unsupported mode capability remain distinct states.
No query converts missing evidence into a `pass`, `fail`, or an assumed application state.

Errors contain `code`, `message`, `phase`, `retryable`, `effects`, and a concrete `recovery` action.
`effects` is `none`, `applied`, or `unknown`; mutation timeouts require status inspection before any retry.
Error responses include a request ID and the Lab ID when resolution succeeded; they do not include a success `result`.

| Exit | Error class | Examples |
| --- | --- | --- |
| `0` | Operation completed | Empty selection, no match, Evidence Gaps, injected RPC errors in evidence |
| `2` | Invalid request | `invalid_argument`, `invalid_field`, `invalid_cursor`, unknown command |
| `3` | Unavailable target | `lab_unavailable`, `journal_unavailable` |
| `4` | Unsupported capability | `unsupported_capability` with mode or instrumentation guidance |
| `5` | State conflict | `state_conflict`, incompatible active Lab settings |
| `6` | Product failure | `internal_error`, Journal read failure |
| `124` | Deadline exceeded | `deadline_exceeded` with phase and effects |
| `130` | Caller cancellation | `cancelled`; completed mutations remain applied |

## CLI and MCP transport behavior

CLI defaults to concise text; `--json` emits one finite response envelope on stdout with no ANSI, banner, or progress prose.
Text summaries show Lab identity, mode, Case state, relevant Findings, Evidence Gaps, and the next drill-down command.
Diagnostics use stderr; structured operation errors use the JSON envelope on stdout, including parser and startup errors.
`--help` and `ronchi schema <operation>` work without a Lab or upstream; schemas describe version `1` requests and responses.
The initial query interface has no follow stream; repeated summary requests fetch new bounded snapshots.
Ctrl-C cancels the current query or wait; it does not stop the Lab or roll back an accepted mutation.
If stdout closes, CLI cancels remaining query work and exits successfully without fetching more pages.

`ronchi mcp stdio` attaches a local adapter to the selected Project Workspace; EOF or adapter exit never stops the Lab.
The adapter may expose explicit `lab.up` and `lab.down` tools, whose effects follow the lifecycle contract.
Stdio stdout contains MCP messages only; diagnostics go to stderr.
MCP tool arguments and structured results map to the shared typed operations; MCP framing stays separate from the result envelope.
If a supported client requires text compatibility content, return a short consistent summary rather than duplicating the evidence JSON.
Malformed protocol requests use MCP protocol errors; valid operations that fail use tool-result error signaling and the structured error.
Successful empty selections and recorded injected RPC errors are successful tool results.
Supported MCP cancellation stops cancellable request work; after uncertain mutation completion, inspect Lab status.
Tool schemas declare actual read-only/idempotent effects; no tool executes shell commands, starts the backend, or accepts arbitrary code.
The adapter contains no model, registration, cloud call, or remote transport requirement.

### Implemented stdio adapter

`ronchi --project <directory> mcp stdio` serves the twelve tools in the operation table with official `rmcp` 3.8.0.
The adapter selects only a Project Workspace.
Global `--lab` and `--lab-id` are rejected; each scoped tool requires its own immutable `lab_id`.
`lab.up` accepts `name` and typed configuration `arguments`.
`lab.status` accepts either `name` or `lab_id`.
Case arm and Investigation tools accept their shared typed `arguments`.
Tool listing includes generated input/output schemas and effect annotations.
The adapter supports legacy initialization and modern discovery through the SDK.
Process checks use protocol `2025-11-25` initialization and `2026-07-28` discovery.
They do not establish compatibility with every MCP client.

Each tool returns one shared structured envelope and a short text status.
Encoded tool arguments are limited to 64 KiB.
The SDK bounds the full framed input line at 128 KiB and discards larger lines without operation execution.
A valid bounded call above the argument budget returns `invalid_argument`.
At most four operations execute concurrently in one adapter.
Structured result data is limited to 256 KiB.
If a completed mutation exceeds the result budget, its error retains the resolved identity and reports `effects:applied`.
The caller must inspect status before another mutation.

Startup cancellation uses the ordinary owned startup cleanup path.
Cancellation of a control mutation wait reports unknown effects because the Lab may have accepted it.
EOF drains SDK responses and detaches; a ready Lab and its continuing Case remain alive.

## Example paths

```bash
ronchi up --detach --case tx-response-lost --json
ronchi investigate summary --lab default --json
ronchi investigate timeline --lab-id L1 --signature A --limit 50 --json
ronchi investigate evidence --lab-id L1 --event-seq 12 --fields kind,source,transaction_signature --json
ronchi investigate findings --lab-id L1 --cursor '<opaque-cursor>' --json
ronchi down --lab default --json
```

Normal summary: `L1 · simulator · tx-response-lost · 1 matching submission`; one Finding links acceptance, withheld response, and a later status request for `A`.
Its separate guidance asks the developer to check how the backend completes the operation after that status; it does not claim a Client Outcome or economic completion.
No RPC traffic: the Journal still contains startup/control records; a matching RPC selection returns `items:[]`, no Finding, and `selection_complete:true` at its actual watermark.
Text asks the developer to drive backend traffic.
No matching Fault: traffic can exist without a selected/applied Fault. Report the actual Case context and retained evidence; absence of a match is not a successful resilience result.
No Finding: matching evidence exists, but no versioned rule has enough facts; the Timeline remains available.
Capability example: a requested execution-only conclusion in simulator mode returns `unsupported_capability`, exit `4`, and Surfpool guidance; ordinary simulator queries still succeed with their stated limits.
Product failure example: a Journal read fails with `internal_error`, exit `6`, `effects:none`, and a Lab status/health inspection path; it never becomes a System Under Test Finding.

Implementation checks cover the same scoped result across all three interfaces, snapshot pagination during appends, projection before payload reads, and stable evidence links after stop.
Process checks cover clean JSON/MCP stdout, parser errors, empty/no-match states, byte-budget continuation, cancellation, pipe closure, and mutation outcome inspection.
