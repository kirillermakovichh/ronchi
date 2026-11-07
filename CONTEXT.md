# Ronchi Domain

Ronchi describes reliability experiments performed on off-chain blockchain applications through their normal RPC boundary. This glossary defines the language used across product, architecture, and implementation discussions.

## Experiment model

**Lab**:
A persistent local Ronchi environment to which an independently running System Under Test connects through its normal RPC configuration. Depending on its execution mode, the Lab also uses either a configured Read Upstream or a local Execution Backend.
_Avoid_: Sandbox, testnet

**Experiment**:
A developer- or agent-led examination of how a live System Under Test behaves while a Case is active in a Lab. It reads from the Lab Journal and may include an optional user-managed comparison.
_Avoid_: Test suite, mandatory paired execution

**Investigation**:
A filtered view of the Lab Journal centered on a fault, transaction identity, RPC method, Case, or time range. It has no independent capture lifecycle and does not duplicate the Journal.
_Avoid_: Run, recording session

**Comparability**:
The evidence that an optional Control Run and Chaos Run began from equivalent relevant state and exercised equivalent Workloads. Application-state comparability is the responsibility of the developer or Coding Agent performing the comparison.
_Avoid_: Assumed identical state, bit-for-bit equality

**Run**:
A bounded automated Lab session used by CI, an existing test runner, or an explicit command wrapper. It is not the primary interactive product workflow.
_Avoid_: Investigation, backend process

**Control Run**:
A Run in which Ronchi observes traffic but injects no RPC Faults. It is an optional comparison feature, not a prerequisite for a useful Chaos Run.
_Avoid_: Happy-path test

**Chaos Run**:
A Run in which a Case is armed and one or more declared RPC Faults may be injected. It is an optional automation workflow over the same Lab core.
_Avoid_: Broken run, failure run

**Case**:
A named, reproducible fault policy that remains active in a Lab until disarmed or replaced. It may inject RPC Faults repeatedly according to its own declared semantics; Cases are the primary fault-selection model, while unconstrained random fault injection is not.
_Avoid_: Test, scenario

**Pack**:
A curated collection of Cases for a type of application or reliability concern.
_Avoid_: Test suite, profile

**Fixture**:
A reproducible starting Simulated Chain State and application state used by optional automated or comparative workflows. It is not required for ordinary interactive investigation.
_Avoid_: Seed data, snapshot

**Workload**:
An action or ordinary application traffic that causes the independently running System Under Test to exercise blockchain behavior. Ronchi observes it but does not own or execute it in the primary workflow.
_Avoid_: Test case, traffic

## Participants

**Transaction Sender**:
An off-chain service that accepts an economic intent, submits a signed transaction, and tracks it to a reconciled terminal outcome.
_Avoid_: Broadcaster, RPC client

**Relayer**:
A Transaction Sender that submits transactions on behalf of another actor or system.
_Avoid_: Bot, worker

**Withdrawal Intent**:
A durable request to transfer a defined asset amount to a recipient exactly once at the economic level.
_Avoid_: Withdrawal transaction, transfer request

**Withdrawal Relayer**:
A Relayer that turns Withdrawal Intents into Solana transactions and reconciles their terminal economic outcome.
_Avoid_: Withdrawal API, sender

**System Under Test**:
The independently running application or service whose blockchain failure handling is being evaluated.
_Avoid_: Client, app under test

**Reference Application**:
The bundled Withdrawal Relayer used by the self-contained Demo and as an acceptance consumer of the same public Ronchi behavior available to real Systems Under Test.
_Avoid_: Mock application, product core

**Reference Behavior Profile**:
A named retry, confirmation, reconciliation, and recovery policy of the Reference Application. Different profiles demonstrate unsafe and resilient outcomes under the same Case without creating a separate application for each fault.
_Avoid_: Separate demo app, fault implementation

**Coding Agent**:
An external AI development agent that controls or investigates a Lab through Ronchi's bounded machine interfaces. It is a user of Ronchi, not a model embedded in the product.
_Avoid_: Ronchi AI, embedded agent

**RPC Simulator**:
The Ronchi component that behaves as the controlled Solana RPC authority seen by the System Under Test. It models only the protocol state and responses needed to investigate off-chain behavior; it does not execute Solana programs.
_Avoid_: Validator, fork, mock chain

**Read Upstream**:
An explicitly configured Solana RPC endpoint to which Ronchi may forward allowlisted read methods from a pre-production Lab. Ronchi never forwards transaction submission, provider-specific writes, or unknown methods to it.
_Avoid_: Write target, production traffic path

**Read-through**:
The handling of an allowlisted RPC read by forwarding it to the Read Upstream before Ronchi returns, alters, delays, or withholds the result according to the active Case.
_Avoid_: Transparent proxy, unrestricted forwarding

**Execution Backend**:
A local RPC-compatible environment that executes transactions and owns the resulting account state for a Lab. When active, all blockchain reads and writes pass through it so the System Under Test sees one coherent state.
_Avoid_: Read Upstream, public RPC, production network

**Execution Mode**:
The Lab-level choice between local transaction simulation with Read-through and transaction execution through an Execution Backend. A Lab uses one mode and never combines their state authorities.
_Avoid_: Case, backend type

**Project Workspace**:
The single project-local `ronchi/` directory containing Lab runtime state, managed backend data, Lab Journals and optional Run artifacts. Installed executables may be shared across projects; runtime data remains project-local.
_Avoid_: Global cache, application support directory, scattered artifacts

**Simulated Chain State**:
The authoritative state of the RPC Simulator for a Lab, including protocol time, recent blockhash validity, known transaction identities, and their assigned lifecycle states.
_Avoid_: Real chain state, ledger, fork state

**Simulated Transaction State**:
The lifecycle state assigned to a submitted transaction by the RPC Simulator, such as pending, successful, failed, or expired. It is evidence about the simulated RPC history, not proof of on-chain execution or economic effects.
_Avoid_: Executed transaction, chain result

## Fault and evidence model

**RPC Fault**:
A deliberate, classified change to an HTTP JSON-RPC interaction that represents a plausible failure visible to a blockchain application. Other transports are future scope.
_Avoid_: Toxic, random error

**Fault Primitive**:
A built-in, versioned Ronchi capability for matching, scheduling, injecting, or observing one bounded RPC or protocol behavior. Fault Primitives are the only building blocks from which users compose Custom Cases.
_Avoid_: Plugin, arbitrary script

**Protocol Fault**:
An RPC Fault that depends on blockchain meaning, such as losing the response after transaction acceptance or returning stale confirmation state.
_Avoid_: Network fault

**Custom Case**:
A portable declarative Case composed from published Fault Primitives, bounded conditions, and evidence-linked Finding rules. It contains no arbitrary executable extension code and is evaluated by the Ronchi core.
_Avoid_: Plugin, user script, built-in Case

**Business Invariant**:
A domain rule that must remain true despite retries, ambiguity, reordering, or reconciliation, such as executing one withdrawal at most once.
_Avoid_: RPC result

**Observation Boundary**:
The explicit set of surfaces from which Ronchi can collect Journal Evidence. The base boundary contains requests received by Ronchi, simulator decisions, and actions taken at its RPC boundary; future SDK instrumentation may add client and application evidence.
_Avoid_: Full observability, everything Ronchi knows

**Evidence Gap**:
An observation required for a requested Finding or comparison that is outside the current Observation Boundary or was not collected successfully.
_Avoid_: Failure, assumed state

**Application Checkpoint**:
A structured event emitted by an optional Ronchi SDK at a meaningful application decision or state transition. It carries correlation attributes and a source-code location so RPC and simulator evidence can be related back to backend logic.
_Avoid_: Unstructured log line, chain fact

**Instrumented RPC Client**:
An application's existing blockchain RPC client wrapped once by the optional Ronchi SDK. Its normal methods remain the application interface while the wrapper adds call-site and correlation evidence.
_Avoid_: Replacement RPC client, per-call closure

**RPC Boundary Result**:
What Ronchi did toward the client for one RPC call, such as returning a response, delaying it, closing the connection, or withholding it. It does not by itself prove how the application RPC library interpreted the outcome.
_Avoid_: Client Outcome, application error

**Client Outcome**:
The result observed by an application's RPC client, such as a returned signature, timeout, or parsed error. It is available only when an explicit evidence source collects that client-side observation.
_Avoid_: RPC Boundary Result, inferred outcome

**Observed Application Action**:
An application action present in Journal Evidence after an RPC Fault. Its evidence source must be explicit: a later RPC request, an SDK event or Application Checkpoint.
_Avoid_: Application reaction, inferred decision

**Coverage Claim**:
The bounded statement of what a Case can reproduce and what conclusions it can support for a given protocol, RPC Simulator capability set, and Observation Boundary, including known limitations.
_Avoid_: Supports everything, compatibility claim

**Timeline**:
An ordered view of selected Lab Journal entries with explicit causal links: RPC requests, simulator decisions, injected RPC Faults, RPC Boundary Results, optional Client Outcomes, and Observed Application Actions. Display order alone does not establish causality.
_Avoid_: Log, trace

**Lab Journal**:
The append-only structured evidence record for one Lab, finalized after a clean stop or marked interrupted after an abnormal stop. It is the primary evidence source for interactive use and contains the facts retained under the active Observation Boundary and persistence policy, with known collection gaps exposed.
_Avoid_: Run, capture, application log

**Journal Evidence**:
The correlated facts actually recorded in the Lab Journal and available to Investigations, Findings, and Evidence Slices.
_Avoid_: Full internal state, telemetry

**Run Evidence**:
The bounded subset of Journal Evidence associated with an optional automated Run.
_Avoid_: Lab Journal, application logs

**Evidence Slice**:
A bounded projection of Journal Evidence selected by relevance, such as particular event types, identifiers, fields, or a Timeline interval. It contains enough context to interpret the selection but does not repeat unrelated evidence.
_Avoid_: Filtered log dump, partial truth

**Finding**:
A deterministic interpretation of observed relationships in Journal Evidence, linked back to the supporting facts, such as "the simulator recorded the transaction as successful, Ronchi withheld the submission response, and the application later queried the same signature." A Finding explains what happened without claiming real chain execution, an unobserved Client Outcome, hidden application intent or business correctness.
_Avoid_: AI diagnosis, business correctness claim, guess

**Chaincase**:
A portable artifact containing enough versioned input and a bounded Journal Evidence selection to reproduce and inspect an Investigation or automated Run.
_Avoid_: Recording, fixture, report

