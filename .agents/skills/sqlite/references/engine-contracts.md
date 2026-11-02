# SQLite engine contracts

Adapted from Bateau's SQLite skill for the project's embedded storage model.
See [sources.md](sources.md) for provenance and primary documentation.

## Schema and connections

Declare stable keys, nullability, uniqueness, checks, and foreign keys according to the accepted data model.
Ordinary SQLite type affinity does not enforce every declared type strictly.
Document representations for timestamps, identifiers, booleans, and exact numeric values when introduced.
Use version-compatible `STRICT` tables when their guarantees suit the schema.
Do not assume a non-integer primary key in an ordinary table implicitly forbids NULL.
Choose `AUTOINCREMENT` only when its committed-ID non-reuse semantics are required.
Do not use a row identifier as a gapless sequence or causal clock without a separate contract.

Initialize consequential connection settings on each physical connection that needs them.
Distinguish database-persistent settings from connection-local settings.
Verify foreign-key enforcement outside active transactions when the schema relies on it.
Read back important PRAGMAs; unknown PRAGMAs can be silently ignored.
Check actual runtime support and applicable release/correctness notices before using version-dependent features.

Use bound parameters for values.
Release finished statements, cursors, and transactions according to the driver contract.
Do not assume pooled `:memory:` connections share a database or reproduce file-backed behavior.
Shared cache is not a general performance fix.

## Transactions and retries

Keep each atomic write unit on one connection.
Prepare input before acquiring its write transaction where practical.
Avoid network calls or unrelated work while holding a transaction.
Consider `BEGIN IMMEDIATE` for a read-then-write unit when early write admission is useful; it can itself return busy.
Use savepoints when nested atomic units are required.

Distinguish ordinary lock contention, invalid snapshot upgrades, constraint failures, and I/O failures.
Bound lock waits and retries according to the operation's deadline and replay safety.
For a failed snapshot upgrade, restart the appropriate transaction with fresh reads.
A busy commit may leave a transaction active; inspect its state before deciding how to continue.
Verify commit and affected-row outcomes when they are part of the contract.
Handle rollback or cleanup failures without returning a success-shaped result.
Do not retry external side effects as part of a database retry.

Select conflict handling from the data contract.
`INSERT OR REPLACE` can delete an existing conflicting row; it is not equivalent to updating it.
Use explicit UPSERT behavior when an update is intended.
Do not hide invalid data behind broad `OR IGNORE` handling.
Bound batches and respect the runtime's parameter limits.
Large `RETURNING` results may be buffered inside SQLite even when application consumption is streamed.

## WAL and durability

WAL permits readers alongside a writer, but still allows only one writer at a time.
Standard WAL assumes database users on the same host and is unsuitable for cross-host network-file sharing.
Long read transactions can prevent complete checkpoints and WAL reuse.
A checkpoint performs work; it is not a free status query.
Never delete a live WAL file as a space-reclamation shortcut.

Choose synchronization from the accepted durability contract.
In WAL mode, `synchronous=NORMAL` preserves consistency but can lose recent committed transactions after power loss or an OS crash.
`FULL` adds commit synchronization; do not change this trade-off without considering the promised failure model.
Do not disable journaling or synchronization as a routine optimization.
Do not prescribe cache size, memory mapping, in-memory temporary storage, or checkpoint thresholds without workload evidence.

## Migration, backup, and recovery

Reuse the existing migration mechanism and verify supported SQLite syntax.
Before a consequential migration, use a suitable SQLite-aware backup and verify restoration in an isolated location.
Copying only a live `.db` file may omit committed data still in its WAL.
When rebuilding a table, follow the current documented ALTER TABLE procedure.
Inventory dependent indexes, triggers, and views before changing the table.
If foreign-key enforcement must be disabled, do that before the migration transaction on its connection.
Recreate dependencies and validate constraints and data before committing.
Restore the intended connection settings on error paths as well as success paths.
Avoid direct `sqlite_schema` edits and `writable_schema` shortcuts for ordinary migrations.
Account for migration locks and temporary disk requirements.
`VACUUM` rebuilds the database and may require substantial extra space; it is not routine cleanup for every query or deletion.
