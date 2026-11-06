# Validating Findings against evidence

Use the glossary and ADR-0005 for the Finding and Observation Boundary contracts.
The initial no-SDK boundary observes RPC requests, simulator decisions, applied Faults, and RPC Boundary Results.
It does not observe every application decision or client-library outcome.

## Validate each claim

Identify the facts required for the Finding and the Journal Evidence that supplies them.
Preserve identifiers, source, mode, and ordering context needed to interpret each fact.
Check that cited evidence exists and belongs to the relevant Lab, transaction, and experimental interval.
Do not substitute a nearby event or an unrelated request with a similar timestamp.
Temporal order alone does not establish causality.

Distinguish:

| Observation | What it can support |
| --- | --- |
| Simulator state transition | A statement about simulated lifecycle state |
| Surfpool execution evidence | A statement about observed local execution within that mode |
| Ronchi withholding a response | The RPC Boundary Result, not an assumed client timeout |
| Later RPC request | An Observed Application Action with that request as its source |
| Future SDK event or Checkpoint | Only the client/application facts that source actually observes |

Different transaction signatures do not alone prove duplicate business intent or economic execution.
The same business correlation identifier does not itself prove an on-chain transfer happened.
State the mode and observation limits in conclusions that depend on them.
Do not treat a simulated successful lifecycle state as evidence of executed instructions or changed balances.

## Check positive and negative conditions

Test a supported pattern with the evidence needed to justify the Finding.
Also select relevant near-miss histories: missing correlation, unrelated transactions, reordered facts, absent observations, or another execution mode.
Verify that those histories do not produce the stronger unsupported conclusion.
If only a weaker fact is supported, report that fact and the Evidence Gap according to the existing contract.
Do not suppress unrelated valid Findings just because another requested conclusion lacks evidence.

Use tests that exercise the rule through its meaningful interface.
Do not mirror its implementation condition by condition and call that independent validation.
Where query projection or pagination changes, ensure the selected evidence retains the context its claims require.

## Determinism and boundaries

For the same relevant evidence and rule/configuration version, check the deterministic conclusion the rule promises.
Do not introduce an embedded model or generated speculation as a Finding mechanism.
Missing evidence is not proof of backend success or failure.
An intentionally injected RPC error is not automatically a defect in Ronchi or the connected backend.
Do not evaluate application business requirements or restore the removed application Check mechanism.
Control comparison remains optional; differences between non-comparable application states do not establish fault causation.

## Example boundary

A history can establish that the simulator accepted a transaction, Ronchi withheld its response, and another RPC submission followed.
Without business correlation it cannot establish that both submissions served one withdrawal intent.
Without execution evidence it cannot establish that two transfers occurred.
Use this distinction to choose the expected Finding, not to require additional instrumentation for every base workflow.
