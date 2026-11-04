# Persistent Lab lifecycle

Status: implemented simulator lifecycle and optional managed native Surfpool, with scoped validation and limitations below.
This document refines [ADR-0013](../adr/0013-managed-surfpool-execution-mode.md), [ADR-0014](../adr/0014-project-local-workspace.md), [ADR-0015](../adr/0015-persistent-lab-first.md), and [ADR-0016](../adr/0016-continuous-lab-journal.md).
The native stack remains defined by [ADR-0019](../adr/0019-resource-efficient-rust-core.md).

## Ownership and identity

`ronchi up` runs in the terminal until Ctrl-C.
After readiness, it prints the RPC URL and a single-use dashboard link.
The developer starts their backend in another terminal.
Ctrl-C requests normal shutdown and waits for the owned Lab to terminate.
SIGTERM, terminal closure, a closed stdout pipe, or owner-process death also request normal shutdown.
`ronchi up --detach` starts a background Lab and returns after readiness.
The detached Lab survives its initiating terminal and CLI process.
The MCP `lab.up` operation and Demo retain their background startup behavior.
Dashboard closure and MCP client disconnection do not stop a Lab.
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

### Implemented configuration surface

The CLI implements persistent `up`, `status`, and `down` for simulator lifecycle controls.
Startup validates configuration before creating workspace files or opening listeners.
Unsupported platform, absent Surfpool installation, and unavailable initial Case capabilities return exit `4` before allocating a Lab.
Task 16 adds the explicit [Read-through profile](solana-rpc-contract.md#implemented-read-through-profile), including credential-free source identities in retained metadata.
Help, version, and schemas for `lab.up`, `lab.status`, and `lab.down` work without project access.
The readiness response lists implemented lifecycle and RPC capabilities.
The bound RPC listener implements the [local bootstrap profile](solana-rpc-contract.md#implemented-bootstrap-profile).
Other methods remain explicit unsupported capabilities until their owning tasks.
This slice does not claim transaction simulation, execution, Case coverage, Findings, or dashboard availability.

`--project <directory>` selects an existing directory.
`--json` selects the version-1 error envelope, including parser failures.
These flags work before or after the subcommand.
Text errors use stderr; JSON errors use stdout.

The optional `ronchi/config.toml` requires `schema_version = 1`.
Unknown keys and invalid types are errors.
The following flat keys use flag, environment, file, then default precedence:

| TOML key | Startup flag | Environment | Default |
| --- | --- | --- | --- |
| `mode` | `--mode` | `RONCHI_MODE` | `simulator` |
| `read_upstream` | `--read-upstream` | `RONCHI_READ_UPSTREAM` | absent |
| `surfpool_datasource` | `--surfpool-datasource` | `RONCHI_SURFPOOL_DATASOURCE` | absent |
| `rpc_listen` | `--rpc-listen` | `RONCHI_RPC_LISTEN` | `127.0.0.1:8899` |
| `control_listen` | `--control-listen` | `RONCHI_CONTROL_LISTEN` | `127.0.0.1:8900` |
| `initial_case` | `--case` | `RONCHI_INITIAL_CASE` | absent |
| `slot_duration_ms` | `--slot-duration-ms` | `RONCHI_SLOT_DURATION_MS` | `400` |

`simulator_state` is an optional file-only structured fixture.
Its [protocol state contract](solana-rpc-contract.md#local-lookup-state-and-json-projection) defines keys, limits, and missing-state behavior.
The Lab retains its immutable startup fixture in configuration and Journal evidence.

`slot_duration_ms` accepts integer values from 1 through 60000.
Modes are `simulator` and `surfpool`.
Datasource values must be absolute HTTP or HTTPS URLs.
Simulator rejects a Surfpool datasource; Surfpool rejects a Read Upstream.
Listeners require loopback socket addresses and distinct nonzero endpoints.
Port `0` remains valid for future dynamic allocation.
Case identifiers contain 1–128 ASCII letters, digits, hyphens, or underscores.
Case availability and mode capability validation remain part of the later Case implementation.
Runtime-limit configuration keys remain unimplemented.
`surfpool_snapshot` is an optional file-only path to an explicit native Surfpool account snapshot inside the selected project.
Simulator mode rejects this setting.
Unknown keys remain errors until their owning slice introduces them.
No environment variable selects the project or an independent workspace location.

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
Repeated `up --detach` or MCP `lab.up` returns the existing ready Lab when the explicitly requested settings match.
If they differ, return a state conflict with the required Case-change or restart action.
Foreground `up` rejects an already running Lab and never takes over its terminal ownership.
An incompatible live daemon requires an explicit stop; a new binary never takes over its lock.

If startup fails, stop only acquired resources and retain its failed manifest, diagnostics, and any interrupted Journal.
If the initiating `up` is cancelled before readiness, cancel that startup and perform the same cleanup.
Once readiness is committed, detached caller cancellation does not stop the Lab.
Foreground cancellation after readiness stops its owned Lab through the normal drain.

### Implemented native ownership

The workspace uses `ronchi/names/<name>.json` for the selected immutable ID.
Per-name operation locks serialize CLI start and stop decisions.
A separate owner lock remains held for the entire daemon lifetime.
The daemon starts in its own operating-system session with no inherited terminal streams.
In foreground mode, the CLI retains its exact child handle and a private lifetime socket.
The daemon treats socket EOF as a normal stop request.
This also stops the Lab if the terminal owner dies without sending a control request.
The CLI waits for the child to exit and checks the retained Journal state.
An external `down` completes that wait.
A failed child or interrupted Journal produces a nonzero foreground exit.
Normal Ctrl-C after readiness exits `0`; pre-readiness cancellation exits `130`.
Detached startup has no terminal lifetime socket.

Startup allocates a fresh random ID and 256-bit control credential.
Workspace directories use mode `0700`; newly created metadata and credential files use mode `0600`.
Atomic replacement writes and syncs a temporary file, renames it, and syncs the parent directory.
Metadata reads and writes are bounded to 64 KiB.
Manifest publication after Journal creation shares the Journal disk accounting lock and uses reserved lifecycle capacity.
The manifest records requested configuration, actual endpoints, component versions, persistence policy, and process identity.

The startup CLI holds a lease until readiness publication.
A gate serializes publication with cancellation inspection.
A lost startup lease aborts pre-readiness startup and joins acquired resources.
If readiness won the race, detached cancellation reports applied effects and leaves the Lab running.
Foreground cancellation instead closes the lifetime socket and waits for normal shutdown.
If completion cannot be established within the cleanup bound, report unknown effects instead of claiming cancellation completed.
The CLI retains its direct child handle until this decision; it never kills a process selected from a manifest PID.
One SIGINT receiver remains registered throughout each start or stop wait.

`lab.starting` records build and persistence provenance before listener setup.
`lab.ready` commits before readiness publication.
`lab.stop_requested` precedes drain and the terminal lifecycle record.
Normal completion commits `lab.finalized`; force, failed cleanup, or abnormal recovery exposes interruption.
A failed startup retains its manifest and any committed startup Journal.
A storage failure stops admission and settles owned connections without deleting acknowledged evidence.
If failure metadata cannot be written, status derives interruption from retained evidence and the absent owner.

Repeated `up` checks settings explicitly supplied by flags, environment, or the project file.
Omitted defaults do not replace a running Lab's configuration.
A mismatch requires an explicit stop and new start.
`status --lab-id <id>` remains available when another Lab reuses the same name.
`down --lab-id <id>` accepts only the current live target; a historical selection returns `state_conflict`.
Stopped or interrupted results omit live endpoints.
A lost stop response triggers bounded status inspection, without resending the mutation.

Control uses a loopback-only address, an exact Host check, and the per-Lab bearer token.
Bearer routes reject Origin headers.
Browser routes use a separate single-use ticket exchange and scoped session cookie.
Tickets live for 60 seconds; sessions live for 30 minutes.
Each Lab bounds unexpired tickets to 16 and sessions to 32.
Browser Host and Origin checks use the actual bound address.
The cookie name includes the Lab ID and port; its path includes the immutable Lab ID.
`ronchi dashboard` prints a finite live launch link.
`ronchi dashboard --serve --lab-id <id>` owns a foreground read-only retained viewer.
The viewer has no writer or simulator and does not change Lab ownership.
See [build and launch](../development.md#dashboard-build-and-launch) for readiness and shutdown.
Each listener owns at most 64 connection tasks.
HTTP headers are bounded to 32 KiB and 64 fields, with a 30-second header deadline.
Control JSON bodies are bounded to 1 KiB with a 30-second handler deadline.
Control clients wait at most 2 seconds for one bounded 64 KiB response.
Shutdown stops RPC admission, joins connection tasks, and joins the Journal writer before releasing ownership.
Normal drain is bounded to 25 seconds; owned cleanup shares one 5-second deadline after drain or force.
The CLI confirms daemon process termination before reporting successful stop.

OS birth identity uses microsecond process start information on macOS and kernel start ticks on Linux.
The authenticated response must match the complete recorded process identity, including the recorded executable path and inode.
A rebuilt executable file does not create a new daemon birth identity.
No retained PID alone authorizes a signal or name takeover.
Recorded children are inspected before name reuse.
A live child whose cleanup capability is unavailable, or an unverifiable child identity, causes a conflict.
Surfpool mode records the native child's birth identity and executable device/inode before waiting for backend readiness.
Shutdown interrupts and reaps that owned child, with bounded kill fallback.
After daemon death, name reuse can clean only a recorded child whose complete identity and checksum-pinned tool still match.
Unknown or unverifiable orphan ownership remains a conflict.
The process tests were executed on macOS; Linux code is present but has not been exercised in this session.

## Case changes and request deadlines

Case activation, replacement, and disarming follow the shared request-registration order in [Cases](cases.md#continuing-case-model).
An acknowledged Case change is durably recorded before control returns.
The native simulator implements `up --case tx-response-lost`, `case arm`, and `case disarm`.
Startup arms its configured Case after protocol-state initialization and before readiness.
An existing Lab retains its startup configuration; use `case arm` to change its live policy.
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
Foreground Ctrl-C uses the same shutdown path and does not imply `--force`.
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
The Lab owns active Check collections and their HTTP cancellation.
Stop rejects new collections and cancels active invocations before closing the Journal writer.
Collection settlement uses the existing five-second cleanup budget.
The writer retains terminal cancellation records when persistence remains available.
This cancellation does not stop a developer-owned observation source.

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
Use `ronchi --lab-id ID lab delete` for explicit whole-Lab deletion.
Retained viewers and finite CLI evidence reads hold a shared maintenance lease; deletion requires exclusive ownership.
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

## Managed native Surfpool implementation

Install the explicit official v1.7.0 platform archive with `ronchi tool surfpool install --archive PATH`.
Installation never downloads a tool or changes global caches.
It stores the archive and executable under `ronchi/tools/surfpool/1.7.0/`.
The installer accepts regular files only, caps the compressed archive at 64 MiB, and caps the extracted executable at 128 MiB.
Installation and verification check a five-minute cooperative deadline.
The system `/usr/bin/tar` extracts only the named `surfpool` entry after archive verification.
Every launch verifies the archive against the pinned SHA-256 and compares the executable with the entry read from that verified archive.
Existing installation names are verified rather than overwritten.

| Platform artifact | Pinned SHA-256 |
| --- | --- |
| `surfpool-darwin-arm64.tar.gz` | `32c309e3b79bfa3913e384b3c9b109000720cfe7f346d145feb6693fccfb202f` |
| `surfpool-darwin-x64.tar.gz` | `b9724b6a67ac33b9cc94fb41adaf47bd638269142c7ddc6a0dae7ca77fd9e760` |
| `surfpool-linux-x64.tar.gz` | `5bef2b0b186a74622508f104b6d2e4e2db040932b6d8f3a811172e241f802a63` |

These hashes identify the official `solana-foundation/surfpool` v1.7.0 release published on 2026-10-28.
Actual local execution is verified on macOS 15.2 arm64.
The other pinned targets remain untested on this host.
Windows and other architectures have no native Ronchi ownership implementation in this build.

Start the installed backend with `ronchi up --mode surfpool`.
Ronchi allocates separate private dynamic loopback RPC and WebSocket ports.
Readiness requires the owned process, health, version response, and a genuine backend blockhash tuple.
The manifest reports the exact Surfpool version and retains its child identity.
The Journal records the verified executable checksum and readiness evidence.
A backend death causes Lab fail-stop and interrupted evidence; Ronchi does not restart it silently.
The ordinary application RPC address remains the Ronchi boundary.

Native startup uses the selected project as its working directory.
If `txtx.yml` exists there, Ronchi selects it as Surfpool's native manifest.
Ronchi passes `--ci --no-deploy --airdrop-amount 0 --db :memory:`.
It also places native logs inside the owned per-Lab backend directory.
Automatic native program deployment is disabled in this build.
The mode does not claim to execute arbitrary project deployment runbooks.
Programs and accounts must be available in the backend's actual state.
No signer funding, faucet call, or airdrop occurs as a startup action.
An explicit application `requestAirdrop` targets only the owned local backend.

Without a configured datasource, Surfpool runs offline.
An explicit `surfpool_datasource` reaches Surfpool only through its private environment.
Public configuration and retained evidence use opaque datasource identities.
Ronchi never opens a second Read-through path in this mode.
All supported blockchain RPC, including bootstrap reads, status/history lookup, submission, preflight, and simulation, reaches the owned backend.
Unknown provider-specific methods remain unavailable.
Method results retain backend state authority; Ronchi never creates simulator lifecycle transitions for them.

For explicit offline state, `surfpool_snapshot = "snapshot.json"` selects a native JSON account map.
Keys are public account addresses.
Values contain the native account fields `lamports`, `owner`, `executable`, `rentEpoch`, `data` (base64), and optional `parsedData`.
Ronchi copies at most 16 MiB into the new per-Lab backend directory before launch.
The selected file must resolve inside the project.
A supplied snapshot may explicitly contain a funded test fee payer.
It is fixture state supplied by the user, not automatic funding by Ronchi.
Each subsequent Lab reloads its declared starting state into fresh memory.

The native acceptance fixture signs a system transfer independently of Ronchi's decoder.
It verifies changed recipient balance, actual transaction status, missing-fee-payer rejection, duplicate backend responses, response loss, clean-state restart, binary tamper rejection, native snapshot startup failure, child death, owned cleanup, and verified orphan cleanup.
Set `RONCHI_TEST_SURFPOOL_ARCHIVE` to the local pinned archive to run `cargo test --locked --test surfpool_mode -- --test-threads=1` with real native execution.
Without that explicit fixture, native tests print or retain their fixture limitation and do not establish execution coverage.
