# Initial development queue

Status: implementation in progress. Completed slices and verification are recorded below.
The [product plan](plan.md) records decisions and risks; this file owns implementation progress.
The user delegated routine decisions in the [baseline brief](design-baseline-brief.md).
The [development stack](../docs/development.md) fixes the toolchain and core/dashboard technology choices.
No further design interview is required to start.

## Working rules

Implement one slice at a time through public behavior.
An early partial slice does not narrow the accepted format, method, mode, or Case scope.
Use the relevant project skills and [Definition of Done](../.agents/references/definition-of-done.md).
Run focused protocol, process, and persistence tests; avoid tests that only mirror implementation.
After a code slice, run `cargo fmt --check`, its focused `cargo test` target, and `cargo check`.
At checkpoints, run the accumulated suite and `cargo clippy --all-targets -- -D warnings`.
These commands become available after task 1; this documentation task has not run them.

Paths below are proposed module homes, not frozen interfaces.
Each task targets small or medium scope with about five substantive files at most.
Task 1 establishes shared build files; later slices reuse them.
If a slice exceeds one focused session, split its remaining work into adjacent subtasks here.
Checkpoints record results and do not require another approval interview.

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

- [ ] **Accept:** Implement configured read-only HTTP observation and public `check.observe`; commit validated facts with source, explicit scope, deadlines, and bounded polling.
- **Verify:** Timeouts, unavailable facts, schema errors, redirects, byte limits, cancellation, and stop; adapter failure remains an Evidence Gap.
- **Dependencies:** 8. **Scope:** Medium. **Likely files:** `src/checks.rs`, `src/checks/http.rs`, `src/checks/schema.rs`, `src/cli/check.rs`, `tests/check_observe.rs`.
- **Source:** [Checks and Verdicts](../docs/product/checks-and-verdicts.md).

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

- [ ] Own collection tasks and transport cancellation; stop polls on Ctrl-C, broken pipe, cancellation, and Lab stop while retaining terminal evidence.

#### 11e. Public histories and evidence integration

- [ ] Verify independent HTTP sources and the real relayer, source failures, budgets, commit failures, retained evidence, credential exclusion, and no Verdict; run locked checks and scoped review.

### 12. Deterministic Check evaluation

- [ ] **Accept:** Implement public `check.evaluate` over a pinned retained snapshot with bounded Verdicts, supporting facts, source authority, and gaps.
- **Verify:** Success, correlated identity violation, authoritative unresolved state at deadline, incomplete intervals, and conflicting facts; repeat without contacting the source.
- **Dependencies:** 9, 11. **Scope:** Medium. **Likely files:** `src/checks/evaluate.rs`, `src/checks/rules.rs`, `src/checks/verdict.rs`, `tests/check_evaluate.rs`.
- **Source:** [Verdict rules](../docs/product/checks-and-verdicts.md#verdict-rules).

### 13. Self-contained public-path Demo

- [ ] **Accept:** Orchestrate fresh Demo-owned Lab/relayer state and public RPC/Checks; return bounded results and retain the Lab unless `--stop-after-demo` is explicit.
- **Verify:** Both profiles offline, unsafe identity failure, evidence-backed resilient success, fresh names, JSON output, and owned-child cleanup.
- **Dependencies:** 10, 12. **Scope:** Medium. **Likely files:** `src/demo.rs`, `src/demo/fixture.rs`, `src/cli/demo.rs`, `tests/demo.rs`.
- **Sources:** [Demo workflow](../docs/product/workflows.md#first-proof-bundled-demo), [reference packaging](../docs/product/reference-application.md#packaging-and-configuration).

### Checkpoint C

- [ ] The Demo distinguishes profiles from observations without trusting their labels.
- [ ] An independently run backend yields a useful RPC-only Investigation with explicit missing business evidence.

## Human and agent access

### 14. Embedded dashboard

- [ ] **Accept:** Ship prebuilt header, summary, Timeline, and evidence views with authenticated public Case/Check controls and bounded drill-down.
- **Verify:** No traffic, no match, Finding selection, Evidence Gaps, stopped/interrupted views, and browser closure without Lab stop.
- **Dependencies:** 9, 12. **Scope:** Medium; split panels if needed. **Likely files:** `dashboard/`, `src/dashboard.rs`, `tests/dashboard_flow.rs`.
- **Source:** [First dashboard](../docs/product/investigation-interfaces.md#first-dashboard).

### 15. Local MCP adapter

- [ ] **Accept:** Expose shared operations with official `rmcp`, structured bounded results, bootstrap identity exceptions, and clean stdio.
- **Verify:** Scoped CLI/MCP parity, schema limits, cancellation, uncertain mutations, and EOF without Lab stop.
- **Dependencies:** 12. **Scope:** Medium. **Likely files:** `src/mcp.rs`, `src/mcp/tools.rs`, `tests/mcp_stdio.rs`.
- **Source:** [CLI and MCP behavior](../docs/product/investigation-interfaces.md#cli-and-mcp-transport-behavior).

## Complete modes and initial catalog

### 16. Explicit Read-through

- [ ] **Accept:** Forward only allowlisted reads to an explicit source; preserve source evidence, local transaction authority, and credential redaction.
- **Verify:** Fake upstream proves no transaction, airdrop, simulation, unknown method, or silent local-history fallback reaches it.
- **Dependencies:** 6, 8. **Scope:** Medium. **Likely files:** `src/upstream.rs`, `src/rpc/routing.rs`, `src/evidence/redact.rs`, `tests/read_through.rs`.
- **Source:** [Read-through allowlist](../docs/product/solana-rpc-contract.md#read-through-allowlist).

### 17. Managed native Surfpool

- [ ] **Accept:** Pin/install/verify an owned native backend under the workspace; route all blockchain RPC through it with clean state and no automatic funding.
- **Verify:** Install/readiness failure, backend death, missing state, actual local execution, owned cleanup, and disabled direct Read-through.
- **Dependencies:** 3, 7, 16. **Scope:** Medium; split tool installation if needed. **Likely files:** `src/backend.rs`, `src/backend/surfpool.rs`, `src/tools.rs`, `tests/surfpool_mode.rs`, `tests/backend_ownership.rs`.
- **Source:** [Lifecycle](../docs/product/lab-lifecycle.md), [mode authority](../docs/adr/0013-managed-surfpool-execution-mode.md).

### 18. Timeout bursts

- [ ] **Accept:** Implement pre-dispatch timeout/recovery in both modes and keep it distinct from post-acceptance response loss.
- **Verify:** Cycle boundaries, no dispatch, held batches, cancellation, Case capacity errors, and switching without invented Client Outcomes.
- **Dependencies:** 7, 9, 17. **Scope:** Small. **Likely files:** `src/cases/timeout.rs`, `src/findings/timeout.rs`, `tests/timeout_case.rs`.
- **Source:** [Timeout family](../docs/product/cases.md#rpc-unavailability-or-timeout).

### 19. Null and stale status views

- [ ] **Accept:** Implement per-identity masking/captured views without altering authoritative lifecycle; preserve mixed-signature order and commitment constraints.
- **Verify:** Source versus boundary evidence in both modes, unknown identities, missing snapshots, recovery, and replacement.
- **Dependencies:** 9, 17. **Scope:** Medium. **Likely files:** `src/cases/status.rs`, `src/findings/status.rs`, `tests/status_cases.rs`.
- **Source:** [Status family](../docs/product/cases.md#unavailable-or-stale-transaction-status).

### 20. Bounded confirmation stalls

- [ ] **Accept:** Assign simulator-only delayed progression and expose the actual Surfpool capability limit; existing schedules survive Case changes.
- **Verify:** Progress without polling, commitment-gated lookups, duplicates, disarm, recovery, and no retroactive expiry after processing.
- **Dependencies:** 6, 9. **Scope:** Small. **Likely files:** `src/cases/confirmation.rs`, `src/findings/confirmation.rs`, `tests/confirmation_case.rs`.
- **Source:** [Stalled confirmation](../docs/product/cases.md#stalled-confirmation).

### 21. Expired-blockhash reuse

- [ ] **Accept:** Reuse genuine tuples with same-authority expiry evidence and retain each mode's actual admission/processing result.
- **Verify:** Heights below/at/above the bound, already processed identities, durable nonces, no expired submission, switching, and recovery.
- **Dependencies:** 6, 9, 17. **Scope:** Small. **Likely files:** `src/cases/blockhash.rs`, `src/findings/blockhash.rs`, `tests/blockhash_case.rs`.
- **Source:** [Expired-blockhash family](../docs/product/cases.md#expired-recent-blockhash-submission).

### Checkpoint D

- [ ] Each initial family has tested Coverage Claims and explicit mode limitations.
- [ ] Full format coverage, source authority, and no-public-write behavior remain intact.

### 22. Declarative Custom Cases

- [ ] **Accept:** Validate bounded YAML v1 into an inspectable plan; activate versioned maintained primitives/templates atomically with custom provenance.
- **Verify:** Composite histories, validation categories, limits, digest stability, mode/version errors, batch binding, and lifetime response-loss memory across reactivation.
- **Dependencies:** 18–21. **Scope:** Medium; split parsing from runtime if needed. **Likely files:** `src/cases/custom.rs`, `src/cases/schema.rs`, `src/cases/sequence.rs`, `tests/custom_cases.rs`, `tests/fixtures/cases/`.
- **Source:** [Custom Cases](../docs/product/custom-cases.md).

## Recovery and prototype measurements

### 23. Retained evidence maintenance

- [ ] **Accept:** Expose explicit stopped-Lab deletion and bounded selected export; preserve originals on migration failure and distinguish export from replayable Chaincase.
- **Verify:** Reject live deletion; check interrupted reads, export limits/redaction, immutable references, WAL recovery, and workspace ownership.
- **Dependencies:** 8, 9. **Scope:** Medium. **Likely files:** `src/journal/maintenance.rs`, `src/export.rs`, `src/cli/artifacts.rs`, `tests/journal_maintenance.rs`.
- **Sources:** [Retention/export](../docs/product/evidence-and-findings.md#finalization-retention-and-export), [recovery](../docs/product/lab-lifecycle.md#recovery-retention-and-cleanup).

### 24. Recovery and resource measurements

- [ ] **Accept:** Verify whole-process fail-stop, reserved drain capacity, safe orphan handling, and independent backend survival; record reproducible measurements before revising provisional limits.
- **Verify:** Writer/disk/child failure and PID reuse; measure idle, representative RPC/Case load, retained history, dashboard, and agent queries.
- **Dependencies:** 13–23. **Scope:** Medium. **Likely files:** `tests/failure_recovery.rs`, `tests/resource_scenarios.rs`, `scripts/measure-resources.*`, a scoped measurement note under `docs/research/`.
- **Sources:** [Resource model](../docs/adr/0019-resource-efficient-rust-core.md), [open validation](../docs/open-questions.md).

### Checkpoint E

- [ ] Record verified capabilities and remaining gaps without claiming production fidelity or adoption.
- [ ] Validate one independent backend workflow and select the next improvement from its results.
- [ ] Preserve deferred CI wrappers, replay, SDK, distribution, and later transports in [open questions](../docs/open-questions.md).
