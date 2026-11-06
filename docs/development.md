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
Surfpool is an optional separately managed native executable.
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

The running process keeps its existing binary and assets. Restart the selected Lab to load a new build.
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
Browser tests launch the real binary in disposable projects. Review screenshots in `dashboard/test-results` after visual changes.
Desktop/laptop layouts are primary; narrow layout receives basic handling.
Screenshots, reports, research scratch files and local runtime artifacts are not committed.

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
