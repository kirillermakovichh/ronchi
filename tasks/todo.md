# Development queue

The initial implementation queue is complete. Current product contracts live in the [documentation map](../docs/README.md).
The only completion checklist is this file. [Implementation progress](implementation-progress.md) records the latest session state.
Current work: no active implementation task. [Task 45](#45-documentation-status-and-navigation-cleanup) records the latest documentation maintenance.

## Remaining validation

- [ ] Repeat the complete independent stateful-backend/pool-discovery workflow after the task-28 fixes and current UI changes. The earlier manual Solana-backend trial provides bounded evidence, not complete compatibility.

Checkpoint C/E's initial manual workflow criterion is recorded below. This full retest remains separate.
SDK, CI wrappers, replay, Homebrew and later capabilities remain [open questions](../docs/open-questions.md), not an implicitly started implementation queue.

## Working rules

Follow [development rules](../docs/development.md#working-rules) and the project [Definition of Done](../.agents/references/definition-of-done.md).
Record scoped checks with each task. Keep a task incomplete if required verification is blocked.
Historical paths, commands and proposed module homes below describe their implementation stage. Current contracts take precedence.
Application Checks were removed by task 36; task 30 superseded browser polling; task 34 superseded hidden/collapsible widgets.
Historical authorization and the completed parallel run do not create a new assignment.

## Completed implementation record

## First working Lab

### 1. Native executable and configuration

- [x] **Accept:** Build one Rust executable with version/help/schema entry points; resolve project/config precedence and reject invalid settings before side effects.
- **Verify:** Nested-root discovery, explicit paths, unknown keys, conflicting modes, and clean text/JSON errors in temporary projects.
- **Dependencies:** None. **Scope:** Medium. **Likely files:** `Cargo.toml`, `Cargo.lock`, `src/main.rs`, `src/config.rs`, `tests/config.rs`.
- **Sources:** [Lifecycle](../docs/product/lab-lifecycle.md#project-and-configuration-discovery), [interfaces](../docs/product/investigation-interfaces.md#cli-and-mcp-transport-behavior), [stack](../docs/adr/0019-resource-efficient-rust-core.md).

- **Completed slice:** Native package, fallible clap parsing, strict versioned TOML, canonical project discovery, flag/environment/file/default precedence, loopback and authority validation, and generated schemas from typed settings/errors.
- **Verification:** `cargo fmt --check`, `cargo check --locked`, `cargo test --locked` (6 process/config tests), `cargo clippy --locked --all-targets -- -D warnings`, `cargo run --locked -- --help`, and `cargo run --locked -- --version` passed.
- **Boundary at task 1 completion:** `up` validated without writes and returned exit `4`. Task 3 supersedes this startup limitation. Runtime-limit configuration keys remain unimplemented.

### 2. Durable Journal admission

- [x] **Accept:** Commit versioned events through bounded SQLite WAL/FULL storage; expose a contiguous watermark and explicit Journal states.
- **Verify:** File-backed reopen after process death, atomic acceptance records, queue saturation, and storage failure without silent evidence loss.
- **Dependencies:** 1. **Scope:** Medium. **Likely files:** `src/journal.rs`, `src/journal/writer.rs`, `src/journal/schema.rs`, `src/evidence.rs`, `tests/journal.rs`.
- **Source:** [Evidence and Findings](../docs/product/evidence-and-findings.md).

#### 2a. Atomic file-backed Journal storage

- [x] **Accept:** Create a version-1 SQLite WAL/FULL Journal, commit bounded event groups and their contiguous watermark atomically, and retain explicit terminal state.
- **Verify:** `cargo test --locked --test journal -- --nocapture` passed (4 behavior tests and one subprocess fixture). A killed writer retained both acknowledged events. An injected SQLite failure rolled back the whole unit and watermark. Finalized Journals reject new events and reinitialization.
- **Checks:** `cargo fmt --check`, `cargo check --locked`, `cargo test --locked` (11 tests including the subprocess fixture), and `cargo clippy --locked --all-targets -- -D warnings` passed.
- **Storage:** Bundled SQLite `3.54.1`, rusqlite `0.41.1`; WAL mode and writer `synchronous=FULL` are read back. Individual encoded events are capped at 64 KiB; atomic groups are capped at 256 events and 1 MiB.
- **Boundary:** This storage primitive accepts caller-validated/redacted facts. RPC event models, retained signed bytes, and transaction acceptance integration remain assigned to their owning slices.

#### 2b. Bounded dedicated writer

- [x] **Accept:** Add one owned writer thread, a count/byte-bounded queue, bounded batching, commit receipts, and observable failure health.
- **Verification:** `cargo test --locked --test journal` passed (8 tests including the process fixture). Tests cover count/byte saturation, permanent failure health, storage failure, cancelled receipt waiting, and drain before atomic finalization.
- **Checks:** `cargo fmt --check`, `cargo check --locked`, `cargo test --locked` (14 tests including the process fixture), and `cargo clippy --locked --all-targets -- -D warnings` passed.
- **Boundary:** Blocking creation and joining must run outside Tokio workers. Lab shutdown deadlines and failure-driven RPC admission await task 3. Disk accounting and drain reserve remain in 2c.

#### 2c. Disk admission and drain reserve

- [x] **Accept:** Account for the full Lab evidence directory, enforce the 1 GiB budget with at least 64 MiB plus admitted liability reserved for drain, and fail explicitly if disk/checkpoint progress cannot sustain the budget.
- **Verify:** Queue liability, WAL/database growth, attachments/log accounting, low-space/storage failure, and preserved committed evidence in disposable directories.
- **Dependencies:** 2a, 2b. This subtask completes task 2 before task 3 starts. Atomic required signed-content integration remains part of task 5.
- **Implemented:** Whole-directory logical/allocated byte accounting, filesystem free-capacity checks, per-group SQLite/WAL/SHM growth reservations, enforced database page bounds, and at least 64 MiB terminal reserve. Budget and write failure permanently stop new admission without evicting history.
- **Verification:** `cargo test --locked --test journal_budget` passed (6 tests). Tests use real files and small 66 MiB budgets with a 64 MiB reserve. They cover WAL/database growth, exact reserve boundary, interruption writes, existing owned-file accounting, concurrent admission, reopen, and an available-capacity check without filling the host disk.
- **Checks:** `cargo fmt --check`, `cargo check --locked`, `cargo test --locked` (20 tests including the process fixture), and `cargo clippy --locked --all-targets -- -D warnings` passed.
- **Task 2 audit:** Versioned atomic groups, FULL/WAL commit acknowledgements, contiguous watermarks, open/finalized/interrupted states, process-death recovery, count/byte queue failure, storage failure, and disk admission are verified. Protocol-specific acceptance semantics and retained signed-content integration remain task 5; Lab failure shutdown remains task 3.

### 3. Persistent Lab controls

- [x] **Accept:** Implement authenticated loopback control, background readiness, immutable IDs, name resolution, and bounded stop; finalize only after owned work/resources settle.
- **Verify:** Concurrent starts, occupied/dynamic ports, cancellation around readiness, terminal closure, force stop, and historical lookup through actual processes.
- **Dependencies:** 1, 2. **Scope:** Medium. **Likely files:** `src/lab.rs`, `src/control.rs`, `src/workspace.rs`, `src/process.rs`, `tests/lab_lifecycle.rs`.
- **Source:** [Lifecycle](../docs/product/lab-lifecycle.md).

- **Implemented slice:** Persistent simulator lifecycle controls, canonical project ownership, atomic per-Lab metadata, operation/lifetime locks, random immutable IDs and credentials, authenticated identity reconciliation, dynamic ports, cancellation, bounded owned connection drain, and retained Journal lookup.
- **Verification:** `cargo test --locked` passed with 14 process tests: concurrent starts, occupied ports, compatibility, exact historical IDs, fresh names/credentials, actual PTY closure, cancellation before locking and at the readiness gate, force stop, forged birth identity, daemon death, rebuilt binary files, unknown recorded children, and fatal disk quota exhaustion.
- **Checks:** `cargo fmt --check`, `cargo check --locked`, `cargo test --locked` (35 tests including the Journal subprocess fixture), `cargo clippy --locked --all-targets -- -D warnings`, `cargo run --locked -- --help`, and `cargo run --locked -- --version` passed.
- **Failure fixes:** A persistent SIGINT subscription avoids cancellation gaps. Status inspection preserves an accepted shutdown when closing control connections loses its response.
- **Boundary:** Readiness lists lifecycle controls only. RPC methods return explicit HTTP `501`; protocol state/methods, Cases, dashboard sessions, and managed Surfpool remain their assigned later tasks. macOS process behavior is verified; Linux remains unverified here.

### Checkpoint A

- [x] CLI exit leaves a ready Lab alive; explicit stop retains readable evidence and creates no global persistent files.
- [x] Record accumulated build, process, persistence, and failure-check results.
- **Observed:** CLI exit and real PTY closure leave the authenticated daemon ready. Normal stop confirms process exit and a readable finalized Journal. Force/death/storage failures retain interrupted evidence. An independent disposable listener and unrelated helper process remain alive. Workspace inspection and code review locate all persistent product files under the selected project.
- **Checks:** The accumulated 35-test suite, locked build, formatting, and Clippy passed. No protocol-fidelity or production-readiness claim follows from this lifecycle checkpoint.

## First RPC Investigation

### 4. Local clock and bootstrap reads

- [x] **Accept:** Expose bounded HTTP JSON-RPC for bootstrap, slots/heights, blockhashes, and validity; advance the coherent clock without polling.
- **Verify:** Parameter errors, commitment views, `minContextSlot`, clock boundaries, and rejected unknown/write forwarding.
- **Dependencies:** 3. **Scope:** Medium. **Likely files:** `src/rpc.rs`, `src/rpc/methods.rs`, `src/simulator.rs`, `src/simulator/clock.rs`, `tests/rpc_clock.rs`.
- **Source:** [RPC contract](../docs/product/solana-rpc-contract.md).

- **Implemented:** Bounded JSON-RPC 2.0 over direct Hyper, seven local bootstrap methods, synthetic per-Lab identity, monotonic 1:1 time, separate slot/height/time coordinates, commitment views, inclusive local hash validity, and strict method configuration.
- **Verification:** `cargo test --locked --test rpc_clock` passed (4 HTTP histories). Tests cover IDs including large numbers, malformed envelopes, notifications, mixed/maximum/oversized batches, body limits, unsupported methods, parameter errors, no-traffic advancement, commitment/min-context views, and validity before/at/after the bound.
- **Checks:** `cargo fmt --check`, `cargo check --locked`, `cargo test --locked` (40 tests including the Journal subprocess fixture), and `cargo clippy --locked --all-targets -- -D warnings` passed.
- **Evidence:** Per-element registrations and prepared responses precede transport completion records. A completed server write never establishes a Client Outcome.
- **Boundary:** Transaction methods remain task 5; full format/encoding coverage remains task 6. No forwarding, execution, or Fault selection is implemented.

### 5. Submission identity and lifecycle lookup

- [x] **Accept:** Commit signed identity before acceptance; derive status/transaction lookup from one lifecycle and preserve duplicate history, processing, and expiry distinctions.
- **Verify:** Independent 0/2/4/6-second progression, duplicate races, expired unprocessed submissions, retained history, and preflight capability errors.
- **Dependencies:** 2, 4. **Scope:** Medium. **Likely files:** `src/simulator/transactions.rs`, `src/rpc/submission.rs`, `src/rpc/lookup.rs`, `src/journal/transactions.rs`, `tests/transaction_history.rs`.
- **Source:** [RPC submission/history](../docs/product/solana-rpc-contract.md).

- **Implemented:** Atomic signed-byte/identity acceptance and indexed lifecycle projection in Journal schema 2, one bounded lifecycle owner, reserved future writes, concurrent duplicate suppression, autonomous 2/4/6-second progression, recent-hash expiry, and coherent status/transaction lookup with `meta:null`.
- **Verification:** `cargo test --locked --test transaction_history -- --test-threads=1` passed (7 actual HTTP/file-backed histories). Independent signed legacy/v0 fixtures cover durable acceptance/reopen, concurrent repeats, notifications, status/history/version negotiation, inclusive processing validity, expiry/unavailable-state distinction, preflight/parameter errors, pending-work drain, force stop, future disk liability, and atomic rollback on identity-projection failure.
- **Checks:** `cargo fmt --check`, `cargo check --locked`, `cargo test --locked` (47 tests including the Journal subprocess fixture), and `cargo clippy --locked --all-targets -- -D warnings` passed.
- **Boundary:** Task 6 retains the full legacy/v0/v1 encoding, parsed projection, lookup-table, fee, and nonce matrix. Missing capabilities return explicit errors. Response-loss remains task 7. Simulator progression does not establish program execution or Client Outcome.

### 6. Full format and lifetime coverage

- [x] **Accept:** Cover legacy/v0/v1, applicable encodings and `jsonParsed`, version negotiation, lookup tables, fees, and durable nonces; expose missing required state explicitly.
- **Verify:** Independent signed fixtures, pinned official-reference matrix, identity-preserving round trips, client version limits, and recent-hash versus nonce validity.
- **Dependencies:** 5. **Scope:** Medium; split by format if needed. **Likely files:** `src/protocol/formats.rs`, `src/protocol/lifetimes.rs`, `src/rpc/fees.rs`, `tests/format_matrix.rs`, `tests/fixtures/`.
- **Source:** [Formats and encodings](../docs/product/solana-rpc-contract.md#transaction-formats-and-encodings).

#### 6a. Formats and encodings

- [x] Verify independent legacy/v0/v1 signed wire fixtures, base58/base64 round trips, malformed input, and client version limits with pinned official codecs.

- **Verified:** `cargo fmt --check`, `cargo check --locked`, `cargo test --locked` (48 tests), and `cargo clippy --locked --all-targets -- -D warnings` passed. The new HTTP history uses manually assembled signed v1 wire and legacy/v0 fixtures, verifies both binary encodings and client version errors, rejects trailing bytes, and reopens retained bytes. Code review checked codec limits, identity, and negotiation. Solana modular crates are pinned to `=6.0.0`.

#### 6b. Lookup tables and JSON

- [x] Resolve explicit local lookup state in protocol order; retain context; serialize JSON/parsed instructions and unknown-instruction fallback without inventing state.

- **Verified:** `cargo fmt --check`, `cargo check --locked`, `cargo test --locked` (51 tests), and `cargo clippy --locked --all-targets -- -D warnings` passed. HTTP histories cover two-table ordering, retained context/reopen, parsed transfer, unknown instruction/program fallback, all-format parsed lookup, warmup, missing table, and invalid index. Self-review checked fixture/config bounds, historical authority, startup persistence, and fail-closed state errors.

#### 6c. Fee queries

- [x] Quote fees through HTTP using an explicit version-aware local model, lifetime/context checks, and bounded decoding.

- **Verified:** `cargo fmt --check`, `cargo check --locked`, `cargo test --locked` (53 tests), and `cargo clippy --locked --all-targets -- -D warnings` passed. HTTP tests cover all message versions, context/lifetime, malformed input, unsupported parameters, priority rounding, default CU allocation, duplicate budgets, v1 fixed priority, and ignored v1 budget instructions. Self-review checked bounded decode, saturating arithmetic, state authority, and the quote/execution distinction.

#### 6d. Durable nonce

- [x] Preserve explicit nonce state and atomic advancement; verify nonce lifetime, retries, competing identities, missing state, and file-backed persistence.

- **Verified:** `cargo fmt --check`, `cargo check --locked`, `cargo test --locked` (60 tests), and `cargo clippy --locked --all-targets -- -D warnings` passed. Nonce HTTP histories cover all three versions, ageing beyond recent-hash lifetime, authority validation, historical fee context, same-signature retries, concurrent competing identities, same-bank non-advancement, v0 lookup-loaded nonce accounts, and file-backed reopen. Injected SQLite nonce failure rolls back processing and preserves acknowledged identity. A native process test loads TOML state and serves a nonce fee quote. Exact packet-size and one-byte-over-limit HTTP checks pass for all formats.
- **Self-review:** Scoped review checked atomic nonce/lifecycle/watermark writes, single-owner ordering, historical queries, bounded fixture/query/event sizes, future transition reservations, and schema-1/2 reader compatibility. No confirmed issue remains in this slice. Schema 3 creates new Journals; live resume/migration remains outside this task.
- **Task 6 complete:** The [verified local matrix](../docs/product/solana-rpc-contract.md#verified-local-format-matrix) records checked formats and the declared local fee/nonce model. Missing account or slot-hash state remains an explicit error. Instruction parsers retain standard partial decoding for other instructions. No execution, forwarding, public writes, or funding was added. Task 7 is next.

### 7. Continuing Case and response loss

- [x] **Accept:** Bind policy in shared registration order, including batches; apply durable once-per-identity response loss with the 20-second hold.
- **Verify:** Actual HTTP silence/termination, duplicates, cancellation, batch effects, disarm/reactivation, and Case replacement while lifecycle lookup continues.
- **Dependencies:** 5. **Scope:** Medium. **Likely files:** `src/cases.rs`, `src/cases/response_loss.rs`, `src/rpc/batch.rs`, `tests/response_loss.rs`, `tests/case_binding.rs`.
- **Source:** [Cases and coverage](../docs/product/cases.md).

#### 7a. Registration, durable selection, and HTTP withholding

- [x] Bind immutable policies in shared registration order, commit acceptance/ledger atomically, and withhold real HTTP responses with bounded capacity and owned completion evidence.

- **Verified:** All four locked checks passed: formatting, check, test (61 tests), and Clippy with `-D warnings`. The first raw HTTP history proves header/body silence, autonomous finalization, ordinary duplicate reply, transport EOF, and durable selection/start/boundary records. Scoped self-review checked commit-before-hold, explicit EOF, future evidence credits, and connection ownership. Public catalog/startup integration and the wider histories remain below.

#### 7b. Public Case operations and startup

- [x] Expose list/arm/disarm, startup Case, active status, strict errors, and schemas without restarting the Lab.

- **Verified:** Formatting, locked check/test (62 tests), and Clippy with `-D warnings` passed. The real CLI/control history checks offline catalog, startup activation, disarm/reactivation, immutable Lab/process identity, invalid replacement, strict JSON errors, retained active revision, and rejection after stop. Scoped review checked authentication, mutation uncertainty, readiness, and bounded held-effect status.

#### 7c. Histories and Checkpoint B

- [x] Verify retries, races, switching, batches, capacity, failures, cancellation/shutdown, and a reproducible response-loss/status workflow.

- **Verified:** `cargo fmt --check`, `cargo check --locked`, `cargo test --locked` (73 tests), and `cargo clippy --locked --all-targets -- -D warnings` passed. Raw HTTP histories prove silence/EOF, concurrent retries, distinct signatures sharing a hash, retained eligibility across reactivation, a no-Case binding paused before later arm, unchanged deadlines across shorter/longer replacements, batch index order and shared boundary effects, notifications/validation/preflight nonselection, capacity rejection/release, real SQLite ledger rollback, and normal/force shutdown. A native held-shutdown process history also verifies clean finalization after normal drain and interrupted evidence after force stop. Legacy/v0/v1 nonce histories advance once despite withheld responses and retries.
- **Self-review:** Scoped review checked ownership, receipt cancellation, start-versus-peer-close ordering, all required held evidence reservations, strict control errors, immutable policy snapshots, and durable non-evicting identity state. The start receipt remains owned after the HTTP waiter ends. Prepared-result recording failure is fatal. No confirmed issue remains in this slice.
- **Boundary:** Simulator Case only. Surfpool response loss, Findings, Custom YAML, MCP, and additional families remain assigned to later tasks. Native verification is on the current macOS host; no Linux claim follows.

### Checkpoint B

- [x] A real HTTP client submits, loses the response, and queries the original signature without restarting the Lab.
- [x] Record format gaps explicitly; the accepted full matrix remains required for a full support claim.

- **Observed:** `cargo test --locked --test lab_lifecycle checkpoint_b_native -- --nocapture` passed. An independently signed legacy fixture lost its HTTP response, then the same native Lab returned `processed` for the original signature. Lab/process identity stayed unchanged. Force stop retained an interrupted Journal with acceptance, selection, observed withholding, actual transport completion, exact bytes, and unknown client receipt.
- **Formats and conclusions:** The task-6 matrix remains required and verified; response-loss nonce histories also cover legacy/v0/v1. Missing account/slot-hash state remains explicit. No new format exclusion was introduced. RPC-only facts do not establish business intent, execution, or a Client Outcome. The [Coverage Claim](../docs/product/cases.md#tx-response-lost-coverage-claim) records scope and reproduction. Task 8 is next.

### 8. Bounded Investigation queries

- [x] **Accept:** Expose summary, Timeline, and Evidence Slice through CLI/control API with storage-level filtering/projection, snapshot pagination, byte caps, and completeness metadata.
- **Verify:** Paging during appends, empty/no-match results, invalid cursors, payload previews, pipe closure, and retained links after stop.
- **Dependencies:** 7. **Scope:** Medium. **Likely files:** `src/query.rs`, `src/journal/query.rs`, `src/api.rs`, `src/cli/investigate.rs`, `tests/investigation.rs`.
- **Source:** [Investigation interfaces](../docs/product/investigation-interfaces.md).

#### 8a–8b. Storage query core and snapshot pagination

- [x] Add typed summary/Timeline/Evidence Slice selection, storage projection and filters, fixed watermarks, and validated bound cursors without persistent reader snapshots.

- **Observed:** `cargo fmt --check`, `cargo check --locked`, `cargo test --locked` (77 tests), and `cargo clippy --locked --all-targets -- -D warnings` passed. Four file-backed query tests cover scoped metadata without payload, fixed-watermark pages during append, changed/damaged cursors, OR/AND and nanosecond time/sequence bounds, mandatory projection context, and explicit unperformed Findings analysis. Existing disk-budget and protocol histories pass.
- **Review:** Query projection commits with the event. Reader permits stay with blocking workers. Each page releases its snapshot. Signature correlations from later appends are bounded by the pinned watermark. Remaining transport, payload, exact-size and cancellation acceptance work stays in 8c/8d.

#### 8c. CLI/control and historical Lab selection

- [x] Expose authenticated live queries and read-only retained queries, schemas/help, cancellation, and finite text/JSON output.

- **Observed:** `cargo fmt --check`, `cargo check --locked`, `cargo test --locked` (79 tests), and `cargo clippy --locked --all-targets -- -D warnings` passed. Native tests verify identical scoped CLI/control results, bearer authentication, malformed/oversized request errors, offline schema, read-only finalized queries, and event references after name reuse.
- **Review:** Live queries share one bounded reader engine. Query transport preserves immutable Lab selection and never creates capture events. CLI emits one finite page. Cancellation and pipe monitoring are implemented; their acceptance checks remain in 8d.
- **Regression diagnosis:** The existing peer-close test raced Hyper transport failure against EOF observation. A failing run retained `transport_failed` with `shared_withholding:true` and `no_transport_response` for both requests. The assertion now accepts either observed close path and checks both boundaries. Twelve focused runs and the full suite passed; no runtime response-loss behavior changed.

#### 8d. Payloads, byte budgets, and completeness

- [x] Verify exact envelope bounds, payload previews, completeness states, reader cancellation/timeout, pipe closure, and concurrent writes.

- **Observed:** `cargo fmt --check`, `cargo check --locked`, `cargo test --locked` (95 tests), and `cargo clippy --locked --all-targets -- -D warnings` passed. Focused commands also passed: `cargo test --locked --test investigation`, `cargo test --locked --test lab_lifecycle investigation_ctrl_c -- --nocapture`, `cargo test --locked --test lab_lifecycle investigation_control_auth -- --nocapture`, `cargo test --locked --test lab_lifecycle investigation_text -- --nocapture`, and `cargo test --locked --test transaction_history investigation_correlates -- --nocapture`.
- **Covered:** Exact UTF-8 envelope boundaries with escaping; valid HTTP requests at 64 KiB and rejection at one extra byte; 200/201 filter values; bounded payload previews and JSON literals without numeric coercion; byte-page and payload truncation independently; normalized and changed cursor bindings; no-match and unperformed Findings; immutable references across finalized/interrupted history and name reuse; all snapshot pages during concurrent appends; SQL payload fault injection; read-only legacy queries; mode/Case context at the watermark.
- **Cancellation:** Real SQLite VM work holds a page snapshot while concurrent writes continue. Cancellation releases it and permits `wal_checkpoint(TRUNCATE)`. Native CLI/control tests verify Ctrl-C (130), default 10-second timeout (124), broken pipe (0), released reader snapshots, unchanged Lab identity, and continuing HTTP `getHealth`/Journal writes. The macOS pipe check requests `POLLOUT` to observe closed readers.
- **Review:** The scoped `ronchi-code-review` checked filters, atomic projection, immutable correlations, SQL-before-payload selection, reader/task ownership, exact encoded bounds, cursor continuation, redaction, and absence of invented Findings. Preview backoff accounts for cursor and truncation overhead. No confirmed blocking defect remains in this slice.
- **Boundary:** This is bounded Investigation over retained Journal evidence. Findings analysis is explicitly `not_performed`; `finding_id` returns a capability error. Legacy correlation gaps remain explicit. Preview fragments do not constitute full export. Dashboard, MCP, export, and follow streams remain separate. Native checks ran on macOS; no Linux process-behavior claim follows. Task 8 is complete; task 9 is next.

### 9. Evidence-backed Findings

- [x] **Accept:** Produce stable versioned Findings with supporting references, separate guidance, and explicit Evidence Gaps.
- **Verify:** Identical snapshot re-evaluation and required-fact removal; different signatures must never imply a shared intent or duplicate economic execution.
- **Dependencies:** 8. **Scope:** Medium. **Likely files:** `src/findings.rs`, `src/findings/rules.rs`, `src/query/summary.rs`, `tests/findings.rs`.
- **Source:** [Deterministic Findings](../docs/product/evidence-and-findings.md#deterministic-findings).

#### 9a. Versioned evaluator and typed rules

- [x] Implement two typed evidence patterns with stable incident identity, explicit support roles, separate guidance, and evaluation states. Candidate selection remains in 9b.
- **Verify:** Independent positive histories and near misses, required-fact removal, ordering, source/mode/version validation, stable IDs, and digest requirements.

- **Verified:** `cargo fmt --check`, `cargo check --locked`, `cargo test --locked` (99 tests), and `cargo clippy --locked --all-targets -- -D warnings` passed. Four independent typed-history tests cover both positive patterns, stable identity at a newer watermark, required-fact removal, digest-dependent statements, authoritative sources, Lab/snapshot isolation, ordering, and unsupported schema/mode.
- **Self-review:** Scoped requirements/correctness/resource review found no blocking defect in the pure evaluator. It performs no I/O and evaluates five supplied facts. Query selection, public histories, operational limits, and transport integration remain unchecked below.

#### 9b. Bounded Journal evaluation

- [x] Adapt saved metadata through the existing query core without reading payloads or simulator state; preserve scope, watermark, deadline, cancellation, and reader limits.
- **Verify:** Legacy correlation gaps, bounded processing, polling deduplication, batch boundary effects, concurrent appends, finalized/interrupted Journals.

#### 9c. Findings query and supporting Evidence Slice

- [x] Expose CLI/control/schema/help, shared live/retained evaluation, five-Finding summary, pinned versioned pagination, and snapshot-preserving Finding references.
- **Verify:** Real CLI/HTTP references, page/projection stability, no hidden scope expansion, and read-only retained queries.

- **Verified implementation slice 9b/9c:** `cargo fmt --check`, `cargo check --locked`, `cargo test --locked` (104 tests), and `cargo clippy --locked --all-targets -- -D warnings` passed. Five independent file-backed histories verify both rules, fixed-snapshot references, page/projection independence, required-fact removal through exact scope, wrong Lab/signature/order, polling deduplication, independent valid Findings with gaps, the 256-incident limit, deadline, and cancellation. Existing CLI/control tests cover operation transport and retained selection; positive real Findings transport and legacy/batch acceptance remain in 9d.
- **Review:** Scope filters apply to every support join. Presentation does not constrain evaluation. Each page releases its read transaction. Cursor fingerprints include rule definitions; Finding IDs exclude watermark and presentation. No writer, background service, schema migration, payload read, or simulator-state dependency was added.
- **Environment:** Running the suite inside a multi-command shell caused sandbox loopback-bind errors. The focused lifecycle test and standalone `cargo test --locked` passed without code changes. Verification commands are run individually below.

#### 9d. Public histories and completion review

- [x] Verify real HTTP tx-response-lost and independent counterexamples; document implemented rules and remaining limits; run all four locked checks and scoped self-review.

- **Verified:** `cargo fmt --check`, `cargo check --locked`, `cargo test --locked` (113 tests), and `cargo clippy --locked --all-targets -- -D warnings` passed. Focused checks also passed: `cargo test --locked --test findings --test findings_query`, `cargo test --locked --test lab_lifecycle findings_native -- --nocapture`, and `cargo test --locked --test findings_query linked_sqlite -- --nocapture`.
- **Public evidence:** A native HTTP history signs an independent transaction, observes an empty withheld transport response, repeats RPC IDs across polls and resubmission, and receives both Findings through real CLI and authenticated HTTP. Both transports resolve each Finding to its five supporting records at the pinned snapshot. Finalized and interrupted retained reads produce the same Finding IDs/statements and leave the Journal file unchanged. No new Lab or simulator lookup supplies evidence.
- **Counterexamples and completeness:** Thirteen file-backed query tests cover original/later fact removal within exact scope, unrelated signatures and Labs, wrong order, selection without withholding, repeated polling, shared batch boundary without sibling Fault selection, legacy correlation gaps with independent valid Findings, unsupported mode, five-Finding summary, byte-limited traversal, the incident limit, and metadata-only access with payload-storage fault injection. Active SQL deadline/cancellation checks prove concurrent writes and reader release via WAL checkpoint. Typed tests cover unsupported fact schema and authoritative source requirements. No test treats missing client/business evidence as timeout, recovery, duplicate intent, or economic execution.
- **Storage review:** Linked SQLite is 3.54.1, source `555e07ef33f69a0957d24d8e555b702464a131d6f1607f4c3c1e4551e8e6ca76`. `EXPLAIN QUERY PLAN` uses `query_request`, `query_kind`, and the covering signature index for the bounded joins. No schema/index migration or derived write was added.
- **Self-review:** The final scoped review checked requirements, source authority, correlation, fixed watermark, deterministic earliest support, digest-dependent wording, missing facts, projection limits, cursor/version binding, SQL budgets, and read-only snapshot ownership. Rule declarations now carry their sources, supporting roles, guidance, and limitations. Projection retains material limitations. Missing Finding links use unavailable-target errors; operationally incomplete resolution is separate. No confirmed blocking defect remains.
- **Boundary:** Two simulator schema-1 rules only. They report later request/submission receipt, not its returned state, application causality, a Client Outcome, business completion, or a Verdict. Batch siblings without their own selection/start facts remain incomplete; shared transport effects are never promoted to separately selected Faults. Analysis stops explicitly after 256 incident candidates and keeps confirmed Findings; narrow the scope to finish. Legacy SQL projection preserves correlation gaps and performs no migration. Native process checks ran on macOS. Task 9 is complete; task 10 is next.

## Application evidence and Demo

### 10. Durable reference relayer

- [x] **Accept:** Run the bundled child mode with idempotent intent intake, durable signed identity, public state/observation endpoints, and bounded unsafe/resilient policies.
- **Verify:** Crash before send, after attempt-start, after acceptance, and before result commit; reopen storage and verify preserved original identity and source facts.
- **Dependencies:** 5, 7. **Scope:** Medium. **Likely files:** `src/reference.rs`, `src/reference/store.rs`, `src/reference/sender.rs`, `src/reference/http.rs`, `tests/reference_recovery.rs`.
- **Source:** [Reference Application](../docs/product/reference-application.md).

#### 10a. Process and durable intent storage

- [x] Add foreground `reference relayer`, separate private workspace/database ownership, immutable Lab/network/profile binding, and idempotent bounded intent intake.
- **Verify:** Concurrent intake/conflict, reopen/profile mismatch, private signing key, isolated public Lab identity checks.

- **Verified 10a:** `cargo fmt --check`, `cargo check --locked`, `cargo test --locked` (115 tests), and `cargo clippy --locked --all-targets -- -D warnings` passed. File-backed storage and native HTTP/process histories cover concurrent identical intake, changed-payload conflict, reopen, immutable profile, seed/directory permissions, public status/genesis binding, and relayer-only shutdown. Existing Lab/RPC/Findings histories remain green.
- **Self-review:** One storage owner, 16-command admission, one active intent, 128 retained intents, bounded HTTP, WAL/FULL/FK readback, atomic schema/config initialization, private signing seed, and no Lab Journal access. This slice explicitly advertises unimplemented submission; worker and active recovery checks remain below.

#### 10b. Submission, reconciliation, and profiles

- [x] Commit signed identities and attempt-start before HTTP, implement finalized reconciliation and bounded unsafe replacement, and persist client/status observations.
- **Verify:** Real RPC response loss, one resilient identity/two unsafe identities, no false failure from null/missing response, and identity-commit failure.

- **Verified 10b:** All four locked checks passed: formatting, check, test (119 tests), and Clippy with `-D warnings`. Native HTTP histories prove a retained client-side 5-second timeout under response loss, resilient original-identity finality, unsafe's one replacement with the same blockhash and a different signature, null status remaining reconciling, terminal stability across process reopen, and no submission after a real SQLite identity-insert failure.
- **Self-review:** Signed bytes/validity and attempt-start commit before network submission. One owned worker and connection task prevent concurrent RPC work; timeout/shutdown abort and join transport tasks. Terminal updates include supporting observations in the same transaction. Original signatures stay immutable; generation and attempt admission bound unsafe behavior. No Case-name branch or simulator/Journal access exists. Crash barriers and complete recovery histories remain in 10d.

#### 10c. Application observation endpoint

- [x] Add read-only `POST /checks/observe` with explicit scope, availability, source revision, requested facts, deadline, and consistent application snapshot.
- **Verify:** Unknown facts, absent intents, bounded input/output, no worker effects, and no private signing material.

- **Verified 10c:** All four locked checks passed (121 tests). Direct HTTP histories verify consistent revision/mapping, both unsafe identities, no RPC/worker/DB changes during repeated observations at a controlled prepared barrier, unavailable missing facts, unknown fact/scope/version/Lab rejection, deadline errors, the 8 KiB input budget, bounded responses, and absence of signing material. The endpoint reports application-only snapshot authority and never emits a Verdict.
- **Self-review:** Observation reads one SQLite snapshot through the same bounded owner, accepts only the published `intent` fact and one explicit intent, and does not notify the worker. This implements the source endpoint only; generic Check collection remains task 11.

#### 10d. Crash recovery and completion review

- [x] Verify prepared, attempt-start, accepted-before-result, replacement-prepared, and terminal crash boundaries with new processes; preserve horizons and Lab identity.
- **Verify:** Profile mismatch, unavailable/replaced Lab, graceful relayer-only shutdown, all four locked checks, scoped self-review, and current implementation docs.

- **Verified 10d:** `cargo fmt --check`, `cargo check --locked`, `cargo test --locked` (131 tests), and `cargo clippy --locked --all-targets -- -D warnings` passed. Focused `cargo test --locked --test reference_recovery --test reference_storage` and individual crash/source histories also passed. The reference suite has 17 native process/HTTP histories and one file-backed storage history.
- **Crash evidence:** Private debug barriers establish prepared, attempt-start, RPC-result-before-local-commit, replacement-prepared, and terminal boundaries before SIGKILL. New processes reopen the same SQLite files and preserve exact signed bytes/signatures, original/tracked mapping, profile, and absolute deadline. A never-sent unknown attempt does not become a timeout or replacement; an already accepted unknown attempt reconciles the original. Repeated unsafe restarts retain two identities and create no third. Terminal restart performs no new RPC.
- **Source and horizon:** Expired prepared recovery remains at the original deadline and submits nothing. Both old/new ID selections against a replacement Lab on the same RPC socket produce unresolved source gaps without redirecting the intent. A changed expected-genesis fixture also prevents submission. SIGTERM during an in-flight HTTP submission joins transport, leaves the Lab healthy, and recovers the original unknown outcome without inventing a Client Outcome.
- **Self-review:** The final scoped review checked public-only Lab access, immutable binding, private key ownership, canonical intake, transaction boundaries, commit-before-send, first ambiguity, original-signature immutability, bounded generations/attempts, terminal evidence atomicity, honest availability, read-only snapshots, deadline preservation, queue/body/response bounds, source replacement, and task/process cleanup. Startup failures join storage ownership; base58 inputs are length-bounded before decoding. No confirmed blocking defect remains in this slice.
- **Boundary:** One legacy inert simulator fixture, one active intent, no economic execution claim. Reference schema 1, at most 128 retained intents and 128 observations per intent, two identities and one attempt per identity. Retained signing bytes stay local; only mappings and metadata are public. Generic Check collection/evaluation, Demo orchestration, SDK, dashboard, MCP, and Surfpool remain unimplemented here. Native histories ran on macOS; no Linux/Windows or power-loss verification claim follows. Task 10 is complete; task 11 is next.

### 11. Public Check collection

- [x] **Accept:** Implement configured read-only HTTP observation and public `check.observe`; commit validated facts with source, explicit scope, deadlines, and bounded polling.
- **Verify:** Timeouts, unavailable facts, schema errors, redirects, byte limits, cancellation, and stop; adapter failure remains an Evidence Gap.
- **Dependencies:** 8. **Scope:** Medium. **Likely files:** `src/checks.rs`, `src/checks/http.rs`, `src/checks/schema.rs`, `src/cli/check.rs`, `tests/check_observe.rs`.
- **Source:** [removed application integration](#36-remove-application-checks).

#### 11a. Check configuration and wire validation

- [x] Add named sources and bounded typed schemas to existing configuration; validate origins, limits, exact identities, availability, and decimal strings.

- **Verified 11a:** All four locked checks passed (134 tests). Three independent configuration/validation histories cover exact decimal/integer preservation, explicit unavailable values, malformed values, identity/source/schema mismatch, records/arrays/enums, origin authorization, URL credential rejection, duplicate definitions, and pre-effect configuration rejection. Scoped review checked schema/input budgets and no Verdict or expression engine. Collection is not exposed at this slice boundary.

#### 11b. Durable bounded HTTP invocation

- [x] Reserve terminal evidence and commit start before HTTP; stream a deadline-bounded response, validate/redact selected facts, and commit completion/gap through the existing writer.

- **Verified 11b:** `cargo fmt --check`, `cargo check --locked`, `cargo test --locked` (138 tests), and `cargo clippy --locked --all-targets -- -D warnings` passed. Four file-backed HTTP histories prove committed start before external POST, application authority and source time/revision preservation, malformed JSON as a gap without partial payload, real start/completion commit failures, active cancellation, and same-scope exclusion. The existing reservation budget also covers bounded Check terminal events; global Journal limits and schema remain unchanged.
- **Ownership review:** A bounded supervisor owns finite collection tasks and their cancellation tokens. A Lab-wide semaphore bounds invocations; no worker queues missed polls. Transport tasks abort/join before completion; writer receipts, not admission, authorize HTTP. Existing navigation indexes provide read-only completed-collection lookup. Public transport and Lab shutdown integration remain in 11c/11d.

#### 11c. Public collection operation

- [x] Expose CLI/control/schema, normalized scopes, bounded status/references, explicit polling, per-Lab concurrency, and Check/scope exclusivity.

- **Verified 11c:** Native CLI collected a fact from an independent HTTP source, resolved its Evidence reference live and after stop, rejected mutation of the stopped Lab, and exposed offline schema. All four locked checks passed (139 tests). Scoped review checked authenticated exact-Lab selection, bounded control bodies/results, no automatic retry, and application authority. Process cancellation and adversarial transport histories remain in 11d/11e.

#### 11d. Cancellation and Lab shutdown

- [x] Own collection tasks and transport cancellation; stop polls on Ctrl-C, broken pipe, cancellation, and Lab stop while retaining terminal evidence.

- **Verified 11d:** Native CLI processes prove Ctrl-C, broken pipe, and Lab stop close the in-flight source connection, retain terminal cancellation, and prevent later HTTP attempts over a full minimum poll interval. Cancellation errors retain the resolved Lab and collection identity. The source stays alive. All four locked checks passed on the cumulative working slice. Scoped review checked ownership and writer-before-producer shutdown ordering.

#### 11e. Public histories and evidence integration

- [x] Verify independent HTTP sources and the real relayer, source failures, budgets, commit failures, retained evidence, credential exclusion, and no Verdict; run locked checks and scoped review.

- **Verified 11e:** `cargo fmt --check`, `cargo check --locked`, `cargo test --locked` (146 tests), and `cargo clippy --locked --all-targets -- -D warnings` passed. Eight native CLI/control histories cover a real relayer mapping without worker advancement; explicit unavailable facts; malformed JSON/types/identity/source/schema; redirects; chunked overflow; a slow streaming body under one deadline; normalized scope exclusion and configured concurrency; busy-poll skips; distinct invocation IDs without overlap; Ctrl-C, broken pipe, and Lab stop; stopped mutation rejection; retained evidence; runtime credential redaction; and complete Unicode observation reconstruction from bounded Evidence chunks. Earlier file-backed tests inject start/completion commit failures.
- **Scoped review:** The real relayer exposed a missing HTTP Host header; the generic transport now sends the parsed URL authority. Application facts remain application-source claims. New `payload_offset` reads preserve the existing 4,096-byte preview cap and response limits. Readers release each request snapshot. No Findings rule or Verdict was added. The effective capture ceiling remains 32 KiB under the unchanged Journal event budget. HTTP is supported; HTTPS remains explicitly unsupported. Next task: 12.

### 12. Deterministic Check evaluation

- [x] **Accept:** Implement public `check.evaluate` over a pinned retained snapshot with bounded Verdicts, supporting facts, source authority, and gaps.
- **Verify:** Success, correlated identity violation, authoritative unresolved state at deadline, incomplete intervals, and conflicting facts; repeat without contacting the source.
- **Dependencies:** 9, 11. **Scope:** Medium. **Likely files:** `src/checks/evaluate.rs`, `src/checks/rules.rs`, `src/checks/verdict.rs`, `tests/check_evaluate.rs`.
- **Source:** [removed business evaluation](#36-remove-application-checks).

#### 12a. Versioned typed definitions

- [x] Add bounded requirement definitions to existing Check configuration; validate logical field paths and exact typed comparisons; preserve unknown predicate results.

- **Verified 12a:** All four locked checks passed (151 tests). Five predicate histories cover exact arbitrary-precision integers and decimal exponents; positive and negative comparisons; distinct identity keys; typed membership; all/any with unavailable/redacted facts; nullable values; schema-aware logical paths; mismatched units; invalid fields; and definition budgets. Scoped review changed distinct counting to sorting and tied availability traversal to declared schemas. Predicate truth is separate from Verdict sufficiency. Retained snapshot evaluation, reference requirements, and public evaluation remain unchecked.

#### 12b. Retained snapshot evaluation

- [x] Resolve pinned retained definitions and complete selected facts; preserve source authority, contradictions, snapshot bounds, and operational limits without writes or HTTP.

- **Verified 12b:** All four locked checks passed (157 tests). Six file-backed histories verify bounded positive proof, an open interval, later violations excluded by the old watermark, stable re-evaluation, declared source contradictions versus normal revisions, aggregate fail with retained gaps, source errors, empty requirements without Verdict, redacted failure facts, exact-signature retained finality, cancellation, deadline, and the 512-observation operational limit. Scoped review preserved same-Lab invocation references, complete selected payloads, credential-free public definitions, bounded consistency state, and decisive supporting references. Reference and public-path acceptance remain in 12c/12d.

#### 12c. Reference requirements

- [x] Express identity, original finality, and resolution requirements through maintained generic predicates; expose only the additional public completeness/timing facts needed by these rules.

- **Verified 12c:** All four locked checks passed (159 tests). A native two-profile history uses the real relayer, public collector, and simulated RPC finality; resilient passes and unsafe fails the distinct-identity requirement. Re-evaluation survives source and Lab stop. Independent retained histories cover early/open intervals, two distinct versus repeated identities, succeeded without finality, authoritative unresolved before/after the original deadline, incomplete/redacted prepared sets, wrong intent mapping, and invalid temporal ordering. Scoped review keeps the source additive, read-only, and snapshot-consistent; its terminal time comes from the committed terminal observation. Generic scope and retained source-time predicates use no evaluator clock or profile name. Public CLI/control acceptance remains 12d.

#### 12d. Public evaluation and histories

- [x] Expose CLI/control/offline schema for live and retained evaluation; verify reference/public histories, replay, resource bounds, and no HTTP; document limits and run locked checks.

- **Verified 12d:** `cargo fmt --check`, `cargo check --locked`, `cargo test --locked` (162 tests), and `cargo clippy --locked --all-targets -- -D warnings` passed. Native CLI/control parity uses an unrelated HTTP source and complete Unicode facts larger than the query preview. A live listener proves evaluation makes no source calls. Retained replay works after invalidating current configuration, stopping the source, clean Lab stop, and actual Lab process death; interrupted completeness remains visible. Version/digest errors, startup definitions without observations, mandatory snapshot selection, wrong Lab selection, aggregate context isolation, and empty aggregate semantics are covered. Process tests wait for an observable reader snapshot before Ctrl-C, broken pipe, or deadline; they verify snapshot release while RPC remains available.
- **Scoped review:** Evaluation reuses the bounded query reader and performs no Journal mutation. Supporting references remain decisive after later observations. Declared cumulative inventories prevent a late shrinking inventory from manufacturing a pass; ordinary growth remains valid. The maintained reference definition is now version 2, while rule/source-schema/evaluation versions stay independently identified. Single-intent/signature scope, 512 selected records, 4 MiB evidence, and 256 KiB serialized consistency state remain explicit evaluation limits. HTTP-only collection and its effective 32 KiB source response cap are unchanged. No Demo, SDK, dashboard, or automatic Verdict for RPC-only Investigations was added. Next task: 13.

### 13. Self-contained public-path Demo

- [x] **Accept:** Orchestrate fresh Demo-owned Lab/relayer state and public RPC/Checks; return bounded results and retain the Lab unless `--stop-after-demo` is explicit.
- **Verify:** Both profiles offline, unsafe identity failure, evidence-backed resilient success, fresh names, JSON output, and owned-child cleanup.
- **Dependencies:** 10, 12. **Scope:** Medium. **Likely files:** `src/demo.rs`, `src/demo/fixture.rs`, `src/cli/demo.rs`, `tests/demo.rs`.
- **Sources:** [Demo workflow](../docs/product/workflows.md#first-proof-bundled-demo), [reference packaging](../docs/product/reference-application.md#demo-configuration-and-observation-limits).

#### 13a. Isolated startup and owned process

- [x] Add the Demo entry point, atomic fresh-name startup, explicit simulator configuration, reserved source URL, and bounded owned child lifecycle.

#### 13b. Public Workload and pinned evidence

- [x] Submit one public intent, collect and evaluate through public clients, pin the retained definition digest and snapshot, and return bounded Investigation data.

- **Verified 13a/13b:** All four locked checks passed (163 tests). The native tracer starts resilient and unsafe in fresh sequential Labs and distinct reference storage, proves pass/fail through public collection/evaluation, confirms reference process reaping, preserves the default Demo ready-Lab behavior, supports retained replay, and stops only its exact Lab with `--stop-after-demo`. The first source digest comes from retained invocation evidence, not runtime configuration digest. Common configuration validation now has an explicit-settings seam; ordinary project-file precedence remains unchanged. Fresh startup selection stays inside the ordinary operation lock and supports owned cancellation without dropping startup cleanup. Source binding has a reservation-before-startup and explicit bind-race failure policy. Failure/concurrency/process-cancellation acceptance remains 13c/13d.

#### 13c. Retained resources and failure cleanup

- [x] Verify default persistence, exact-ID stop-after-demo, cancellation/output closure, source/child failures, concurrent isolation, and preserved user configuration.

#### 13d. Public histories and Checkpoint C

- [x] Run native profile and replay histories, maintain workflow/limits, perform scoped review and all locked checks, and record applicable Checkpoint C evidence.

- **Verified 13c/13d:** All four locked checks passed (169 tests, including seven native Demo histories). HTTP/process tests require local socket permission outside the sandbox. Histories cover evidence-derived profile outcomes, retained replay, sequential/concurrent isolation, unchanged user config/default Lab, text/JSON lifecycle parity, startup/bind failures, SIGINT/output closure, child death, source timeout, and bounded cleanup. Secret checks cover the control token and signing seed encodings. Scoped review checked ownership, primary-error preservation, exact-ID cleanup, and bounded output drains. Simulator-only, HTTP-only, 32 KiB source cap, one intent/signature, and 512-record/4 MiB evaluation limits remain unchanged. SIGKILL cannot guarantee Demo cleanup. Task 13 is complete; task 14 is next.

### Checkpoint C

- [x] The Demo distinguishes profiles from observations without trusting their labels.
- [x] An independently run backend yields a useful RPC-only Investigation with explicit missing business evidence. User-reported manual Surfpool 1.7.0 trial: two local transfers executed while submission responses were withheld; Journal inspection identified the effects and missing status queries. No client/business conclusion is inferred from RPC evidence alone. Full Raydium discovery was blocked and remains a retest target in task 28.
- **Technical reference-consumer probe:** A separately launched reference relayer used a fresh Lab RPC URL without Checks or SDK. Its intent reached `succeeded`; RPC-only Investigation returned one Finding, `application_coverage_unknown`, zero `check.observation` events, and no formal Verdict at snapshot 37. Owned processes were stopped. This verifies independent reference-process wiring only. At that probe, no suitable third-party backend was available. The later manual trial above supplies bounded evidence; the complete retest remains open.

## Human and agent access

### 14. Embedded dashboard

Historical continuation: `b430db5` added bounded Findings pages and exact detail bookmarks; the final task-14 verification remains recorded below.

- [x] **Accept:** Ship prebuilt header, summary, Timeline, and evidence views with authenticated public Case/Check controls and bounded drill-down.
- **Verify:** No traffic, no match, Finding selection, Evidence Gaps, stopped/interrupted views, and browser closure without Lab stop.
- **Dependencies:** 9, 12. **Scope:** Medium; split panels if needed. **Likely files:** `dashboard/`, `src/dashboard.rs`, `tests/dashboard_flow.rs`.
- **Source:** [First dashboard](../docs/product/dashboard.md#workspace).
- [x] 14a. Reproducible React package and embedded asset serving.
- [x] 14b. Bounded browser tickets and Lab-scoped sessions.
- [x] 14c. Summary, Timeline, Findings, and evidence detail.
- [x] 14d. Pinned selection and public Case/Check controls.
- [x] 14e. Retained viewer and startup/Demo launch paths.
- [x] 14f. Native browser acceptance, accessible narrow layout, and visual review.
- **Continuation/link slice:** More than five real Findings were tested through public RPC-generated incidents. Five-item `investigate.findings` continuation pins Lab/snapshot/scope/projection. Errors preserve the previous page. Event and Finding bookmarks restore after fresh bootstrap. The Finding bookmark also restores through a new retained viewer after Lab-name reuse. Full integer coordinates serialize exactly. Focused Vitest, typecheck, build, native browser tests, fmt/check/Clippy passed; final accumulated acceptance is recorded below.
- **Controls/viewer slice:** Live Case controls and explicit Check collect/status/cancel/evaluate use existing handlers. The retained foreground viewer uses the shared read-only query core without a writer or simulator. Startup and Demo expose ticket launch metadata or exact-ID viewer argv. Live status polling does not move pinned evidence. HTTP bodies retain existing limits. Browser transport caps responses while reading and preserves unsafe integer fact representations.
- **Earlier controls/viewer verification:** Four Vitest/Testing Library tests and three real native Playwright flows passed. Browser flows cover Case mutations, two Lab sessions on one host, ticket reuse, client cookie expiry, response-loss Finding evidence, actual relayer collection/evaluation, and retained replay after source/Lab stop. Retained name reuse keeps the original identity. Desktop, narrow, and visible narrow evidence screenshots were inspected. The full Rust suite and release build passed. One pre-viewer `down` failure did not reproduce in ten focused runs; the harness now retains CLI error envelopes. Its cause remains unconfirmed.
- **Final acceptance:** `npm --prefix dashboard ci`, `run typecheck`, `test -- --run` (13 tests), `run build`, `cargo build --locked`, and `run test:browser` (8 native Chromium flows) passed. `cargo fmt --check`, `cargo check --locked`, `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings`, and `cargo build --release --locked` passed with built embedded assets. A focused real-process viewer test confirms stdout closure exits the owner, releases its listener, preserves Journal bytes, and leaves another Lab ready. Desktop and 390px screenshots, including connection loss and retained recovery controls, were viewed. Long recovery commands now wrap without overflow.
- **Coverage boundary:** Native browser flows cover no selected Lab, no traffic/waiting for match, more than five real Findings, event/Finding bookmarks, retained name reuse, late-read cancellation, keyboard focus, source collection with lost response, status inspection without retry, confirmed collection cancellation, Unicode payload byte continuation, operational errors, connection loss, and browser closure with the same armed Case. Visibility uses a controlled `document.hidden` signal in real Chromium with native HTTP. Starting, unsupported capability, analysis-limit, and core-failure renderer branches use controlled DTO tests; native lifecycle was not fabricated for those branches.
- **Known observation:** The earlier isolated normal-down failure remains unconfirmed, not fixed. No original structured envelope was found. The harness now preserves known IDs, original command/output, operation sequence, and read-only resource inspection before owned failure cleanup. See [unconfirmed-down-observation.md](history/unconfirmed-down-observation.md). No runtime retry or speculative Lab fix was added.
- **Preserved boundaries:** HTTP-only source collection; 32 KiB effective source cap; one intent/signature per evaluation; 512 records and 4 MiB evaluation evidence; read-only retained viewing; no runtime Node dependency. Checkpoint C's external-backend item remains unchecked. Task 14 completed after the explicit detail-error recovery regression passed. The subsequent tasks 15–24 run is recorded in [the historical ledger](history/implementation-run-2026-11-03.md).

### 15. Local MCP adapter

- [x] **Accept:** Expose shared operations with official `rmcp`, structured bounded results, bootstrap identity exceptions, and clean stdio.
- **Verify:** Scoped CLI/MCP parity, schema limits, cancellation, uncertain mutations, and EOF without Lab stop.
- **Dependencies:** 12. **Scope:** Medium. **Likely files:** `src/mcp.rs`, `src/mcp/tools.rs`, `tests/mcp_stdio.rs`.
- **Source:** [CLI and MCP behavior](../docs/product/investigation-interfaces.md#cli-and-mcp-transport-behavior).

## Complete modes and initial catalog

- **Implemented / verified:** Official rmcp3.8.0 stdio with 12 shared tools, schemas/annotations, exact-ID enforcement, bounded structured content, cancellation/uncertain-effect inspection, retained maintenance lease, and EOF preserving persistent Lab. Main606b1fd; seven real SDK/process tests passed. Wave1 accumulated checkpoint passed.

### 16. Explicit Read-through

- [x] **Accept:** Forward only allowlisted reads to an explicit source; preserve source evidence, local transaction authority, and credential redaction.
- **Verify:** Fake upstream proves no transaction, airdrop, simulation, unknown method, or silent local-history fallback reaches it.
- **Dependencies:** 6, 8. **Scope:** Medium. **Likely files:** `src/upstream.rs`, `src/rpc/routing.rs`, `src/evidence/redact.rs`, `tests/read_through.rs`.
- **Source:** [Read-through allowlist](../docs/product/solana-rpc-contract.md#read-through-allowlist).

- **Implemented / verified:** Seven explicit read methods through final-boundary allowlist; local transaction authority, no redirect/fallback, opaque source identity, runtime-only credentials, bounded redacted source chunks. Main0b8b297; ten fake-upstream/process histories passed. Full suite/build/browser/Clippy passed. Positive trusted-cert HTTPS remains untested; invalid TLS fails closed.

### 17. Managed native Surfpool

- [x] **Accept:** Pin/install/verify an owned native backend under the workspace; route all blockchain RPC through it with clean state and no automatic funding.
- **Verify:** Install/readiness failure, backend death, missing state, actual local execution, owned cleanup, and disabled direct Read-through.
- **Dependencies:** 3, 7, 16. **Scope:** Medium; split tool installation if needed. **Likely files:** `src/backend.rs`, `src/backend/surfpool.rs`, `src/tools.rs`, `tests/surfpool_mode.rs`, `tests/backend_ownership.rs`.
- **Source:** [Lifecycle](../docs/product/lab-lifecycle.md), [mode authority](../docs/adr/0013-managed-surfpool-execution-mode.md).

- **Implemented / verified:** Main7326fd3 pins native Surfpool1.7.0, explicit checksum-verified workspace install, exclusive blockchain authority, actual signed execution from an explicit snapshot, no automatic funding/deployment, readiness/death/clean restart/verified orphan cleanup, and first-only backend response loss. Combined native8 + Read-through10 + transaction33 passed. Mainbd3f8bf keeps source RPC -32603 distinct from Journal failure; actual fake-backend RED→GREEN passed. Native execution is verified on macOS arm64 only. Datasource hydration and other native platforms remain untested. Response-loss Findings remain explicitly simulator-only.

### 18. Timeout bursts

- [x] **Accept:** Implement pre-dispatch timeout/recovery in both modes and keep it distinct from post-acceptance response loss.
- **Verify:** Cycle boundaries, no dispatch, held batches, cancellation, Case capacity errors, and switching without invented Client Outcomes.
- **Dependencies:** 7, 9, 17. **Scope:** Small. **Likely files:** `src/cases/timeout.rs`, `src/findings/timeout.rs`, `tests/timeout_case.rs`.
- **Source:** [Timeout family](../docs/product/cases.md#rpc-unavailability-or-timeout).

- **Implemented / verified:** Commit `51cf510` implements frozen pre-dispatch outage/recovery, bounded shared holds, notifications/capability validation and later-attempt Findings. Combined `cargo test --locked` passed291 tests; explicit pinned native suite passed14. Source/dispatch boundaries remain distinct from acceptance response loss.

### 19. Null and stale status views

- [x] **Accept:** Implement per-identity masking/captured views without altering authoritative lifecycle; preserve mixed-signature order and commitment constraints.
- **Verify:** Source versus boundary evidence in both modes, unknown identities, missing snapshots, recovery, and replacement.
- **Dependencies:** 9, 17. **Scope:** Medium. **Likely files:** `src/cases/status.rs`, `src/findings/status.rs`, `tests/status_cases.rs`.
- **Source:** [Status family](../docs/product/cases.md#unavailable-or-stale-transaction-status).

- **Implemented / verified:** Commit `7ac917b` preserves authoritative lifecycle and ordered per-identity null/snapshot views. Combined transaction histories, backend source tests and explicit native status/Custom lookup histories passed within Rust 291 + native 14. Native first observed state can already be finalized; no prior backend state is invented.

### 20. Bounded confirmation stalls

- [x] **Accept:** Assign simulator-only delayed progression and expose the actual Surfpool capability limit; existing schedules survive Case changes.
- **Verify:** Progress without polling, commitment-gated lookups, duplicates, disarm, recovery, and no retroactive expiry after processing.
- **Dependencies:** 6, 9. **Scope:** Small. **Likely files:** `src/cases/confirmation.rs`, `src/findings/confirmation.rs`, `tests/confirmation_case.rs`.
- **Source:** [Stalled confirmation](../docs/product/cases.md#stalled-confirmation).

- **Implemented / verified:** Main53528f7/b0b0eea assigns bounded 2/34/36 and 2/4/36 schedules at atomic acceptance. Four controlled-clock histories now cover autonomous progression, gating, duplicates/disarm, expiry-after-processing exclusion, bounds/filters/notifications, and frozen batch registration times. Source-backed recovery Finding supports pinned drill-down and rule-bound cursors. Actual Surfpool arm and startup reject authoritative confirmation control without changing policy. Mode checkpoint passed.

### 21. Expired-blockhash reuse

- [x] **Accept:** Reuse genuine tuples with same-authority expiry evidence and retain each mode's actual admission/processing result.
- **Verify:** Heights below/at/above the bound, already processed identities, durable nonces, no expired submission, switching, and recovery.
- **Dependencies:** 6, 9, 17. **Scope:** Small. **Likely files:** `src/cases/blockhash.rs`, `src/findings/blockhash.rs`, `tests/blockhash_case.rs`.
- **Source:** [Expired-blockhash family](../docs/product/cases.md#expired-recent-blockhash-submission).

- **Implemented / verified:** Commit `d7503b9` reuses genuine tuples with same-authority inclusive height expiry, mode-specific outcomes and retained Findings. Combined Rust 291 + explicit native 14 passed, including actual backend height expiry and returned safehash admission. Durable nonce and processed-identity exclusions remain explicit.

### Checkpoint D

- [x] Each initial family has tested Coverage Claims and explicit mode limitations.
- [x] Full format coverage, source authority, and no-public-write behavior remain intact.

### 22. Declarative Custom Cases

- [x] **Accept:** Validate bounded YAML v1 into an inspectable plan; activate versioned maintained primitives/templates atomically with custom provenance.
- **Verify:** Composite histories, validation categories, limits, digest stability, mode/version errors, batch binding, and lifetime response-loss memory across reactivation.
- **Dependencies:** 18–21. **Scope:** Medium; split parsing from runtime if needed. **Likely files:** `src/cases/custom.rs`, `src/cases/schema.rs`, `src/cases/sequence.rs`, `tests/custom_cases.rs`, `tests/fixtures/cases/`.
- **Source:** [Custom Cases](../docs/product/custom-cases.md).

- **Implemented / verified:** Compiler commits `bfd4851`/`89d41c4`, runtime `c0e2294`, public schemas `0fcfe4a`, provenance `b64ac1c`, and UI isolation `e82a417` are integrated. Eleven YAML validation tests, thirteen public custom RPC/lifecycle histories (including 256 identities), five actual CLI/rmcp histories, source-failure/blockhash unit histories and actual native Custom Cases passed. Final combined Rust 291, dashboard 17, Chromium 8, fmt/check/Clippy and release build passed. Author provenance, scoped template evidence, immutable batch binding and Lab-lifetime response-loss memory remain explicit.

## Recovery and prototype measurements

### 23. Retained evidence maintenance

- [x] **Accept:** Expose explicit stopped-Lab deletion and bounded selected export; preserve originals on migration failure and distinguish export from replayable Chaincase.
- **Verify:** Reject live deletion; check interrupted reads, export limits/redaction, immutable references, WAL recovery, and workspace ownership.
- **Dependencies:** 8, 9. **Scope:** Medium. **Likely files:** `src/journal/maintenance.rs`, `src/export.rs`, `src/cli/artifacts.rs`, `tests/journal_maintenance.rs`.
- **Sources:** [Retention/export](../docs/product/evidence-and-findings.md#finalization-retention-and-export), [recovery](../docs/product/lab-lifecycle.md#recovery-retention-and-cleanup).

- **Implemented / verified:** Exact-ID stopped-Lab delete and bounded selected NDJSON export, shared reader leases, workspace ownership, preserved independent exports, interrupted WAL reads, cancellation, explicit payload chunk limits, and unchanged DB/WAL on unsupported-schema rejection. Main949a7ec plus process-disappearance fix31baf3f; ten maintenance process histories passed. No schema migration command is exposed; retained readers never upgrade original files. Wave1 accumulated checkpoint passed.

### 24. Recovery and resource measurements

- [x] **Accept:** Verify whole-process fail-stop, reserved drain capacity, safe orphan handling, and independent backend survival; record reproducible measurements before revising provisional limits.
- **Verify:** Writer/disk/child failure and PID reuse; measure idle, representative RPC/Case load, retained history, dashboard, and agent queries.
- **Dependencies:** 13–23. **Scope:** Medium. **Likely files:** `tests/failure_recovery.rs`, `tests/resource_scenarios.rs`, `scripts/measure-resources.*`, a scoped measurement note under `docs/research/`.
- **Sources:** [Resource model](../docs/adr/0019-resource-efficient-rust-core.md), [open validation](../docs/open-questions.md).

- **Implemented / verified:** Harness `82258a0` and prepared-evidence fix `2c12722` passed real writer/disk fail-stop, preserved acknowledged bytes, independent reference survival, native child/orphan/identity histories and reserved drain checks. Final Rust 291, native 14, dashboard 17 / Chromium 8 and release build passed. Optimized `e82a417` measurements completed simulator 14 and Surfpool 11 phases with no sampler/workload failures and complete owned cleanup. [Measurement record](../docs/research/resource-measurement-harness.md#measurement-record) records host, commands, hashes, latency, RSS/CPU, Journal size/rate, query overhead and limitations. Provisional limits were not revised.

### Checkpoint E

- [x] Record verified capabilities and remaining gaps without claiming production fidelity or adoption.
- [x] Validate one independent backend workflow and select the next improvement from its results. The user-reported Solana-backend trial selected task 28. This is bounded manual evidence, not a claim of full backend compatibility.
- [x] Preserve deferred CI wrappers, replay, SDK, distribution, and later transports in [open questions](../docs/open-questions.md).

- **External validation update:** The subsequent user-reported Solana-backend trial supplies independent evidence for Checkpoints C/E. Task 28 tracks the discovered gaps. A complete pool-discovery workflow and a repeat of the fixed UI with that backend remain external retest work. The bundled relayer and local fixtures do not substitute for that retest.

## Foreground terminal experience

### 25. Branded inline Lab panel

- [x] **25a:** TTY-only branded startup, bounded inline animation, readable endpoints, and a short dashboard hyperlink. Plain and JSON output retain their contracts.
- [x] **25b:** Authenticated live Case, Journal watermark, and held-request status; normal animated shutdown; truthful completion and unavailable states. No Findings polling or raw terminal mode.
- [x] **25c:** Native PTY verification for Ctrl-C, external stop, narrow resize, `NO_COLOR`, `--no-animation`, interrupted evidence, and machine output. Rebuild the release executable.
- **Dependencies:** Existing foreground lifecycle. **Scope:** `src/lab/terminal.rs`, `src/lab/foreground.rs`, CLI parsing, dependencies, and focused process tests.
- **Sources:** [Concept and implementation plan](../docs/product/investigation-interfaces.md#foreground-terminal-presentation), [CLI interfaces](../docs/product/investigation-interfaces.md), and [brand](../docs/product/positioning-and-brand.md#visual-identity).

- **Implemented / verified:** Console/Indicatif inline panel with Tokio-driven animation, one bounded authenticated status request at a time, short dashboard hyperlinks, opt-outs, and verified completion labels. `cargo fmt --check`, `cargo check --locked`, 300 Rust tests, Clippy with `-D warnings`, and release build passed. Five native PTY tests verify live Case and missing-status behavior, 36/88-column layouts, actual Ctrl-C, external stop, static/NO_COLOR/JSON/CI/dumb output, real held-RPC Fault color and drain, and interrupted completion. Release PTY smoke checks passed in simulator and real pinned Surfpool, including owned-child cleanup.
- **Visual verification:** Reviewed rendered VT100 screens from actual PTY output. Resize synchronizes pending frames with the emulator width; it does not emulate a particular window manager's scrollback reflow. Actual Zed rendering and hyperlink clicks were not automated. No frontend application changes or new business Verdicts were added. Checkpoints C/E remain open.


### 26. Dashboard link for the Lab lifetime

- [x] **Accept:** Repeated dashboard commands return the same link; that link and browser authorization remain reusable until Lab stop. Viewer access lasts until viewer exit.
- [x] **Verify:** Repeated native HTTP exchanges, real browser reopen after cookie removal, stale-key rejection after same-port Lab restart, former-timeout smoke check, terminal copy, full checks and release build.
- **Sources:** [Access decision](../docs/product/dashboard.md#access-and-retained-viewing), [browser contract](../docs/product/dashboard.md#access-and-retained-viewing), and [lifecycle](../docs/product/lab-lifecycle.md).

- **Implemented / verified:** One reusable launch key and one distinct browser key per live Lab or retained viewer, held only in runtime memory. Launch metadata reports null seconds and an explicit stop boundary. Native HTTP tests cover 40 repeated launches/exchanges, bearer/Host/Origin separation, and old-link/cookie rejection after a same-port name reuse. All 301 Rust tests, 17 UI tests, 8 native Chromium flows, fmt/check/Clippy, frontend production build and release build passed. A release smoke check confirmed the original link and cookie still work after 62 seconds, then stopped its isolated Lab normally. Browser flows reopen the original link after cookie removal. The former 30-minute server timer was removed; no 30-minute wall-clock soak was performed.

## Human dashboard navigation

### 27. Guided overview, observation detail and RPC inspector

Dependencies: tasks 8, 9, 14, 22 and 26. Preserve open Checkpoints C/E.

#### 27a. Bounded RPC exchange projection

- [x] Add `investigate.requests` and normalized `request_id` selection across shared queries and CLI.
- [x] Project same-snapshot related RPC facts through the existing index without raw-body reads or schema changes.
- [x] Verify pagination during appends, missing boundary, batch identity, scope/cursor isolation and response budgets.

#### 27b. Navigation and overview

- [x] Add Overview, Observations, Traffic, Case and Checks navigation with readable Lab state and first-use instructions.
- [x] Preserve filters, exact coordinates, detail bookmarks, Back navigation and retained recovery.
- [x] Verify onboarding, no-match, hidden-tab polling, stale-response cancellation and error recovery.

#### 27c. Observation investigation

- [x] Present readable Finding titles, statements, guidance, unknowns and linked evidence with source labels.
- [x] Keep Findings/evidence pagination and explicit bounded payload continuation.
- [x] Verify same-snapshot drill-down, exact signature links, unknown rules and unavailable evidence.

#### 27d. RPC inspector and controls

- [x] Add request list/detail tabs and an advanced Journal view with visible filters and paging.
- [x] Keep Case and Check actions on their own pages, including independent Custom YAML validation and collection cancellation.
- [x] Verify live and retained operation, current versus historical policy and preservation of form state on navigation.

#### 27e. Completion

- [x] Update the maintained interface contract and relevant usage guidance.
- [x] Pass UI typecheck/tests/build, native browser flows, Rust fmt/check/tests/Clippy and release build.
- [x] Inspect desktop/narrow screenshots and commit only implementation, tests and maintained documentation.

- **Implemented / verified:** Five-page navigation, guided first connection, readable Findings, same-snapshot RPC exchange projection and request-ID filtering. Existing bounded queries, Custom Case authoring, Check collection/evaluation, retained recovery and payload byte continuation remain available. Full Rust suite: 303 tests. After boxing the expanded CLI selection, 37 affected CLI/query/MCP/session tests were repeated. UI typecheck, 21 UI tests, 9 native Chromium flows, fmt/check/Clippy with `-D warnings`, production assets and release build passed.
- **Visual and release checks:** Reviewed desktop and narrow Overview, RPC inspector and Finding detail, plus Case/Checks screens from the release Demo. Verified long-signature wrapping, saved Findings page on Back, same-tab launch reauthorization after cookie loss, preserved YAML/intent forms, exact CLI request filtering and offline schema. Temporary verification Labs were stopped and removed. Research, mockups and screenshots remain local and excluded from commits. Checkpoints C/E remain open.

## Independent backend feedback

### 28. Surfpool discovery and response-loss investigation

- [x] **28a:** Forward `getProgramAccounts` to owned Surfpool with unchanged supported parameters and existing bounds. Verify filters, local state, source errors and simulator non-forwarding.
- [x] **28b:** Explain Surfpool signature return and observed withholding without requiring later activity. Preserve evidence links, deterministic IDs, mode/source checks and bounded observation windows. Verify replay and near-miss histories.
- [x] **28c:** Confirm successful Case mutations. Show activation time and committed application count for the current activation. Verify waiting/applied states and replacement.
- [x] **28d:** Explain empty Observations and link to relevant Fault records. Show a bounded transaction chain and later same-signature requests with exact record links, times and durations.
- [x] **28e:** Update contracts, run focused and full checks, inspect browser states and rebuild release. Record external retest limits.

Dependencies: tasks 15, 22 and 27. Source: [manual integration follow-up](../docs/product/dashboard.md#case-activity-and-history).

- **Verification record:** The original preparation test reproduced `getProgramAccounts` rejection. A retained schema-1 Surfpool history reproduced empty unsupported Findings. The fixes passed 308 Rust tests, including the 15-test real Surfpool 1.7.0 suite, plus 25 UI tests and 10 native Chromium flows. Native histories cover local account discovery and source errors, actual transfer response loss before any lookup, later signature lookup, Case counters, and owned cleanup. UI histories cover acknowledgment, applied state, observation-window bookmarks, exact source links and narrow layout. Screenshots remain ignored local QA artifacts.
- **Test corrections:** Surfpool can expose executed balances and `getTransaction` before its status-cache entry; the stale-status fixture now waits for the first actually observed status. An intermediate whole-suite run compared process executable inodes while another verification rebuilt the debug binary. The final complete suite ran without concurrent binary replacement and passed.
- **Release verification:** Rust fmt/check/Clippy with `-D warnings` and the optimized build passed. A temporary release Surfpool Lab verified advertised `getProgramAccounts`, a context-wrapped account query, activation time, committed activity, and the offline Finding schema. The Lab stopped normally. No user Lab was restarted.
- **External limit:** These fixes have not been rerun against the user's full backend/Raydium workflow. The manual integration report is retained as user-supplied evidence; native fixtures do not replace that external retest.

### 29. Live dashboard by default

- [x] **29a:** Add bounded descending record/Findings queries and Finding offsets. Preserve ascending defaults, snapshot isolation, cursor binding and response limits.
- [x] **29b:** Keep ordinary navigation and detail selection live. Update traffic, Findings, transaction chains, Case and collection status automatically. Preserve tabs, forms, payloads and focus.
- [x] **29c:** Add explicit pause/resume and retained bookmark behavior. Verify new traffic beyond 50 registrations, delayed boundaries, older pages, connection recovery, hidden tabs and cancellation.
- [x] **29d:** Update documentation, pass focused and required checks, inspect browser results and rebuild release.

Dependencies: tasks 27–28. Source: [Live dashboard plan](../docs/product/dashboard.md#live-investigation-and-recent-pages).

- **Implemented / verified:** Default-live navigation and details; explicit pause/resume; newest bounded Traffic/Journal pages and live same-signature history; descending core queries and bounded Finding offsets; watermark-driven analysis and visible-page polling. Runtime polling continues through `stopping` while the control API is available. Background refresh preserves detail tabs, form state, payloads and focus. Hidden destinations do not initiate list reads.
- **Checks:** 310 Rust tests passed, including the real Surfpool suite. UI typecheck/build and 28 UI tests passed. All 12 native Chromium scenarios passed, including live traffic beyond 50 requests, older pages, explicit pause/resume, late HTTP-boundary updates, live Surfpool Finding windows, retained bookmarks, hidden-tab behavior and connection recovery. Rust fmt/check/Clippy with `-D warnings` and the release build passed. Desktop/narrow views were inspected. Release smoke verified default/latest ordering, Finding offset, a live detail URL and embedded assets in an isolated Lab; that Lab stopped normally.
- **Limits:** Updates use serialized one-second status polling and finite Journal queries, not a new push transport. Existing query, response, payload and Finding-analysis limits remain in force. Saved evidence links retain explicit snapshots. Checks still require explicit collection/evaluation; live rendering does not repeat mutations or claim new Verdicts. No user Lab was restarted and no independent backend was launched.

### 30. Server-pushed WebSocket dashboard

- [x] **30a:** Add an owned, bounded WebSocket transport for live and retained views. Reuse core operations and committed/runtime/collection notifications. Verify authentication, command validation, cancellation, capacity and shutdown.
- [x] **30b:** Move browser reads, subscriptions and commands onto one socket. Remove browser polling code and polling-specific tests. Preserve selection, evidence bounds, payload continuation, pause and automatic reconnect without replaying mutations.
- [x] **30c:** Remove browser-only HTTP API duplicates. Preserve useful existing agent routes and bootstrap/static infrastructure. Replace affected HTTP/browser tests with push, idle, reconnect, visibility and agent-API checks.
- [x] **30d:** Update maintained contracts and verification records. Pass focused/full checks, inspect actual browser views and rebuild release.

Source: [WebSocket transport plan](../docs/product/dashboard.md#browser-websocket-transport). Task 29's polling transport is superseded; its earlier test record remains historical.

- **Implemented / verified:** One cookie-authenticated WebSocket carries bounded read subscriptions and explicit commands for live Labs and retained viewers. Commit/runtime/collection notifications push new data. Browser polling and duplicate browser HTTP data routes are removed. Existing bearer APIs remain unchanged. Read subscriptions resynchronize after visibility restoration and reconnect; mutations are never replayed.
- **Checks:** 313 Rust tests passed, including 15 Surfpool tests. After final query-admission and shutdown changes, all six native dashboard transport/session tests passed again. UI typecheck/build and 32 tests passed. All 13 native Chromium scenarios passed, including idle with no periodic frames or HTTP data requests, native push, visibility catch-up, reconnect, lost mutation acknowledgement without replay, payload continuation, late boundaries, Findings and retained viewing. Rust fmt/check/Clippy with `-D warnings` and release build passed. Desktop/narrow screenshots were inspected. An isolated release smoke verified the socket, commit push, Case command, bearer status and exact-ID normal stop.
- **Limits:** Live views coalesce changes into finite query results; they do not deliver every Journal event to the browser. Existing response/payload/analysis budgets remain. Initial cookie exchange and static assets remain HTTP. Explicit bounded server Check collection polling remains available. No user Lab was restarted. Independent-backend retesting was not performed in this task.

### 31. Human-readable Observations

- [x] **31a:** Add family-specific plain-language copy and a deterministic grouped story with exact evidence links and request counters.
- [x] **31b:** Integrate bounded pushed history, completeness/unknown states and one technical-details disclosure without changing Journal or Finding rules.
- [x] **31c:** Verify all maintained Finding families, Custom/fallback behavior, counting, limits, live updates, disclosure and the supplied snapshot. Update contracts and rebuild release.

- **Implemented / verified:** Plain-language cards and stories for every maintained Finding family, the Custom lookup template and conservative unknown-rule fallback. Detail opens with request counters, grouped evidence-linked steps, observed results and unknowns. One Technical details disclosure retains the former technical panels, exact records, transaction history and payload pagination. Only the UI and documentation changed in this task; Journal facts, core Finding rules and APIs are unchanged.
- **Checks:** UI typecheck, 47 UI tests, production build, all 13 native Chromium flows and six native Rust dashboard/session tests passed. Rust fmt/check/Clippy with `-D warnings` and the release build passed. Desktop/narrow screenshots were inspected. Release smoke confirmed the new embedded story assets, authenticated socket, commit push, Case command, existing bearer API and normal exact-ID stop.
- **Supplied example:** Read-only metadata from Lab `La056ac3064640eaa9200d5c4dd363726faa0c4e1a5464452553c90bc2dab2096` through snapshot `629` produced five steps, one submission, two status checks and one transaction lookup. The first follow-up is approximately 15 seconds after the original request. The final step links to event 629. The original Lab was not restarted or modified; its data and screenshots were not added as repository fixtures.
- **Limits:** The readable story uses at most 200 metadata records under existing response limits. Partial, restricted, legacy or interrupted evidence exposes lower-bound counts and does not support absence claims. Full technical history remains paginated. English copy follows the existing dashboard language. No unrelated backend integration or full Rust suite rerun was required for this presentation-only change.

### 32. Compact Observation timeline and actual RPC results

- [x] **32a:** Resolve bounded retained lookup replies with exact request/signature correlation and separate source, prepared and HTTP-boundary evidence.
- [x] **32b:** Add compact outcome/counts/timestamps and accessible state icons/colors for all Observation families. Move caveats and evidence below Technical details.
- [x] **32c:** Verify reply states, overrides, missing/redacted evidence, live updates, the supplied snapshot, native browser flows and rebuilt release.

- **Implemented / verified:** Compact outcome and inline counts above a timed, icon/text/color timeline. Intentional Faults are blue; confirmation success is green, execution errors red, processed amber, unavailable/RPC/null/delivery states neutral. Equivalent consecutive outcomes use counts and a real time range. Repeat submissions remain visible after earlier lookups. Caveats, identifiers, source links and retained replies are behind Technical details; snapshot and partial-evidence limits remain visible. Journal/runtime formats and Finding rules are unchanged.
- **Reply evidence:** Existing WebSocket evidence reads resolve exact request/signature positions, prepared results, verified replacements and complete retained source chunks. No simulator transition is substituted for an RPC reply. A getTransaction without a confirmation level does not invent finality. Simulated states are labelled. Missing, redacted, oversized, uncorrelated or unverifiable data stays unknown. Failed reads can recover on pushed resynchronization without polling.
- **Checks:** Typecheck, 66 UI tests, production build, 13 native Chromium flows, six native dashboard/session tests, fmt/check/Clippy with `-D warnings`, and release build passed. The native Surfpool flow compared the displayed status with the actual RPC response. Desktop/narrow screenshots were inspected. The release smoke verified new compact assets, cookie-authenticated WebSocket, commit push, Case command, existing bearer API and normal exact-ID cleanup.
- **Supplied history:** Read-only replay of the original Lab through snapshot 629 resolved both getSignatureStatuses replies to finalized with null err. getTransaction returned a transaction with null meta.err but supplied no new confirmation level. The compact result remains finalized, with one submission, two status checks, one transaction lookup and the original no-response boundary at 20 seconds. No user Lab was restarted or modified; replay data/screenshots remain outside tracked sources.
- **Bounds:** Up to 200 history metadata records, 16 recent lookup resolutions, 32 KiB/two chunks per automatically loaded source response, two workers and 32 cached entries. Longer/partial histories retain explicit limits and existing technical pagination. No business-completion Verdict is inferred.

### 33. Solid widget Workspace rewrite

- [x] **33a:** Replace React tooling and screens with a structured Solid app, authenticated live transport, default Workspace and virtualized Traffic. Verify exact coordinates, subscription lifecycle and production build.
- [x] **33b:** Add movable/resizable/collapsible/hideable widgets, project-scoped layout persistence, reset/maximize and live Case controls. Verify pointer/keyboard behavior, mutation feedback and laptop/monitor layouts.
- [x] **33c:** Group confirmed fault applications into live episodes, link Findings, highlight/filter related Traffic and add expandable timed Inspector steps. Verify repeat applications, missing evidence and late replies without invented outcomes.
- [x] **33d:** Preserve request/record/payload inspection, bounded history navigation, bookmarks, retained viewing, Custom Cases, Checks and Lab diagnostics. Verify cancellation, errors, Unicode continuation and reconnect without mutation replay.
- [x] **33e:** Run focused/full frontend and native checks, inspect desktop browser screenshots, update maintained contracts and rebuild the embedded release. Remove obsolete React code/dependencies and document verification limits.

Source: [accepted widget redesign and rewrite](../docs/product/dashboard.md#workspace). No comparative framework prototypes or new agent HTTP operations are required. Independent-backend Checkpoints C/E remain separate.

- **Implemented / verified:** The React renderer and hooks were replaced by a Solid application organized under app, shared and feature-owned modules. Workspace opens by default with editable Cases, Observations, Traffic and Inspector widgets. Committed fault applications appear immediately as episodes; linked Findings share that episode. Inspector paths expand in place and retain expanded steps when returning from a request. Existing WebSocket/core operations, exact coordinates, source authority, explicit mutations, retained viewing and raw-payload byte continuation remain.
- **Checks:** Strict typecheck, Prettier check, 53 frontend tests, production build and all eight native Chromium scenarios passed. Native flows cover simulator and actual Surfpool 1.7.0, immediate episodes before follow-ups, returned status, filters, pointer/keyboard layout editing, form/layout retention, reconnect without mutation replay, independent Check collection/cancellation, Unicode continuation and retained exact-ID bookmarks. All six Rust dashboard/session tests passed with loopback permission. Rust fmt/check/Clippy with `-D warnings` and the embedded release build passed. A release smoke compared served JavaScript byte-for-byte with the completed build and verified session bootstrap and exact-ID shutdown.
- **Visual verification:** Desktop and 1280×720 laptop screenshots were inspected. Essential Case controls remain visible at the initial layout size. Screenshots, research and test artifacts remain outside tracked sources.
- **Limits:** No comparative performance benchmark or independent-backend retest was performed. The full Rust suite was not rerun for this frontend replacement; the six native dashboard tests were rerun. Core/query/payload budgets remain. Browser layout storage is scoped to project and origin, so another viewer port has separate preferences. Reading uses bounded windows rather than accumulating the whole Journal. No worker, Wasm renderer, new agent HTTP endpoint, production forwarding or core Finding rule was added. Existing user Labs were not restarted or modified. No commits or publication were performed.

### 34. Fixed widgets, reliable Solid drag and Ronchi visual identity

- [x] **34a:** Reproduce title dragging with all four widgets visible. Replace the grid-fit gesture with Solid DnD, visible drop targets and fixed-panel slot exchange. Add pointer/keyboard dividers and persistence without hiding any widget.
- [x] **34b:** Remove adding, hiding, collapsing and maximizing widgets. Restore Ronchi's readable typography, high-contrast surfaces and coral accent. Verify laptop/desktop screenshots and preserved form/Inspector state.
- [x] **34c:** Run regression/model/browser checks, update current contracts and rebuild/verify release assets. Keep research and screenshots outside committed sources.

The original regression failed on the previous build: dragging Traffic by its title with four visible widgets left its x-coordinate unchanged. The former browser test moved only the small grip after hiding Inspector and did not cover that user workflow.

- **Implemented / verified:** Replaced the failed grid-fit gesture with Solid DnD header activation, drag overlay, pointer hit-testing and slot exchange. All four widgets stay mounted and visible. Adjacent split dividers resize columns/rows within the viewport; pointer previews use animation frames and preferences persist after completion. Pointer cancellation, Escape/outside drops, keyboard moves and reload persistence are covered. Obsolete add/hide/collapse/maximize controls and state are removed.
- **Regression:** The title-drag test failed on the original build (Traffic x remained 380) and passed after the replacement with all panels present. A second native test verifies resize without hiding panels; cancellation/outside-drop tests preserve the layout. The former test's hidden-Inspector shortcut was removed.
- **Checks:** 55 frontend tests and ten native Chromium scenarios passed, including actual Surfpool, immediate live episodes, returned replies, retained viewing, Check cancellation and Unicode payloads. Strict typecheck, production build and formatter passed. Laptop/desktop screenshots were inspected. Release assets were rebuilt; no independent-backend retest or comparative performance claim is added.
- **Visual direction:** Restored Ronchi's original system font stack and 15px base with readable table/control text, brighter text and surfaces, coral accent and white primary actions. The blue accent is removed, including focused draggable headers. Research materials and screenshots remain outside tracked sources. No user Lab was restarted.

### 35. Lab-wide RPC rates

- [x] **35a:** Add constant-size committed RPC counters and retained replay without changing original Journal facts. Verify batches, exact ten-second splits, rollback and terminal bounds.
- [x] **35b:** Show `RPS avg` and `RPS active` beside health. Verify browser-local idle updates, scope independence, missing data and stopped history without polling.
- [x] **35c:** Update the metric contract, run focused native/browser checks and rebuild embedded release assets.

- **Implemented / verified:** Header rates use all committed `rpc.registered` records. Batch members count separately. The writer maintains constant-size counters; failed commits do not publish them. Real monotonic Lab time drives average duration. The browser advances the display locally without network traffic. Retained replay freezes at the terminal record, or labels the last-record bound when termination is unknown. Missing/disconnected counters show unavailable. Filters and selected history do not scope the rates.
- **Checks:** Full Rust suite, 65 frontend tests, strict typecheck, formatting, Clippy with `-D warnings`, production and release builds passed. The new native Chromium scenario verified batch counting, filtering independence, idle updates without socket/HTTP requests, retained reopening and fixed stopped values. The laptop screenshot was inspected. Release smoke verified embedded JavaScript byte-for-byte, session bootstrap and exact-ID shutdown. Native Surfpool fixture-dependent paths were not exercised in this run; no independent-backend performance claim is added.
- **Limits:** Each active interval and the average denominator have a one-second minimum. Active intervals describe arrivals, not actual backend CPU work. Retained reconstruction streams the history once per status/viewer opening; it does not modify the Journal or cache the whole history. Existing user Labs remain unchanged. Nothing was committed or published.

### 36. Remove application Checks

- [x] **36a:** Remove collection/evaluation modules, configuration and public runtime/agent/browser surfaces. Keep generic historical Journal reads and existing RPC behavior.
- [x] **36b:** Adapt the reference relayer and Demo; remove feature-specific tests and preserve remaining recovery/investigation coverage.
- [x] **36c:** Remove dashboard Checks, update current docs and superseded decisions, verify remaining flows and rebuild release.

- **Implemented / verified:** Removed the HTTP collector, deterministic requirement evaluator, source configuration, Journal-specific application queries, Check CLI/schema/MCP/control/socket operations and dashboard route. The reference relayer no longer serves its observation adapter. Demo awaits its ordinary intent endpoint and reports only RPC summary, Findings and evidence commands. Retained generic Journal reads remain available, including original retired records. New configurations reject removed source settings. No current application-fact collection or business Verdict surface remains.
- **Checks:** 284 Rust tests, 65 frontend tests and all ten native Chromium scenarios passed. Browser coverage includes real Surfpool, live fault episodes, retained viewer, layout editing, WebSocket reconnect and Lab-wide RPS. Negative checks cover removed CLI/schema/config/control routes and MCP catalog; generic retired record readback remains intact. Typecheck, frontend formatting/build, Rust fmt/Clippy with `-D warnings` and release build passed. The release smoke compared served JavaScript byte-for-byte with the current build and confirmed the absent Checks navigation/configuration. All 404 local Markdown links and heading targets passed. The final laptop screenshot was inspected. Fixture-dependent native Rust Surfpool paths were not separately exercised; the real Surfpool browser scenario passed.
- **Documentation:** Deleted the dedicated feature contract and removed current product, interface, setup and glossary claims. Updated the project Finding-validation reference. Existing ADR/research/task records preserve history with explicit retirement notices; they do not authorize restoring the feature. No original Lab files or user backend data were changed. No commits or publication were performed.

### 37. Remove confirmed dead code

- [x] **37a:** Audit backend, test fixtures and dashboard reachability; remove unused presentation helpers, styles, icons and retired metadata without dropping active schemas or evidence readers.
- [x] **37b:** Run affected verification, inspect the dashboard and rebuild release. Record findings and limits.

- **Removed:** The unused parallel Finding title/description table, helpers and their obsolete test/assertions; nine unused SVG entries; retired application-event labels and two unreachable generic title entries; unused checkbox/tooltip styles; the unread relayer readiness field in the shared integration fixture; four retired completeness/clock response fields and their unnecessary terminal-time query in the reference store. Original relayer observations, schema-only Rust types, dynamic outcome styles and active source/RPC behavior remain.
- **Audit:** All backend/test Rust files resolve through crate roots, modules or `include!`; dashboard source/test files resolve through entrypoints or test roots. No detached source file was confirmed. Declared Rust dependencies have active source uses; frontend dependencies also retain consumers. Existing TypeScript unused-local/parameter checks and all-target Rust Clippy passed. This is a scoped static/caller audit, not a proof that every possible unreachable branch has been found.
- **Checks:** 284 Rust tests and 64 frontend tests passed. The native Chromium response-loss/lookup scenario passed after cleanup. Production typecheck/build, Prettier, Rust fmt, all-target Clippy with `-D warnings` and release build passed. The final embedded release was rebuilt after the last title-table cleanup. No runtime schema migration, new test dependency, comparative performance claim or original Lab mutation was introduced.

## Current documentation maintenance

### 38. Documentation organization

- [x] **38a:** Consolidate current dashboard, shared interfaces, development and reference-consumer docs with one owner per topic. Remove obsolete launch guides after preserving durable rules and unique evidence.
- [x] **38b:** Keep ADR addresses and completed task evidence stable, isolate historical run logs, update agent routes and verify paths, anchors and retained contracts.

- **Consolidated:** Browser behavior/access/transport/RPS now live in `docs/product/dashboard.md`; shared query and CLI/MCP contracts remain at `investigation-interfaces.md`. Development owns build/verification/source layout and durable commit rules. The reference relayer document keeps its process, storage, recovery and evidence limits without repeating Demo orchestration. Root navigation and AGENTS route directly to these owners.
- **Removed / retained:** Removed four obsolete initial-run/editor launch guides after extracting durable rules. Shortened plan and progress to current roles; completed evidence remains in this checklist. Moved the completed run ledger and unconfirmed diagnostic record into `tasks/history`. Preserved commit IDs from the old progress snapshots. All 19 numbered ADR addresses and non-link decision text remain; a dedicated ADR index makes current relationships explicit. Research sources and external links remain in their research owners.
- **Corrections:** Documented twelve current MCP tools, implemented MCP/Solid modules, coral Fault styling and current dependencies. Removed nonexistent module names, removed-Check actions and unsupported example response fields. Clarified that deferred blockchain WebSocket is distinct from the implemented dashboard socket. The full independent-backend retest remains open; bounded earlier manual evidence is retained.
- **Verification:** All 326 local Markdown links/heading targets passed. ADR paths/decision text and CONTEXT were checked against a temporary pre-edit snapshot. Cases, Custom Cases, RPC and evidence contracts remain unchanged, including candidate/Pack names and numeric limits. All UI/query/transport bounds remain in their consolidated owners; obsolete single-host Node/npm observations were not turned into support requirements. Commit references and research external sources were compared. `git diff --check` passed. Runtime/tests/assets were unchanged by this task, so no runtime test or native rebuild was performed. No commits or publication.

### 39. README redesign

- [x] **39a:** Study official developer-tool READMEs and GitHub formatting; record the brief and implementation boundaries in plan.md. Keep research notes outside tracked docs.
- [x] **39b:** Direction selected: Human + agent with the branded Ronchi parallel-band header. Approved narrative leads with a real static Workspace scene. No mockups.
- [x] **39c:** Produce the selected README, real terminal example and dashboard captures from an isolated known workload.
- [x] **39d:** Verify current commands, truthful agent/mode claims, media readability and local links.

- **Implemented:** Replaced the root README with the approved Human + agent presentation, an original parallel-band SVG header, a real populated Workspace, a focused Observation path and an actual PTY terminal transcript rendered as PNG. Added copyable build, Demo, own-backend, Case, filtered CLI and MCP commands. Linked detailed contracts instead of embedding their full specification. Final README media live in `docs/assets/readme/`; the development guide distinguishes these assets from disposable test captures.
- **Evidence:** The isolated release scenario recorded one accepted submission, response withholding, client disconnect, two status checks (`null`, then simulated `finalized` with `err: null`) and no repeat submission. CLI summary, requests, Findings, field-projected evidence and exact-event payload queries succeeded against the same history. A separate real Demo confirmed the stopped relayer, retained ready Lab and dashboard launch command. A foreground PTY capture exited successfully after Ctrl-C. All capture Labs were stopped by their exact IDs; user Labs were not touched.
- **Verification:** Local Markdown/HTML rendering loaded all four images with alt text, produced no browser errors or horizontal overflow at GitHub-like content width, and was inspected in light/dark presentation. All 329 local Markdown links/anchors passed; HTML media/navigation references were checked separately. PNG metadata and sources contain no dashboard credentials or personal paths. `git diff --check` passed. Runtime source, dependencies and existing release assets were unchanged; no broad runtime suite or rebuild was needed. Research, raw output, render previews and capture scripts remain outside the repository. No commit or publication.

Research compared uv, Bun, Gum, VHS, LazyGit, OpenCode, mitmproxy and HTTP Toolkit. These references informed presentation; they do not establish usability or performance measurements.

### 40. Curl installation and bundled Surfpool

- [x] **40a:** Add MIT metadata and installed companion discovery with pinned integrity validation; retain source-build archive installation and project-local runtime state.
- [x] **40b:** Package Ronchi plus compatible Surfpool and notices; implement checksummed, versioned curl installation and rerun updates without changing working installations on failure.
- [x] **40c:** Add platform build/package/install checks and explicitly triggered release automation. Defer Homebrew and external publication.
- [x] **40d:** Verify real installed Simulator/Surfpool flows and meaningful installer failure/update cases; update current contracts, README and progress with tested platform boundaries.

Dependencies: 40a → 40b → 40c → 40d. Existing runtime ownership, Journal and RPC evidence contracts remain in force.

- **Implemented:** MIT license and package metadata; shared pinned Surfpool catalog; canonical-executable companion discovery with archive/binary verification and explicit source-build fallback. Added a POSIX curl installer, reproducible native packaging, Rust/UI dependency notices and covered Surfpool dependency sources. Versioned, content-addressed installations preserve active processes; updates switch only the user-local symlink. No PATH-selected Surfpool or per-project executable copy is needed for installed distributions. Project-local Journals, runtime state and process ownership remain unchanged. ADR-0020 records the executable-storage exception to ADR-0014.
- **Removal:** Removed the user-requested system Surfpool 0.12.0 at `/usr/local/bin/surfpool` through macOS administrator authorization and the existing project-local 1.7.0 executable/archive. No user Lab, Journal or chain-state directory was deleted. Archives and disposable executable copies used for verification remain test/build artifacts, not system installations.
- **Verification:** 286 Rust tests, all-target Clippy with `-D warnings`, fmt and release build passed. The Surfpool suite passed all 15 tests with the real pinned archive enabled. Sixteen installer cases passed through a loopback HTTP mirror, covering install, rerun, update, SHA mismatch, unavailable downloads, unsafe/duplicate entries, wrong executable version, PATH quoting, concurrent locks, unrelated files/symlinks and damaged installed permissions/directories. The actual macOS archive installed and ran Simulator plus two Surfpool Labs in three clean projects without tool copies or system Surfpool. Embedded dashboard and project-local retained Journals were checked; a tampered companion failed before Lab allocation. A real Surfpool Lab survived a rebuilt-package update with unchanged owned-child identity, then stopped normally. Packaging rejected a wrong architecture and unpinned upstream input; re-packaging identical inputs produced the same archive checksum. Shell/Python syntax, workflow YAML structure and local documentation links passed.
- **Boundaries:** The macOS arm64 archive is prepared locally under `target/distribution/` (ignored). The workflow is configured for macOS arm64, macOS Intel and Linux x86_64, but GitHub-hosted Intel/Linux execution remains unrun. The initial declared distribution baseline is macOS 15.5+ and Ubuntu 22.04-compatible glibc 2.35+ with libgcc_s; this is not a test claim for every OS at those versions. The local smoke host was macOS 15.2; the pinned ARM Surfpool itself declares 15.5. No frontend source changed, so the existing embedded dashboard was exercised without rerunning the unchanged UI suite. No Homebrew setup, global installer execution, commit, tag, push or release publication was performed. README labels curl installation as release preparation until actual artifacts are published.

### 41. First native release

- [x] **41a:** Push main and manually dispatch Native distribution at the implementation revision.
- [x] **41b:** Resolve CI failures and obtain a successful full native matrix without bypassing checks.
- [x] **41c:** Push v0.1.0 at the verified revision and confirm successful tagged build/publication.
- [x] **41d:** Verify public curl installation, remove the public-installation warning and push the final public-install documentation.

The user explicitly authorized all four steps, including corrective commits/pushes and release publication. Homebrew remains deferred. The user separately requested faster Actions.

**CI fixes and retained evidence:**

- Run `42396477538` at `fea899d` exposed Linux-only `openpty` Clippy and timing-sensitive macOS batch/hold fixtures. Commit `aa1af54` uses the portable raw FFI pointer and gives those fixtures two-second hold budgets without dropping assertions. Local history (60), terminal (5), fmt and Clippy passed.
- Run `42397741867` passed the complete Linux pipeline. Commit `6cae931` adds an explicit registration-within-outage assertion. Commit `11c130d` fixes a real cancellation race by publishing committed dispatch prevention before hold-start exposes its deadline. The slow SQLite commit regression failed before the fix and passed afterward, including the resulting Finding. Timeout (10), history (60), hold-ownership (4), fmt and Clippy passed locally.
- Run `42399439120` exposed an incorrect no-data assertion for the existing `journal_failed` response and a five-second helper limit in the bulk fixture. Commit `83e7e52` corrects both expectations.
- Run `42400276900` exposed a real deadline problem: separately durable per-entry Custom lookup records exceeded the unchanged thirty-second RPC deadline. Commit `dfe3a48` writes source/effect pages of at most eight events. All per-entry records and exact source references remain. The injected effect-page failure was red before batching and green afterward; all 256 source links, 61 histories, 18 unit tests, 15 real Surfpool tests, cancellation/schema restoration, fmt and Clippy passed locally. The Linux cancellation fixture restores bounded lock waiting before schema writes; its checkpoint probes remain nonblocking.
- Run `42401521251` passed the complete Linux and Intel pipelines. ARM's twenty-second tamper-rejection timeout exposed fixed ten-millisecond sleeps while reading tar stdout. Commit `f2bcf51` uses bounded readiness polling instead. Archive/binary checksums, operation deadlines and the twenty-second smoke limit remain. Streaming/EOF/deadline tests, Clippy, release packaging and the actual local install/reinstall/tamper/live-update flow passed.

**CI acceleration:** Commit `0ca02a0` adds separate platform-keyed Cargo-download, compiled-output and pinned `cargo-about` caches. Standalone save steps retain completed work after later failures. Every product/release gate still runs on hits. CI keeps line-table debug information and disables incremental state to limit cache volume; the shipped release profile is unchanged. Official cache v6.1.0 is SHA-pinned. Actionlint, YAML parsing, shell syntax and an isolated Rust profile smoke passed.

**Release:** [Manual matrix `42404486999`](https://github.com/kirillermakovichh/ronchi/actions/runs/42404486999) passed on macOS ARM, macOS Intel and Linux x86_64 at `0ca02a0`. The annotated `v0.1.0` tag was pushed at that exact revision. [Tagged run `42406451668`](https://github.com/kirillermakovichh/ronchi/actions/runs/42406451668) repeated all gates and published [v0.1.0](https://github.com/kirillermakovichh/ronchi/releases/tag/v0.1.0). The Rust suite passed 290 tests; installer fixtures and actual installed Simulator/Surfpool checks passed. ARM's completed tagged log confirms exact hits for all three new cache categories.

Observed job durations at the same source revision:

| Target | Empty caches | Restored caches |
| --- | --- | --- |
| macOS ARM | 8m57s | 7m51s |
| Linux x86_64 | 15m10s | 11m39s |
| macOS Intel | 22m39s | 15m49s |

These are two observed CI runs, not a fixed latency guarantee. Cached compilation does not remove integration-test execution, notice generation or installation checks.

**Published-asset verification:** All eight release files were downloaded through authenticated GitHub access. All three archive SHA-256 files and version/target/Surfpool metadata matched; the published installer matched its maintained source. The downloaded macOS ARM archive passed actual installation, embedded dashboard/Simulator/two Surfpool projects, retained Journals, tamper rejection, reinstall and live-update checks. The fixture ignored an unrelated system Surfpool. All verification installations and Labs were isolated; no original user Lab/backend or global installation was changed.

**Public installation completed:** The user made the repository public and resumed verification on 2026-11-08. The exact public installer URL downloaded without GitHub credentials; the test used an empty curl config, removed GitHub token variables and installed into an isolated temporary prefix. Its bytes matched the maintained installer. Actual installed macOS ARM Simulator and two Surfpool projects, embedded dashboard HTTP, project-local retained Journals, tamper rejection and same-version reinstall all passed. Every test Lab stopped and its temporary installation was removed. Anonymous downloads of all three platform archives matched their published SHA-256 files; `release-version.txt` and the eight-file release inventory matched. Intel/Linux runtime evidence remains the completed native CI jobs, rather than execution on this macOS host. README's public-installation warning is removed and release notes describe the working command and platform baseline. The unchanged `v0.1.0` tag still points to `0ca02a0`; yellow branding/terminal updates remain newer `main` source changes. Temporary logs, downloads, diagnostic scripts and reports remain outside the repository. Homebrew and the independent-backend retest remain separate work.

### 42. Yellow identity and README media

- [x] **42a:** Replace the dashboard coral accent/backgrounds with centralized yellow tokens and align the compact terminal mark/runtime text.
- [x] **42b:** Verify the affected UI/PTY presentation, rebuild the executable and commit/push the palette and terminal changes.
- [x] **42c:** Replace README images with the three supplied originals, align captions/header and commit/push the complete README refresh.

Palette/terminal changes are pushed in `722b8a7`. Frontend build and 64 tests passed; five real PTY tests verify the compact four-band mark, requested label/value colors, narrow layout and terminal opt-outs. Rust fmt, Clippy and the local release build passed. The user supplied the original Desktop screenshots. All three PNGs were copied byte-for-byte into the existing README media paths: terminal 926×514, Observation 1248×1282 and Workspace 3024×1642. Source/destination SHA-256 values matched. README captions now describe the shown retained Surfpool history; the header uses yellow. Local links/anchors, SVG XML and diff whitespace passed. README media was not regenerated. Temporary previews and research files are not committed. Repository visibility and the existing release/tag remain unchanged.

### 43. Publish the yellow UI as v0.1.1

- [x] **43a:** Bump native/frontend versions, retain dependency versions, verify the source build and keep dependency-cache reuse across the patch bump.
- [x] **43b:** Push the new tag and obtain a successful complete native matrix/publication without bypassing checks or replacing v0.1.0.
- [x] **43c:** Verify latest public curl installs v0.1.1 with the yellow embedded dashboard and terminal, verify checksums/update behavior and push release documentation.

The user explicitly requested a rebuilt release so curl installation includes the accepted yellow palette and terminal changes. Use the existing tag-triggered build/test/package/install/publish gates. A separate duplicate manual matrix is unnecessary for this patch; publication still waits for the complete tagged matrix. The CI fallback cache is target-scoped; Cargo revalidates compiler, source, features and profile fingerprints.

**Release and verification:** The annotated v0.1.1 tag points to `662e990`. [Tagged workflow `42417380310`](https://github.com/kirillermakovichh/ronchi/actions/runs/42417380310) passed the complete macOS ARM, macOS Intel and Linux x86_64 matrix and published [v0.1.1](https://github.com/kirillermakovichh/ronchi/releases/tag/v0.1.1). Rust's 290 tests, fmt, Clippy, sixteen installer fixtures, notices, packaging and actual installed Simulator/Surfpool checks remained enabled. The v0.1.0 tag and assets were preserved.

All eight assets were present. Anonymous downloads of all three archives matched their SHA-256 files; published version metadata was `0.1.1`. The public latest installer matched the maintained script and installed v0.1.1 without GitHub credentials into a temporary prefix. Served embedded CSS contained `--accent:#f3d65b`; real PTY output contained v0.1.1, the compact four-band mark and yellow 221. Ctrl-C stopped that Lab normally. An isolated v0.1.0 Surfpool Lab stayed ready across the upgrade with the same owned-child identity and a responding RPC. The old executable remained available; the new CLI stopped the old Lab normally. Actual installed Simulator/two Surfpool projects, embedded dashboard HTTP, retained Journals, tamper rejection and same-version reinstall passed. Every temporary Lab stopped and its prefix was removed. No user installation or Lab changed. Intel/Linux runtime evidence remains the native CI jobs, not execution on this macOS host.

README now links v0.1.1. Both release pages describe public installation; v0.1.1 explains the palette and restarting a Lab after update. Temporary logs, helpers and downloads remain outside Git. Local link/anchor and diff-whitespace checks passed. Homebrew and independent-backend retesting remain separate work.

### 44. Automatic shell PATH setup

- [x] **44a:** Configure shell profiles after successful installation, preserve existing settings and avoid duplicate PATH entries. Add an explicit opt-out.
- [x] **44b:** Update the one-command installation flow and verify current/new terminal behavior, reinstall and the real native archive.
- [x] **44c:** Publish the maintained installer and README, verify the anonymous public command and update installation instructions on release pages.

The user accepted automatic profile setup and one install command that activates PATH in its parent terminal. The maintained installer is downloaded from `main`; native version selection, archives, checksums and private Surfpool still come from Releases. This separates installer fixes from native-binary releases. Existing release tags and attached files remain unchanged.

The installer adds a guarded POSIX block to zsh's selected `.zshrc`, Bash's interactive/selected login profiles or a POSIX login profile. Existing bytes and valid dotfile symlinks are preserved. Reinstall and repeated sourcing do not duplicate the block or PATH entry. Unknown shells or unusable profiles receive an actionable fallback. `--no-modify-path` leaves profiles unchanged. Prefixes containing colons or newlines are rejected. README's command ends with `&& export PATH="$HOME/.local/bin:$PATH"` for the current zsh/bash terminal.

**Verification:** Twenty-nine installer tests passed with a local HTTP mirror and isolated shell homes. Tests cover actual new Bash login/interactive and zsh interactive processes, ZDOTDIR, profile precedence, existing settings/symlinks, quotes/metacharacters, failures, opt-out and the README command. Shell/Python syntax, local Markdown links/anchors and diff whitespace passed. The new installer also passed the actual published macOS ARM v0.1.1 archive's full install smoke: shell PATH, Simulator, two Surfpool projects, embedded dashboard, retained Journals, tamper rejection, reinstall and live Surfpool update. All test homes, installations and Labs were temporary. User profiles, installations and Labs were not modified. No Rust/frontend source or native binary changed; the existing v0.1.1 native matrix remains its platform evidence.

**Public verification:** Commit `c009cf1` publishes the maintained installer and README without changing v0.1.1's native archives or attached installer snapshot. The raw public installer matched the checked source byte-for-byte. The exact README command ran anonymously with an empty curl config, no GitHub credentials and a disposable zsh home. `ronchi --version` worked immediately in the installing terminal and in a new interactive zsh, returning `0.1.1`. Automatic `.zshrc` setup was present. The temporary installation was removed; no user profile or installation changed. Both release pages now show the same maintained-installer command. No native rebuild, new tag or archive replacement was necessary.

### 45. Documentation status and navigation cleanup

- [x] **45a:** Reconcile current task/release/install status and remove duplicated session history from the progress summary.
- [x] **45b:** Correct verified stale Case, RPC and Journal status text; keep deferred work separate from implemented behavior.
- [x] **45c:** Verify local paths/anchors and preserve decision bodies, glossary, research, scoped limits and remaining validation.

The documentation map now routes installation directly and no longer carries a stale ADR count. The task header no longer points to completed task 41. Progress links the detailed evidence in tasks 36–44 instead of copying release history. Development owns native support baselines and installation behavior; task records retain run IDs, CI timings, checksum checks and supplied-media provenance. The working plan records this completed maintenance. Homebrew remains explicitly deferred; the independent-backend retest remains open.

Current contracts now identify the implemented seven-Case catalog across five families, its mode restrictions and typed arm fields. Journal documentation matches the writer's schema 6 and the reader's support for schemas 1–6. Obsolete storage-integration promises are replaced by the existing durable-commit/acceptance boundary. RPC status text distinguishes implemented profiles from broader compatibility goals. SDK instrumentation is marked deferred. Brand copy records the accepted yellow accent and preserves the remaining unverified naming/registry checks despite public publication.

All maintained documentation paths remain in place. Numbered ADR decision bodies, CONTEXT, research sources, candidate Case/Pack names and historical task evidence are unchanged. Scoped operational limits and protocol/evidence restrictions are preserved. All 412 local Markdown paths/anchors across 92 tracked Markdown files, including project skills, passed. The preservation check confirmed unchanged bodies for all 20 numbered ADRs, CONTEXT, tracked research and historical task evidence. Diff whitespace passed. Runtime, tests, installer, assets and release files did not change; no runtime checks or builds were repeated for this documentation-only task.
