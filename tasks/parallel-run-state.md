# Remaining run assignments

Status: active. Wave 1 implementation started on 2026-11-03.
Runtime baseline at preparation: `700949e`, with a clean `main` and no remote.
Tasks 1–14 are recorded complete in [todo.md](todo.md).
The preparation commit is later than this runtime baseline; use actual current HEAD for new worktrees.
Read [remaining-implementation-run.md](remaining-implementation-run.md) before dispatch.

## Run identity

- Run ID: `20261103-a`.
- Coordinator checkout: `/Users/kirillermakovic/Downloads/PROJECTS/SELF/ronchi`, branch `main`.
- Agent runtime: Codex collaboration tools (`spawn_agent`, messaging, follow-up) available.
- Actual concurrency: one coordinator and three isolated implementation workers; no recursive delegation.
- Current integration HEAD: `7b94a2484ed84c8d7a28bb6ea051d3629740cd72` (clean starting tree; no remotes).

## Assignments

This table records execution ownership, not completion of product requirements.
Only [todo.md](todo.md) owns completion checkboxes.
Use states `pending`, `active`, `ready`, `integrated`, `verified`, or `blocked`.
Record exact focused checks and their outcomes before a row becomes `verified`.

| Task | State | Owner | Branch / worktree / base | Commits | Verification / blocker |
| --- | --- | --- | --- | --- | --- |
| 15 MCP | active | `/root/task15` | `run/20261103-a/task-15` / `.worktrees/20261103-a-task-15` / `7b94a24` | — | Isolated packet dispatched; focused checks pending |
| 16 Read-through | active | `/root/task16` | `run/20261103-a/task-16` / `.worktrees/20261103-a-task-16` / `7b94a24` | — | Isolated packet dispatched; focused checks pending |
| 17 Surfpool | pending | — | — | — | — |
| 18 Timeout bursts | pending | — | — | — | — |
| 19 Status views | pending | — | — | — | — |
| 20 Confirmation stalls | ready | coordinator | `main` / coordinator checkout / `7b94a24` | pending commit | 3 new controlled-clock HTTP histories passed; 33 transaction histories + 13 Findings query tests passed. fmt/check/Clippy passed. Managed-mode capability test awaits task17; checkbox remains open. |
| 21 Expired-blockhash reuse | pending | — | — | — | — |
| 22 Custom Cases | pending | — | — | — | — |
| 23 Evidence maintenance | active | `/root/task23` | `run/20261103-a/task-23` / `.worktrees/20261103-a-task-23` / `7b94a24` | — | Isolated packet dispatched; focused checks pending |
| 24 Recovery / measurements | pending | — | — | — | — |

## Integration checkpoints

- Initial base: `7b94a24`; task-14 checks are recorded baseline, not rerun here. Git status was clean and no remote exists. Each worker owns its isolated target/build and module slice. Common-file wiring will be reconciled by coordinator. Git metadata writes need ordinary sandbox escalation; all three worktree creations succeeded after escalation.
- Wave 1 integration: not recorded.
- Mode / Case integration: coordinator implemented task20 simulator schedule and evidence/query seam while wave1 runs. Read selected project architecture, Rust, TDD, Case-validation, CLI, codebase-design, review, and docs-maintenance skills. RED: confirmation Cases unsupported. GREEN: deadlines survive replacement and duplicate; autonomous source progression and gated lookups verified. CLI presets use catalog; dashboard catalog controls will be reconciled with the complete Case wave.
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

Wave 1 active: task15 MCP, task16 Read-through, task23 maintenance. Absolute checkout paths use the coordinator root plus the paths in the table. Coordinator is preparing common Case interfaces and later Surfpool requirements without mutating worker paths. No product acceptance check has yet been rerun in this run.
Do not discard an active worker checkout to produce a clean status.
Do not record secrets, bootstrap tickets, signing material, or raw credential-bearing URLs.

## Surfpool prerequisite probe

- Official GitHub release metadata: v1.7.0, published 2026-10-28; source repository redirects to `solana-foundation/surfpool`.
- Darwin arm64 archive SHA-256: `32c309e3b79bfa3913e384b3c9b109000720cfe7f346d145feb6693fccfb202f`; downloaded archive matches.
- Probe artifacts: `/private/tmp/ronchi-surfpool-v1.7.0-darwin-arm64.tar.gz`, extracted `/private/tmp/ronchi-surfpool-probe/surfpool` (disposable development probe, not product installation).
- Actual `--version`: 1.7.0. Offline `start --ci --no-deploy --airdrop-amount 0` with loopback dynamic RPC/WS and temporary logs answered getHealth/getVersion/getLatestBlockhash. Version reports solana-core 4.3.2.
- Probe child stopped/reaped; SIGTERM did not exit within 5 seconds, then owned child kill/reap completed. Test SIGINT for managed cleanup. No application transaction execution claim yet.
