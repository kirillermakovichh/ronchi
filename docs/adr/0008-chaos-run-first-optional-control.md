# Chaos Run first, optional Control Run

Status: primary-workflow wording superseded by [ADR-0015](0015-persistent-lab-first.md); optional Control comparisons remain accepted.

The original decision made a single Chaos Run the primary workflow. ADR-0015 and ADR-0016 replace that framing with a persistent interactive Lab and Continuous Lab Journal; Chaos and Control Runs remain optional automated workflows.

Ronchi does not own or orchestrate private System Under Test state. A developer or Coding Agent requesting Control-versus-Chaos comparison is responsible for preparing equivalent application state and Workload. Ronchi reports Comparability only from available evidence and never assumes it.

## Considered Options

- Mandatory paired Runs were rejected because Ronchi cannot reliably reset arbitrary databases, queues, workers, signers, or external dependencies.
- A general application lifecycle orchestrator was rejected because it expands the product beyond RPC fault injection and evidence analysis without guaranteeing correct state preparation.
- Removing Control Runs entirely was rejected because a user-managed baseline can still improve investigation when comparable state is available.

## Consequences

- A useful investigation does not require application lifecycle scripts or a paired baseline.
- The developer runs the application and Workload through existing tools, tests, or a Coding Agent.
- A single Chaos Run explains observed behavior but does not claim that every difference was caused by the Fault.
- Reference Demo and CI workflows may use Control Runs where they fully control state.
- "Full observability" remains bounded to supported RPC, injection, backend, chain, and optional application observation boundaries.
