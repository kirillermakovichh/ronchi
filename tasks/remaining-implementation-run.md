# Autonomous remaining implementation run

## Objective and authority

Complete the authorized implementation queue from task 15 through task 24 in [todo.md](todo.md).
Use parallel subagents for independent work when the selected runtime provides them.
Continue through implementation, integration, verification, and local commits without a new user handoff after each task.
Preparation of this document did not start implementation.

The user delegated reversible implementation decisions and authorized local commits and parallel work.
Choose routine details without a design interview.
Preserve the accepted product scope and resource/evidence contracts.
Keep all work local; do not add remotes, push, publish, or deploy.
Use ordinary permission mechanisms when a tool needs filesystem, socket, or network access.
Do not bypass a rejected action or change global approval policy.

Report to the user in Russian.
Write maintained docs and commit messages in English.

## Start and source selection

1. Read root [AGENTS.md](../AGENTS.md).
2. Read [implementation-progress.md](implementation-progress.md) for the completed task-14 baseline.
3. Inspect actual Git status, current branch, remotes, and recent history.
4. Read tasks 15–24 and checkpoints C–E in [todo.md](todo.md).
5. Read the [development stack and build instructions](../docs/development.md).
6. Use [docs/README.md](../docs/README.md) to select each task's authoritative topic.
7. Resolve domain terms through [CONTEXT.md](../CONTEXT.md).

Do not restart from the historical task-1 handoff.
Do not repeat the whole product-design interview or preload every reference.
Read affected source and tests when starting their implementation slice.
Preserve pre-existing user changes.
Keep [parallel-run-state.md](parallel-run-state.md) as the compact dispatch and integration ledger.

## Runtime capability check

Inspect the actual agent tools before spawning work.
Zed's native agent and an external ACP agent can expose different delegation and filesystem capabilities.
Do not assume the current host's tools, concurrency limit, or configuration also apply to the next session.

Use one coordinator and up to three active implementation workers.
This is a Ronchi resource policy, not a statement about a Zed product limit.
Use fewer workers if the runtime or resource pressure requires it.
Workers must not recursively spawn more workers.

If subagents are absent, finish the same queue sequentially in the coordinator.
Record that delegation was unavailable; do not pretend concurrent work happened.
Do not start an unverified nested CLI harness merely to imitate a missing tool.
If writing inside an isolated checkout is unavailable, use subagents for read-only investigation/review and keep one implementation writer.
Continue independent authorized work when one capability is blocked.

## Roles and isolated checkouts

The coordinator owns task status, scheduling, shared contracts, integration, and final acceptance.
Only the coordinator edits the shared task ledger and completion checkboxes.
Workers implement and verify the packet they receive.

Create worker Git worktrees only when dispatching a task.
Use a unique run identifier and branches such as `run/RUN_ID/task-15`.
Put disposable development checkouts under the ignored root `.worktrees/` directory when the runtime can access them.
Use actual current integration HEAD as the recorded base.
Check for existing paths and branches; preserve them instead of overwriting them.

Spawning a subagent does not itself prove filesystem isolation.
Give every worker its exact absolute checkout path and allowed paths.
Require it to confirm its working directory and branch before mutation.
Each terminal invocation must select that checkout explicitly; shell working directories need not persist between tool calls.
Workers must not edit the coordinator checkout or another worker's checkout.

Keep separate native target directories, frontend build output, temporary projects, and process resources per checkout.
Do not share mutable `target/` binaries across branches.
Tests must exercise the binary built from the relevant checkout.
Prepare the embedded frontend assets before Cargo builds in each checkout.
Use [development.md](../docs/development.md#dashboard-build-and-launch) for the current build procedure.

Common integration files include `Cargo.toml`, `Cargo.lock`, `src/lib.rs`, `src/main.rs`, `src/config.rs`, `src/control.rs`, `src/lab.rs`, `src/rpc.rs`, `src/cases.rs`, `src/query.rs`, and `src/journal.rs`.
The coordinator owns their final combined state.
A worker may make minimal wiring/dependency changes in its own branch when necessary to compile its slice.
It must identify those hunks in its report.
Reconcile shared changes deliberately during integration; do not discard another task's dependency or branch to resolve a conflict.

## Dependency waves

Use dependencies, not a requirement to keep every worker slot occupied.
Dispatch a newly unblocked task as soon as its required contracts are integrated.

| Wave | Work that can run independently | Integration condition |
| --- | --- | --- |
| 1 | 15 MCP; 16 Read-through; 23 retained evidence maintenance | Existing tasks 1–14 are the base |
| 2 | 17 managed Surfpool; 20 simulator confirmation stalls | Integrate 16 before 17; 20 does not require Surfpool |
| 3 | 18 timeout bursts; 19 null/stale status; 21 expired-blockhash reuse | Integrate and verify 17; establish common Case hooks first |
| 4 | 22 Custom Cases | Integrate the maintained primitives from 18–21; check Checkpoint D |
| 5 | 24 failure recovery and resource measurements; accumulated acceptance | Integrate 15–23 before measuring the combined prototype |

Before parallel Case changes, settle the smallest shared policy/evidence/backend interfaces needed by those workers.
Use the existing continuing Case and batch contracts.
Avoid building a speculative framework for deferred transports.
Read-only review or documentation preparation can overlap other work when its scope is already stable.

## Task packets and skills

Every worker reads its selected project `SKILL.md` files before applying them.
Load only their references needed by that task.
The following source links supplement the exact acceptance criteria in [todo.md](todo.md).

| Task | Main scope and authoritative sources | Project skills |
| --- | --- | --- |
| 15 | Official rmcp stdio adapter over shared operations; [interfaces](../docs/product/investigation-interfaces.md), [Rust MCP reference](../.agents/skills/rust-best-practices/references/mcp.md). Verify actual SDK framing, discovery, schemas, cancellation, stdout, and EOF without Lab stop. | `rust-best-practices`, `ronchi-cli-design`, `codebase-design` |
| 16 | Allowlisted reads and final-boundary write firewall; [RPC contract](../docs/product/solana-rpc-contract.md#read-through-allowlist), ADR-0012. Preserve source provenance and local lookup authority; test redirects/credentials/unknown methods and writes with fake upstreams. | `rust-best-practices`, `architecture-foundation`, `ronchi-case-validation` |
| 17 | Pinned native tool, checksum, owned process, readiness, exclusive state authority, and cleanup; [Lab lifecycle](../docs/product/lab-lifecycle.md), [ADR-0013](../docs/adr/0013-managed-surfpool-execution-mode.md). Verify actual local execution with explicitly prepared fixtures. | `architecture-foundation`, `rust-best-practices`, `ronchi-case-validation` |
| 18 | Pre-dispatch timeout/recovery in both modes; [timeout family](../docs/product/cases.md#rpc-unavailability-or-timeout). Keep it distinct from accepted transaction response loss. | `ronchi-case-validation`, `rust-best-practices`, `codebase-design` |
| 19 | Per-identity null/stale views from actual source history; [status family](../docs/product/cases.md#unavailable-or-stale-transaction-status). Preserve authoritative progression, mixed-signature order, and query constraints. | `ronchi-case-validation`, `rust-best-practices`, `codebase-design` |
| 20 | Bounded simulator lifecycle delays; [confirmation family](../docs/product/cases.md#stalled-confirmation). Preserve assigned schedules across Case changes and report unsupported Surfpool capability. | `ronchi-case-validation`, `rust-best-practices` |
| 21 | Genuine cached blockhash tuple and same-authority expiry; [blockhash family](../docs/product/cases.md#expired-recent-blockhash-submission). Preserve mode-specific admission and processing; exclude nonce lifetimes. | `ronchi-case-validation`, `rust-best-practices` |
| 22 | Bounded YAML validation, versioned primitives/templates, atomic activation, and custom provenance; [Custom Cases](../docs/product/custom-cases.md). Preserve the Lab-lifetime response-loss ledger. | `ronchi-case-validation`, `codebase-design`, `rust-best-practices` |
| 23 | Explicit stopped-Lab deletion and bounded selected export; [retention](../docs/product/evidence-and-findings.md#finalization-retention-and-export), [recovery](../docs/product/lab-lifecycle.md#recovery-retention-and-cleanup). Preserve originals on migration failure and keep export distinct from replay. | `sqlite`, `rust-best-practices`, `ronchi-cli-design`, `architecture-foundation` |
| 24 | Whole-process failure recovery and reproducible resource evidence; [ADR-0019](../docs/adr/0019-resource-efficient-rust-core.md). Verify ownership, drain reserve, storage failure, and independent backend survival. | `diagnosing-bugs`, `architecture-foundation`, `sqlite`, `rust-best-practices`, `ronchi-case-validation` |

All implementation packets use `tdd` for meaningful public behavior checks where applicable.
Use `ronchi-code-review` for scoped review before integration.
Use `ronchi-docs-maintenance` when a maintained contract or reading route changes.
Use `context-engineering` for compact restartable state.
Use `domain-modeling` only for an actual vocabulary change or an ADR-worthy decision.
The user's delegation supplies approval for routine plans and interface refinements; do not reopen the TDD/design approval interview.

## Dispatch and return format

Each packet must state:

1. Task/subslice and the concrete behavior to implement.
2. Exact checkout, branch, and base commit.
3. Owned module paths and allowed common-file wiring.
4. Contract links, selected skills, and dependency interfaces.
5. Acceptance histories and exact focused checks.
6. Current product limits and the task's excluded scope.
7. Required return data: commits, paths, shared hunks, command outcomes, remaining gaps, and owned resources still running.

A worker can split a large packet into working substeps.
It must not change another worker's contract silently.
Return a specific missing interface or real blocker to the coordinator and continue independent work when possible.
Do not ask the user to choose routine Rust types, module names, flags, or reversible defaults.

## Integration loop

1. Inspect the worker report and actual branch diff.
2. Check the reported public behavior and relevant failure path.
3. Reconcile shared modules, dependency features, lockfiles, and schemas.
4. Integrate the verified commits locally in the coordinator checkout.
5. Run focused integration checks on the combined code.
6. Update authoritative topic docs, the assignment ledger, and task status.
7. Dispatch newly unblocked work and continue.

Use explicit shared contracts when an early module change is required to unblock workers.
Report changing interfaces to affected workers and update their base deliberately.
Do not merge an implementation merely because a worker says it is complete.
Do not mark a task complete from its isolated branch before its required integration checks pass.
Preserve actual worker evidence; do not describe a worker's unrun check as a coordinator check.

Keep CLI, dashboard, MCP, schemas, Case catalog, and capability errors consistent as new core behavior becomes available.
Retained Journals and previous Case evidence must remain readable under documented compatibility.
Do not broaden network effects or silently drop accepted format coverage during integration.

## Verification and resource scheduling

Workers run formatting, compilation, scoped Clippy, and task-relevant public tests on their checkout.
Run the focused failing behavior first during development.
The existing task-14 baseline is recorded; preparation did not rerun it.

Only one accumulated native/browser suite should occupy the machine at a time.
Focused independent checks may run concurrently when they do not share binaries, ports, fixtures, or resources.
Coordinate expensive cold builds instead of launching all test matrices repeatedly.
Do not treat passing retries as a replacement for a failed result.

At integration checkpoints and final acceptance, run on the combined checkout:

```bash
npm --prefix dashboard ci
npm --prefix dashboard run typecheck
npm --prefix dashboard test -- --run
npm --prefix dashboard run build
cargo fmt --check
cargo check --locked
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
npm --prefix dashboard run test:browser
cargo build --release --locked
```

Build the native executable first if the browser suite requires a freshly built debug binary.
Update lockfiles deliberately before `--locked` checks when dependencies change.
Verify actual MCP SDK transport, fake-upstream firewall behavior, managed Surfpool execution, new Cases, Custom Case histories, and maintenance paths in addition to generic compilation.
Use existing Chromium unless there is a demonstrated setup mismatch.

Run resource measurements after compilation and competing agent tests stop.
Record host, versions, build mode, workload, relevant limits, and commands.
Measure core and Surfpool separately.
Do not kill unrelated user processes to make the host look idle.
Report uncontrolled host load and untested platforms honestly.
Operational defaults are not benchmark results or pre-existing performance release gates.

## Boundaries and incomplete work

- Preserve local/CI/isolated-staging scope and the upstream write firewall.
- Surfpool owns all blockchain RPC in its mode; direct Read-through remains disabled there.
- Ronchi never automatically funds signers or calls faucets; execution tests prepare explicit isolated fixtures.
- Preserve one continuing Case, immutable request/batch binding, durable acceptance, and once-per-identity response-loss memory.
- Preserve current collector HTTP-only/32 KiB limits and single-intent evaluation budgets unless a separately recorded, tested change is necessary for the authorized task.
- Findings and Verdicts retain their source authority and Evidence Gaps.
- Arbitrary executable plugins, production routing, replayable Chaincase, SDK, and public distribution remain outside this run.
- Delete only disposable test state in explicit temporary test workspaces during verification; do not clean the user's existing Labs to test deletion.

If a tool download, real backend, permission, or external prerequisite blocks a slice, record the exact blocker and what was verified.
Continue other unblocked tasks.
Mock-only Surfpool tests do not establish actual local execution.
Do not mark dependent mode coverage verified when it remains untested.

The [normal-down observation](unconfirmed-down-observation.md) remains unconfirmed.
If it recurs, preserve the original envelope and operation sequence before owned cleanup.
Use the debugging skill on the actual trigger; do not introduce speculative fixes or hidden retries.

Checkpoint C and Checkpoint E's external-backend validation require a real suitable application.
Do not claim the reference relayer is third-party product validation.
Keep those items open if the needed application is unavailable; do not search or modify unrelated repositories just to close a checkbox.

## Commits, continuation, and finish

Commit verified working slices with concise English subjects and relevant tests/docs.
Workers commit only their packet changes on their assigned branch.
The coordinator may integrate them with local cherry-picks or merges and resolve conflicts.
The user authorized this local integration; no separate approval is needed after each slice.
Do not rewrite pre-existing history or discard unrelated changes.
Inspect staged changes and run `git diff --cached --check` before each commit.

Keep worktrees until their work is integrated or explicitly recorded as unfinished.
Remove only this run's clean, integrated development checkouts when cleanup is useful.
Do not force-remove dirty worker checkouts to obtain a clean report.

Persist state before context compaction or session exit.
Record exact active checkout paths, uncommitted files, integration HEAD, checks, and the next action.
Resume from actual Git/task state after compaction; do not restart completed work.
If the host forces a new session, the same runbook and ledger provide the restart point.
A prompt cannot override an agent runtime's hard session or permission limits.

Do not stop merely because one task or wave is complete.
Finish when all authorized implementation work is verified, or only concrete external blockers remain after independent work is exhausted.
Leave unavailable acceptance items unchecked.
The final Russian report must list completed tasks, verified integrated commits, actual checks, measurements, open blockers, retained resources, and Git state.
