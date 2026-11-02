# Persistent Lab lifecycle

Status: accepted initial product contract, pending implementation and validation.
This document refines [ADR-0013](../adr/0013-managed-surfpool-execution-mode.md), [ADR-0014](../adr/0014-project-local-workspace.md), [ADR-0015](../adr/0015-persistent-lab-first.md), and [ADR-0016](../adr/0016-continuous-lab-journal.md).
The native stack remains defined by [ADR-0019](../adr/0019-resource-efficient-rust-core.md).

## Ownership and identity

`ronchi up` starts a background Lab and waits for readiness.
The Lab survives the terminal, dashboard, CLI command, and MCP client that connected to it.
Ronchi owns its daemon, listeners, Journal writer, and managed Surfpool child.
The System Under Test remains independently owned.
Ronchi never stops the backend, resets its database, or cancels its external Workload.

The default Lab name is `default`.
`--lab <name>` selects a project-local name and permits explicitly isolated Labs in one project.
Each successful start after a stop creates a new immutable `lab_id` and a new Lab Journal.
A name can refer to successive Labs; a `lab_id` is never reused.
Evidence references use `lab_id` and Journal sequence numbers.
The current Lab is the live Lab assigned to that name.
If no Lab is live, queries by name select its latest stopped Lab and disclose `selection_basis: latest`.
Queries against a live Lab disclose `selection_basis: current`.
`--lab-id <id>` selects a specific retained Lab for historical queries.
Mutating operations require the current live Lab and never fall back to history.

## Project and configuration discovery

Resolve the project root in this order:

1. Use the explicit `--project <directory>`.
2. Use the nearest ancestor containing `ronchi/config.toml`.
3. Use the enclosing Git worktree root.
4. Use the current directory.

Canonicalize the selected root before locking or discovery.
All persistent product files stay inside `<project-root>/ronchi/`.
There is no global Lab registry, tool cache, or application-support directory.
The first version has no independent workspace-location override.
The project configuration is `ronchi/config.toml` with an explicit schema version.
Resolve settings as explicit command flags, supported `RONCHI_*` environment variables, configuration, then defaults.
Reject unknown configuration keys, invalid values, and conflicting execution authorities before starting resources.
Relative configuration paths resolve against the project root.

Configuration covers the execution mode, Read Upstream or Surfpool datasource, listeners, initial Case, and bounded runtime limits.
Record the redacted effective configuration and component versions in the Lab manifest and Journal.
Execution mode, upstream authority, listener addresses, and persistence policy are fixed for one Lab lifetime.
Applying changed startup configuration requires a new Lab.
Case changes use `case arm` and `case disarm` without restarting it.

## Discovery, listeners, and local control

Store each Lab under `ronchi/labs/<lab_id>/`.
Keep the name-to-current/latest mapping and per-name lock inside `ronchi/`.
The manifest records identity, process birth identity, effective endpoints, lifecycle state, and owned child identity.
An operating-system lock serializes start, stop, and recovery for one name.
PID files and lock-file existence alone do not establish process ownership or liveness.
Publish manifest changes atomically.
Authenticate the discovered daemon and compare its `lab_id` before attaching.

The initial host workflow binds RPC to `127.0.0.1:8899` and control plus dashboard to `127.0.0.1:8900`.
Initial listeners accept loopback bindings only.
If a requested port is occupied, fail with its address and a concrete port-override instruction.
Never terminate the occupying process or silently choose a different port.
An explicit port value of `0` requests an operating-system-assigned port, which the manifest and command result report.
Additional Lab names require free explicit ports or port `0`.
Managed Surfpool uses private dynamically allocated loopback ports.
If its bind loses a port race, fail startup and clean up the owned child.

The control API requires a per-Lab random bearer token stored in an owner-readable workspace file.
Do not put that token in a URL, Journal, or diagnostic log.
The dashboard exchanges a short-lived, single-use bootstrap ticket for an HttpOnly, SameSite=Strict session cookie.
Validate the dashboard Host and Origin against its actual loopback endpoint.
Permit no cross-origin control access.
The application-facing RPC endpoint needs no control token.
Local filesystem access by the same operating-system user remains inside the trust boundary.

## Startup and readiness

Before allocating listeners, validate configuration, Case capabilities, workspace access, and available Journal storage.
Allocate `lab_id`, acquire the name lock, and create the startup manifest.
Open a new Journal and start its dedicated writer before accepting application RPC requests.
Simulator mode creates clean Simulated Chain State and starts its clock.
It never restores an old transaction lifecycle or old response-loss identity set.

In Surfpool mode, resolve its native project configuration from the selected project root.
Use a pinned compatible native binary from `ronchi/tools/surfpool/<version>/`.
Verify its checksum before launch and report the exact version.
If the binary is absent, install the pinned artifact into that project or fail with the required offline-install action.
Tool installation has a separate bounded deadline from process readiness.
Launch only an owned Surfpool instance with per-Lab runtime files.
Wait for the owned process to remain alive and answer its expected local readiness checks.
All application blockchain reads and writes then use that instance.
Surfpool may hydrate its datasource internally; Ronchi never adds a direct competing read path.
Each Lab starts with clean chain state and never auto-funds signers or invokes a faucet.

Report `ready` only after the listeners, Journal writer, state authority, and requested initial Case are usable.
Commit the startup evidence before acknowledging readiness.
Readiness proves the local Lab is usable; it does not prove that every datasource read or application operation will succeed.
Publish the RPC and dashboard endpoints, `lab_id`, mode, active Case, and Journal health.
Repeated `up` returns the existing ready Lab when the explicitly requested settings match.
If they differ, return a state conflict with the required Case-change or restart action.
An incompatible live daemon requires an explicit stop; a new binary never takes over its lock.

If startup fails, stop only acquired resources and retain its failed manifest, diagnostics, and any interrupted Journal.
If the initiating `up` is cancelled before readiness, cancel that startup and perform the same cleanup.
Once readiness is committed, caller cancellation does not stop the Lab.

## Case changes and request deadlines

Case activation, replacement, and disarming follow the shared request-registration order in [Cases](cases.md#continuing-case-model).
An acknowledged Case change is durably recorded before control returns.
Each in-flight request retains its original Case and settings.
`tx-response-lost` retains its original 20-second withholding deadline after a Case change.
The change neither sends its withheld response nor resets its accepted transaction lifecycle.
The status response exposes remaining in-flight effects of previous Cases.

Every accepted request has bounded ordinary processing time.
A declared Fault hold has its own deadline measured from Fault application; it is not restarted by retries or control changes.
The ordinary processing deadline does not silently shorten a declared hold.
Observed client cancellation releases request resources but does not undo acceptance or local execution.
Cancellation of a forwarded request does not prove that Surfpool rejected or rolled back the transaction.
Record the observed boundary result and any unavailable backend outcome separately.

## Shutdown and failures

`ronchi down` stops new RPC admission and Case mutations, then waits for accepted work to settle.
Keep status available while the Lab is stopping.
Existing holds retain their original deadline within the shutdown drain budget.
After accepted RPC work settles, quiesce other producers and stop the verified owned Surfpool process.
Keep the Journal writer alive while collecting actual cancellation, child-termination, and cleanup results.
Then flush all admitted evidence and commit the terminal lifecycle event.
Close the remaining control listener and release the name lock after that commit.
If work or owned-resource termination remains uncertain, mark the Journal interrupted instead of finalized.
Do not discard accepted producer evidence by stopping the writer first.
A successful normal stop reports a finalized Journal and verifies owned process termination.

If the drain deadline expires, cancel remaining work and record a shutdown interruption where writing is still possible.
`down --force` skips the drain and starts cancellation immediately.
A forced stop is distinct from disarming a Case and marks the Journal interrupted.
After a bounded graceful child-stop interval, terminate only a verified owned child that remains alive.
If cleanup cannot complete, report the remaining owned resources and return an error.
Never report a requested cancellation or sent signal as confirmed termination.
Cancellation or disconnection of the `down` client does not reverse an accepted shutdown.

If the Journal writer fails, its queue saturates, or storage reaches its bound, set Lab health to `failed` and stop admitting RPC work.
Perform bounded shutdown and treat the Journal as interrupted.
Do not continue an unrecorded experiment or turn a Ronchi failure into a System Under Test Verdict.
If managed Surfpool exits unexpectedly, apply the same fail-stop behavior.
Do not restart it transparently against a new chain state.

## Recovery, retention, and cleanup

`status` reconciles the manifest with an authenticated daemon response and process identity.
If no matching daemon exists, report the stopped or interrupted Lab rather than an active endpoint.
An open Journal without a valid terminal event is interrupted after process death.
Recovery retains committed evidence and exposes possible missing tail events.
It never invents successful completion or resumes the former chain state.

Before starting a new Lab under a stale name, inspect recorded owned child processes.
Verify their process birth identity and executable identity before sending any signal.
If ownership cannot be established, report the suspected orphan and refuse reuse of that name until resolved.
Never signal a process solely because its PID or port matches a stale manifest.
After verified orphan cleanup, retain the old Journal and allocate a fresh `lab_id`.
Rotate control credentials on every new Lab.

Retain stopped Labs until explicit cleanup under the [evidence retention contract](evidence-and-findings.md).
Cleanup and deletion reject live Labs and operate only on selected workspace-owned paths.
Archiving preserves immutable IDs and evidence references; it does not preserve a running chain.
Workspace removal requires stopping its live Labs first.
Never delete user backend data, native Surfpool project sources, or files outside the Project Workspace.

## Initial tunable limits and verification

These defaults bound waiting and cleanup; they are provisional settings, not measured performance release gates.

| Setting | Initial default |
| --- | --- |
| Managed-tool installation | 5 minutes, including download and verification |
| Simulator startup after prerequisites | 30 seconds |
| Surfpool startup after installation | 120 seconds |
| Ordinary RPC processing, excluding an explicit Fault hold | 30 seconds |
| Normal shutdown drain | 25 seconds |
| Owned-resource cleanup after drain or force | 5 seconds |

CLI and MCP waiting deadlines do not extend these server-side limits.
UI closure and MCP stdio EOF disconnect only their adapter; neither stops or disarms the Lab.
Verify concurrent `up`, occupied ports, cancellation around readiness, a held request during Case replacement, and normal versus forced stop.
Verify writer failure, Surfpool startup failure and death, stale manifests, PID reuse, unknown orphan ownership, and cleanup failure through public controls.
Verify that every new Lab has clean chain state, retains prior Journals, and leaves the independently running backend alive.
