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

## Native release and CI

[v0.1.0](https://github.com/kirillermakovichh/ronchi/releases/tag/v0.1.0) is published in the currently private GitHub repository. The annotated tag points to `0ca02a0`.
Manual matrix `42404486999` and tagged release `42406451668` passed on macOS ARM, macOS Intel and Linux x86_64. The Rust suite passed 290 tests; all release/installer gates remained enabled. [Task 41](todo.md#41-first-native-release) owns corrective commits, exact checks and remaining acceptance.

The requested CI caches are implemented and exercised. Observed cold/warm job times: ARM 8m57s → 7m51s, Linux 15m10s → 11m39s, Intel 22m39s → 15m49s. The ARM log confirms exact Cargo-download, native-build and license-generator cache hits. Tests, notices, packaging and installed checks still execute on hits.

All eight published files were downloaded with GitHub authentication. All three archive SHA-256 values and metadata were verified. The actual downloaded ARM archive passed installation, embedded dashboard, private Surfpool, retained Journal, tamper, reinstall and live-update checks. Temporary installations and Labs were cleaned; original user Labs/backends were not touched.

Anonymous public curl verification remains open because the repository is private. The user has been asked asynchronously to authorize making the whole repository/history public; no answer has arrived. Do not change visibility without that authorization. README accurately marks public downloads as pending. After permission, verify the actual public installer in an isolated prefix, apply the prepared release notes, remove the warning, close task 41d and push final docs.

MIT and bundled pinned Surfpool are implemented by [task 40](todo.md#40-curl-installation-and-bundled-surfpool) and ADR-0020. Runtime data remains project-local. Homebrew remains deferred.

Earlier documentation/README work is recorded in [tasks 38–39](todo.md#38-documentation-organization).

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
No original user Lab or backend files were changed. Runtime fixes and CI acceleration are pushed; the tagged release is published. Follow the remaining task 41d sequence above.
