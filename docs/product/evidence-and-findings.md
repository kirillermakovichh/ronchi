# Journal, evidence, and Findings

Status: initial implementation contract. Task 2 implements bounded durable storage and disk admission; the queue records remaining runtime integration.
The defaults below are reversible product choices to validate in the first prototype.
They refine [ADR-0005](../adr/0005-explanation-first-outcomes.md), [ADR-0016](../adr/0016-continuous-lab-journal.md), and [ADR-0019](../adr/0019-resource-efficient-rust-core.md).
This document owns evidence semantics, persistence, Findings, and retention; the [RPC contract](solana-rpc-contract.md) owns protocol behavior.

## Journal identity and envelope

Each Lab start creates an immutable `lab_id` and one Journal.
A reusable Lab name selects a Lab; it is not its evidence identity.
A later start creates a new Lab and does not resume old chain state, Case state, or Fault history.
Each immutable event reference is `(lab_id, event_seq)`.

| Envelope field | Contract |
| --- | --- |
| `event_seq`, `kind`, `schema_version` | Increasing per-Lab sequence, typed event kind, and versioned event representation |
| `recorded_at`, `elapsed_ns` | UTC wall time for navigation and monotonic elapsed time from Lab start for durations |
| `simulated_time`, `slot`, `block_height` | Separate optional simulator coordinates when relevant; never substitute one for another |
| `source`, `execution_mode` | Ronchi, RPC Simulator, Read Upstream, Execution Backend, or explicit external evidence source |
| `request_id`, `batch_id`, `registration_seq` | Internal call identity, optional batch identity, and applicable registration/policy-selection order |
| `transaction_signature`, `cause_refs` | Available transaction identity and explicit references to the facts responsible for this event |
| `policy_ref`, `data` | Immutable Case/configuration revision reference and bounded typed facts |

The Lab-start record identifies the Ronchi build, protocol profile, redaction policy, and enabled evidence sources.
Case records retain the effective definition, versioned primitives and Finding rules, configuration digest, and any seed.
RPC IDs remain protocol values; an internal `request_id` distinguishes repeated IDs, notifications, and batch elements.
Each source record identifies its authority without persisting endpoint credentials.

## Order, time, and correlation

`event_seq` orders Journal admission; it does not prove application causality or RPC completion order.
`registration_seq` gives RPC registrations and Case-control operations their shared policy-selection order under the [Case contract](cases.md#continuing-case-model).
A batch has one registration binding and separate element request identities.
The writer commits events in sequence order and publishes a contiguous committed watermark, `snapshot_seq`.
Queries and Findings read only committed evidence through that watermark.
Monotonic time measures holds and deadlines; wall-clock corrections do not change those durations.
Simulator time records the controlled clock and does not measure a client's timeout.
A source timestamp from optional external evidence remains a source claim and does not reorder earlier Journal facts.

For supported valid signed transactions, the first signature identifies the transaction.
Each submission has its own request identity and content digest, even when signed bytes repeat.
The lifetime history preserves every submission attempt and the once-per-signature response-loss decision across Case changes.
Different signatures do not establish different business intents or duplicate withdrawals.
Business correlation requires explicit source evidence, such as a Checkpoint or a configured Check.
A shared connection, blockhash, nearby timestamp, or account address does not create that correlation.

## Required event families

| Family | Required facts |
| --- | --- |
| Lab and storage | Start, stop request, clean finalization, health changes, persistence policy, and any recorded collection failure |
| Case control | Activation, replacement, disarming, configuration revision, and resulting registration binding |
| RPC registration and validation | Method, request identity, selected bounded parameters, validation/capability errors, and batch membership |
| Transaction history | Submission attempt, acceptance/rejection decision, identity, retained content reference, and simulator lifecycle transitions |
| Source exchange | Read Upstream or Execution Backend request/result classification, source context, and unavailable-source errors |
| Fault decision and application | Matching policy, affected identity/request, selected primitive, and observed application of that Fault |
| RPC Boundary Result | Response classification, withheld response, observed cancellation/disconnect, transport termination, or unresolved completion |
| Optional external evidence | Client Outcome, Application Checkpoint, Check result, or attachment with explicit source and correlation |

Keep a planned action distinct from its observed result.
An intent to write a response does not prove the write completed or that the client received it.
For shared batch transport failure, record the affected elements without inventing separately selected Faults.
The expected lack of a notification response is not a Fault.

## Deterministic Findings

A Finding rule is a versioned declarative pattern over typed committed facts.
Built-in and Custom Case rules use the same maintained primitives; neither executes arbitrary code.

| Rule/output element | Required meaning |
| --- | --- |
| Identity and scope | Rule ID/version, relevant configuration, Lab, transaction/request scope, and bounded observation interval |
| Preconditions | Required execution mode, evidence sources, event predicates, identity joins, and causal/order constraints |
| Evidence roles | Named supporting event references, such as submission, acceptance, withholding, and later status query |
| Result | Stable Finding ID, factual statement, explicit limits, and independently presented developer guidance |
| Incomplete evaluation | Missing prerequisites and Evidence Gaps; no stronger substitute conclusion |

The same evidence, rule version, and configuration produce the same Finding identity and statement.
Finding identity derives from the rule version, scope, and supporting event references.
Derived Finding indexes may be rebuilt; the Journal remains their authority.
Rules never infer a negative fact from the absence of an event in an open interval.
A closed interval can support only “not observed within this collected interval,” subject to its completeness metadata.
A new snapshot may add evidence and Findings; it does not rewrite the cited facts.

| Committed history | Supported Finding and limit |
| --- | --- |
| Simulator accepts `A`; Ronchi withholds its response; a later request queries `A` | The original signature was queried after response withholding; client timeout and business completion remain unknown |
| The same signed bytes for `A` are submitted again after withholding | Another submission attempt used the same transaction identity; no second transaction or duplicate transfer is established |
| `B` is submitted after withholding for `A`, without business correlation | Another signature was submitted; its relationship to the same Withdrawal Intent is unavailable |
| Configured Check reports an application transition for an explicit intent | Report that observed transition with its source; simulated success still does not establish an executed transfer |

A Finding is not a Verdict.
Configured Checks and sufficient bounded evidence are required for `pass`, `fail`, or `inconclusive`.
An injected failure, a capability error, and an internal Ronchi failure have different classifications.
A Ronchi storage failure cannot establish a System Under Test defect.

## Completeness and payload policy

Each Investigation declares its Observation Boundary, selected interval, committed watermark, and known Evidence Gaps.
Each payload reference declares whether content was retained, omitted by policy, redacted, truncated, or lost through collection failure.
An Evidence Slice projection or page limit does not make the underlying Journal incomplete; it exposes continuation or omission explicitly.
A missing business source remains an Evidence Gap for business conclusions even when the RPC history is complete.
A crash can leave the final boundary action unknown; the absence of a terminal event does not prove the action never happened.
Valid Findings from preserved facts remain available when another conclusion lacks evidence.

Before queue admission or persistence, Ronchi applies the configured redaction policy to events, attachments, and diagnostic logs.
Default capture allowlists typed RPC facts and excludes raw HTTP bodies, unselected headers, application logs, and arbitrary error text.
Ronchi never persists authorization/cookie headers, endpoint user information, or credential-bearing URL query values.
Field selectors can remove additional values; affected correlation or rule evaluation exposes the resulting limitation.
Redaction cannot promise to discover a secret placed in arbitrary transaction instruction data.

Simulator-mode accepted signed transaction bytes are required local protocol state, separate from optional raw RPC capture.
Ronchi retains their exact content for identity-preserving `getTransaction`, including the accepted legacy/v0/v1, encoding, `jsonParsed`, lookup-table, and durable-nonce requirements.
It retains required lookup/nonce context and lifecycle history while the Lab exists; raw-capture settings do not disable this state.
If an explicit persistence policy forbids required content, reject the affected submission before acceptance with a clear policy/capability error.
Never modify signed bytes to claim redaction succeeded.
Raw RPC capture is off by default; an explicit bounded opt-in still passes through redaction before persistence.
Normal views and exports omit retained signed bytes and raw attachments unless the caller explicitly selects them.

## Persistence, bounds, and failure

One dedicated SQLite WAL writer batches events and required protocol-state records through `rusqlite`.
Initial writer limits are 256 events, 1 MiB, or 10 ms per batch, whichever is reached first.
The pending queue is bounded by both 4,096 events and 8 MiB.
These are tunable safety defaults, not numerical release-performance gates under ADR-0019.
The initial durability setting is `synchronous=FULL`; queue admission never counts as commit acknowledgement.
SQLite commit is the persistence boundary under the local filesystem's guarantees; Ronchi makes no stronger hardware guarantee.

Commit transaction identity, required retained content, lifecycle acceptance, and the response-loss selection/consumption decision atomically before publishing acceptance or initiating the selected hold.
Record the subsequent observed boundary action separately; a crash between the decision and that observation remains explicit uncertainty.
Journal failure must not roll back or falsely deny an external Execution Backend effect that already occurred.
Reserve bounded evidence capacity before admitting work that can mutate state or reach that backend.
If the writer fails or its admission budget is exhausted, fail the Lab, stop new RPC admission, and settle owned work under the shutdown contract.
Expose this as a Ronchi failure and an interrupted Journal; never silently drop events or continue an apparently complete capture.
When recording the failure is impossible, recovery derives interruption from the missing clean terminal marker.

The initial Lab evidence budget is 1 GiB including database, WAL, retained content, attachments, and Lab diagnostic logs.
Stop admission before the projected work exceeds that budget; reserve at least 64 MiB plus the maximum admitted evidence liability for draining and terminal records.
Bound individual records, attachments, pending requests, readers, and exports before allocation under their owning contracts.
Use automatic WAL checkpoints and bounded reader snapshots; do not delete a live WAL or keep readers open across paginated client pauses.
If checkpoint progress or available disk space cannot sustain the budget, stop the Lab with the same explicit storage-failure outcome.
No active history is evicted to keep the Lab running; protocol lookup and lifetime Fault memory remain intact.

### Implemented storage primitive

`JournalStore` creates a new file-backed version-2 Journal. Readers also accept version-1 Journals.
Its strict tables store one immutable Lab identity, the committed watermark, terminal state, and JSON event envelopes keyed by sequence.
Sequence assignment, event insertion, watermark update, and terminal state update share one transaction.
Schema 2 adds an indexed transaction projection maintained by SQLite triggers in the same event transaction.
Acceptance stores the first signature, exact signed bytes, digest, context, lifetime bounds, and progression deadlines before RPC success.
Transition events update the projection atomically.
Acceptance also retains resolved lookup addresses; historical JSON projection does not query mutable upstream state.
A bounded `protocol.fixture` event records explicit simulator startup state.
A failed projection write rolls back the submission attempt and acceptance together.
Version-1 readers expose no transaction records; no live migration or simulator resume is implemented.
The writer reads back WAL mode and `synchronous=FULL` before creating the schema.
Lock waiting is bounded to 250 ms; automatic WAL checkpoints remain enabled at 1,000 pages.
Existing Journals are not reinitialized or resumed.

The initial storage envelope contains version, kind, UTC timestamp text, monotonic elapsed nanoseconds, source, execution mode, and data.
Callers must validate event-family semantics and redact facts before storage.
Protocol correlation fields and typed event families arrive with their owning slices.
Each encoded event is bounded to 64 KiB before serialization can exceed that limit.
An atomic group contains at most 256 events and 1 MiB.
These are storage safety limits, not measured throughput results.

A reader exposes committed events and a watermark without loading full history.
If the lifecycle owner confirms that no authenticated writer remains, an open Journal is reported as interrupted.
The reader does not authenticate processes itself.
`JournalWriter` moves the SQLite connection to one dedicated thread.
Its queue admits at most 4,096 events and 8 MiB, including the in-flight batch.
An owner may configure smaller queue limits.
The writer batches whole atomic groups up to 256 events, 1 MiB, or 10 ms from the first dequeued group.
Receipts acknowledge commits; dropping a receipt cancels only waiting.
Queue exhaustion permanently closes admission with failed health.
Previously admitted groups drain when storage remains usable.
A storage failure resolves outstanding receipts as failures and preserves the prior watermark.
Watch subscribers observe health and the committed watermark separately from Journal state.

Explicit blocking shutdown closes admission, drains accepted groups, writes a supplied terminal event atomically, and joins the thread.
If the writer has failed, it never writes a clean finalization marker.
Creation and joining belong outside Tokio worker threads.
The lifecycle owner supplies cancellation deadlines and failure-driven listener shutdown in task 3.
Bounded lifecycle manifest replacement shares the disk account and its terminal reserve.
Disk admission uses the whole selected Lab directory, including nested owned files, SQLite database, WAL, SHM, retained content, attachments, and diagnostics when those files exist.
It counts the greater of logical length and allocated bytes on Unix.
No attachment or logging subsystem is implied by this accounting.
The default total is 1 GiB; `DiskLimits` permits smaller explicit budgets for isolated tests.
The shutdown reserve cannot be configured below 64 MiB.
It grows if the conservative terminal-write liability requires more space.
All outstanding admitted groups have additional disk reservations.
SQLite logical pages not yet allocated in the database file count as checkpoint-growth liability, even while their content exists in WAL.
Transaction admission reserves three future lifecycle writes before committing acceptance.
Each future credit covers 256 dirty WAL frames, 32 new pages, and SHM/allocation rounding.
Future events are capped at 4 KiB; unused credits are released after terminal progression or owned shutdown.
These conservative bounds can reject work before the directory reaches its quota.

Admission scans actual files and checks filesystem available space under the same accounting lock used for commits and checkpoints.
A reservation covers potential database growth, WAL frames, SHM growth, and filesystem allocation rounding.
The version-2 schema bounds new pages by encoded overflow pages, six pages per event, and sixteen metadata pages per group.
`max_page_count` enforces that bound for each write transaction.
`cache_spill=OFF` prevents repeated spilled WAL frames during the transaction.
The reservation conservatively charges a frame for every potential database page, including existing pages.
This can stop admission earlier than an estimate based on average writes.
It is a safety bound, not a performance result or a promise of exclusive filesystem capacity.
Other processes can consume disk capacity after a check; write failures remain fatal.

A scan is bounded to 16,384 entries and depth 32.
Symlinks and special files are rejected instead of excluding their bytes silently.
Future writers of owned files must share the admission boundary before their side effects.
An exhausted budget, insufficient free space, accounting failure, or SQLite write failure permanently rejects new admission.
The owner receives an error; writer health becomes failed.
Already admitted work can drain against its reservations while storage remains usable.
An explicit interruption record can consume the shutdown reserve.
A failed Journal cannot be finalized as clean.
No budget recovery removes events or deletes a live WAL.
An error after SQLite commit can leave committed evidence; the reader and writer watermark retain that fact.
This primitive does not yet acknowledge RPC acceptance or persist signed protocol content.

## Finalization, retention, and export

Journal states are `open`, `finalized`, and `interrupted`; health is reported separately.
A clean stop drains producers and committed evidence before writing its terminal marker.
Forced stop, unresolved writer failure, and an unclean recovered Journal are `interrupted`.
Completed Journals remain readable until explicitly deleted; there is no automatic age-based deletion in the first version.
Deletion targets an entire stopped Lab and its owned artifacts; a running Lab must stop first.
Deletion invalidates its evidence references and does not remove independently saved exports.
Keep schema migration and cleanup offline for the affected Journal; preserve the original if migration fails.

Evidence selection uses the bounded snapshot and page contract in [Investigation interfaces](investigation-interfaces.md).
An export records the Lab, snapshot, scope, versions, redaction policy, omitted payload classes, and truncation/completeness metadata.
Export streams bounded pages with an explicit output-byte limit; it does not hold a long SQLite snapshot or implicitly include the entire Journal.
Raw content requires explicit selection and a byte budget; export never removes existing redaction.
A portable replayable Chaincase format remains deferred; an evidence export does not promise reproduction of an external backend or live upstream.
