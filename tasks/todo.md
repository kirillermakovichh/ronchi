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
- **Boundary:** `up` validates without writes and returns exit `4` (`unsupported_capability`). Lab startup, runtime limits, and successful operation schemas remain assigned to later slices.

### 2. Durable Journal admission

- [ ] **Accept:** Commit versioned events through bounded SQLite WAL/FULL storage; expose a contiguous watermark and explicit Journal states.
- **Verify:** File-backed reopen after process death, atomic acceptance records, queue saturation, and storage failure without silent evidence loss.
- **Dependencies:** 1. **Scope:** Medium. **Likely files:** `src/journal.rs`, `src/journal/writer.rs`, `src/journal/schema.rs`, `src/evidence.rs`, `tests/journal.rs`.
- **Source:** [Evidence and Findings](../docs/product/evidence-and-findings.md).

#### 2a. Atomic file-backed Journal storage

- [x] **Accept:** Create a version-1 SQLite WAL/FULL Journal, commit bounded event groups and their contiguous watermark atomically, and retain explicit terminal state.
- **Verify:** `cargo test --locked --test journal -- --nocapture` passed (4 behavior tests and one subprocess fixture). A killed writer retained both acknowledged events. An injected SQLite failure rolled back the whole unit and watermark. Finalized Journals reject new events and reinitialization.
- **Checks:** `cargo fmt --check`, `cargo check --locked`, `cargo test --locked` (11 tests including the subprocess fixture), and `cargo clippy --locked --all-targets -- -D warnings` passed.
- **Storage:** Bundled SQLite `3.54.1`, rusqlite `0.41.1`; WAL mode and writer `synchronous=FULL` are read back. Individual encoded events are capped at 64 KiB; atomic groups are capped at 256 events and 1 MiB.
- **Boundary:** This storage primitive accepts caller-validated/redacted facts. RPC event models, retained signed bytes, and transaction acceptance integration remain assigned to their owning slices.

#### 2b. Bounded dedicated writer and storage admission

- [ ] **Accept:** Add the dedicated writer, bounded event/byte queue, bounded batching, explicit failure health, and disk admission/drain reserve. Preserve the committed-watermark contract.
- **Verify:** Queue saturation, writer failure, byte bounds, and recovery through actual file-backed storage. Include atomic required-content admission when transaction models arrive in task 5.
- **Dependencies:** 2a. This subtask completes task 2 before task 3 starts.

### 3. Persistent Lab controls

- [ ] **Accept:** Implement authenticated loopback control, background readiness, immutable IDs, name resolution, and bounded stop; finalize only after owned work/resources settle.
- **Verify:** Concurrent starts, occupied/dynamic ports, cancellation around readiness, terminal closure, force stop, and historical lookup through actual processes.
- **Dependencies:** 1, 2. **Scope:** Medium. **Likely files:** `src/lab.rs`, `src/control.rs`, `src/workspace.rs`, `src/process.rs`, `tests/lab_lifecycle.rs`.
- **Source:** [Lifecycle](../docs/product/lab-lifecycle.md).

### Checkpoint A

- [ ] CLI exit leaves a ready Lab alive; explicit stop retains readable evidence and creates no global persistent files.
- [ ] Record accumulated build, process, persistence, and failure-check results.

## First RPC Investigation

### 4. Local clock and bootstrap reads

- [ ] **Accept:** Expose bounded HTTP JSON-RPC for bootstrap, slots/heights, blockhashes, and validity; advance the coherent clock without polling.
- **Verify:** Parameter errors, commitment views, `minContextSlot`, clock boundaries, and rejected unknown/write forwarding.
- **Dependencies:** 3. **Scope:** Medium. **Likely files:** `src/rpc.rs`, `src/rpc/methods.rs`, `src/simulator.rs`, `src/simulator/clock.rs`, `tests/rpc_clock.rs`.
- **Source:** [RPC contract](../docs/product/solana-rpc-contract.md).

### 5. Submission identity and lifecycle lookup

- [ ] **Accept:** Commit signed identity before acceptance; derive status/transaction lookup from one lifecycle and preserve duplicate history, processing, and expiry distinctions.
- **Verify:** Independent 0/2/4/6-second progression, duplicate races, expired unprocessed submissions, retained history, and preflight capability errors.
- **Dependencies:** 2, 4. **Scope:** Medium. **Likely files:** `src/simulator/transactions.rs`, `src/rpc/submission.rs`, `src/rpc/lookup.rs`, `src/journal/transactions.rs`, `tests/transaction_history.rs`.
- **Source:** [RPC submission/history](../docs/product/solana-rpc-contract.md).

### 6. Full format and lifetime coverage

- [ ] **Accept:** Cover legacy/v0/v1, applicable encodings and `jsonParsed`, version negotiation, lookup tables, fees, and durable nonces; expose missing required state explicitly.
- **Verify:** Independent signed fixtures, pinned official-reference matrix, identity-preserving round trips, client version limits, and recent-hash versus nonce validity.
- **Dependencies:** 5. **Scope:** Medium; split by format if needed. **Likely files:** `src/protocol/formats.rs`, `src/protocol/lifetimes.rs`, `src/rpc/fees.rs`, `tests/format_matrix.rs`, `tests/fixtures/`.
- **Source:** [Formats and encodings](../docs/product/solana-rpc-contract.md#transaction-formats-and-encodings).

### 7. Continuing Case and response loss

- [ ] **Accept:** Bind policy in shared registration order, including batches; apply durable once-per-identity response loss with the 20-second hold.
- **Verify:** Actual HTTP silence/termination, duplicates, cancellation, batch effects, disarm/reactivation, and Case replacement while lifecycle lookup continues.
- **Dependencies:** 5. **Scope:** Medium. **Likely files:** `src/cases.rs`, `src/cases/response_loss.rs`, `src/rpc/batch.rs`, `tests/response_loss.rs`, `tests/case_binding.rs`.
- **Source:** [Cases and coverage](../docs/product/cases.md).

### Checkpoint B

- [ ] A real HTTP client submits, loses the response, and queries the original signature without restarting the Lab.
- [ ] Record format gaps explicitly; the accepted full matrix remains required for a full support claim.

### 8. Bounded Investigation queries

- [ ] **Accept:** Expose summary, Timeline, and Evidence Slice through CLI/control API with storage-level filtering/projection, snapshot pagination, byte caps, and completeness metadata.
- **Verify:** Paging during appends, empty/no-match results, invalid cursors, payload previews, pipe closure, and retained links after stop.
- **Dependencies:** 7. **Scope:** Medium. **Likely files:** `src/query.rs`, `src/journal/query.rs`, `src/api.rs`, `src/cli/investigate.rs`, `tests/investigation.rs`.
- **Source:** [Investigation interfaces](../docs/product/investigation-interfaces.md).

### 9. Evidence-backed Findings

- [ ] **Accept:** Produce stable versioned Findings with supporting references, separate guidance, and explicit Evidence Gaps.
- **Verify:** Identical snapshot re-evaluation and required-fact removal; different signatures must never imply a shared intent or duplicate economic execution.
- **Dependencies:** 8. **Scope:** Medium. **Likely files:** `src/findings.rs`, `src/findings/rules.rs`, `src/query/summary.rs`, `tests/findings.rs`.
- **Source:** [Deterministic Findings](../docs/product/evidence-and-findings.md#deterministic-findings).

## Application evidence and Demo

### 10. Durable reference relayer

- [ ] **Accept:** Run the bundled child mode with idempotent intent intake, durable signed identity, public state/observation endpoints, and bounded unsafe/resilient policies.
- **Verify:** Crash before send, after attempt-start, after acceptance, and before result commit; reopen storage and verify preserved original identity and source facts.
- **Dependencies:** 5, 7. **Scope:** Medium. **Likely files:** `src/reference.rs`, `src/reference/store.rs`, `src/reference/sender.rs`, `src/reference/http.rs`, `tests/reference_recovery.rs`.
- **Source:** [Reference Application](../docs/product/reference-application.md).

### 11. Public Check collection

- [ ] **Accept:** Implement configured read-only HTTP observation and public `check.observe`; commit validated facts with source, explicit scope, deadlines, and bounded polling.
- **Verify:** Timeouts, unavailable facts, schema errors, redirects, byte limits, cancellation, and stop; adapter failure remains an Evidence Gap.
- **Dependencies:** 8. **Scope:** Medium. **Likely files:** `src/checks.rs`, `src/checks/http.rs`, `src/checks/schema.rs`, `src/cli/check.rs`, `tests/check_observe.rs`.
- **Source:** [Checks and Verdicts](../docs/product/checks-and-verdicts.md).

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
