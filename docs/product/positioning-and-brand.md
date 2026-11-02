# Positioning and brand

## Brand core

**Product name:** Ronchi  
**Wordmark:** RONCHI  
**Primary tagline:** Make hidden failures visible.  
**Descriptor:** An open-source, local-first reliability lab for blockchain applications.

Ronchi is the accepted working brand. Public launch still requires trademark, domain, repository-name, and package-registry checks.

## Product value

Ronchi makes a real blockchain backend's behavior under controlled RPC faults observable and explainable.
The canonical user, scope, modes, and evidence limits are in the [product overview](overview.md).
Public copy must stay within those boundaries and distinguish intended behavior from verified implementation.

## Why Ronchi

The Ronchi optical test uses a regular grating to make deviations in an optical system visible as changes in an observed band pattern. The test introduces a controlled probe; the resulting pattern exposes defects that are difficult to see under ordinary observation.

The product analogy is direct:

| Ronchi optical test | Ronchi software product |
| --- | --- |
| Optical system | Blockchain backend |
| Ronchi grating | Active protocol-aware Case |
| Controlled optical probe | Controlled RPC fault policy |
| Observed band pattern | Continuous Lab Journal and Timeline |
| Visible distortion | Evidence-linked Finding |
| Interpretation in test context | Checks, Evidence Gaps, and explicit limits |

The scientific analogy also sets an important trust boundary. An observed deviation does not automatically justify a universal quality verdict. Likewise, Ronchi shows observable behavior and produces deterministic Findings, but reports `PASS` or `FAIL` only when explicit Checks and sufficient evidence justify it.

## Positioning

Ronchi is not positioned primarily as a generic chaos engineering tool or as a blockchain RPC proxy. Chaos injection and RPC proxying are mechanisms. The product category is:

> An open-source, local-first reliability lab for blockchain applications.

Recommended hero copy:

> **RONCHI**  
> **Make hidden failures visible.**  
> An open-source, local-first reliability lab for blockchain applications.
>
> Run your real backend through controlled, protocol-aware RPC failures. Ronchi keeps a continuous evidence journal, explains observable behavior, and makes uncertainty explicit—without touching production.

Supporting line:

> Break the assumptions. Reveal the behavior.

Technical principle:

> The fault is the probe, not the verdict.

RPC-focused supporting line:

> Trust the evidence, not the response.

## Product principles

### 1. A fault is a probe, not a verdict

Applying a fault proves only that Ronchi changed a supported boundary. Correctness conclusions require observable application evidence and, when appropriate, explicit Checks.

### 2. Evidence over assumptions

Ronchi separates RPC receipt, simulator or Execution Backend decisions, fault application, RPC Boundary Results, later RPC calls, optional application evidence, and Evidence Gaps. It never presents an unobserved application state as fact.

### 3. Real backend, controlled environment

The primary experience is a persistent local Lab connected to the developer's independently running backend. Ronchi is not primarily a test runner and does not own the backend lifecycle.

### 4. Compare only when comparison is valid

A Control Run is optional. Ronchi may compare Control and Chaos Runs only when the developer or Coding Agent prepares equivalent relevant state and activity and the available evidence establishes Comparability. Every investigation does not require a baseline.

### 5. One evidence source, two first-class users

Humans use the local dashboard. Coding Agents use compact structured CLI and local MCP operations with filtered, paginated Evidence Slices. Both read the same Continuous Lab Journal.

## Product experience in copy

Describe a persistent local Lab connected to an independently running backend, with a continuing Case and Continuous Lab Journal.
The default mode is a non-executing RPC Simulator; optional managed Surfpool supplies local execution when required.
Describe the dashboard for humans and bounded CLI/MCP evidence queries for Coding Agents.
Exact commands remain illustrative; use [developer workflows](workflows.md) for the canonical journey.
Do not imply mandatory YAML, Docker, SDK instrumentation, a Control baseline, a faucet, or a public-chain write.

## First proof and honest claims

The self-contained command remains a useful first proof:

```bash
ronchi demo tx-response-lost
```

The bundled Reference Application may provide known correlation and Checks, allowing the Demo to show a stronger conclusion than an uninstrumented user backend. For example, it may show that an ambiguous submission was followed by a second transaction for the same Withdrawal Intent.

For a backend connected only by changing its RPC URL, Ronchi may show two observed submissions but must not claim that they represent one economic operation. That relationship remains an Evidence Gap unless an SDK, Check, or adapter supplies it.

The initial Reference Application does not promise real SOL or SPL transfers. Copy such as “two transfers finalized” is valid only when a selected local Execution Backend and collected evidence actually support that conclusion.

## Claims to avoid

- “Every experiment compares Control and Chaos.” Control is optional.
- “Ronchi runs every application against a disposable blockchain.” The default mode is an RPC Simulator; Surfpool is an optional Execution Backend.
- “Ronchi detects application bugs automatically.” It explains supported observable behavior and adds Verdicts only when Checks justify them.
- “Ronchi provides full internal observability.” Its visibility is complete only within the declared Observation Boundary.
- “Ronchi currently tests WebSocket behavior.” Initial Traffic scope is one Solana HTTP JSON-RPC path.
- “Ronchi reproduces consensus or mainnet.” It does not claim production consensus, validator topology, or an authoritative mainnet fork.

## Visual identity

The visual system is based on regular parallel bands whose controlled deformation reveals a deviation.

Recommended direction:

- near-black background;
- white or warm-neutral parallel lines;
- one signal color for faults and deviations;
- high contrast and restrained typography;
- no blockchain cubes, chains, lightning bolts, skulls, or generic DevOps imagery.

The band motif can appear in the wordmark, documentation dividers, dashboard Timeline, fault markers, Findings, and launch graphics. Visuals should be presented as a conceptual system inspired by Ronchi patterns, not as physically calculated ronchigrams unless they actually are.

The dashboard should not require a Control-versus-Chaos split view. Its primary composition is the Continuous Lab Journal with clear visual deformation around a fault, plus filtered Investigations and Findings. Optional comparisons may use aligned side-by-side views when Comparability is established.

The CLI should remain sparse and structured. The visual identity must not add decorative output that wastes terminal space or Coding Agent tokens.

## Naming system

- Product and prose name: `Ronchi`
- Display wordmark: `RONCHI`
- CLI and executable: `ronchi`
- Project-local workspace: `ronchi/`
- SDK: `Ronchi SDK`
- Persistent evidence source: `Lab Journal`
- Continuing fault policy: `Case`
- Filtered evidence view: `Investigation`

Previous working names must not appear in current product copy, commands, paths, packages, or interface names.

## Pre-publication checks

Before a public release:

- search relevant software and developer-tool trademarks in target markets;
- check `ronchi` domains and social handles;
- check GitHub organization and repository names;
- check package and container registries;
- verify that the chosen wordmark and visual system do not copy an existing identity;
- fact-check the public historical explanation and any optical diagrams.
