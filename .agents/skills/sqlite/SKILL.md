---
name: sqlite
description: Design, implement, or review SQLite storage in Ronchi. Use for Journal schema, queries, transactions, WAL/checkpoints, migrations, backup, and recovery. Use rust-best-practices for rusqlite and Tokio integration.
license: MIT
metadata:
  author: Ronchi contributors
  upstream-author: Bateau
  version: "1.0.0-ronchi"
---

# SQLite for Ronchi

This is a project adaptation of Bateau's SQLite skill.
Follow `AGENTS.md` and the accepted Journal, workspace, and resource decisions.
Keep engine behavior distinct from Rust driver and task-lifecycle behavior.

## Establish the affected contract

- Inspect the actual application's SQLite library and driver, not only the system `sqlite3` executable.
- When executable code exists, inspect `sqlite_version()`, `sqlite_source_id()`, relevant compile options, and effective connection settings as needed.
- Inspect affected schema objects, queries, migration history, connection ownership, and filesystem use.
- Identify the intended atomic unit, accepted failure behavior, and required data after reopen or recovery.
- Distinguish existing decisions, proposed settings, and observed implementation behavior.

## Select the relevant reference

| Work | Reference |
| --- | --- |
| Schema, SQL, connection settings, transaction errors, WAL, migrations, or recovery | [engine-contracts.md](references/engine-contracts.md) |
| Journal writes, evidence queries, persistence acknowledgment, disk growth, or lifecycle | [journal-storage.md](references/journal-storage.md) |

Read only the references needed for the current change.
Use `rust-best-practices` for rusqlite calls, blocking isolation, task ownership, and shutdown integration.
Do not adopt a new ORM, connection pool, database engine, or migration framework to satisfy a generic example.

## Verify the changed behavior

Use a real disposable file-backed database for file, WAL, multi-connection, migration, or reopen behavior.
Use the actual linked driver when its transaction or error semantics matter.
Verify relevant data, constraints, rollback, commit outcomes, and recovery through the affected interface.
Use `foreign_key_check` for referential checks when applicable.
Do not treat `integrity_check` or `quick_check` as proof of application-level correctness.
Keep timing and resource claims separate from correctness checks.
Follow the repository's verification workflow without adding test layers or tooling unrelated to the change.

Report the change, checks performed, relevant runtime details, and remaining uncertainty.
If execution is unavailable, label proposed SQL, configuration, and verification steps as unexecuted.

## Provenance

See [sources.md](references/sources.md) for the reviewed revision, adaptations, primary SQLite documentation, and MIT attribution.
