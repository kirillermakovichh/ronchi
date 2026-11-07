# Open questions

These choices remain unresolved or deliberately deferred.
Current contracts are recorded; these deferred choices do not restart the completed initial queue.
Accepted behavior lives in the [documentation map](README.md), [product overview](product/overview.md), and [decision index](README.md#decisions).
The [working plan](../tasks/plan.md) records current maintenance scope.
Implementation gaps and their verification belong in the [development queue](../tasks/todo.md), not in this list of undecided choices.

## CI and optional comparisons

- Define the optional bounded Run wrapper, terminal conditions, parallel isolation, cancellation, and cleanup over existing test commands.
- Define portable evidence for Control transparency and Comparability beyond the initial reference fixture.
- Keep private application-state preparation and Workload ownership with the developer or Coding Agent.

## Chaincase and replay

- Define exactly which versioned inputs and evidence belong in a Chaincase.
- Decide whether replay restores Simulated Chain State or can reproduce selected transport-only histories from responses alone.
- Define artifact migration and compatibility across format, primitive, and backend versions.
- Keep initial bounded evidence exports distinct from a promise of replay or restoration.

## Optional SDK

- Use prototype investigations to identify useful observations unavailable through RPC.
- Define Go client wrapping, Application Checkpoint APIs, local transport, and correlation ergonomics when that need is established.
- Preserve the accepted Go-first direction and one-time wrapping of the existing RPC client.

## Later capabilities

- Evaluate new histories and primitives against observed backend needs before promoting [candidate Cases or Packs](product/cases.md#later-candidates).
- Define explicit commitment-regression semantics before promoting `confirmation-regresses`; never equate the simulator with production consensus.
- Revisit blockchain WebSocket, multi-provider routing, Jito, EVM, generic external-service chaos, and constrained Random Exploration only as separate scope decisions.
- Require an explicit policy before adding persistent chain-state reuse or broader listener bindings.

## Distribution and resources

- Homebrew packaging and tap publication remain deferred.
- Select any first Testcontainers language adapter. Native distribution targets and verification status live in [development](development.md#native-distribution).
- Complete public naming and registry checks listed in [positioning and brand](product/positioning-and-brand.md#pre-publication-checks).
- Define measured release gates from the existing executable and representative workloads. Operational safety limits already live in their topic contracts.
- Use [ADR-0019](adr/0019-resource-efficient-rust-core.md) for the accepted measurement areas; do not reopen the stack merely because limits are not numerical yet.
- Revise provisional safety defaults from measured resource and investigation costs; they are not benchmark results.

## Product validation

- Validate setup and migration from the bundled Demo to an independently running stateful backend.
- Validate the accepted RPC format/encoding matrix and Case histories against real client behavior and isolated backend versions.
- Validate repeat use, adoption, and the maintenance burden of Cases and integrations.
- Compare the usefulness and total investigation cost of dashboard, CLI, MCP, and the Fault Primitive catalog empirically.
- Preserve uncertainty about demand and outcomes; technical feasibility does not establish product adoption.
