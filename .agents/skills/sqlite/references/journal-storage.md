# Ronchi Journal storage

Use ADR-0014, ADR-0016, and ADR-0019 as the source of accepted product constraints.
These instructions do not select a Journal schema, retention policy, durability level, or numerical resource budget.

## Accepted model

The Continuous Lab Journal uses project-local SQLite WAL through rusqlite.
One dedicated writer batches writes.
The complete Journal history is not retained in memory.
Other interfaces query the same evidence through bounded views.
Use the selected project's `ronchi/` workspace for persistent product files.
Do not introduce a persistent global cache or a separate database service.

## Evidence and writes

Distinguish event observation, queue admission, transaction commit, and durability promises under the chosen failure model.
Do not report queued evidence as already committed.
State what remains observable after writer failure or interrupted shutdown.
Do not invent replay guarantees, delivery semantics, or acceptable evidence loss while they remain undecided.
Use explicit stable identifiers for correlation where required by the data contract.
Storage order and timestamps alone do not prove application causality.

Keep batches bounded by relevant count, byte, and waiting-time constraints when those are defined.
Do not improve write throughput by allowing unbounded pending evidence.
Coordinate writer failure, producer admission, queue draining, and final commit with the runtime owner.
Use the Rust async reference for the task and shutdown implementation.

## Concurrent readers and queries

Apply filtering, ordering, pagination, and field projection in the query layer.
Do not retrieve every payload before discarding unrequested fields in a CLI or MCP renderer.
Index real query patterns rather than every potentially searchable field.
Use `EXPLAIN QUERY PLAN` and representative data when evaluating a query or index change.
Account for the extra writes and storage introduced by indexes.
Keep dashboard queries and exports from retaining unnecessary read snapshots.
Define cursor behavior while new events arrive instead of assuming a static dataset.

## Disk growth and recovery

Consider database pages, WAL, temporary query files, backups, and migration headroom.
Deleting rows does not necessarily shrink the database file immediately.
Choose retention, checkpointing, and reclamation behavior through the existing decision process when needed.
Do not disable automatic checkpoints without an alternative that controls WAL growth.
Do not use integrity checks as a substitute for verifying required evidence and correlation data.

## Relevant verification

For changed behavior, select checks such as concurrent reads during writes, rollback, reopen, writer failure, migration failure, or restored-backup queries.
Use temporary file-backed workspaces and the application's linked SQLite library.
Verify the acknowledged-write contract rather than treating every uncommitted event as promised durable data.
Avoid destructive checks against an active developer Lab.
Resource measurements remain deferred unless the current implementation task requires them.
