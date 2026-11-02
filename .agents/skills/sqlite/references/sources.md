# Sources and adaptation

Reviewed on 2026-11-02.

Adapted from [Bateau's SQLite skill](https://github.com/bateau84/skills/blob/0e135d01c8debf2fe3992a0830fe3c39e3b0c5a0/skills/sqlite/SKILL.md), revision `0e135d01c8debf2fe3992a0830fe3c39e3b0c5a0`.
The source declares author `Bateau`, version `1.0.0`, and license `MIT` in its frontmatter.
The reviewed repository tree contains no separate license or copyright notice file.
The declaration and attribution are retained here; the standard MIT license text is included in [LICENSE](../LICENSE).
That file is supplied locally, not represented as an upstream license-file copy.

The adaptation keeps transaction ownership, connection-specific settings, bounded retries, WAL lifecycle, migration, backup, and recovery guidance.
It removes the assumed 100 MB–1 GB workload size and dependencies on the absent `sql` and `sql-performance` skills.
It separates general engine guidance from Ronchi Journal constraints.
It does not add fixed memory settings, durability settings, retention rules, or measurement gates.
Rust driver and Tokio integration remain in the project's `rust-best-practices` skill.

## Primary references

- [WAL](https://www.sqlite.org/wal.html), [checkpoint modes](https://www.sqlite.org/c3ref/wal_checkpoint_v2.html), and [synchronous](https://www.sqlite.org/pragma.html#pragma_synchronous): concurrency, file growth, and durability trade-offs.
- [Transactions](https://www.sqlite.org/lang_transaction.html), [isolation](https://www.sqlite.org/isolation.html), and [conflict handling](https://www.sqlite.org/lang_conflict.html): atomic units, retries, and commit outcomes.
- [PRAGMAs](https://www.sqlite.org/pragma.html), [foreign keys](https://www.sqlite.org/foreignkeys.html), [data types](https://www.sqlite.org/datatype3.html), and [STRICT tables](https://www.sqlite.org/stricttables.html): connection settings and schema guarantees.
- [Online backup](https://www.sqlite.org/backup.html), [ALTER TABLE rebuild procedure](https://www.sqlite.org/lang_altertable.html#otheralter), and [VACUUM](https://www.sqlite.org/lang_vacuum.html): backup, migration, recovery, and disk requirements.
- [RETURNING](https://www.sqlite.org/lang_returning.html#processing_order), [limits](https://www.sqlite.org/limits.html), and [query plans](https://www.sqlite.org/eqp.html): bounded work and query verification.
- [Release notes](https://www.sqlite.org/changes.html): check the actual embedded build and applicable fixes when implementing version-sensitive behavior.
