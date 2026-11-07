# Current implementation state

Ronchi is a local investigation prototype with a native Rust core and Solid dashboard.
The initial implementation queue and tasks 25–37 are complete at their recorded scope.
Application Checks are removed; the dashboard uses live WebSocket subscriptions and four fixed movable/resizable panels.
Current behavior belongs to [product contracts](../docs/README.md#current-documents), not earlier handoffs.

## Latest verified runtime baseline

[Task 37](todo.md#37-remove-confirmed-dead-code) records 284 Rust tests, 64 frontend tests, a native Chromium Observation/lookup flow, typecheck, formatting, Clippy and release build.
Its release smoke confirmed the embedded assets match the frontend build.
[Task 36](todo.md#36-remove-application-checks) separately records all ten browser scenarios, including actual Surfpool.
These are recorded results, not a claim that the documentation task reran them.

## First native release in progress

[Task 40](todo.md#40-curl-installation-and-bundled-surfpool) implements MIT-licensed native curl distribution with private pinned Surfpool. ADR-0020 keeps runtime data project-local while permitting shared installed executables. Homebrew remains deferred.
The user authorized the entire [task 41 release sequence](todo.md#41-first-native-release), including corrective commits/pushes, native CI, the verified `v0.1.0` tag and release, public curl verification and final documentation.

Source changes are pushed through `f2bcf51`. CI exposed and fixed timing-sensitive fixtures, portable PTY FFI, a hold-deadline evidence race, and Custom lookup's per-entry durable-write bottleneck. Custom source/effect pages retain all per-entry records and exact links; an injected storage failure verifies page rollback before a masked response can escape.
At `dfe3a48`, the complete local Rust suite passed 288 tests; all 15 explicit real Surfpool tests, Clippy, fmt, release and the source-link/cancellation checks passed. The subsequent pipe-readiness fix adds tested streaming/EOF/deadline coverage and passed Clippy, release packaging and the full installed-artifact/reinstall/tamper/live-update flow. It preserves the twenty-second tamper-test limit and all checksum/deadline checks.

Manual matrix `42401521251` at `dfe3a48` finished with complete Linux and Intel success. ARM passed Rust/build/notices/package and failed the tamper check due to the subsequently fixed archive-pipe polling overhead. The next full matrix must include `f2bcf51` and the requested CI acceleration. Do not tag before that complete latest-main matrix passes.

The user requested faster Actions. The updated workflow caches Cargo downloads, debug/release outputs and the pinned license-generator executable separately. Compatible builds restore by platform/toolchain/manifests/workflow prefix; exact revision hits retain the native build. Completed caches can save after later failures. Every product/release gate still runs. CI uses line-table debug information and no incremental state to limit cache volume, without changing the shipped release profile. SHA-pinned official cache v6.1.0, actionlint, YAML parsing, shell syntax and a Rust profile smoke passed. Cold/warm GitHub execution has not yet been measured.

No version tag or release exists yet. GitHub reports the repository as private. Changing the entire repository's visibility requires separate user authorization before anonymous public curl verification can work. Do not silently expose it.
README's preparation notice must remain until public installation actually works. Current validation details and earlier runs are recorded in task 41. Temporary logs/scripts and isolated installations remain outside tracked files.

Earlier completed documentation/README work is recorded in [tasks 38–39](todo.md#38-documentation-organization).

## Remaining validation

Repeat the complete independent stateful-backend/pool-discovery workflow after the earlier fixes and latest UI changes.
The user-reported Solana-backend trial established bounded manual evidence for Checkpoints C/E; it did not establish full compatibility.
The bundled relayer does not replace that external retest.
The [normal-down observation](history/unconfirmed-down-observation.md) remains unconfirmed; preserve the original envelope if it recurs.
Deferred product choices remain in [open questions](../docs/open-questions.md).

## Resume and workspace

Read `AGENTS.md`, the [documentation map](../docs/README.md) and actual `git status` before choosing new work.
This checkpoint groups the accumulated implementation/UI work, application-Checks removal, cleanup and documentation organization.
Inspect Git log/status for its commit and any subsequent changes.
Do not restart the completed parallel run or infer commit/publish authorization from its historical ledger.
No original user Lab or backend files were changed. Source fixes are pushed; release publication is still pending. Follow the authorized task 41 sequence above.
