# Ronchi runtime contracts

These questions apply to affected parts of the design.
They do not prescribe a new module tree or finalize unresolved product behavior.
Use the accepted ADRs as authority, especially ADR-0010 through ADR-0017 and ADR-0019.

## One executable, distinct responsibilities

Keep RPC forwarding and fault handling separate from control commands, evidence queries, and persistence.
These responsibilities may share a process and Tokio runtime.
Avoid putting dashboard queries, CLI formatting, or blocking storage work on the RPC path.
Keep transport DTOs and user-facing formatting outside Case decision logic when practical.
Do not add a generic plugin system; Custom Cases compose maintained primitives.

## State and ordering

Identify the owner of active Case state, simulator state, request correlation, and Journal writes.
Specify what happens when a Case changes while requests are in flight.
Distinguish request observation, fault application, upstream outcome, client-visible response, and persisted evidence.
Record the ordering actually known; timestamps alone do not prove causality.
Keep the persistent Lab and continuing Cases independent of application test-run lifetimes.

## Network effects

In simulator mode, enforce ADR-0012's allowlisted read-through and upstream write firewall at the final forwarding boundary.
Account for batches and requests the tool cannot classify.
An unknown method must not acquire upstream write permission through a fallback path.
In Surfpool mode, route reads and writes through Surfpool so they observe its account state.
Do not mix simulated transaction outcomes with claims of observed economic effects.
Do not put Ronchi between production applications and production RPC.

## Capacity and evidence

Identify limits for connections, concurrent work, request bodies, buffered responses, queued evidence, and query results.
Specify whether saturation waits, rejects work, or degrades a documented capability.
Bound retained bytes as well as item counts when items can contain large payloads.
Do not silently discard evidence or claim that an enqueued event is already durable.
Keep the Continuous Lab Journal on disk; do not retain its entire history in memory.
Separate runtime diagnostics from product evidence and its query contract.
Expose missing observations as uncertainty instead of inferring backend business state.

## Startup and readiness

Define which resources must be available before the Lab reports readiness.
On partial startup failure, release resources the process already acquired.
Keep optional Surfpool startup within Surfpool mode.
Track the managed child process, readiness failure, unexpected exit, and cleanup.
Use the project-local workspace in ADR-0014 for runtime artifacts, including Surfpool state and logs. ADR-0020 permits shared installed Ronchi/Surfpool executables; it does not permit global Lab data or runtime caches.
Do not introduce account registration, cloud services, or a global application-data directory.

## Shutdown and failure

Define how admission stops, in-flight work settles, producers stop, and Journal writes finish.
Keep the writer alive while accepted producers can still submit evidence.
Wait for tasks and managed child processes according to their actual completion contract.
If shutdown has a deadline, state what remains incomplete after it expires.
A cancellation request or fixed delay is not proof that cleanup completed.
Distinguish an injected RPC failure from an internal failure of Ronchi itself.

## Verification

Choose focused checks for changed contracts: overload, partial startup, cancellation, writer failure, child exit, or forwarding policy.
Use fake upstreams or isolated local backends when they can demonstrate behavior.
Do not require mainnet writes, an application SDK, or a full backend project to verify Ronchi's internal contracts.
Real backend integration remains a separate product workflow.
Keep future resource measurements separate for the core and Surfpool mode.
Do not invent numerical acceptance limits while they remain deferred.
