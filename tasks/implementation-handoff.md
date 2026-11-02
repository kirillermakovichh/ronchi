# Implementation handoff

## Assignment

Implement Ronchi from the recorded baseline.
Start with task 1 in [todo.md](todo.md#1-native-executable-and-configuration).
Complete one small slice before starting the next one.
Continue through the queue in order while the session permits useful work.
Make local commits after verified slices.
Keep the repository local; do not add a remote, push, publish, or deploy.

The user delegated routine implementation decisions.
Choose small reversible details without another design interview.
Use prototype results to revise initial defaults when necessary.
Record a changed contract in its authoritative document.
Preserve the accepted scope and evidence limits.
Reply to the user in Russian; write maintained documentation and commit messages in English.

## Starting checkpoint

- Repository: `/Users/kirillermakovic/Downloads/PROJECTS/SELF/ronchi`.
- Product baseline commit: `3321832` (`chore: initialize Ronchi product baseline`).
- Local branch at preparation: `main`; no remote is configured.
- At preparation, no runtime code, Cargo package, or dashboard package exists.
- All 24 implementation tasks are unchecked at preparation.
- Rust 1.99.0, Cargo, rustfmt, Clippy, and a working C compiler/linker are installed on this host.
- A temporary edition-2024 program compiled and ran; the application itself has not been built or tested.

This is a starting snapshot, not a claim about a later session.
Inspect actual files, Git history, task status, and recorded verification before acting.
Do not reset the repository to the baseline commit.

## Read first

1. Read root [AGENTS.md](../AGENTS.md).
2. Read [docs/README.md](../docs/README.md) for document ownership.
3. Read the [product overview](../docs/product/overview.md).
4. Read the [development stack](../docs/development.md) and [ADR-0019](../docs/adr/0019-resource-efficient-rust-core.md).
5. Read the selected task and its dependencies in [todo.md](todo.md).
6. Read that task's linked source sections and relevant project skills.

Read [CONTEXT.md](../CONTEXT.md) when resolving domain terms.
Use [plan.md](plan.md) when a decision or dependency needs context.
Do not preload all ADRs, research, skills, or product documents.
Research is evidence and history; it does not override current contracts.

Start by running:

```bash
git status --short
git log -5 --oneline
git remote -v
rustc --version
cargo --version
```

If the working tree contains changes, inspect and preserve them.
Do not include unrelated changes in your commits.

## Selected stack

Use the exact toolchain in [rust-toolchain.toml](../rust-toolchain.toml).
Use Rust edition 2024 and one Cargo package with a library and the `ronchi` binary.
Use Tokio, direct Hyper/hyper-util RPC, Rustls, Axum control, SQLite WAL/rusqlite, clap, and official rmcp.
Use the supporting dependencies selected in the [development stack](../docs/development.md).
Add dependencies only when the current slice uses them.
Commit `Cargo.lock` when creating the package.

The dashboard uses React, strict TypeScript, Vite, npm, CSS, and SVG.
Create its package and lockfile at the dashboard slice.
The delivered executable serves embedded built assets and requires no Node runtime.
Do not set up the dashboard, Surfpool, Docker, or a validator to complete task 1.

## Repeat for each task

1. Select the next unchecked task whose dependencies are complete.
2. Read its acceptance criteria and linked contracts.
3. Inspect existing code and tests in the affected modules.
4. Write a short implementation checklist for this slice.
5. Implement the behavior through a small module interface.
6. Add meaningful checks for its public behavior and failure paths.
7. Run the verification commands below.
8. Update the task status and affected documentation.
9. Inspect and commit the complete slice locally.
10. Continue with the next task.

If a task is too large, split it into adjacent subtasks in `todo.md`.
Keep its remaining acceptance work visible and unchecked.
Do not create all future modules or placeholder services in advance.
Do not report success, readiness, a Finding, or a Verdict for unimplemented behavior.
If an early build exposes a command whose behavior is pending, return an explicit unsupported-capability error.
Keep planned schemas distinguishable from implemented operations.

Use project skills from `.agents/skills/`.
For task 1, read [rust-best-practices](../.agents/skills/rust-best-practices/SKILL.md) and its [CLI/control reference](../.agents/skills/rust-best-practices/references/cli-and-control-api.md).
Also read [ronchi-cli-design](../.agents/skills/ronchi-cli-design/SKILL.md) and its [output contract](../.agents/skills/ronchi-cli-design/references/output-contracts.md).
For later tasks, select skills from the root routing table.
Use `sqlite` for storage, `ronchi-case-validation` for protocol histories, and `architecture-foundation` for cross-module ownership changes.
Use `diagnosing-bugs` for a difficult failure rather than repeatedly making speculative edits.
Do not restart a product interview for a routine code choice.

## First slice: task 1

Create a normal Cargo package in the existing repository.
Keep package publication disabled while distribution remains deferred.
Use `src/lib.rs` for reusable application logic and `src/main.rs` for the process entry point.
Implement clap parsing, version/help, and the schema entry point described in the interface contract.
Derive exposed schemas from the same typed models used by their operations where practical.
Implement project discovery, configuration precedence, strict unknown-key validation, and configuration errors before side effects.
Document the supported flags, environment variables, and configuration keys that you introduce.
Follow the initial version-1 JSON envelope and exit-code contract for operation errors.
Keep diagnostics on stderr.

Verify explicit and nested project paths, configuration precedence, invalid keys, conflicting modes, and machine-readable errors.
Exercise actual CLI processes where stdout, exit status, or current directory matters.
Use temporary projects so tests do not write runtime state into this repository.
Task 1 does not claim to start a working Lab; Journal and lifecycle implementation follow in tasks 2 and 3.

## Product boundaries to protect

- The independently run backend owns its lifecycle; Ronchi owns only its Lab and verified child processes.
- Use only local development, CI, or isolated staging; never put Ronchi in a production application RPC path.
- Keep all persistent product state under the selected project's `ronchi/` directory.
- Simulator mode does not execute programs, change balances, broadcast transactions, or fund signers.
- Simulator submission requires `skipPreflight: true`; full preflight and execution use optional Surfpool.
- Keep local lifecycle lookup local; forward only explicitly allowlisted reads to a configured Read Upstream.
- In Surfpool mode, send all blockchain reads and writes through Surfpool; disable direct Read-through.
- Keep one continuing Case and immutable in-flight policy binding, including whole-batch registration.
- Preserve once-per-identity response loss for the Lab lifetime, the 20-second real-time hold, and independent 2/4/6-second simulated progression.
- Preserve full accepted legacy/v0/v1, encoding, lookup-table, and durable-nonce scope; a library gap remains unfinished work.
- Distinguish RPC Boundary Result, Client Outcome, and business state; different signatures alone do not prove a shared intent or two transfers.
- Commit required acceptance evidence before publishing acceptance; storage failure stops collection explicitly instead of silently losing evidence.
- Keep queues, payloads, queries, and retained bytes bounded; provisional limits are not measured performance results.

Use each task's authoritative source for detailed semantics.
Do not infer runtime guarantees from this compact checklist.

## Verification before a code commit

Run these commands after the Cargo package exists:

```bash
cargo fmt --check
cargo check --locked
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

Generate or update `Cargo.lock` deliberately before using `--locked` after dependency changes.
During development, use the relevant focused test target first.
For the initial small package, run the whole suite before each code commit.
If the suite later becomes expensive, use task-relevant tests per slice and the accumulated suite at the queue checkpoints.
Run the slice's process, protocol, persistence, or browser checks in addition to compilation.
For task 1, also run `cargo run --locked -- --help` and `cargo run --locked -- --version`.
Use actual temporary state for persistence/recovery checks; compilation does not establish durability.

Record exact commands and outcomes in a short note under the completed task.
Mark a task complete only after its acceptance criteria and applicable checks pass.
If a required check is blocked, record the cause and keep the affected task incomplete.
Do not weaken a test, change a required contract, or claim an unrun check passed to obtain a green result.

## Local commit policy

Local implementation commits are part of this assignment.
Use the existing branch unless the user requests another branch or actual concurrent work needs isolation.
Create one commit per verified behavior slice.
Keep its implementation, relevant tests, task status, and documentation together.
Split a large task into working substeps instead of saving the whole project for one final commit.
Do not commit intentionally failing intermediate test states.

Before committing, inspect `git diff` and `git status --short`.
Stage only the intended paths or hunks.
Inspect `git diff --cached` and run `git diff --cached --check`.
Keep secrets, `.env` files, local Lab state, build output, and dependency directories out of Git.
Keep source fixtures, required third-party notices, and lockfiles in Git.
Do not modify unrelated imported skills or licenses during implementation.

Use concise English commit subjects, for example:

```text
feat(cli): add project configuration and structured errors
feat(journal): persist bounded Lab evidence
feat(lab): add persistent startup and shutdown
fix(rpc): preserve identity across repeated submissions
```

After committing, run `git status --short` and `git log -1 --oneline`.
Preserve pre-existing unrelated changes if the tree cannot be clean.
Do not amend existing commits, rewrite history, or discard changes unless explicitly requested.
If the sandbox blocks `.git` writes, use the tool's permission mechanism.
Do not work around that restriction by editing Git metadata manually.
Do not add remotes, push, create remote PRs, publish packages, or deploy.
Queue checkpoints do not require another approval interview.

## Session exit and restart

At a completed slice boundary, leave the next unchecked task identifiable in `todo.md`.
Record verification outcomes with that slice before its commit.
If the session must end during a slice, keep its task unchecked.
Record the partial files, remaining work, latest checks, and any real blocker in `tasks/implementation-progress.md`.
Preserve uncommitted work; do not label it a completed slice.
On restart, read that progress note if it exists and compare it with actual Git state.

Report briefly in Russian: completed tasks, observed checks, commit hashes, remaining limitations, and the next task.
Do not claim that a completed prototype proves production fidelity or product adoption.

Begin with task 1 now; do not spend the session replanning the whole queue.
