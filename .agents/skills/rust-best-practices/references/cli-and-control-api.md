# Rust CLI and control API implementation

These are locally written implementation notes for Ronchi.
Use `ronchi-cli-design` for the command's product behavior and output contract.
See [sources.md](sources.md) for primary documentation and reviewed design references.

## Process entrypoints and clap

Keep parsing, configuration resolution, core operations, and rendering separate enough to verify independently.
Use typed clap definitions for accepted arguments, conflicts, and value ranges.
Reuse those definitions for help or introspection where practical.
Do not maintain a second hand-written command schema without a synchronization strategy.
Keep pure help and schema paths independent of Lab startup, upstream access, and unnecessary runtime initialization.

If structured errors are promised for invalid arguments, handle clap errors at the rendering interface.
`Parser::parse` can exit before a custom error renderer runs; choose fallible parsing where the contract requires interception.
Preserve successful help/version behavior and their documented streams and statuses.
Check formatting mode selection for parse failures rather than promising JSON only after parsing succeeds.

Use `PathBuf` and `OsString` where paths or process arguments need not be UTF-8.
Start child processes with argument arrays, not a shell string assembled from user input.
Validate relevant paths, identifiers, and limits before performing effects.

## Configuration and files

Follow the project's explicit precedence rules for flags, environment, and files.
Do not invent a precedence order or new config framework for a narrow change.
Resolve the selected project workspace once and propagate it explicitly.
Place persistent product state in that workspace's `ronchi/` directory.
Do not import XDG, application-support, global cache, or credential-store defaults from generic CLI examples.
Initialize writes and migrations through their owning module.
Use atomic replacement or transactions where partially written configuration or state would break recovery.

## HTTP and shared operations

Keep direct Hyper/hyper-util transport on the RPC path as specified by ADR-0019.
Use Axum for the low-volume loopback control API.
Do not copy a `0.0.0.0` control bind from a generic server example.
Reuse shared core operations and query models from CLI, control API, and MCP.
Keep protocol-specific rendering out of those operations.
Map typed errors into each transport's documented response rather than exposing Rust debug strings.
For Journal queries, apply filters, limits, ordering, and projections before loading large payloads.

## Instrumentation and process ownership

Install diagnostic subscribers at the application entrypoint.
Keep stdout available for command results in machine mode.
Avoid recording full RPC payloads, tokens, or credentials through automatic debug instrumentation.
Use explicit correlation identifiers and selected fields.
Keep product evidence and diagnostic tracing distinct; tracing output alone is not the Journal contract.

Handle readiness, partial startup cleanup, signals, and child exit through the runtime owner.
Do not let each CLI command or HTTP handler create a competing background writer or global runtime.
Follow the async reference when tasks, cancellation, or shutdown are involved.

## Verification

Use process-level checks when parser exit behavior, streams, environment precedence, or signals are part of the change.
Use isolated workspaces and fake or local endpoints.
Test actual parsing and serialization contracts instead of matching incidental debug text.
