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

[v0.1.0](https://github.com/kirillermakovichh/ronchi/releases/tag/v0.1.0) is published in the public GitHub repository. The annotated tag points to `0ca02a0`.
Manual matrix `42404486999` and tagged release `42406451668` passed on macOS ARM, macOS Intel and Linux x86_64. The Rust suite passed 290 tests; all release/installer gates remained enabled. [Task 41](todo.md#41-first-native-release) owns corrective commits, exact checks and remaining acceptance.

The requested CI caches are implemented and exercised. Observed cold/warm job times: ARM 8m57s → 7m51s, Linux 15m10s → 11m39s, Intel 22m39s → 15m49s. The ARM log confirms exact Cargo-download, native-build and license-generator cache hits. Tests, notices, packaging and installed checks still execute on hits.

All eight published files were downloaded with GitHub authentication. All three archive SHA-256 values and metadata were verified. The actual downloaded ARM archive passed installation, embedded dashboard, private Surfpool, retained Journal, tamper, reinstall and live-update checks. Temporary installations and Labs were cleaned; original user Labs/backends were not touched.

The user made the repository public and resumed verification on 2026-11-08. Anonymous public curl installation passed in a temporary prefix without GitHub credentials. Installed Simulator and two Surfpool projects, embedded dashboard HTTP, retained Journals, tamper rejection and same-version reinstall passed. Anonymous SHA-256 checks passed for all three platform archives and all eight public assets were present. Temporary test Labs/installations were cleaned; the user's installation and Labs were not changed. README's public-installation warning is removed and release notes document the working install command. Task 41 is complete.

The `v0.1.0` archive remains the previously verified `0ca02a0` build. Yellow branding and terminal-mark changes are available in newer `main` source builds; the existing version/tag/assets were not replaced.

MIT and bundled pinned Surfpool are implemented by [task 40](todo.md#40-curl-installation-and-bundled-surfpool) and ADR-0020. Runtime data remains project-local. Homebrew remains deferred.

Earlier documentation/README work is recorded in [tasks 38–39](todo.md#38-documentation-organization).

## Yellow palette and README refresh

The user selected a yellow signal palette. Dashboard accent is `#f3d65b`; related backgrounds are centralized in its CSS tokens. The terminal mark uses a 10×8 Braille dot grid to retain four raised bands in 5×2 text cells. Runtime details use plain labels/hints with the Lab name and endpoint URLs in the mark's yellow; other secondary hints retain yellow. The local release executable is rebuilt. Palette and terminal changes are committed and pushed as `722b8a7`. Frontend build/64 tests and the updated terminal PTY suite (5), fmt and Clippy passed at their respective change scopes.
README now uses the three original screenshots supplied by the user from the Desktop: foreground Simulator terminal, retained Surfpool Workspace and its Observation detail. The PNGs were copied byte-for-byte without redrawing, cropping or recoloring. The SVG header uses the yellow accent. Captions describe one submission, twenty-second withholding, two finalized status replies and one transaction lookup; no application business outcome is inferred. The README refresh did not replace the existing `v0.1.0` release. The user subsequently made the repository public.

## v0.1.1 release in progress

The user requested that curl installation include the accepted yellow dashboard and terminal. Native/frontend versions are bumped to `0.1.1`; dependency versions remain unchanged. The tagged native pipeline will repeat all build/test/package/install gates and publish only after the complete matrix passes. The target-specific Rust cache gains a broader restore prefix across patch-version changes; Cargo still checks source/compiler/features/profile fingerprints. v0.1.0 is preserved.
The local frontend and release executable build and actionlint pass. Public verification of v0.1.0 is complete. Publication and latest curl/update/color verification of v0.1.1 remain pending; do not claim the newer version is available until those checks pass.

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
No original user Lab or backend files were changed. Runtime fixes and CI acceleration are pushed; the tagged release is published. Public installation verification is complete; continue from the user's next request.
