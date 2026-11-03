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
Run the frontend build before Cargo in a fresh checkout. Rebuild it after frontend source changes.

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

## Dashboard build and launch

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

The dashboard exposes live summary, bounded Timeline pages, pinned evidence, filters, and explicit payload previews.
Case controls use the existing authenticated arm/disarm handlers.
Check collection starts only on an explicit action.
Collection status and cancellation use the Lab-owned collection lifecycle.
Evaluation always sends the selected definition version and displayed snapshot.
The browser displays the core Verdict; it has no evaluator.
Unsafe JSON integer facts are displayed with their exact decimal representation as strings.
The transport stops reading after 256 KiB.

For retained evidence, run `ronchi dashboard --serve --lab-id <immutable-id>`.
This foreground process binds an OS-selected loopback port and prints one readiness record.
With `--json`, readiness is a version-1 envelope with `read_only:true` and a ticket launch URL.
The process then waits until Ctrl-C or stdout pipe closure.
It cancels readers and drains its owned connections within three seconds before forcing closure.
It never starts a simulator, writer, source application, or relayer.
It does not modify the Lab manifest.
Its routes expose queries and evaluation only.
It rejects live Labs; use the ordinary finite dashboard launch command for those.
The viewer remains bound to the resolved immutable Lab even if another Lab reuses its name.
A stopped header labels the old RPC address as recorded, not live.

`up` includes an optional top-level `dashboard` launch object.
The Demo includes `dashboard_launch` for a ready Lab and `dashboard_argv` for recovery or retained viewing.
Ticket-capacity or launch transport failure does not undo a successful Lab startup or Demo.
Run the exact-ID dashboard command for a fresh link if launch metadata is absent.
No command opens a browser automatically.
Closing the browser leaves the persistent Lab running.

### Pinned dashboard links and Findings pages

Summary exposes at most five Findings.
`Next Findings` uses `investigate.findings` with five-item pages and an opaque cursor.
The first continuation resolves the same first page to acquire its cursor.
It then requests one next page.
Later clicks request one page each; they never fetch all Findings.
Changing scope or snapshot discards the incompatible continuation.
A failed continuation keeps the previous valid page visible.

Copied links retain the immutable Lab ID, snapshot, semantic scope, and selected event or Finding.
They omit the bootstrap ticket, credentials, and cursor.
Bootstrap removes only the ticket fragment.
Integer coordinates retain the full signed SQLite integer range without conversion through an imprecise JavaScript number.
Invalid or unavailable references display an error rather than selecting the latest snapshot.
To move a bookmark to a new live session or retained endpoint, pass its query to the launch command:

```bash
ronchi dashboard --lab-id <id> --selection 'snapshot=42&event=7'
ronchi dashboard --serve --lab-id <id> --selection 'snapshot=42&event=7'
```

The CLI rejects credential, cursor, unknown, and oversized selection fields before requesting a ticket or binding a viewer.
A live endpoint does not survive Lab stop.
The interface shows exact-ID retained viewer instructions for its current selection.
