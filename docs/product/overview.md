# Product overview

Status: accepted product direction. This document defines intent and scope, not implemented capabilities or a stable API.

Ronchi is an open-source, local-first reliability lab for blockchain applications.
A real backend uses Ronchi as its Solana HTTP JSON-RPC endpoint.
Ronchi applies controlled protocol-aware faults, records the supported observations, and makes the backend's observable behavior available for investigation.
The product works without registration, accounts, licensing servers, or a hosted control plane.

## User and problem

The first user develops a Solana Transaction Sender or Relayer responsible for submission, confirmation, and reconciliation.
An RPC acceptance response does not prove confirmation, and a missing response does not prove rejection.
Unsafe handling can abandon an existing signature, create another transaction for the same intent, or leave an operation unresolved.

The first reference incident is `tx-response-lost`.
Ronchi records a local transaction lifecycle decision, withholds the submission response, and observes subsequent RPC activity.
The bundled [Withdrawal Relayer](reference-application.md) supplies known intent correlation and observable outcomes for the Demo.
Its initial Withdrawal Intent is correlation context; the default simulator does not execute a SOL or SPL transfer.

Other potential consumers include bots, wallets, bridges, deposit processors, indexers, and exchange or custody integrations.
Their relevance remains a future validation question; initial coverage does not expand to every transport or workflow they use.
EVM is a later protocol adapter, not part of the first Solana workflow.

## Product value

An Investigation should answer:

1. What RPC request did Ronchi receive?
2. What did the simulator or local Execution Backend do?
3. Which Fault did Ronchi apply, and what RPC Boundary Result followed?
4. What later action was observed, and which evidence source supports that statement?

The primary output is an explanatory Timeline and deterministic, evidence-linked Findings.
A failure before acceptance and a lost response after acceptance can look alike at the transport boundary; their lifecycle and boundary facts must remain distinct.
The tool must remain useful without reducing every Investigation to a terminal test result.
Ronchi does not collect application facts through configurable endpoints or evaluate business requirements.
Business correctness remains the developer's responsibility.
See [ADR-0005](../adr/0005-explanation-first-outcomes.md).

For the first version, a Finding is accompanied by short guidance on what to check in the backend.
The guidance uses the observed RPC history to suggest a concrete developer verification step.
Interfaces present it separately from the evidence-backed statement so a suggested check does not imply an observed application state or established defect.
See the [first no-SDK Investigation](workflows.md#first-no-sdk-investigation) for an illustrative result.

## Primary workflow

The developer starts a persistent local Lab and runs the backend through its normal development tools.
One Case remains active until disarmed, replaced, or the Lab stops.
It may affect multiple matching interactions; it is not a one-shot injection.
Switching Cases preserves the Lab, backend, and Continuous Lab Journal.
Multiple coordinated Faults belong inside one composite Case.

The Journal starts with the Lab and is finalized after a clean stop; an abnormal stop leaves it interrupted.
Investigations, Timelines, and Evidence Slices are filtered views of that record, not separately started captures.
Ronchi does not primarily start tests, supervise the backend, or own its Workload.
Bounded Runs, CI wrappers, and Control comparisons remain optional workflows over the same core.
See [workflows.md](workflows.md) for the intended user experience.
The [Lab lifecycle contract](lab-lifecycle.md) defines native startup, discovery, readiness, stop, and recovery.

## Execution modes

| Mode | State authority | Supported conclusion |
| --- | --- | --- |
| Default RPC Simulator, with optional Read-through | Local transaction lifecycle overlay; explicitly allowlisted reads may use a configured public or private Read Upstream | Controlled RPC-visible history, not executed instructions or balance changes |
| Optional managed Surfpool | Local Surfpool owns all application-facing blockchain reads and writes; it may hydrate remote state through its own datasource | Local execution and resulting account state only where the collected evidence supports them |

The modes use exclusive state authorities.
Default mode never forwards transaction submission, airdrops, provider writes, or unknown methods to the Read Upstream.
Surfpool mode never bypasses Surfpool for direct public account reads after local execution.
Ronchi manages a pinned native Surfpool binary and its owned process; it does not initially fork or embed Surfpool.

The default simulator needs no validator, mainnet fork, deployed programs, prepared account list, or funds.
Surfpool can hydrate existing programs and accounts, but it cannot create missing application state or make an unfunded signer usable.
Ronchi never auto-funds signers or invokes faucets and airdrops.
See [ADR-0011](../adr/0011-stateful-solana-rpc-simulator.md), [ADR-0012](../adr/0012-read-through-upstream-with-write-firewall.md), and [ADR-0013](../adr/0013-managed-surfpool-execution-mode.md).

### Initial local RPC methods

The accepted [Solana RPC contract](solana-rpc-contract.md) defines local submission, status and transaction lookup, blockhash and clock methods, bootstrap methods, fee queries, Read-through, and batches.
Local methods use one coherent Simulated Chain State.
All transaction formats already present on Solana and the applicable RPC encodings are in scope, including `jsonParsed` and durable-nonce handling.
Compatibility remains subject to implementation verification and validation against independently running backends.

### Limited getTransaction

Simulator-mode `getTransaction` returns retained transaction content with `meta: null` for unavailable execution metadata.
The [lookup and history contract](solana-rpc-contract.md#transaction-lookup-and-history) defines its scope.
Workflows requiring execution metadata for local submissions need Surfpool.

### Program simulation

Simulator mode does not provide program simulation or full execution preflight.
`sendTransaction` requires `skipPreflight: true` in this mode.
Requests requiring full preflight or `simulateTransaction` receive an explicit capability error with Surfpool guidance.
See the [submission and preflight contract](solana-rpc-contract.md#submission-and-preflight).

### Simulator time

In a normal interactive simulator Lab, elapsed simulator time advances at a 1:1 rate with elapsed real time.
It advances while the Lab runs, independently of RPC traffic.
The [Case's simulated processing and confirmation intervals](cases.md#tx-response-lost-default-simulated-outcome) use that time.
The [clock and validity profile](solana-rpc-contract.md#clock-and-validity-profile) defines the initial 400 ms slot, distinct block height, and local blockhash rules.

## Observation and trust

| Evidence source | What becomes observable |
| --- | --- |
| Configured RPC path and simulator/backend | Received requests, local lifecycle decisions, applied Faults, RPC Boundary Results, and later RPC calls |
| Optional Instrumented RPC Client | Client Outcomes, call sites, and explicitly supplied business correlation |
| Optional Application Checkpoints | The application decisions or transitions those events report |

Changing the RPC URL is sufficient for the base Observation Boundary.
It does not expose hidden application state, prove how the client interpreted a response, or relate different signatures to one business intent automatically.
Every Finding links to its supporting Journal Evidence.
Summaries expose the active boundary, Evidence Gaps, and unavailable conclusions.
Missing evidence is not proof that an event did not occur.
The [evidence contract](evidence-and-findings.md) defines recorded facts, deterministic rules, redaction, retention, and storage failure behavior.

A Control Run is optional.
The developer or Coding Agent owns equivalent application state and Workload for any comparison.
Ronchi does not reset private databases, queues, caches, workers, or external dependencies.
Unverified Comparability permits separate observations, not causal attribution from a difference between Runs.

## Cases and extensibility

The first catalog is bounded to RPC unavailability/timeout, lost submission response, unavailable or stale status, stalled confirmation, and expired blockhash submission.
Every implemented Case publishes a Coverage Claim describing its semantics, required capabilities, evidence, supported conclusions, and limitations.
See [cases.md](cases.md) for the accepted boundary and clearly separated later candidates.

Users compose Custom Cases as YAML from maintained Fault Primitives, bounded conditions, sequencing, and Finding rules.
The same Case model serves built-in and user-authored Cases, with distinct provenance.
Arbitrary executable fault plugins are outside the product model.
Missing primitives require a maintained core change.
The [Custom Case contract](custom-cases.md) defines the initial YAML schema, primitive catalog, bounds, and compatibility rules.

## Human and agent interfaces

Humans investigate through a local web dashboard.
Coding Agents use structured CLI and local MCP operations over the same core and Journal.
Agent access is summary-first, with filtered, paginated, field-selectable Evidence Slices and bounded responses.
Browser scraping, full trace dumps, and complete application logs are not the normal agent workflow.
Ronchi contains no embedded model and exposes no arbitrary shell-execution tool.
The [interface contract](investigation-interfaces.md) defines the first dashboard, CLI/MCP operations, machine schemas, and query limits.

An optional first-party SDK is accepted but deferred while the CLI and no-SDK workflow are defined.
The accepted direction starts with Go, wraps an existing RPC client once without per-call closures, and emits structured Checkpoints.
Business identifiers remain explicit application input.
Raw application logs are opt-in bounded attachments, not a default SDK stream.
See [ADR-0006](../adr/0006-human-and-agent-interfaces.md) and [ADR-0009](../adr/0009-optional-go-application-sdk.md).

## Delivery and resources

The accepted core is one native Rust executable with Tokio, a direct Hyper RPC path, an Axum loopback control API, a SQLite WAL Journal, and embedded dashboard assets.
Normal execution requires no Node, JVM, Python, external database, or telemetry collector.
Resource efficiency takes priority over implementation speed.
Detailed stack and resource constraints live in [ADR-0019](../adr/0019-resource-efficient-rust-core.md).
Topic contracts set provisional operational safety limits; measured performance release gates remain deferred.

Containers and future Testcontainers wrappers are delivery/integration options over the same core.
Docker or Kubernetes is not required for the default local workflow.
All runtime data, logs and investigation artifacts belong under the project's `ronchi/` workspace. Installed Ronchi and its private pinned Surfpool may be shared across projects.
See [ADR-0014](../adr/0014-project-local-workspace.md) and its executable-installation refinement in [ADR-0020](../adr/0020-installed-native-tool-distribution.md).

## Boundaries and open validation

Supported environments are local development, CI, and isolated staging.
Ronchi never sits between a production application and production RPC, including passive observation.
An explicitly configured public Read Upstream is allowed behind the write firewall; that does not make the application production traffic supported.

The initial fault surface is one Solana HTTP JSON-RPC path.
Blockchain WebSocket, multi-provider routing, Jito, provider-specific APIs, OS interception, generic service chaos, and EVM support are deferred.
Ronchi is not a load tester, smart-contract verifier, replacement node, or proof of production consensus and economic correctness.

Local-first does not make a live Read Upstream or an arbitrary application's dependencies offline or deterministic.
Reproducibility is bounded by controlled inputs, state, and ordering; a seed alone is insufficient.
The self-contained Demo is intended to work without an existing project or cloud call.

The integration pattern and RPC ambiguity have technical evidence, but repeat-use demand, no-SDK usefulness, onboarding cost, dashboard use, and total agent investigation cost remain product hypotheses.
See [research](../README.md#research) for supporting material and [open questions](../open-questions.md) for deferred work.
Public positioning and visual identity live in [positioning-and-brand.md](positioning-and-brand.md).
