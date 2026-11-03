# Developer workflows

Status: intended user experience, pending runtime implementation.
Initial command behavior comes from [Investigation interfaces](investigation-interfaces.md), [Lab lifecycle](lab-lifecycle.md), and the [Reference Application](reference-application.md).
Deferred CI wrappers and SDK APIs remain illustrative.
The [product overview](overview.md) defines scope; this document owns the user journeys.

## First proof: bundled Demo

```bash
ronchi demo tx-response-lost
```

The Demo starts the default simulator and bundled [Withdrawal Relayer](reference-application.md), drives a bounded example, and produces Journal Evidence through the real core.
It needs no existing project or cloud call.
The command returns a bounded summary, Check results, Findings, and evidence references.
It reports actual RPC/control endpoints and project-aware CLI drill-down commands.
Dashboard and MCP interfaces remain later tasks.
Known correlation and Checks may support stronger conclusions than a backend connected only by changing its RPC URL.
A Control comparison is optional.
The default profile is `resilient`; `--profile unsafe` selects the deliberate identity-replacement example.
Each Demo uses a fresh `demo` or `demo-N` Lab, fresh application storage, and free loopback ports.
After collecting Checks, the reference Workload and relayer finish, while the Lab remains available for investigation.
The result returns its name, immutable `lab_id`, endpoints, summary, and evidence references.
`--stop-after-demo` explicitly stops that Demo-owned Lab; `--json` changes rendering only.

The native Demo allocates a never-used name under the ordinary Lab operation lock.
It ignores project configuration and upstream environment settings for its explicit simulator startup.
It never writes or temporarily replaces `ronchi/config.toml`.
Its source URL uses a reserved loopback port selected before Lab startup.
The reservation closes before normal relayer bind; a bind race is an explicit startup failure before intent submission.
Reference storage is an atomically created new private directory under `ronchi/reference/`.

The command collects the maintained `reference-intent` definition version 2 through the public control API.
It reads the retained public definition digest and evaluates only the pinned committed prefix.
It continues until a final observation follows a public terminal intent state.
An early identity failure does not stop collection of the other terminal proofs.
The application keeps its original 60-second acceptance horizon.
The command has a separate 70-second Workload wait, bounded startup, HTTP, collection-terminal, and cleanup waits.
A command timeout does not become an application failure.
The HTTP-only collector, effective 32 KiB response cap, single-intent evaluation, and 512-record/4 MiB evaluation budgets remain unchanged.

`--json` emits one final envelope; it does not include child output or open a browser.
Exit 0 reports completed orchestration even when unsafe has a confirmed Check fail.
Cancellation exits 130; operational errors report phase, effects, known IDs, resource state, and recovery argv.
Default errors after Lab readiness retain that exact Lab.
`--stop-after-demo` also attempts exact-ID cleanup on error and cancellation.
The reference child is signalled and reaped before command completion.
SIGKILL cannot guarantee cleanup.

The Demo must not use canned traces or a private fault/evidence path unavailable to real applications.
Unsafe and resilient profiles share one reference codebase; they do not implement the catalog one application at a time.

## Connect a real backend

1. Start a persistent Lab with a built-in Case.
2. Configure the backend to use the local Ronchi RPC URL.
3. Start or use the backend through its normal tools and interfaces.
4. Trigger an ordinary API action, queue message, UI action, command, or background workflow.
5. Investigate the marked Faults and later observations in the continuous Journal.

Initial command path:

```bash
ronchi up --case tx-response-lost

# In the backend's normal terminal or development environment:
SOLANA_RPC_URL=http://127.0.0.1:8899 ./withdrawal-worker
```

Ronchi provides the endpoint and observes traffic; it does not primarily launch, supervise, or decide when the backend has finished.
An existing integration test may also produce traffic, but tests are not the defining workflow.
Common Cases are selected by name; normal use does not require YAML for each interaction.
The [Case contract](cases.md) defines initial matching, repetition, recovery, and mode-specific limits.

In simulator mode, configure the RPC client to submit with `skipPreflight: true`.
This is an ordinary client option and requires no Ronchi SDK, but it can require configuration beyond changing the RPC URL.
If the backend needs full preflight, `simulateTransaction`, or execution metadata, select Surfpool.
See the [Solana RPC contract](solana-rpc-contract.md) for method, format, and mode coverage.

An explicitly configured Read Upstream may supply allowlisted real chain reads in simulator mode.
Writes and unknown methods never reach that upstream.
The local overlay must preserve the supported transaction history.
See [execution modes](overview.md#execution-modes) for the account-state limitation.

## When local execution is needed

```bash
ronchi up --execution surfpool --case tx-response-lost
```

Ronchi starts and checks its managed native Surfpool instance before exposing readiness.
The application still uses the Ronchi endpoint.
All blockchain reads and writes pass through local Surfpool; its datasource stays behind that boundary.

The normal path follows Surfpool's project discovery and configuration.
When the workflow uses programs and accounts available from the datasource, lazy hydration avoids a separately prepared clone list or custom deployment manifest.
Programs or state absent from both the datasource and local execution environment remain absent.
An unfunded fee payer produces the normal local execution failure; Ronchi does not invoke a faucet or fund it automatically.

Ronchi owns only the Surfpool instance it starts.
Each managed Surfpool Lab begins with clean in-memory chain state by default and discards it on stop.
Any future persistent chain-state reuse must be explicit.

## Continue the investigation

One Case is active per Lab and applies its continuing policy to matching requests.
Disarming or replacing it does not restart the backend, Lab, or Journal.
Each RPC request retains the Case and settings selected at registration before method execution under the [continuing Case model](cases.md#continuing-case-model).
The Journal exposes a shared increasing sequence for request registrations and Case-control operations so their policy-selection order remains visible when timestamps coincide.
For `tx-response-lost`, requests already being held keep their original termination deadline when the Case is disarmed or replaced.
See the [Case-switching rules](cases.md#tx-response-lost-case-switching) for the boundary between those requests and new requests.
An armed Case with no matching traffic remains visibly waiting; lack of a match is not a successful resilience check.
The [initial families](cases.md#accepted-initial-boundary) and [Custom Case contract](custom-cases.md) define further sequencing and recovery rules.

```bash
ronchi status --json
ronchi case arm signature-status-null
ronchi case disarm
ronchi investigate summary --json
```

The Timeline separates received RPC calls, simulator/backend decisions, Fault application, RPC Boundary Results, and later observed requests.
Client Outcomes and application decisions appear only when their evidence sources exist.
For example, withholding a response is observable at Ronchi's boundary; an application timeout requires client-side evidence if the conclusion depends on it.

A deterministic Finding compresses a supported relationship and links to its evidence.
Selecting it opens an Evidence Slice rather than another capture.
The primary Investigation does not require a Business Invariant, formal Verdict, SDK, or baseline.

### First no-SDK Investigation

The first expected result uses `tx-response-lost` against an independently running backend without an SDK.
An illustrative collected history is:

1. The backend submits a transaction with signature `A`.
2. The simulator accepts it and records its transaction identity in a pending Simulated Transaction State.
3. Ronchi holds the submission request open without a response under the [bounded withholding policy](cases.md#tx-response-lost-response-withholding).
4. After the configured delay in simulator time, the simulator records a successful Simulated Transaction State.
5. The backend later requests the status of `A`.
6. Ronchi returns the successful simulated status.

The result separates:

- **Finding:** after Ronchi withheld the submission response, it observed a status query for the original signature.
- **Evidence:** references to the submission, simulator decision, applied Fault, and later status exchange.
- **Evidence Gap:** the Client Outcome and final business-operation state were not collected.
- **What to check in the backend:** check how the backend maps the returned transaction status to completion of its business operation.

The [product overview](overview.md#product-value) defines the guidance boundary.
The [default simulated outcome and independent delayed transition](cases.md#tx-response-lost-default-simulated-outcome) are accepted.
The [Solana RPC contract](solana-rpc-contract.md) governs the remaining standard response fields and parameters; their implementation still needs verification.
The bundled [Demo](#first-proof-bundled-demo) can support stronger conclusions through its known correlation and Checks.

The initial usefulness criterion is that a developer can explain the difference between local acceptance and the withheld response, identify the later observed action, and name a concrete next check in their backend.
They must also be able to identify which application conclusions remain unsupported.
Whether this result is useful in practice remains a product-validation question for the prototype.

## Human and Coding Agent paths

| Human | Coding Agent |
| --- | --- |
| Open the local dashboard | Use structured CLI or local MCP |
| Inspect the active boundary, Findings, and Timeline | Fetch a compact summary with identifiers, Findings, and Evidence Gaps |
| Select a Fault, signature, method, or interval | Query only relevant identifiers, fields, event types, or intervals |
| Open supporting evidence as needed | Follow evidence references with bounded drill-down |

Both paths use the same Journal, stable identifiers, and core operations.
Machine responses require server-side filtering, field projection, pagination, deterministic ordering, and explicit continuation or truncation.
They retain enough correlation context to interpret the selection without attaching unrelated payloads or the entire Timeline.
Raw payloads and selected application log intervals are requested explicitly.

A Coding Agent starts or drives the backend through its existing development environment.
Ronchi exposes bounded Lab/Case control and observation operations, not arbitrary shell execution.
The local MCP server contains no model and makes no cloud call.
The [interface contract](investigation-interfaces.md) defines the initial command and MCP names, bounded responses, and snapshot queries.
`ronchi mcp stdio` attaches an adapter to the project; its exit does not stop the Lab.
Closing the dashboard or completing a CLI command also leaves the Lab running.
Queries by name resolve the current Lab or its latest stopped Journal and disclose that choice.
Use the returned immutable `lab_id` to pin subsequent evidence queries.

Configured application Checks use explicit public operations:

```bash
ronchi check observe intent-state --lab-id L1 --intent-id W1 --json
ronchi check evaluate intent-state --lab-id L1 --intent-id W1 --snapshot-seq 42 --json
```

Collection adds sourced facts to a live Journal; evaluation reads a retained snapshot without contacting the application.
See [Checks and Verdicts](checks-and-verdicts.md) for configuration, authority, and evidence requirements.

## Stop and maintain artifacts

```bash
ronchi down
```

An explicit normal Lab stop drains accepted work, stops owned resources, and finalizes its Journal.
Forced or incomplete shutdown leaves an interrupted Journal and exposes known gaps.
All persistent product files belong under the selected project's `ronchi/` directory, including managed Surfpool tools, configuration, runtime files, logs, and optional Run artifacts.
The [Lab lifecycle contract](lab-lifecycle.md) refines the project-local decision in [ADR-0014](../adr/0014-project-local-workspace.md).
Stopped Labs remain readable until explicitly deleted; there is no automatic age-based deletion.
Deletion targets a stopped Lab and its workspace-owned artifacts under the [retention contract](evidence-and-findings.md#finalization-retention-and-export).
Stop all live Labs before removing the entire project `ronchi/` directory.

## Optional automation and Control comparison

CI or tests may use a bounded workflow around an existing command:

```bash
ronchi run --case tx-response-lost -- npm run integration
```

The command is a proposal, not an implemented interface.
The intended wrapper starts the controlled environment, exposes endpoints, runs the user command, observes a terminal condition, evaluates configured Checks, writes evidence/artifacts, and cleans up.
Its exact lifecycle, parallel execution, and Chaincase format remain open.
The wrapped command owns application startup and Workload.

A user-managed Control Run can provide a baseline without Faults.
The developer or Coding Agent prepares equivalent application state and Workload for Control and Chaos Runs.
Ronchi does not reset private application databases, queues, caches, workers, or external services.
Without evidence of Comparability, show the Runs independently and do not attribute their difference to the Fault.
Formal Verdicts require the configured Checks and sufficient evidence; a useful Investigation can have no Verdict.

## Deferred instrumentation and delivery

The optional Go-first SDK direction wraps the existing RPC client once and emits structured Checkpoints; its API and transport remain deferred.
SDK statements describe application behavior and do not turn simulator state into execution evidence.
See [ADR-0009](../adr/0009-optional-go-application-sdk.md) for that decision.

Native execution is the primary local delivery.
An official container image and future Testcontainers adapters use the same core where they fit the user's workflow.
Docker is optional; Kubernetes packaging needs demonstrated user demand.
The supported environments remain local development, CI, and isolated staging.
Initial listeners bind only to loopback; local control uses the authentication contract in [Lab lifecycle](lab-lifecycle.md#discovery-listeners-and-local-control).
The [evidence contract](evidence-and-findings.md#completeness-and-payload-policy) defines redaction before persistence and explicit raw-payload selection.
