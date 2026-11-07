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

The current binary release is [v0.1.1](https://github.com/kirillermakovichh/ronchi/releases/tag/v0.1.1) at `662e990`. It includes the yellow dashboard and terminal. [Task 43](todo.md#43-publish-the-yellow-ui-as-v011) records its full native matrix and anonymous installation checks.

The first release, [v0.1.0](https://github.com/kirillermakovichh/ronchi/releases/tag/v0.1.0), remains unchanged at `0ca02a0`.
Manual matrix `42404486999` and tagged release `42406451668` passed on macOS ARM, macOS Intel and Linux x86_64. The Rust suite passed 290 tests; all release/installer gates remained enabled. [Task 41](todo.md#41-first-native-release) owns corrective commits, exact checks and remaining acceptance.

The requested CI caches are implemented and exercised. Observed cold/warm job times: ARM 8m57s → 7m51s, Linux 15m10s → 11m39s, Intel 22m39s → 15m49s. The ARM log confirms exact Cargo-download, native-build and license-generator cache hits. Tests, notices, packaging and installed checks still execute on hits.

All eight published files were downloaded with GitHub authentication. All three archive SHA-256 values and metadata were verified. The actual downloaded ARM archive passed installation, embedded dashboard, private Surfpool, retained Journal, tamper, reinstall and live-update checks. Temporary installations and Labs were cleaned; original user Labs/backends were not touched.

The user made the repository public and resumed verification on 2026-11-08. Anonymous public curl installation passed in a temporary prefix without GitHub credentials. Installed Simulator and two Surfpool projects, embedded dashboard HTTP, retained Journals, tamper rejection and same-version reinstall passed. Anonymous SHA-256 checks passed for all three platform archives and all eight public assets were present. Temporary test Labs/installations were cleaned; the user's installation and Labs were not changed. README's public-installation warning is removed and release notes document the working install command. Task 41 is complete.

The `v0.1.0` archive remains the previously verified `0ca02a0` build. Yellow branding and terminal-mark changes are distributed as v0.1.1; the v0.1.0 tag/assets were not replaced.

MIT and bundled pinned Surfpool are implemented by [task 40](todo.md#40-curl-installation-and-bundled-surfpool) and ADR-0020. Runtime data remains project-local. Homebrew remains deferred.

Earlier documentation/README work is recorded in [tasks 38–39](todo.md#38-documentation-organization).

## Yellow palette and README refresh

The user selected a yellow signal palette. Dashboard accent is `#f3d65b`; related backgrounds are centralized in its CSS tokens. The terminal mark uses a 10×8 Braille dot grid to retain four raised bands in 5×2 text cells. Runtime details use plain labels/hints with the Lab name and endpoint URLs in the mark's yellow; other secondary hints retain yellow. The local release executable is rebuilt. Palette and terminal changes are committed and pushed as `722b8a7`. Frontend build/64 tests and the updated terminal PTY suite (5), fmt and Clippy passed at their respective change scopes.
README now uses the three original screenshots supplied by the user from the Desktop: foreground Simulator terminal, retained Surfpool Workspace and its Observation detail. The PNGs were copied byte-for-byte without redrawing, cropping or recoloring. The SVG header uses the yellow accent. Captions describe one submission, twenty-second withholding, two finalized status replies and one transaction lookup; no application business outcome is inferred. The README refresh did not replace the existing `v0.1.0` release. The user subsequently made the repository public.

## v0.1.1 published and verified

Tagged workflow `42417380310` passed the complete macOS ARM, macOS Intel and Linux x86_64 matrix and published v0.1.1 at `662e990`. Dependency versions and every build/test/package/install gate remain unchanged. The target-scoped cache fallback reuses compatible dependencies across patch versions; Cargo validates fingerprints.

Anonymous public downloads matched all three SHA-256 files and the eight-asset inventory. The public latest installer selected v0.1.1 in an isolated prefix. Actual installed terminal output contained the compact mark, version and yellow 221 color. Served embedded CSS contained `--accent:#f3d65b`. An isolated v0.1.0 Surfpool Lab stayed ready with the same owned-child identity after installation of v0.1.1. The new CLI stopped it normally. Simulator/two Surfpool projects, embedded dashboard HTTP, retained Journals, tamper rejection, same-version reinstall and Ctrl-C checks passed. All temporary Labs stopped; user installations and Labs were unchanged. Intel/Linux runtime claims remain bounded to native CI.

Release notes and README now point users to v0.1.1. Homebrew remains deferred. Temporary verification scripts, logs and downloads stay outside the repository.

## Automatic PATH installation

The user accepted automatic shell-profile configuration and one command that activates PATH immediately. The maintained installer now configures zsh/Bash/POSIX profiles with a guarded, idempotent block. It preserves existing contents and valid dotfile symlinks. `--no-modify-path` opts out. README uses the installer from `main` and appends the parent-shell export; binaries still come from the verified native Releases.

[Task 44](todo.md#44-automatic-shell-path-setup) records 29 fixture tests and the actual published macOS ARM v0.1.1 archive smoke. New terminals, reinstall, retained Journals, tamper rejection and live Surfpool update passed with temporary shell homes and projects. User profiles and Labs were untouched. Public source publication/command verification is the remaining step. No new native version or tag is needed for the installer change.

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
