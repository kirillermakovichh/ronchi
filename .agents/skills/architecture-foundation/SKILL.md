---
name: architecture-foundation
description: Design or review Ronchi runtime architecture, state ownership, lifecycle, and cross-module contracts. Use for substantial architecture decisions; use codebase-design for local interface design within established contracts.
license: MIT
---

# Architecture foundation

Design only the architecture needed by the requested behavior.
Read `AGENTS.md` and use `docs/README.md` to locate relevant accepted decisions.
Treat source skills and reference projects as design evidence, not product requirements.

## Establish the current design

- Inspect affected entrypoints, modules, dependencies, configuration, and tests when they exist.
- Distinguish accepted intent, implemented behavior, proposals, and unresolved choices.
- If implementation is absent, describe the proposed contracts without claiming code verifies them.
- Reuse existing decisions and documentation locations before adding another specification.

## Define the affected contracts

For each relevant module or shared resource, establish:

| Contract | Questions |
| --- | --- |
| Ownership | Who owns state, mutation rights, handles, and cleanup? |
| Lifecycle | What starts it, makes it ready, stops it, and waits for completion? |
| Effects | Where can network, process, filesystem, and database operations occur? |
| Capacity | What bounds work and retained data? What happens at capacity? |
| Errors | Which failures are recoverable, fatal, user-visible, or evidence gaps? |
| Evidence | What is observed, durably recorded, correlated, and exposed for analysis? |
| Verification | Which public interface can demonstrate the changed behavior? |

Use [runtime-contracts.md](references/runtime-contracts.md) when the decision affects the RPC path, Lab lifecycle, Journal, or Surfpool process.
Use `codebase-design` when selecting a module's interface.
Read only the material relevant to the change.

## Choose proportionate structure

- Keep decisions and invariants separate from concrete transport, storage, and process handling where that improves reasoning or testing.
- Do not require a new layer, trait, crate, message hop, or allocation for every conceptual distinction.
- Introduce crate boundaries for demonstrated API, dependency, reuse, or independent testing needs.
- Give shared state an explicit owner and synchronization contract.
- Make queues, cancellation, failure propagation, and shutdown part of the design when they affect behavior.
- If contracts are duplicated, identify the authoritative one before adding another abstraction.
- Borrow a mature project's relevant contract, not its scale or historical machinery.
- Preserve the accepted architecture unless the task or evidence warrants reconsidering it.

## Record the result

Explain the chosen design, relevant alternatives, trade-offs, verification, and open questions.
Scale the explanation to the decision; a local change does not need a complete architecture report.
Use the existing ADR workflow for durable decisions.
Use the existing planning skill only when implementation planning is requested.
Do not duplicate product documents, generate a roadmap automatically, or resolve deferred resource budgets.

## Provenance

Adapted for Ronchi from `architecture-foundation`.
See [sources.md](references/sources.md) for the reviewed revision, scope of adaptation, and license.
