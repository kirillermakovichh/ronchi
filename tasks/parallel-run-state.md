# Remaining run assignments

Status: active. Tasks18/19/21 integrated; Checkpoint D review and task22 implementation active.
Runtime baseline at preparation: `700949e`, with a clean `main` and no remote.
Tasks 1–14 are recorded complete in [todo.md](todo.md).
The preparation commit is later than this runtime baseline; use actual current HEAD for new worktrees.
Read [remaining-implementation-run.md](remaining-implementation-run.md) before dispatch.

## Run identity

- Run ID: `20261103-a`.
- Coordinator checkout: `/Users/kirillermakovic/Downloads/PROJECTS/SELF/ronchi`, branch `main`.
- Agent runtime: Codex collaboration tools (`spawn_agent`, messaging, follow-up) available.
- Actual concurrency: one coordinator and three isolated implementation workers; no recursive delegation.
- Current integration code HEAD: `c0e2294` (plus retained state docs; no remotes). Starting HEAD was `7b94a24`.

## Assignments

This table records execution ownership, not completion of product requirements.
Only [todo.md](todo.md) owns completion checkboxes.
Use states `pending`, `active`, `ready`, `integrated`, `verified`, or `blocked`.
Record exact focused checks and their outcomes before a row becomes `verified`.

| Task | State | Owner | Branch / worktree / base | Commits | Verification / blocker |
| --- | --- | --- | --- | --- | --- |
| 15 MCP | verified | `/root/task15` | `run/20261103-a/task-15` / `.worktrees/20261103-a-task-15` / `7b94a24` | worker `e2e7bfc` → main `606b1fd` | 7 actual rmcp stdio process tests passed on combined main; retained read lease added during integration; accumulated checkpoint passed |
| 16 Read-through | verified | `/root/task16` | `run/20261103-a/task-16` / `.worktrees/20261103-a-task-16` / `7b94a24` | worker `131d0f9` → main `0b8b297` | 10 Read-through HTTP/process tests passed on combined main; redaction review fix included; accumulated checkpoint passed |
| 17 Surfpool | verified | `/root/task23` | `run/20261103-a/task-17` / `.worktrees/20261103-a-task-17` / `606b1fd` | worker755c20d/c3e4f13 → main7326fd3/bd3f8bf | Native8 + Read-through10 + transaction33 passed; fake source -32603 regression passed; native8 passed again after shared registration seam. macOS arm64 actual execution only. |
| 18 Timeout bursts | integrated | `/root/task15` | `run/20261103-a/task-18` / `.worktrees/20261103-a-task-18` / `bd3f8bf` | — | Implementation packet dispatched after verified mode + registration contracts; focused checks pending |
| 19 Status views | integrated | `/root/task16` | `run/20261103-a/task-19` / `.worktrees/20261103-a-task-19` / `bd3f8bf` | — | Implementation packet dispatched after verified mode + registration contracts; focused checks pending |
| 20 Confirmation stalls | verified | coordinator | `main` / coordinator checkout / `7b94a24` | `53528f7`, `b0b0eea` | 4 controlled-clock histories; combined transaction34 passed. Finding drill-down/rule cursor tests passed; native Surfpool confirmation capability rejection verified. fmt/check/Clippy passed. |
| 21 Expired-blockhash reuse | integrated | `/root/task23` | `run/20261103-a/task-21` / `.worktrees/20261103-a-task-21` / `bd3f8bf` | — | Implementation packet dispatched after verified mode + registration contracts; focused checks pending |
| 22 Custom Cases | active | coordinator + three workers | runtime main `c0e2294`; acceptance packets below | `bfd4851`, `89d41c4`, `c0e2294` | Compiler11 + five custom RPC histories + dashboard16 passed; remaining public/native/lifecycle/template acceptance active |
| 23 Evidence maintenance | verified | `/root/task23` | `run/20261103-a/task-23` / `.worktrees/20261103-a-task-23` / `7b94a24` | worker `6481a7f` → main `949a7ec` | 10 maintenance process tests passed on combined main after precise process-disappearance fix 31baf3f; original first failure preserved below; accumulated checkpoint passed |
| 24 Recovery / measurements | active | coordinator | harness integrated; final main executable required | `82258a0`, `2c12722` | Combined recovery3 + RPCbudget2 passed; simulator/native harness smoke passed in worker. Actual final measurements pending |

## Integration checkpoints

- Initial base: `7b94a24`; task-14 checks are recorded baseline, not rerun here. Git status was clean and no remote exists. Each worker owns its isolated target/build and module slice. Common-file wiring will be reconciled by coordinator. Git metadata writes need ordinary sandbox escalation; all three worktree creations succeeded after escalation.
- Wave 1 integration: `949a7ec` maintenance, `0b8b297` Read-through, `606b1fd` MCP. Combined focused tests: maintenance10 + MCP7 + Read-through10 passed. Cargo.lock conflicts reconciled as union; `cargo check --locked --offline` passed. No dependency was discarded. MCP retained query holds maintenance read lease. Full checkpoint passed after the documented fixture correction: npm ci (audit0), typecheck, unit14, build; fmt/check; full cargo test; all-target Clippy; native build; browser8; release build (optimized1m18s).
- Mode / Case integration: task17 native8, Read-through10, transaction33 and fake-backend error isolation passed on main. Shared activation seam08b47f6 passed transaction34 and all-target Clippy; native8 passed after it. Task20 unsupported Surfpool capability verified. Coordinator implemented task20 simulator schedule and evidence/query seam while wave1 runs. Read selected project architecture, Rust, TDD, Case-validation, CLI, codebase-design, review, and docs-maintenance skills. RED: confirmation Cases unsupported. GREEN: deadlines survive replacement and duplicate; autonomous source progression and gated lookups verified. CLI presets use catalog; dashboard catalog controls will be reconciled with the complete Case wave.
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

Tasks18/19/21 active in exact worktrees above, all basebd3f8bf. Tasks15/16/17/20/23 checkboxes complete. Main clean at dispatch. No accumulated suite or resource measurement is running. Next: inspect incoming worker contracts/diffs, reconcile Arm/catalog/RPC/actor/query wiring; checkpointD then22 then24. Continue without user handoff.
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

Tasks18/19/21 are active. Integrate verified commits deliberately, then CheckpointD and22. Do not start measurements before15–23 combined implementation is stable. Shared runtime activation seam is implemented as08b47f6; pending cache fields will be unioned during integration.

## Shared Case handoff contracts and remaining module work

- Worker15 will implement18 after17 integration. Target seam: prepare/validate -> timeout selection -> dispatch. Invalid/unsupported requests and notifications cannot consume a missing-response Fault. Simulator ALT/nonce capability checks currently occur inside submit and need a validate-only stage. Backend prepare currently delegates some read/simulation validation;18 must cover known argument shapes before selection.
- Freeze activation and registration real/simulator timestamps before batch execution. Timeout phase must not drift while earlier batch elements execute.
- HoldPool lower-limit check+semaphore acquisition currently lacks one shared critical section. Current response-loss actor serializes it;18 concurrent pre-dispatch selection must fix/test max_held=1 race while preserving one hold per HTTP batch.
- Worker16 will implement19 after17. Preserve status-array order/duplicates/current source context; getSignatureStatuses has no commitment selector. Unknown/already-null results are not injected Faults. getTransaction snapshots must preserve encoding/version/commitment; backend getTransaction alone cannot distinguish confirmed from finalized. Use actual source status observation or explicit no-snapshot passthrough.
- Coordinator established registration-owned Arc<Activation> in08b47f6 before parallel18/19/21. The actor must clone that Arc at registration, not later in RPC processing. Case replacement swaps only active Arc; old registrations keep their old caches. Keep serializable Binding separate from runtime state. Add typed timing fields and let19/21 own their cache state modules; do not create arbitrary plugin state.
- Actor now replies via typed Reply::Registered and ordinary Reply::Value; the old public job(Value) behavior remains behind its helper. Policy records activation real/simulator times; RPC references freeze registration times perbatch.
- Task21 requires genuine tuple/source context/authority/commitment, same-authority getBlockHeight, inclusive equality, then10 real seconds expiry window/recovery. Durable nonce and unresolved lifetime do not establish expiry. Surfpool rejection can end the cache cycle while preserving actual backend outcome.
- Task21 must also fix known expired-at-admission simulator behavior: it currently stores normal successful2/4/6 deadlines before immediate Expired transition. Preserve signature acceptance but make expiry-prevented processing explicit without advertising a success schedule.
- Old clean integrated worktrees15/16/23 were removed normally after verification. Their branches/commits remain. Only coordinator and task17 checkout remain active. Main has no owned background test process.

- Native oracle correction in17 regression: a32-byte base58 genesis may have43 or44 characters. tests/read_through now decodes to32 bytes. No runtime change followed. The original43!=44 failure is not a product regression.
- Native backend may return -32002 for a default-preflight replay of an already processed identity. Tests preserve this backend result instead of fabricating unconditional duplicate success.
- Source -32603 originally poisoned Journal health in backend-submit actor. Actual fake-backend first-error/second-acceptance RED→GREEN is integrated inbd3f8bf.

## Resumed coordinator audit

Actual main HEAD `d7503b9`, clean checkout, no remote. Integrated18=`51cf510`,19=`7ac917b`,21=`d7503b9`; checkboxes await fresh integration evidence and scoped review. Active source-error correction: `/root/task15`, branch `run/20261103-a/source-errors`, checkout `.worktrees/20261103-a-source-errors`, base `d7503b9`; backend canonical errors/data/provenance. Worker16 reviews integrated Case wave before parser work. Worker23 owns task24 recovery/harness. Coordinator owns Custom Case runtime and all ledgers. No accumulated suite or measurement running at resume.

## Custom runtime integration in progress

Current committed main `89d41c4`; uncommitted runtime/interface/query/UI slice belongs to coordinator.
Compiler5f57c2f/171c7af integrated asbfd4851/89d41c4; eleven isolated compiler tests passed with scoped check/Clippy.
Source-error dad116b integrated8612f1e. Task24 harness4aa213a integrated82258a0.
Source-budget7ccc314 integrated as2c12722 after combined rpc_response_budget2 + failure_recovery3 passed. Parsed batch RED→GREEN confirmed; writer/disk histories preserve acknowledged bytes and independent reference survival.
Custom public focused histories passed: first-lookup window, mixed signature case-key selection + empty methods, atomic capacity/counters, unsupported v0 lookup not consuming a step, transaction-first complete snapshot/commitment gates.
The first main tracer attempt could not bind loopback under sandbox; ordinary escalation enabled it and it passed. This is a permission result, not a product failure.
Custom UI RED→GREEN passed; typecheck/build passed. All-target Clippy passed after moving helpers before the test module and documenting the existing lookup seam argument count.
Reviewers15/23 checked parser/runtime. Confirmed issues corrected: numeric fallback, effective stage filters, exact case-key eligibility, any-selected timeout projection, atomic key/hold admission, bounded selection evidence, complete status projection, source/prepared/boundary lookup selection, and later actual registration evidence.
Remaining task22 histories: response-loss ledger reactivation, whole-batch old activation, template positive/incomplete scopes, timeout/repeat/confirmation/blockhash primitive composition, native mode and public authoring surfaces.
No accumulated suite or actual measurement has run for this slice. Tasks22/24 stay open.

Custom runtime slice pre-commit: compiler11, five separate public custom RPC histories, dashboard unit16/typecheck/build, all-target Clippy, budget2 and recovery3 passed. Full accumulated acceptance and remaining Custom histories remain pending.

## Current acceptance packets after c0e2294

- Worker23: branch `run/20261103-a/task-22-lifecycle`, exact reused clean checkout `.worktrees/20261103-a-task-24-harness`, base `c0e2294`. Owns new `tests/cases/custom_lifecycle.rs` and one include in `tests/transaction_history.rs`; response-loss reactivation, old batch binding, confirmation, timeout/repeat histories. No core changes.
- Worker15: branch `run/20261103-a/task-22-native`, exact reused clean checkout `.worktrees/20261103-a-source-errors`, base `c0e2294`. Owns native test additions `tests/surfpool_mode.rs`, `tests/cases/custom_native.rs`, optional new `src/rpc/custom_tests.rs`; coordinator owns cfg wiring. Actual prepared archive remains `/private/tmp/ronchi-surfpool-v1.7.0-darwin-arm64.tar.gz`.
- Worker16: branch `run/20261103-a/task-22-surfaces`, exact reused parser checkout `.worktrees/20261103-a-task-22-parser`, base `c0e2294`. Owns only `tests/custom_surfaces.rs`; CLI/offline/schema/actual rmcp validation and activation parity.
- Coordinator: template positive/incomplete histories and surface corrections in main. New eight-signature template history passed, including scoped-source exclusion. CLI `schema case.validate` clap allowlist fixed after worker RED; specific ValidationEnvelope schema now wired for CLI/MCP, awaiting public tests. Uncommitted paths: `src/lib.rs`, `src/mcp.rs`, `src/cases/custom.rs`, `dashboard/src/api.ts`, `tests/cases/custom.rs`, this ledger.
- No accumulated native/browser suite or actual measurement is running. Keep at most three implementation workers. Next: integrate acceptance tests, correct observed failures, run combined Case/CheckpointD and full accumulated acceptance; then release measurements with builds/tests stopped. Tasks22/24 and unchecked checkpoints remain open.

Coordinator follow-up: combined custom6 passed including eight-signature template and missing-source scope. CLI schema.case.validate now runs and publishes Plan/ValidationError; dedicated validation output envelope added to CLI/MCP. Dashboard typecheck + unit16 passed after showing category/path for validation errors. Worker16 observed both schema bugs RED before corrections.
