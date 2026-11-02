# Lifecycle and mutation commands

Ronchi operates a persistent Lab for an independently running backend.
Do not redesign the primary workflow as a one-shot test runner.
Use ADR-0014 through ADR-0017 for accepted workspace and lifecycle decisions.

## Command lifetimes

Distinguish finite queries and mutations from process owners and follow streams.
For a foreground Lab process, readiness and eventual exit are separate events.
For an attaching client, closing the client need not stop the Lab.
Define that behavior explicitly; do not infer it from a generic timeout or Ctrl-C handler.
Do not make a finite query inherit the lifetime of the Lab it inspects.
Bound connection and startup waits separately from an intentionally continuing session.
An agent must be able to start and inspect a Lab without keeping a tool call open indefinitely.
Choose the startup/attachment mechanism from the agreed UX; this skill does not require a particular daemonization design.

## Startup and stop

Validate local configuration before starting processes or opening upstream connections where possible.
Report the selected workspace, mode, endpoints, and readiness concisely.
Surface port conflicts, writer initialization failure, and missing optional Surfpool as actionable errors.
Do not require Surfpool for simulator mode.
Do not silently download executables, request funds, or mutate a developer's shell configuration during ordinary startup.
Keep persistent product and Surfpool artifacts under the selected project's `ronchi/` workspace.

When a command owns shutdown, stop new work and wait for required cleanup.
Report incomplete shutdown accurately rather than treating receipt of a signal as successful persistence.
For a follow stream, define whether cancellation only detaches the observer.
Use the same lifecycle semantics from human CLI, automation, and MCP.

## Repetition and uncertainty

For mutations, define repeated invocation as no-op, conflict, resume, or a new operation according to intent.
Do not make a Case restart silently harmless if it actually resets its state or creates another experimental interval.
When a client times out, the Lab may already have applied its command.
Provide operation identifiers or a state inspection path where needed to resolve that uncertainty.
Use deduplication or expected-state checks only where they protect a concrete retry or concurrency contract.
Avoid automatic retries of mutations with unknown completion.

## Reset and cleanup

Keep ordinary inspection free of confirmation prompts.
For destructive reset or artifact deletion, identify the exact workspace and affected Lab state.
Check whether files or processes are still in use.
Offer explicit noninteractive intent and a preview when it materially reduces deletion risk.
Do not impose confirmation flags on every harmless state change.
Do not delete outside the selected workspace or remove an unrelated process because a stored PID matches.
Preserve enough diagnostics to explain failures without dumping the whole Journal.

## Verification choices

For affected behavior, check relevant process boundaries: readiness, signals, exit status, pipe closure, repeated mutations, and partial startup.
Use isolated temporary workspaces so verification does not alter an active developer Lab.
Test TTY-specific behavior only when it is part of the changed contract.
Do not add authentication or global credential storage to satisfy an upstream checklist.
