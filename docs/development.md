# Development

This document owns build tools, source layout and verification commands.
[ADR-0019](adr/0019-resource-efficient-rust-core.md) records the native architecture.
Product behavior belongs to the [topic contracts](README.md), not this build guide.

## Toolchain and stack

Rust 1.99.0 stable, edition 2024, rustfmt and Clippy are pinned in [rust-toolchain.toml](../rust-toolchain.toml).
Use a platform C compiler/linker for the native build and bundled SQLite.
The project is one Cargo package with a library and the `ronchi` executable.
Commit and preserve the existing Cargo and npm lockfiles when changing dependencies.

| Area | Technology |
| --- | --- |
| Async runtime and cancellation | Tokio, tokio-util |
| RPC transport | Hyper, hyper-util, http-body-util |
| Outbound HTTPS | hyper-rustls / Rustls |
| Loopback control and dashboard server | Axum |
| Journal and reference storage | rusqlite with bundled SQLite; WAL and dedicated owners |
| CLI and foreground panel | clap, console, indicatif |
| Local MCP | Official rmcp 3.8.0 |
| JSON, configuration, errors | serde, serde_json, toml, schemars, thiserror |
| Custom Cases | yaml-rust2 with Ronchi's bounded subset |
| Solana formats and signatures | Pinned modular protocol crates behind src/protocol |
| Dashboard | Solid, strict TypeScript, Vite, ordinary CSS |
| Lists and layout | TanStack Virtual, CSS Grid, Solid DnD |
| Frontend verification | Vitest, Testing Library, Playwright / Chromium |

Node.js and npm are build/test tools. The executable embeds compiled assets with `include_bytes!`.
A released Lab needs no Node process, frontend directory, Docker, Solana CLI or validator.
Surfpool is a separate native executable included in native distributions. Source builds can install its pinned archive explicitly. It runs only in Surfpool mode.
Keep Rust dependencies and protocol versions pinned against the [RPC compatibility contract](product/solana-rpc-contract.md#transaction-formats-and-encodings).
Do not add workers, Wasm, a second renderer, a grid engine or more crates without a demonstrated need.
No comparative frontend benchmark or public platform-support matrix has been established.

## Build and run

From the repository root:

```bash
npm --prefix dashboard ci
npm --prefix dashboard run build
cargo build --locked
./target/debug/ronchi up
```

Build the frontend before each native build that must include UI changes.
Cargo reads only the completed `dashboard/dist` tree; it never runs npm or Vite.
A missing `index.html` produces a build instruction. Unknown asset paths return 404.

```bash
cargo build --release --locked
./target/release/ronchi up
```

The running process keeps its existing binary and assets.
For source builds, stop the Lab before replacing its executable. Lifecycle authentication checks the recorded executable identity; replacing that file while the owner runs can make authentication fail.
Start a new Lab to load a new build. The earlier Journal remains retained.
See [workflows](product/workflows.md) for backend connection and [dashboard access](product/dashboard.md#access-and-retained-viewing) for live and retained links.

## Verification

Run checks that cover the changed behavior. Use temporary projects and exact Lab IDs for lifecycle tests.
Never stop or overwrite user Labs to make a test pass.
Persistence and recovery require real file-backed databases or processes; compilation is not durability evidence.

```bash
cargo fmt --check
cargo check --locked
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
npm --prefix dashboard run typecheck
npm --prefix dashboard test -- --run
npm --prefix dashboard run format:check
npm --prefix dashboard run build
cargo build --locked
npm --prefix dashboard run test:browser
```

Install Playwright's matching Chromium once with `npm --prefix dashboard exec -- playwright install chromium`.
`RONCHI_CHROMIUM=/absolute/path/to/chromium` selects an existing executable.
`RONCHI_BROWSER_BINARY=/absolute/path/to/ronchi` selects an already built native executable for browser tests; the default is `target/debug/ronchi`.
For a release-only local check, build the embedded release and run `RONCHI_BROWSER_BINARY="$PWD/target/release/ronchi" npm --prefix dashboard run test:browser`.
Browser tests launch the real binary in disposable projects. Review screenshots in `dashboard/test-results` after visual changes.
Desktop/laptop layouts are primary; narrow layout receives basic handling.
Test screenshots, reports, research scratch files and local runtime artifacts are not committed.
Final product media referenced by the root README live in `docs/assets/readme/`; these curated assets are versioned.

Browser tests cover live episodes, actual retained replies, retained viewing, layout editing and reconnect without mutation replay.
Unit/model tests cover exact integers, transport ownership and all maintained Observation families.
Controlled DTO tests do not establish every native failure lifecycle. Recorded historical checks are not claims about unrun current tests.
The Rust dashboard suite also checks viewer stdout closure, released listeners and unchanged Journal bytes.
The [normal-down observation](../tasks/history/unconfirmed-down-observation.md) is unconfirmed; preserve its original result if it recurs.
No hidden retry or speculative lifecycle fix is part of that record.

### Optional native Surfpool verification

The [lifecycle contract](product/lab-lifecycle.md#managed-native-surfpool-implementation) owns the pinned artifact, hashes, flags and fixture limits.
Native fixture-dependent tests require an explicit platform archive:

```bash
RONCHI_TEST_SURFPOOL_ARCHIVE=/absolute/path/surfpool-darwin-arm64.tar.gz cargo test --locked --test surfpool_mode -- --test-threads=1
RONCHI_TEST_SURFPOOL_ARCHIVE=/absolute/path/surfpool-darwin-arm64.tar.gz npm --prefix dashboard run test:browser
```

Without that input, conditional native tests may skip their execution path. Report that limitation.
Use disposable projects and explicit local snapshots. No public RPC, faucet or Solana CLI is required.
The archive is test input and is not committed or bundled.
The [measurement record](research/resource-measurement-harness.md#measurement-record) owns measured resource observations and their limits.

## Source ownership

| Location | Responsibility |
| --- | --- |
| src/lib.rs, src/config.rs | CLI dispatch, offline schemas, project configuration |
| src/lab, src/control.rs, src/network.rs | Lab ownership, authenticated controls, native listeners |
| src/rpc, src/simulator, src/protocol | Request registration, simulator state and protocol handling |
| src/backend, src/upstream.rs | Managed Surfpool and allowlisted Read-through |
| src/cases | Continuing policies and Custom Case validation |
| src/journal, src/query.rs | Durable evidence, bounded reads and deterministic Findings |
| src/dashboard | Browser sessions, WebSocket operations and retained viewer |
| src/reference, src/demo | Bundled relayer and bounded Demo orchestration |
| src/mcp.rs | Local stdio adapter |
| dashboard/src/app | Composition, routing, Lab context and global filters |
| dashboard/src/shared | Protocol, transport, UI primitives and common styles |
| dashboard/src/features | Feature-owned views, models and subscriptions |
| dashboard/tests | Native browser fixtures and scenarios |

Solid views own their active subscriptions and cleanup. Pure Observation models group retained evidence and keep unknowns explicit.
`features/observations/replies.ts` resolves bounded retained replies; Inspector owns manual payload continuation.
`src/journal/traffic.rs` owns commit-derived rate counters; `RpcRates.tsx` owns their browser-local clock.
See the [dashboard contract](product/dashboard.md) for UI behavior and bounds.
Tests live beside frontend models or in Rust integration targets. Keep modules navigable without a parallel legacy renderer.

## Working rules

Read `AGENTS.md`, the relevant product contract and actual Git status before editing.
Use project skills and the [Definition of Done](../.agents/references/definition-of-done.md).
Implement one reviewable behavior slice at a time. Record applicable checks in [tasks/todo.md](../tasks/todo.md).
A test fixture or unsupported dependency does not silently narrow the agreed protocol scope.
Preserve pre-existing unrelated changes. Do not weaken tests or hide failed checks to obtain a green result.
Keep the current session state in [implementation-progress.md](../tasks/implementation-progress.md); do not duplicate product contracts there.
A missing external backend is an explicit validation gap, not permission to substitute the bundled relayer.

### Commits

Commit only when the current request or active assignment authorizes it. Historical runbooks grant no standing authorization.
For an authorized implementation run, use one verified behavior slice per commit with its tests and documentation.
Inspect `git diff`, stage only intended paths, inspect `git diff --cached`, and run `git diff --cached --check`.
Use concise English subjects such as `feat(rpc): preserve transaction identity` or `fix(dashboard): retain selected evidence`.
Keep lockfiles, required source fixtures and third-party notices. Exclude secrets, `.env`, Lab state, build output and dependency directories.
After committing, inspect `git status --short` and `git log -1 --oneline`.
Do not amend, rewrite history, discard changes, add remotes or publish without authorization.
If Git metadata is sandboxed, use the permission mechanism; do not edit Git metadata manually.

## Native distribution

Ronchi's source is MIT-licensed. The distribution carries the pinned Surfpool under Apache-2.0 and includes third-party notices and covered dependency sources.
[ADR-0020](adr/0020-installed-native-tool-distribution.md) owns the separation between shared installed executables and project-local runtime data.
Homebrew is deferred. No self-update command, automatic runtime download or public release is implied by having release scripts.

`release/surfpool.json` is the shared upstream version/target/hash catalog. `release/about.toml` selects accepted dependency licenses; it does not replace their terms.
The curl installer must have the same Surfpool version pin. Packaging rejects a mismatch.

Native distribution targets:

| Target | Initial baseline |
| --- | --- |
| macOS Apple Silicon | macOS 15.5+ |
| macOS Intel | macOS 15.5+ |
| Linux x86_64 | Ubuntu 22.04-compatible glibc 2.35+ with libgcc_s |

Windows and musl/Alpine are outside this distribution. The pinned ARM Surfpool declares macOS 15.5 in its load commands. Bounded local smoke checks also passed on macOS 15.2, but do not establish full support below that upstream baseline.

The workflow builds on each native runner and tests the actual installed archive, including Simulator and Surfpool. The current [v0.1.1 release](https://github.com/kirillermakovichh/ronchi/releases/tag/v0.1.1) passed the full matrix. Anonymous archive checksums and macOS ARM installation, embedded colors and live upgrade were verified in [task 43](../tasks/todo.md#43-publish-the-yellow-ui-as-v011). [Task 41](../tasks/todo.md#41-first-native-release) retains the first release's CI fixes and publication evidence. [Task 44](../tasks/todo.md#44-automatic-shell-path-setup) records the independently updated public installer and automatic PATH checks. Intel/Linux runtime evidence comes from native CI, not execution on this macOS host. Runner checks do not establish every OS version at the declared baseline.
A cross-compiled file or a successful checksum is not a native runtime check.

Build the frontend and release executable as above, then install cargo-about 0.9.3 as a release-only build tool. It is not required on end-user machines.

```bash
cargo install cargo-about --version 0.9.3 --locked --features cli
python3 scripts/release-notices.py --target aarch64-apple-darwin
python3 scripts/package-release.py --target aarch64-apple-darwin \
  --binary target/release/ronchi \
  --notices target/distribution/THIRD-PARTY-LICENSES.html \
  --surfpool-notices target/distribution/SURFPOOL-THIRD-PARTY-LICENSES.html
python3 scripts/test-installer.py
python3 scripts/test-release.py --directory target/distribution
```

These Python scripts require Python 3.12 on release/test machines only. `--surfpool-archive PATH` lets packaging reuse an existing official archive after checking the catalog hash.
Packaging downloads official pinned Surfpool artifacts when no local archive is given. Notice generation verifies the pinned source archive, scans the Rust dependencies and Ronchi dashboard production dependencies, and includes the upstream source plus exact Cargo.lock-verified MPL-covered crate sources.
No placeholder notices or skipped license failures are allowed in a distributable package.

Native CI caches Cargo downloads, platform-specific debug/release build outputs and the pinned `cargo-about` executable separately. Rust cache keys include the toolchain, manifests, workflow and source revision; compatible earlier builds supply dependency reuse. Cargo still runs every test, lint and release-build command. License generation, packaging and installed-artifact checks also run on cache hits. Completed caches can be saved when a later check fails; cache save does not turn that job into success. CI disables incremental compilation and keeps line-table debug information to limit cache size; the shipped release profile is unchanged. The first run fills the caches and is not evidence of warm-cache speed. [Task 41](../tasks/todo.md#41-first-native-release) records the first release's same-revision cold/warm comparison.

`.github/workflows/release.yml` builds and tests on pull requests or manual dispatch without publishing. A matching `v<VERSION>` tag triggers publication only after every build/install job passes. It refuses to overwrite an existing release. Prerelease tags remain prereleases.
Publishing a tag/release is a separate authorized action. The current implementation does not publish or push by itself.

### Curl installation layout

The default prefix is `~/.local`; `--prefix /absolute/path` or `RONCHI_INSTALL_PREFIX` selects another prefix. Prefixes cannot contain colons or newlines. The installer uses curl, tar and sha256sum or shasum; it never invokes sudo, Cargo, npm or Python.
The README downloads the maintained installer from `main` through `raw.githubusercontent.com`. Installer fixes can ship independently of native binaries. Existing release-attached installers remain the snapshots published with those releases. The maintained installer downloads native archives from GitHub Releases. `--version VERSION` selects a fixed binary release; otherwise `release-version.txt` from the latest release selects it. `RONCHI_RELEASE_BASE_URL` supports an explicit HTTPS mirror or a loopback HTTP fixture.
Archives and their SHA-256 files come from the release host. Checksums detect corruption; they do not authenticate a compromised release host.

```text
<prefix>/bin/ronchi -> ../lib/ronchi/releases/<version-target-content>/bin/ronchi
<prefix>/lib/ronchi/releases/<version-target-content>/
  bin/ronchi
  libexec/surfpool/1.7.0/{surfpool,artifact.tar.gz}
  LICENSE
  licenses/{SURFPOOL-LICENSE,THIRD-PARTY-LICENSES.html,
            SURFPOOL-THIRD-PARTY-LICENSES.html,SURFPOOL-SOURCES.tar.gz}
  release.json
```

The actual symlink target is absolute. The content suffix permits a rebuilt version without changing an executable already used by a live Lab.
The installer validates checksums and archive paths/types, stages both executables, verifies their versions and switches the symlink only after success. Same-version reinstall verifies existing files instead of rewriting them.
A per-prefix lock rejects concurrent installers; failures clean staging files and leave the active symlink unchanged. After an uncatchable kill, remove a stale `.ronchi-install-lock` only after confirming no installer is running.
Existing unrelated executables and symlinks are not replaced.

By default, the installer appends a guarded `# Ronchi` PATH block to the selected shell configuration. Existing contents and valid dotfile symlinks are preserved. Reinstalling does not append the same block again; sourcing it twice does not duplicate the PATH entry. zsh uses `${ZDOTDIR:-$HOME}/.zshrc`. Bash uses `~/.bashrc` and the first existing login profile among `~/.bash_profile`, `~/.bash_login` and `~/.profile`; it creates `~/.profile` if none exists. POSIX login shells use `~/.profile`. Unknown shells and unwritable profiles receive a warning and manual PATH instructions; binary installation still succeeds. `--no-modify-path` disables profile changes.

The README command ends with `&& export PATH="$HOME/.local/bin:$PATH"` to enable the default installation in the current zsh/bash terminal. The installer subprocess cannot change its parent shell's environment. When invoking `install.sh` directly or selecting a custom prefix, use a new terminal or export that prefix's `bin` directory explicitly.
Updating does not stop Labs or migrate Journals. Old release directories remain for active processes; remove old versions only after their Labs stop.
To uninstall, stop Labs started by that installation. Remove its `bin/ronchi` symlink. Remove only its `lib/ronchi` release tree. Remove its `# Ronchi` PATH block from the configured shell files. Keep each project's `ronchi/` to retain investigation history.
