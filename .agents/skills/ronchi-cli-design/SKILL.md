---
name: ronchi-cli-design
description: Design or review Ronchi CLI behavior for humans, scripts, and coding agents. Use for command semantics, help, structured output, filtering, errors, cancellation, and persistent Lab operations.
license: MIT
---

# Ronchi CLI design

Treat the CLI as both a human interface and an automation contract.
Use `AGENTS.md`, the relevant product documents, and ADR-0006 for accepted behavior.
CLI examples in planning documents are proposals until implemented or explicitly agreed as contracts.

## Scope the change

Inspect the affected command, callers, help, output, and tests when available.
For a focused fix, assess the changed contract without redesigning unrelated commands.
For a new command, identify its user goal and whether it queries, mutates, starts a process, or follows a stream.
Preserve existing compatibility unless a change is intentional and documented.

## Shared principles

- Offer concise human output and an explicit stable machine format.
- Keep result data on stdout and diagnostics on stderr.
- In automation, avoid prompts, browser launches, terminal decoration, and implicit waiting for input.
- Return summaries first; fetch details only when selected.
- Bound large results with filtering, pagination, and field selection where applicable.
- Make incomplete output, evidence gaps, and continuation explicit.
- Give errors a stable machine identity and an actionable human explanation.
- Define retry behavior per operation; do not assume every command is idempotent.
- Keep CLI and MCP semantics aligned through shared core operations and query models.
- Allow CLI use without MCP, an SDK, registration, or cloud access.

## Load the relevant contract

| Work | Reference |
| --- | --- |
| JSON/NDJSON, errors, introspection, filters, pagination, or token use | [output-contracts.md](references/output-contracts.md) |
| Startup, readiness, follow commands, cancellation, retries, reset, or cleanup | [lifecycle-and-mutations.md](references/lifecycle-and-mutations.md) |

For Rust parser and transport implementation, use the relevant `rust-best-practices` reference.
For MCP protocol and rmcp integration, use the Rust [MCP reference](../rust-best-practices/references/mcp.md); shared output semantics remain here.
Do not load every reference for a narrow command change.

## Verify the observable behavior

Choose checks that can demonstrate the changed contract.
Use the real CLI process when prompts, stdout/stderr, exit status, signals, or TTY behavior matter.
Check agreement among parsing, validation, help, structured output, and schemas when those surfaces change.
Do not require every optional CLI feature or a numerical score before a command can ship.
Report what was checked and any material uncertainty.

## Provenance

Combines selected guidance from `cli-ux` and `agentic-cli-design`.
See [sources.md](references/sources.md) for reviewed revisions, adaptations, and MIT notices.
