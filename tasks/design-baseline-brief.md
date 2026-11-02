# Autonomous design baseline brief

Status: execution brief authorized by the user on 2026-11-02; documentation baseline completed.

## Objective

Finish the remaining product contracts quickly so implementation can start.
Use one subagent for each of the five remaining topics.
Make routine design decisions autonomously without another user interview.
Treat reversible choices as initial implementation defaults that can change after prototype tests.

## Scope and ownership

| Topic | Agent-owned output |
| --- | --- |
| Journal, evidence, and Findings | `docs/product/evidence-and-findings.md` |
| Dashboard, CLI, and MCP investigation paths | `docs/product/investigation-interfaces.md` |
| Persistent Lab lifecycle | `docs/product/lab-lifecycle.md` |
| Reference Application and Checks | `docs/product/reference-application.md`, `docs/product/checks-and-verdicts.md` |
| Other initial Cases and Custom Cases | `docs/product/cases.md`, `docs/product/custom-cases.md` |

The root agent integrates navigation, glossary changes if necessary, open questions, and development tasks.
Run agents in two waves because only three subagent slots are available beside the root.
Keep edits to shared files under one owner.

## Decision rules

Preserve accepted product scope, vocabulary, mode authority, evidence limits, and ADR-0019's stack.
Preserve all agreed Solana format coverage, including legacy, v0, v1, applicable RPC encodings, and durable-nonce handling.
Choose a small coherent implementation contract instead of exhaustively specifying every wire field.
Do not reinterpret unimplemented design as verified runtime behavior.
Label new autonomous choices as initial defaults authorized by the user's delegation.
Use bounded operational defaults where needed; keep numerical performance release gates for measurements after an executable exists.
Use official sources only when a new external technical fact materially affects a choice.
Avoid research that does not affect the next implementation step.

## Deliverables

- Five topic outcomes recorded in their maintained documents.
- Cross-topic identifiers, states, query semantics, and lifecycle behavior aligned.
- The discussion checklist updated without losing prior decisions or later work.
- Open questions reduced to deliberate deferrals, empirical validation, and later capabilities.
- An ordered initial implementation task list with acceptance criteria and focused verification.
- Working local links and heading references.

## Completion boundary

This task produces a development baseline and handoff, not the runtime implementation.
The remaining product topics should no longer require a design interview before coding starts.
Prototype findings may revise initial defaults while preserving accepted product boundaries and truthful evidence.

## Recorded result

All five delegated topic outputs are recorded at the paths above.
The root integration aligns identity, Journal order, shutdown, public Check operations, and Case resource failures.
The [product plan](plan.md) marks all eight design outcomes complete.
The [development queue](todo.md) contains the ordered implementation slices.
The [documentation map](../docs/README.md) routes to each maintained contract.
Runtime support and empirical validation remain unverified because implementation has not started.

## Verification

- Checked 340 local Markdown links and heading targets across 86 files; no errors were found.
- Parsed three JSON examples and the Custom Case YAML example; this checks syntax, not an implemented schema.
- Confirmed retention of all 19 ADRs, 17 Case names, and five Pack names.
- Confirmed eight completed design outcomes and 24 unchecked implementation tasks with dependencies and verification steps.
- No runtime build or behavior tests were available because runtime code does not exist yet.
