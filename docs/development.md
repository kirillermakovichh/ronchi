# Development stack

Status: selected stack. The native CLI package is implemented; the task queue records runtime progress.
[ADR-0019](adr/0019-resource-efficient-rust-core.md) owns the native runtime decision.
This document owns the initial build tools and supporting technology choices.
The [development queue](../tasks/todo.md) owns implementation order.

## Rust application

Use Rust 1.99.0 stable with edition 2024.
The project [toolchain file](../rust-toolchain.toml) pins the compiler, rustfmt, and Clippy.
Start with one Cargo package containing a library and the `ronchi` binary.
Keep runtime modules in that package; introduce more crates only when a concrete dependency or reuse need warrants them.
Commit `Cargo.lock` when task 1 creates the package.
Resolve and lock compatible dependency versions when each slice first uses them.
An unresolved application lockfile is implementation work, not a missing stack decision.

| Area | Selected technology |
| --- | --- |
| Async runtime and cancellation | Tokio and `tokio-util` |
| HTTP JSON-RPC data path | Hyper, `hyper-util`, `http-body-util` |
| Outbound HTTPS | Rustls through `hyper-rustls` |
| Loopback control and dashboard serving | Axum |
| Journal and reference database | `rusqlite` with bundled SQLite; WAL and a dedicated writer |
| CLI | `clap` |
| Local MCP | Official `rmcp` |
| Typed JSON and configuration | `serde`, `serde_json`, `toml` |
| Custom Case parsing | `yaml-rust2` with the bounded YAML subset enforced by Ronchi validation |
| Typed errors and diagnostics | `thiserror`, `tracing`, `tracing-subscriber` |
| Embedded dashboard files | `include_bytes!` over built assets; generate an asset map during packaging |

Use modular Solana/Agave protocol crates behind the protocol module.
Verify and lock the required versions against the full accepted [format and lifetime contract](product/solana-rpc-contract.md#transaction-formats-and-encodings).
A dependency's unsupported existing format remains an implementation gap; it does not narrow product scope.
Keep Surfpool optional and pin its verified native artifact when its implementation slice starts.
Neither Solana CLI tools nor Surfpool is required to start the core package.

## Dashboard

Use React, TypeScript in strict mode, and Vite under `dashboard/`.
Use npm and commit `package-lock.json` when creating the dashboard package.
Use ordinary CSS, CSS custom properties, and SVG for the first interface.
Start with React state and the browser Fetch API; add a state or component library only for a demonstrated need.
Follow the accepted [visual direction](product/positioning-and-brand.md#visual-identity) and [interface contract](product/investigation-interfaces.md#first-dashboard).
Build static assets before embedding them into the Rust executable.
Node.js and npm are development/build tools; the delivered executable serves the embedded files.
Core-only development does not need to build dashboard assets before the dashboard slice exists.

## Verification tools

Use Rust's test harness and Tokio tests for core behavior.
Use temporary file-backed SQLite databases and actual subprocesses where persistence or ownership matters.
Use Vitest and Testing Library for dashboard logic and Playwright for focused browser flows.
Use the Cargo checks in the [task queue](../tasks/todo.md#working-rules).
Do not require Docker, a public RPC, a validator, or a faucet for initial core tests.

## Local prerequisite check

The development host has Rust 1.99.0, Cargo, rustfmt, Clippy, and an Apple C compiler/linker installed.
It also has Node.js 25.5.0 and npm 11.8.0 available for dashboard work.
These observations concern this host; they are not a public platform-support matrix or a dependency compatibility result.
The pinned compiler successfully built and ran a temporary edition-2024 program through the local linker.
Package initialization creates and verifies the application lockfiles.
The native package and application lockfile now exist.
Task 1 in the queue records the application build and process checks.

## Dashboard build (task 14, initial slice)

Build the frontend before every native build that must include changed assets:

```bash
npm --prefix dashboard ci
npm --prefix dashboard run typecheck
npm --prefix dashboard test -- --run
npm --prefix dashboard run build
cargo build --locked
npm --prefix dashboard exec -- playwright install chromium
npm --prefix dashboard run test:browser
```

The browser suite starts the real native executable in an empty temporary project.
`RONCHI_CHROMIUM=/absolute/path/to/chromium` selects an existing Chromium executable for that suite.
The default uses Playwright's matching installed Chromium.
Inspect screenshots under `dashboard/test-results/` after browser tests.
Do not commit these screenshots or reports.

Cargo's build script embeds only files from the completed `dashboard/dist` build.
It fails with a build instruction if `index.html` is absent.
It never runs npm or Vite.
The executable requires no frontend files or Node process at runtime.
Unknown static files return 404; API errors do not fall back to HTML.

For a ready Lab, run `ronchi dashboard --lab-id <immutable-id>`.
This finite command prints a launch URL and never opens a browser.
`--json` returns a version-1 envelope.
The fragment contains a single-use ticket valid for 60 seconds.
The browser removes that fragment before exchanging it for a 30-minute HttpOnly, SameSite=Strict cookie.
The Lab owns at most 16 unexpired tickets and 32 sessions.
Cookies have Lab- and port-specific names and Lab-specific paths.
Browser API requests require the actual Host and same-origin browser context.
Bearer control requests retain their existing authorization rules.
Do not share launch URLs as evidence links.
The dashboard's copy-link action omits tickets, credentials, and pagination cursors.

This initial slice exposes live summary, bounded Timeline pages, pinned evidence, filters, and explicit payload previews.
Case/Check UI controls, retained viewer, startup/Demo launch integration, and broader browser acceptance remain in task 14.
Closing the browser leaves the persistent Lab running.
