# Checks and Verdicts

Status: generic Check collection and Verdict evaluation remain implementation work (tasks 11–12). The Reference Application implements the read-only HTTP source endpoint described below; that does not implement the generic adapter.
These are reversible defaults for the first implementation.
The [evidence contract](evidence-and-findings.md) owns Journal facts and Findings.
This document owns external application observations and their bounded outcome evaluation.
The optional application SDK remains deferred; neither the Demo nor generic Checks requires it.

## Observation and evaluation

A Check observes explicitly selected state and evaluates a declared requirement over that state and committed Journal Evidence.
An observation reports typed facts from its named source.
An evaluator applies a versioned deterministic rule; an observer does not supply an authoritative Verdict.
A `pass` is bounded to that rule, scope, evidence, and observation interval.
It does not establish general business correctness or production resilience.
Without configured Checks, an Investigation has Findings and no Verdict.

| Check definition field | Contract |
| --- | --- |
| `check_id`, `version` | Stable identity and immutable definition version |
| `source` | Named HTTP observation endpoint, allowed origin, schema, and source authority |
| `scope` | Explicit intent IDs, transaction signatures, and bounded Run or Investigation interval |
| `requires` | Required evidence sources, fields, correlation, mode, and completeness |
| `rule` | Maintained declarative predicate with typed field paths and explicit success/failure conditions |
| `deadline`, `poll_interval` | Observation horizon and bounded polling policy |
| `limits` | Invocation timeout, response bytes, and allowed fact count |

The first evaluator supports typed equality, ordering, set membership, counts, and all/any composition.
It has no embedded language, code evaluation, shell command, or executable plugin.
A rule can compare reported intent signatures with retained submission identities only when explicit business correlation supplies that relationship.
An injected RPC error is not itself a failed application Check.
An adapter failure is not evidence that the System Under Test violated a Business Invariant.

## Implemented Check configuration (task 11a)

Named sources live in the existing `ronchi/config.toml` version-1 configuration.
The initial transport accepts exact HTTP URLs; HTTPS is not implemented by this adapter.
URL user information, query strings, and fragments are rejected before Lab startup.
Loopback IPs and `localhost` are allowed by default.
Other hosts require an explicit `allowed_origin` equal to the parsed URL origin.
Redirects are never part of origin authorization.
`authorization_env` names runtime Authorization configuration; it does not contain the credential value.

```toml
schema_version = 1
[checks]
concurrency = 4
[[checks.sources]]
check_id = "intent-state"
version = 1
source = "reference-relayer"
schema_version = 1
url = "http://127.0.0.1:9001/checks/observe"
[checks.sources.facts.intent]
type = "record"
exact = false
[checks.sources.facts.intent.fields.state]
type = "enum"
values = ["accepted", "prepared", "submitting", "reconciling", "succeeded", "failed", "unresolved"]
[checks.sources.facts.intent.fields.original_signature]
type = "availability"
[checks.sources.facts.intent.fields.original_signature.value]
type = "string"
max_bytes = 88
```

Schemas support boolean, arbitrary-precision JSON integer, finite decimal string, bounded string, enum, bounded array, and record.
`nullable` explicitly permits a null value; `availability` validates a nested availability wrapper.
Available facts require a value; unavailable/redacted facts forbid a substituted value.
A record retains its declared fields; `exact:true` additionally rejects extra fields.
Unselected record fields never become validated evidence.
Numeric unit strings and enum values are part of the declared schema.
Decimal strings stay strings through validation and persistence.
Schema budgets are depth 12, 128 nodes, 32 fields/enum values, 4,000 array elements, and 32 KiB strings.
The complete Check configuration is bounded to 16 KiB and 16 sources.
Limits default to `timeout_ms=5000`, `poll_interval_ms=1000`, `horizon_seconds=60`, `response_bytes=262144`, and `max_facts=1000`.
Hard limits follow the invocation contract below.
The effective source response ceiling is additionally 32 KiB so one complete validated result and its metadata fit the unchanged 64 KiB Journal event limit.
Oversized source JSON must become a gap, never a complete truncated observation.
Collection transport and commands remain later task-11 slices at this checkpoint.

## No-SDK HTTP observation adapter

The first adapter invokes a configured HTTP JSON observation operation with `POST`.
The operation is read-only despite its HTTP method; it must not submit work, advance a worker, reset state, or acknowledge a queue item.
The application developer owns the endpoint and this read-only guarantee.
An ordinary application outcome API may supply the facts through a developer-owned adapter.
The adapter needs no Ronchi SDK and does not grant Ronchi direct access to a private database.
The endpoint is an exact configured URL, with loopback as the default allowed origin.
A non-loopback endpoint requires explicit configuration for an isolated supported environment.
Redirects are disabled; an observation never follows an endpoint to a new origin.
Credentials remain runtime configuration and pass through the evidence redaction policy.

Input contains `contract_version:1`, `invocation_id`, `check_id`, `lab_id`, and explicit `scope`.
It also contains the selected `snapshot_seq`, real-time deadline, and requested typed fact names.
`snapshot_seq` identifies the Ronchi evidence prefix; it does not imply an atomic snapshot of the external application.
The endpoint must reject unsupported fact names rather than silently substitute another observation.

```json
{"contract_version":1,"invocation_id":"C1","check_id":"intent-state","lab_id":"L1","snapshot_seq":42,"scope":{"intent_ids":["W1"]},"facts":["intent"],"deadline":"2026-11-02T12:00:05Z"}
```

Output echoes `contract_version` and `invocation_id` and identifies its source and schema version.
It contains `observed_at`, optional source revision, typed `facts`, and explicit per-fact availability.
A source revision is an application claim; Ronchi does not infer a global order from it.
The first fact types are boolean, integer, finite decimal string, string, enum, and bounded arrays or records of those types.
Every fact declares its unit or enum schema where that affects interpretation.
A missing, unavailable, or redacted value is distinct from `false`, zero, or an empty collection.

```json
{"contract_version":1,"invocation_id":"C1","source":"reference-relayer","schema_version":1,"observed_at":"2026-11-02T12:00:01Z","revision":"17","facts":{"intent":{"availability":"available","value":{"intent_id":"W1","state":"succeeded","original_signature":"A","prepared_signatures":["A"],"commitment":"finalized"}}}}
```

The adapter validates the schema before committing the observation.
Ronchi records invocation start, completion/error, source, scope, and bounded validated facts as Check evidence.
A statement about reported application state cites this evidence and preserves its source authority.
The endpoint may explicitly report a Client Outcome only when it retained that client-side observation; transport withholding alone cannot supply it.
The response cannot append arbitrary trusted Journal events or causal links.
Untrusted supplied references must resolve to the same Lab and allowed scope before evaluation.

## Invocation limits and lifecycle

Initial defaults are a 5-second invocation timeout, a 1-second minimum poll interval, and four concurrent invocations per Lab.
The default collection horizon is 60 seconds; configuration may set a positive horizon up to 300 seconds.
Each HTTP invocation keeps its own identity and timeout within that horizon.
Configuration may raise timeout to 30 seconds and concurrency to 16 within the Lab resource budget.
The default response limit is 256 KiB and 1,000 facts; the hard limits are 1 MiB and 4,000 facts.
Reject excess bytes while streaming; do not allocate the whole response first.
Only one invocation of the same Check and scope may be active at a time.
Polls skip an occupied slot instead of building an unbounded queue.
No automatic transport retry occurs inside an invocation; the declared polling policy owns later attempts.
A timeout, malformed response, unavailable source, or cancellation records an observation gap.
Cancellation stops owned HTTP work and prevents further polls; it does not undo external application activity.
Stopping the Lab cancels remaining invocations under the Lab shutdown contract.
Checks cannot extend the Lab shutdown deadline.
Re-evaluation of retained evidence is read-only; collecting a fresh observation is an explicit mutating operation.
A finalized or interrupted Journal accepts no fresh Check observations.
The public `check.observe` operation collects fresh observations; `check.evaluate` evaluates a selected retained snapshot without contacting the source.
Their CLI, MCP, and dashboard paths are defined in [Investigation interfaces](investigation-interfaces.md#public-check-operations).
The Demo uses these same public operations, adapter, and evaluator.

## Verdict rules

An evaluation pins the Check definitions, relevant configuration, immutable `lab_id`, scope, committed `snapshot_seq`, and required observation interval.
Its result identifies required facts, cited event references, Evidence Gaps, and evaluation version.
After fresh observations commit, evaluation captures a new watermark that includes them.
It never attaches new facts to an earlier snapshot.
A result for an open Investigation may be provisional; the UI shows its interval and pending deadlines.
Closed absence claims require collection completeness for the entire relevant interval.
Missing traffic, an unmatched Case, or a missing terminal application state is not automatic success.

| Verdict | Required evidence |
| --- | --- |
| `pass` | All configured requirements have positive supporting evidence; required terminal conditions and intervals are complete; no relevant contradiction or gap remains |
| `fail` | A declared failure predicate is positively established from its required complete-enough evidence, or a declared resolution deadline expires with authoritative complete observation of unresolved state |
| `inconclusive` | A required source, correlation, comparison, terminal observation, or complete interval is unavailable; neither success nor failure is established |

A failure predicate may be decisive before the overall horizon closes, such as two explicitly correlated prepared identities under an at-most-one-identity rule.
A local Check timeout cannot establish an application-resolution timeout.
If required source observations conflict, preserve both and return `inconclusive` unless the rule defines a justified source authority.
For multiple required Checks, a positively established `fail` makes the aggregate `fail`; otherwise any inconclusive Check makes it `inconclusive`; otherwise it is `pass`.
An aggregate failure does not hide the other Checks' gaps.
An empty Check set produces no Verdict.
A Ronchi infrastructure failure remains separately classified and cannot become an application defect.

## Optional Control comparison

Control is optional for ordinary Checks and Findings.
A comparison Check additionally requires declared equivalent Workload, relevant application-state preparation, mode, and configuration evidence.
The developer owns private databases, queues, caches, workers, and external dependency state.
Ronchi never resets those resources through the read-only observation adapter.
A failed Control or unverified Comparability makes the comparison inconclusive.
Independent directly supported application violations can still have their own Verdict.
A source's state digest supports only the declared fields and preparation procedure; it does not prove hidden state equivalent.

## Acceptance histories

Verify known success, an explicit invariant violation, unresolved state at a completed deadline, and a source timeout.
Verify missing correlation, incomplete intervals, conflicting source revisions, excess response bytes, and cancellation.
The same retained facts and evaluation version must reproduce the same Verdict without calling the source again.
Verify that an RPC-only Investigation remains useful without a configured Check or SDK.
