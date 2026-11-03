# Investigation interfaces

Status: accepted initial implementation contract, version `1`; no released API compatibility promise.
This document owns dashboard behavior, CLI/MCP operation names, query limits, and machine responses.
The [evidence contract](evidence-and-findings.md) owns facts, correlation, Findings, and persistence limits.
The [Lab lifecycle](lab-lifecycle.md) owns workspace resolution, process ownership, readiness, and mutation effects.
All three interfaces call the same core operations against one Lab Journal.

## Implemented Investigation surface

The Rust query core supports finite summary, Timeline, Evidence Slice, and Findings queries.
Schema 6 stores indexed navigation metadata in the same transaction as each Journal event.
Signed bytes remain in the original event storage.
Queries use one read-only connection per page and release its SQLite snapshot before returning.
Cursors bind the immutable Lab, committed watermark, normalized selection, projection, payload choice, and page size.
Older Journals use a read-only SQL adapter with an explicit correlation gap.
The core limits concurrent readers to four and cancels SQLite VM work at its deadline.
Summary executes the versioned Findings rules and reports their evaluation states and Evidence Gaps.
CLI exposes `investigate summary|timeline|evidence|findings` with `--signature`, `--rpc-method`, `--case-activation`, `--kind`, `--event-seq`, `--event-ref LAB_ID:EVENT_SEQ`, `--from-seq`, `--to-seq`, `--since`, `--until`, `--fields`, `--limit`, and `--cursor`.
Each invocation returns one page and never follows a stream or fetches all pages.
Live CLI queries use authenticated `POST /v1/labs/{id}/investigate` with the typed core request.
Retained CLI queries open the exact Journal read-only without starting a Lab.
`schema investigate.summary|investigate.timeline|investigate.evidence|investigate.findings` exposes shared request and success/error models offline.
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
Supply those arguments to `investigate.evidence` to resolve the five supporting records.
The CLI equivalent uses `--finding-id <id> --snapshot-seq <seq>` with the original scope filters.
An unknown or out-of-scope ID returns `finding_unavailable` (exit 3); incomplete analysis that cannot resolve it returns `analysis_incomplete` (exit 5).
Finding evidence requires an explicit watermark and never silently selects a newer snapshot.
All operations accept optional `snapshot_seq` (`--snapshot-seq`) to pin an existing committed watermark.
Findings cursors additionally bind the maintained rule definitions and versions.
The authenticated control operation uses the existing `POST /v1/labs/{id}/investigate` route.
`schema investigate.findings` works offline and includes the Finding and rule-definition schemas.
Dashboard, MCP, follow streams, and export remain separate tasks.
The sections below retain the full accepted contract.

## Operations and identity

| Core operation | CLI | MCP tool | Effect |
| --- | --- | --- | --- |
| `lab.up` | `ronchi up` | `ronchi_lab_up` | Start a background Lab and wait for readiness within the startup deadline |
| `lab.status` | `ronchi status` | `ronchi_lab_status` | Inspect readiness, mode, endpoints, Journal state, health, and active Case |
| `lab.down` | `ronchi down` | `ronchi_lab_down` | Stop owned resources and finalize the Journal |
| `case.list` | `ronchi case list` | `ronchi_case_list` | List available built-in Cases and mode requirements |
| `case.arm` | `ronchi case arm <case>` | `ronchi_case_arm` | Arm or replace the active Case under the lifecycle contract |
| `case.disarm` | `ronchi case disarm` | `ronchi_case_disarm` | Stop selecting the policy for new requests |
| `check.observe` | `ronchi check observe <check-id>` | `ronchi_check_observe` | Collect bounded observations from a configured source into a live Journal |
| `check.evaluate` | `ronchi check evaluate <check-id>` | `ronchi_check_evaluate` | Evaluate retained evidence at a pinned snapshot without contacting the source |
| `investigate.summary` | `ronchi investigate summary` | `ronchi_investigate_summary` | Return compact counts, up to five Findings, gaps, and drill-down references |
| `investigate.timeline` | `ronchi investigate timeline` | `ronchi_investigate_timeline` | Return a bounded Timeline page |
| `investigate.evidence` | `ronchi investigate evidence` | `ronchi_investigate_evidence` | Resolve selected evidence references or an explicit payload projection |
| `investigate.findings` | `ronchi investigate findings` | `ronchi_investigate_findings` | Return a bounded page of deterministic Findings |

Journal queries and mutations require an immutable `lab_id` after target resolution; the connection never selects an implicit Lab.
Bootstrap operations are exceptions: `lab.up` accepts a project-local name, `lab.status` can resolve a name, and the offline `case.list` needs no Lab.
Schemas and help also need no Lab.
Bootstrap responses expose the resolved identity when one exists.
CLI `--lab <name>` resolves the human Lab name and returns the resolved `lab_id`.
CLI `--lab-id <id>` selects an exact retained Journal, including a stopped Lab.
Without an exact ID, queries select the current named Lab, or its latest stopped Lab; responses declare `selection_basis:current|latest|explicit`.
Mutations resolve only the current named Lab; an absent live target returns a state conflict instead of modifying history.
Every new Lab has a new `lab_id`; names may be reused without making old references ambiguous.
An event reference is `{lab_id,event_seq}`; a Finding reference adds its stable `finding_id`.
Links preserve the source Lab, selected scope, and snapshot so the dashboard and machine views agree.
An Investigation has query state, not a separate start, capture, or stop lifecycle.

### Implemented Case operations

`case list` works offline and currently lists only simulator `tx-response-lost`.
`case arm tx-response-lost` arms or replaces a continuing policy without restarting the Lab.
`case disarm` changes later registrations; existing holds retain their original policy and deadline.
Arm accepts `--hold-ms` (1..20000, default 20000), `--max-held` (1..16, default 16), and up to 16 repeated exact `--signature` filters.
Each successful arm/disarm commits a new `registration_seq`; arm uses that sequence as its immutable policy revision.
Repeated arm creates a new revision; it does not clear the identity ledger.
Invalid settings or unsupported Case IDs leave the current revision unchanged.
Mutations require an authenticated ready Lab; retained or stopping Labs return `state_conflict`.
A transport deadline can leave mutation effects unknown; inspect status before another mutation.

Authenticated control routes are `GET /v1/labs/{id}/cases`, `POST /v1/labs/{id}/case/arm`, and `POST /v1/labs/{id}/case/disarm`.
Arm's strict JSON model uses `case_id`, `hold_ms`, `max_held`, and `signatures`.
Unknown arm keys return a structured `invalid_argument` without changing policy.
Disarm accepts an empty body or `{}` and rejects supplied settings.
Transient control-queue capacity returns `state_conflict`; Journal failure returns a product error and fails the Lab.
The CLI uses these routes and the same core mutation operation.
`schema case.list`, `schema case.arm`, and `schema case.disarm` work offline.
Status exposes `active_case`, `case_registration_seq`, and bounded `held_effects` with original policy revision and remaining real milliseconds.
No MCP adapter or Findings operation is implied by these controls.

## First dashboard

The embedded React interface now uses the same query, Case, and Check core as the CLI.
`ronchi dashboard` launches a live browser session without opening a browser automatically.
`ronchi dashboard --serve --lab-id <id>` serves retained evidence without starting a Lab.
See [build and launch](../development.md#dashboard-build-and-launch) for commands and session limits.
The browser keeps live status separate from its pinned query prefix.
It uses server filtering, 50-item pages, and explicit payload previews.
Application collection and evaluation require explicit actions.
The current Check form selects one intent and a configured definition version.
HTTP-only collection, its 32 KiB cap, and evaluation budgets remain unchanged.
Broader acceptance gaps remain tracked under task 14.


The first dashboard has a Lab header, a compact Investigation summary, a Timeline, and an evidence detail panel.
The header shows mode, RPC URL, Lab status, Journal health, Observation Boundary, and active Case.
Case controls list mode requirements and expose arm, replace, and disarm actions.
Changing the Case retains the selected Investigation and the Journal.
The summary shows counts, bounded Findings, Evidence Gaps, and whether the selected scope is still open.
Selecting a Finding opens its supporting Evidence Slice and highlights the related Timeline entries.
Selecting a Fault or transaction signature narrows the same Investigation.
The first filters are Case activation, signature, RPC method, event kind, and time or sequence interval.
The Timeline distinguishes request receipt, simulator/backend decisions, Fault application, RPC Boundary Result, and later observations.
The detail panel labels every observation's source and follows its correlation and causal references.
It shows the evidence-backed Finding and suggested backend checks in separate blocks.
Raw payloads are collapsed and fetched only on explicit selection.
Configured Checks expose separate collect and evaluate actions under the same public operation contract.
Collect requires a live Lab; evaluate can use retained evidence after stop.
The dashboard polls bounded summaries while live; opening a detail freezes its snapshot until the user selects refresh.

| State | Visible behavior |
| --- | --- |
| No Lab | Show startup options; do not imply a running capture |
| Starting | Show readiness phase and bounded startup progress |
| Ready, no traffic | Show the RPC URL and the instruction to drive the independently running backend |
| Armed, no match | Show `waiting_for_match`; do not show a resilience success |
| Traffic, no Finding | Show the Timeline and `no_finding`; do not invent a defect or Verdict |
| Evidence Gap | Show the missing source and the conclusions it prevents |
| Stopped/interrupted | Keep retained evidence readable and label Journal state and last committed sequence |
| Core failure | Show a product failure separately from an injected RPC Fault or an observed application action |

## Bounded query contract

Requests use `contract_version:1`, a transport-specific `request_id`, an operation, and typed arguments.
Lab-scoped requests include the resolved `lab_id`; bootstrap exceptions follow the identity rules above.
Investigation arguments contain `scope`, `limit`, `cursor`, `fields`, and `include_payload`; omitted scope means the selected Lab.
`scope` accepts event references, `finding_id`, transaction `signature`, `rpc_method`, `case_activation_id`, event `kind`, `from_seq`, `to_seq`, `since`, and `until`.
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

## Public Check operations

Both Check operations select a configured `check_id`, its definition version, immutable `lab_id`, and explicit bounded `scope`.
Check scope includes the relevant intent IDs or signatures and the observation interval; it does not infer business correlation from a Timeline selection.
`check.observe` pins the current committed `snapshot_seq` for the source request and invokes the configured read-only HTTP adapter.
The implemented CLI returns bounded collection metadata, the last `invocation_id`, committed observation references, and the new watermark after collection.
An ordinary call collects once; `--poll --horizon-seconds 60` explicitly enables bounded polling.
The authenticated control start/status/cancel paths are documented in the [Check contract](checks-and-verdicts.md).
For a complete retained observation, select its exact event and follow bounded `payload.next_offset` values with `--include-payload --payload-offset N`.
The first offset is zero; keep the same pinned snapshot for subsequent chunks.
If collection uses polling, each HTTP invocation has its own ID and timeout within the declared bounded horizon.
Cancellation stops the remaining polls.
The ordinary query deadline does not replace the explicit Check invocation and horizon limits.
`check.evaluate` is implemented as `ronchi check evaluate <check-id> --definition-version N --snapshot-seq N`.
It requires one explicit intent/signature scope and `snapshot_seq` and evaluates only retained facts through that watermark.
It returns the pinned definition version/digest, evaluation version, source authority, bounded Verdict, supporting references, and Evidence Gaps without appending observations or calling external sources.
Its authenticated control path is `POST /v1/labs/{id}/checks/evaluate`.
The JSON request uses `arguments.evaluation` for Check identity/version/digest and exact normalized source scope.
An empty requirement set has no Verdict.
Exit 0 means completed evaluation, including a fail or inconclusive result.
Re-evaluation is available for stopped or interrupted Journals and preserves their completeness limits.
The [Check contract](checks-and-verdicts.md) owns adapter limits, rules, and Verdict meanings.

## Example paths

```bash
ronchi up --case tx-response-lost --json
ronchi investigate summary --lab default --json
ronchi investigate timeline --lab-id L1 --signature A --limit 50 --json
ronchi investigate evidence --lab-id L1 --event-seq 12 --fields kind,source,transaction_signature --json
ronchi investigate findings --lab-id L1 --cursor '<opaque-cursor>' --json
ronchi check observe intent-state --lab-id L1 --intent-id W1 --json
ronchi check evaluate intent-state --lab-id L1 --intent-id W1 --snapshot-seq 42 --json
ronchi down --lab default --json
```

Normal summary: `L1 · simulator · tx-response-lost · 1 matching submission`; one Finding links acceptance, withheld response, and a later status request for `A`.
Its separate guidance asks the developer to check how the backend completes the operation after that status; it does not claim a Client Outcome or economic completion.
No RPC traffic: the Journal still contains startup/control records; a matching RPC selection returns `items:[]`, no Finding, and `selection_complete:true` at its actual watermark.
Text asks the developer to drive backend traffic.
No match: the Lab has traffic, `case_state:waiting_for_match`, and `matched_requests:0`; exit remains `0`.
No Finding: matching evidence exists, but no versioned rule has enough facts; the Timeline remains available.
Capability example: a requested execution-only conclusion in simulator mode returns `unsupported_capability`, exit `4`, and Surfpool guidance; ordinary simulator queries still succeed with their stated limits.
Product failure example: a Journal read fails with `internal_error`, exit `6`, `effects:none`, and a Lab status/health inspection path; it never becomes a System Under Test Finding.

Implementation checks cover the same scoped result across all three interfaces, snapshot pagination during appends, projection before payload reads, and stable evidence links after stop.
Process checks cover clean JSON/MCP stdout, parser errors, empty/no-match states, byte-budget continuation, cancellation, pipe closure, and mutation outcome inspection.
