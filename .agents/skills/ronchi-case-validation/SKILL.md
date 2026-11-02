---
name: ronchi-case-validation
description: Validate Ronchi Cases, simulator histories, and evidence-backed Findings. Use when implementing or reviewing protocol state transitions, fault sequencing, Coverage Claims, or generated history tests.
metadata:
  author: Ronchi contributors
  version: "1.0.0"
---

# Ronchi Case validation

Check whether Ronchi honors the declared Case and observation contracts.
Keep this separate from deciding whether the connected backend is business-correct.
Use `AGENTS.md`, `CONTEXT.md`, and relevant accepted ADRs as the project authority.

## Establish scope

- Identify the affected Case, Fault Primitives, execution mode, and declared Coverage Claim.
- Identify the relevant protocol rules, state transitions, Observation Boundary, and expected evidence.
- Distinguish accepted behavior from open decisions and implementation assumptions.
- If a required contract is unresolved, expose that gap instead of inventing an assertion.
- Bound the investigation to the requested behavior; no defect quota or automatic expansion is required.

For Solana facts, select the relevant [shared RPC reference](../../references/solana-rpc/index.md).
Those facts do not silently define unimplemented capabilities or detailed Case semantics.

## Choose the verification method

Use explicit example histories when they demonstrate the changed behavior clearly.
Use generated operation sequences and an independent reference model when history-dependent interactions justify them.
Do not copy production transition logic into the expected-result model.
Inspect a disagreement for errors in both the implementation and the test model.

| Work | Reference |
| --- | --- |
| Stateful histories, generators, time, replay, or concurrency | [history-testing.md](references/history-testing.md) |
| Findings, evidence correlation, observation gaps, or supported conclusions | [finding-validation.md](references/finding-validation.md) |

Read only the reference needed for the change.
Use the existing TDD or debugging workflow when the task requires it.
Do not add a testing framework, discovery branch, separate worktree, or subagent merely to follow an external methodology.

## Preserve product boundaries

Use fake upstreams or isolated local execution for Ronchi verification.
Do not send transactions or airdrops to public RPC as a test oracle.
Keep optional Surfpool execution distinct from simulator behavior.
Do not require an SDK or Control Run for base RPC-and-simulator observations.
A finite verification history does not turn the product into a one-shot test runner.
Cases remain continuing policies in a persistent Lab according to ADR-0017.

## Report

State the tested contract, mode, relevant histories, checks performed, and material gaps.
Separate confirmed defects from test-model errors, unsupported capabilities, and unresolved requirements.
For a defect, retain its minimal input/history and a reproducible check.
An empty defect list is valid; passing selected histories is not proof of universal coverage.

## Sources

These are locally written instructions informed by property-testing methodology.
See [sources.md](references/sources.md) for reviewed sources and deliberate scope limits.
