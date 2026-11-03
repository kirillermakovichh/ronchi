# Remaining run assignments

Status: active. Wave1 verified; task17 implementation active.
Runtime baseline at preparation: `700949e`, with a clean `main` and no remote.
Tasks 1–14 are recorded complete in [todo.md](todo.md).
The preparation commit is later than this runtime baseline; use actual current HEAD for new worktrees.
Read [remaining-implementation-run.md](remaining-implementation-run.md) before dispatch.

## Run identity

- Run ID: `20261103-a`.
- Coordinator checkout: `/Users/kirillermakovic/Downloads/PROJECTS/SELF/ronchi`, branch `main`.
- Agent runtime: Codex collaboration tools (`spawn_agent`, messaging, follow-up) available.
- Actual concurrency: one coordinator and three isolated implementation workers; no recursive delegation.
- Current integration HEAD: `606b1fd` (wave 1 integrated; no remotes). Starting HEAD was `7b94a24`.

## Assignments

This table records execution ownership, not completion of product requirements.
Only [todo.md](todo.md) owns completion checkboxes.
Use states `pending`, `active`, `ready`, `integrated`, `verified`, or `blocked`.
Record exact focused checks and their outcomes before a row becomes `verified`.

| Task | State | Owner | Branch / worktree / base | Commits | Verification / blocker |
| --- | --- | --- | --- | --- | --- |
| 15 MCP | verified | `/root/task15` | `run/20261103-a/task-15` / `.worktrees/20261103-a-task-15` / `7b94a24` | worker `e2e7bfc` → main `606b1fd` | 7 actual rmcp stdio process tests passed on combined main; retained read lease added during integration; accumulated checkpoint passed |
| 16 Read-through | verified | `/root/task16` | `run/20261103-a/task-16` / `.worktrees/20261103-a-task-16` / `7b94a24` | worker `131d0f9` → main `0b8b297` | 10 Read-through HTTP/process tests passed on combined main; redaction review fix included; accumulated checkpoint passed |
| 17 Surfpool | active | `/root/task23` | `run/20261103-a/task-17` / `.worktrees/20261103-a-task-17` / `606b1fd` | — | Native ownership/tool install/execution packet active; separate target/jobs2 |
| 18 Timeout bursts | pending | `/root/task15` preparation only | next checkout not yet allocated | — | Read-only prepare/validate-before-dispatch analysis complete; await17 integration |
| 19 Status views | pending | `/root/task16` preparation only | next checkout not yet allocated | — | Read-only source/cache/query analysis; await17 integration |
| 20 Confirmation stalls | ready | coordinator | `main` / coordinator checkout / `7b94a24` | `53528f7`, `b0b0eea` | 3 new controlled-clock HTTP histories passed; 33 transaction histories + 13 Findings query tests passed. fmt/check/Clippy passed. Managed-mode capability test awaits task17; checkbox remains open. |
| 21 Expired-blockhash reuse | pending | — | — | — | — |
| 22 Custom Cases | pending | — | — | — | — |
| 23 Evidence maintenance | verified | `/root/task23` | `run/20261103-a/task-23` / `.worktrees/20261103-a-task-23` / `7b94a24` | worker `6481a7f` → main `949a7ec` | 10 maintenance process tests passed on combined main after precise process-disappearance fix 31baf3f; original first failure preserved below; accumulated checkpoint passed |
| 24 Recovery / measurements | pending | — | — | — | — |

## Integration checkpoints

- Initial base: `7b94a24`; task-14 checks are recorded baseline, not rerun here. Git status was clean and no remote exists. Each worker owns its isolated target/build and module slice. Common-file wiring will be reconciled by coordinator. Git metadata writes need ordinary sandbox escalation; all three worktree creations succeeded after escalation.
- Wave 1 integration: `949a7ec` maintenance, `0b8b297` Read-through, `606b1fd` MCP. Combined focused tests: maintenance10 + MCP7 + Read-through10 passed. Cargo.lock conflicts reconciled as union; `cargo check --locked --offline` passed. No dependency was discarded. MCP retained query holds maintenance read lease. Full checkpoint passed after the documented fixture correction: npm ci (audit0), typecheck, unit14, build; fmt/check; full cargo test; all-target Clippy; native build; browser8; release build (optimized1m18s).
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

Task17 active in its exact worktree above. Workers15/16 completed wave1; only read-only next-Case preparation is authorized until17 integration. Coordinator committed the test-fixture regression after complete checkpoint success. Tasks15/16/23 checkboxes are complete. Task20 remains open only for managed-mode capability integration. No accumulated process is running. Keep the run moving after checkpoint and17 integration.
Do not discard an active worker checkout to produce a clean status.
Do not record secrets, bootstrap tickets, signing material, or raw credential-bearing URLs.

## Surfpool prerequisite probe

- Official GitHub release metadata: v1.7.0, published 2026-10-28; source repository redirects to `solana-foundation/surfpool`.
- Darwin arm64 archive SHA-256: `32c309e3b79bfa3913e384b3c9b109000720cfe7f346d145feb6693fccfb202f`; downloaded archive matches.
- Probe artifacts: `/private/tmp/ronchi-surfpool-v1.7.0-darwin-arm64.tar.gz`, extracted `/private/tmp/ronchi-surfpool-probe/surfpool` (disposable development probe, not product installation).
- Actual `--version`: 1.7.0. Offline `start --ci --no-deploy --airdrop-amount 0` with loopback dynamic RPC/WS and temporary logs answered getHealth/getVersion/getLatestBlockhash. Version reports solana-core 4.3.2.
- Probe child stopped/reaped; SIGTERM did not exit within 5 seconds, then owned child kill/reap completed. Test SIGINT for managed cleanup. No application transaction execution claim yet.

## Integration failures and corrections

- First combined maintenance run: `process_death_retains_wal_and_export_never_migrates_original_schema` failed at `process::matches(...).unwrap()` with OS ESRCH after owned daemon SIGKILL. macOS `proc_pidpath` can lose the process after successful `proc_pidinfo`. Main `31baf3f` maps only disappearance to absence; other errors propagate. Linux read_link disappearance receives analogous handling; Linux runtime remains untested. Original trigger passed after correction; combined maintenance10 passed. This is distinct from the earlier unconfirmed normal-down observation.
- Task20 scoped review found missing maintained-rule cursor fingerprint, invalid Finding ID shape, and hardcoded human catalog. Main `b0b0eea` fixes them and adds evidence drill-down, old-rule-cursor rejection, and dashboard core-catalog selection checks. Combined Findings14 + transaction33 passed; dashboard unit14/typecheck/build passed.
- First accumulated run stopped in `checks_collection::start_and_completion_commit_failures_are_product_failures`: expected injected Journal failure, got successful SourceGap `source_json`. Narrow rerun passed, then diagnostic probe confirmed macOS accepted socket O_NONBLOCK inheritance. New deterministic `source_fixture_waits_for_request_bytes_after_nonblocking_accept` failed with early WouldBlock and null request before any bytes. Test fixture now explicitly sets blocking with finite read timeout and fails on premature EOF. New regression + original collection suite5 passed. No product collector change/retry was added; debug probes removed. Full accumulated rerun passed, including all Rust tests, all-target Clippy, eight native Chromium flows, and optimized build. First failed results remain recorded here.

## Next unblocked action

Task17 remains active at `.worktrees/20261103-a-task-17` on base606b1fd. Coordinator should review native ownership/fixtures and settle the smallest registration-owned activation state plus prepare/source callbacks before dispatching18/19/21. Workers15/16 are idle after read-only preparation; give each a new isolated checkout only after17 integration. Coordinator implements21 in main or dispatches the third worker if17 worker is free. Do not rerun the accumulated suite until new integrated changes justify it. No user handoff is needed.

## Shared Case handoff preparation (not implemented yet)

- Worker15 will implement18 after17 integration. Target seam: prepare/validate -> timeout selection -> dispatch. Invalid/unsupported requests and notifications cannot consume a missing-response Fault. Simulator ALT/nonce capability checks currently occur inside submit and need a validate-only stage. Backend prepare currently delegates some read/simulation validation;18 must cover known argument shapes before selection.
- Freeze activation and registration real/simulator timestamps before batch execution. Timeout phase must not drift while earlier batch elements execute.
- HoldPool lower-limit check+semaphore acquisition currently lacks one shared critical section. Current response-loss actor serializes it;18 concurrent pre-dispatch selection must fix/test max_held=1 race while preserving one hold per HTTP batch.
- Worker16 will implement19 after17. Preserve status-array order/duplicates/current source context; getSignatureStatuses has no commitment selector. Unknown/already-null results are not injected Faults. getTransaction snapshots must preserve encoding/version/commitment; backend getTransaction alone cannot distinguish confirmed from finalized. Use actual source status observation or explicit no-snapshot passthrough.
- Coordinator will establish registration-owned Arc<Activation> before parallel18/19/21. The actor must clone that Arc at registration, not later in RPC processing. Case replacement swaps only active Arc; old registrations keep their old caches. Keep serializable Binding separate from runtime state. Add typed timing fields and let19/21 own their cache state modules; do not create arbitrary plugin state.
- Current actor register replies with serialized Value. A small typed Reply::Registered plus ordinary Reply::Value can preserve the Arc without changing every operation interface. Existing job(Value) callers remain behind their helper.
- Task21 requires genuine tuple/source context/authority/commitment, same-authority getBlockHeight, inclusive equality, then10 real seconds expiry window/recovery. Durable nonce and unresolved lifetime do not establish expiry. Surfpool rejection can end the cache cycle while preserving actual backend outcome.
- Task21 must also fix known expired-at-admission simulator behavior: it currently stores normal successful2/4/6 deadlines before immediate Expired transition. Preserve signature acceptance but make expiry-prevented processing explicit without advertising a success schedule.
- Old clean integrated worktrees15/16/23 were removed normally after verification. Their branches/commits remain. Only coordinator and task17 checkout remain active. Main has no owned background test process.
