---

Application Checks and business Verdict evaluation were removed by user decision. References to them below are historical and do not authorize reimplementation. See [current scope](../product/overview.md) and [task 36](../../tasks/todo.md#36-remove-application-checks).
status: superseded by ADR-0011
---

# Compose existing chain backends

Ronchi will orchestrate established local execution engines such as Anvil and Surfpool instead of implementing its own EVM or SVM. Ronchi owns fault semantics, experiment lifecycle, evidence, and verdicts; the Chain Backend owns transaction execution and chain state. This reduces protocol reimplementation risk while making backend limitations explicit and replaceable.

This decision is superseded for the primary product. Ronchi now uses a stateful Solana RPC Simulator and does not require transaction execution. See [ADR-0011](0011-stateful-solana-rpc-simulator.md).

## Consequences

- Backend adapters must expose capabilities and version information.
- A Case may be unavailable when the selected backend cannot reproduce the required behavior.
- Each Case must publish a Coverage Claim that maps required backend capabilities and fidelity limits to its supported Findings and Checks.
- Ronchi must support attaching to a user-owned backend without assuming lifecycle ownership.

