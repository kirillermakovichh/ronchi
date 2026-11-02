---
name: rust-best-practices
description: Implement, review, or optimize Rust in Ronchi. Covers ownership, typed errors, Tokio task and resource management, clap/Axum implementation, rmcp integration, focused verification, and native binary releases.
license: MIT AND Apache-2.0
metadata:
  author: Ronchi contributors
  version: "2.0.0"
---

# Rust best practices for Ronchi

This is the project adaptation of the former Apollo-based skill, with selected async and tooling guidance.
Follow `AGENTS.md` and accepted ADRs before external examples.
Use the repository's declared Rust toolchain, dependency versions, and feature policy.
Do not infer an MSRV from a source skill's metadata.

## Load only the relevant reference

| Work | Reference |
| --- | --- |
| Ownership, borrowing, enums, traits, state types, errors, or API documentation | [language-and-errors.md](references/language-and-errors.md) |
| Tokio tasks, channels, locks, blocking work, cancellation, or shutdown | [async-runtime.md](references/async-runtime.md) |
| clap parsing, configuration, Axum control handlers, or process entrypoints | [cli-and-control-api.md](references/cli-and-control-api.md) |
| rmcp tools, protocol versions, schemas, transports, cancellation, or client compatibility | [mcp.md](references/mcp.md) |
| RPC allocations, resource use, tests, Clippy, or performance work | [performance-and-testing.md](references/performance-and-testing.md) |
| Native binary packaging, dependency checks, or release automation | [releases.md](references/releases.md) |

Read the smallest relevant set; do not preload every reference.
For CLI behavior and output contracts, use `ronchi-cli-design`.
For system ownership and lifecycle decisions, use `architecture-foundation`.
For SQLite engine behavior and Journal storage contracts, use the project [sqlite](../sqlite/SKILL.md) skill.
For Solana protocol behavior, select the relevant [shared RPC reference](../../references/solana-rpc/index.md).
For Case, simulator-history, or Finding correctness, use [ronchi-case-validation](../ronchi-case-validation/SKILL.md).
These skills complement this one; they are not required for a local Rust fix.

## Apply the project model

- Follow ADR-0019: Tokio, direct Hyper/hyper-util RPC path, Rustls, Axum control API, and a dedicated batched rusqlite writer.
- Keep work, buffers, queues, and retained bytes bounded.
- Stream unmodified large bodies instead of collecting them by default.
- Preserve typed failures where callers need to distinguish outcomes.
- Own background tasks and observe their completion and failures.
- Keep persistent product artifacts in the project-local workspace from ADR-0014.
- Treat examples as techniques to adapt, not permission to add frameworks or change the product.

## Verify proportionately

Run the repository's relevant checks for the changed contract.
For a defect, verify its trigger and resulting behavior through a meaningful interface.
Do not add tests that merely repeat implementation details.
Distinguish correctness evidence, resource-risk reasoning, and measured performance.
Concrete resource budgets and measurement work remain deferred until requested or needed for an executable implementation task.
Do not claim that a lint, pattern, or added skill proves lower CPU or memory use.

## Provenance

The previous nine handbook chapters are consolidated into the topic references above.
Source material is adapted and corrected for Ronchi; it is not an unmodified upstream distribution.
See [sources.md](references/sources.md) for reviewed revisions, the scope of each contribution, primary references, and preserved license texts.
