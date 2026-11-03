# Remaining run assignments

Status: prepared, not started.
Runtime baseline at preparation: `700949e`, with a clean `main` and no remote.
Tasks 1–14 are recorded complete in [todo.md](todo.md).
The preparation commit is later than this runtime baseline; use actual current HEAD for new worktrees.
Read [remaining-implementation-run.md](remaining-implementation-run.md) before dispatch.

## Run identity

- Run ID: not allocated.
- Coordinator checkout: not allocated; the existing main checkout is the default.
- Agent path and delegation tools: not yet checked in the implementation session.
- Actual concurrency: not allocated; project policy permits up to three workers.
- Current integration HEAD: resolve at run start.

## Assignments

This table records execution ownership, not completion of product requirements.
Only [todo.md](todo.md) owns completion checkboxes.
Use states `pending`, `active`, `ready`, `integrated`, `verified`, or `blocked`.
Record exact focused checks and their outcomes before a row becomes `verified`.

| Task | State | Owner | Branch / worktree / base | Commits | Verification / blocker |
| --- | --- | --- | --- | --- | --- |
| 15 MCP | pending | — | — | — | — |
| 16 Read-through | pending | — | — | — | — |
| 17 Surfpool | pending | — | — | — | — |
| 18 Timeout bursts | pending | — | — | — | — |
| 19 Status views | pending | — | — | — | — |
| 20 Confirmation stalls | pending | — | — | — | — |
| 21 Expired-blockhash reuse | pending | — | — | — | — |
| 22 Custom Cases | pending | — | — | — | — |
| 23 Evidence maintenance | pending | — | — | — | — |
| 24 Recovery / measurements | pending | — | — | — | — |

## Integration checkpoints

- Initial verified base and shared-interface notes: not recorded.
- Wave 1 integration: not recorded.
- Mode / Case integration: not recorded.
- Custom Case integration: not recorded.
- Final accumulated acceptance: not recorded.
- Isolated resource measurements: not recorded.

Record commands, relevant commit, host, and actual result.
Keep lengthy logs and generated reports outside tracked task documents.

## Preserved observations

- The [normal-down observation](unconfirmed-down-observation.md) remains unconfirmed.
- Checkpoint C's external-backend validation remains open.
- Checkpoint E's independent-backend validation requires actual application evidence.
- Collector HTTP-only support and its 32 KiB effective cap remain explicit implementation limits.
- Evaluation remains single-intent/signature, with 512 records and 4 MiB evidence.

## Resume record

Before a context reset or session exit, record active owners, checkout paths, uncommitted files, completed checks, and the next unblocked action here.
Do not discard an active worker checkout to produce a clean status.
Do not record secrets, bootstrap tickets, signing material, or raw credential-bearing URLs.
