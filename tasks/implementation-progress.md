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

## Latest implementation

[Task 40](todo.md#40-curl-installation-and-bundled-surfpool) implements the MIT-licensed native curl distribution with private pinned Surfpool. Homebrew is deferred.
The installer, packager, release workflow and current installation/runtime contracts are updated. ADR-0020 permits shared installed executables while keeping Journals and all runtime data in the project.
The requested system Surfpool and old project-local executable installation were removed. Existing user Labs and Journals were not changed.

286 Rust tests, 15 explicit native Surfpool tests, 16 installer cases, fmt, Clippy and release build passed. The actual macOS distribution passed clean installation, cross-project Simulator/Surfpool startup, embedded dashboard, retained-Journal, tamper and live-update checks.
The local archive and standalone installer are under ignored `target/distribution/`. The repository contains no executable/archive payloads.
GitHub Actions is configured for macOS ARM/Intel and Linux x86_64; remote runs and release publication have not happened. The user requested a local commit after verification. No tag, push or release publication was performed.
The README distinguishes release preparation from a currently published installation command.

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
No original Lab or backend files were changed. No remote publication was performed.

Run the new native workflow on GitHub to validate macOS Intel and Linux artifacts before publishing a binary release. macOS arm64 has local runtime evidence; other target support remains conditional on those jobs passing.
