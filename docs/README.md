# Documentation map

This is the entry point for maintained Ronchi knowledge.
Read the smallest set relevant to the task.
Product documents describe intended behavior; they do not prove an implementation exists.
Use the root [README](../README.md) for project stage and [AGENTS.md](../AGENTS.md) for working rules and skills.

## Find the right document

| Task | Start here |
| --- | --- |
| Understand the product, user, scope, and observation limits | [Product overview](product/overview.md) |
| Connect a backend, investigate a Lab, or understand human/agent/CI flows | [Developer workflows](product/workflows.md) |
| Define Case coverage or distinguish the first catalog from later ideas | [Cases and coverage](product/cases.md) |
| Author or validate declarative YAML and primitive composition | [Custom Cases](product/custom-cases.md) |
| Check simulator RPC methods, formats, preflight, history, Read-through, or batches | [Solana RPC contract](product/solana-rpc-contract.md) |
| Design Journal facts, ordering, persistence, redaction, or Findings | [Evidence and Findings](product/evidence-and-findings.md) |
| Implement dashboard, CLI/MCP operations, schemas, or bounded queries | [Investigation interfaces](product/investigation-interfaces.md) |
| Start, attach to, stop, or recover a native Lab | [Persistent Lab lifecycle](product/lab-lifecycle.md) |
| Work on the bundled Demo or its intent/reconciliation behavior | [Reference Withdrawal Relayer](product/reference-application.md) |
| Observe application state or evaluate a bounded Verdict without an SDK | [Checks and Verdicts](product/checks-and-verdicts.md) |
| Write public copy or design visual identity | [Positioning and brand](product/positioning-and-brand.md) |
| Clarify a domain term | [CONTEXT.md](../CONTEXT.md) |
| Identify an unresolved or deferred choice | [Open questions](open-questions.md) |
| Review the completed design baseline and remaining validation | [Product plan](../tasks/plan.md) |
| Start implementation in dependency order | [Development queue](../tasks/todo.md) |
| Complete tasks 15–24 with a coordinator and parallel workers | [Remaining implementation run](../tasks/remaining-implementation-run.md) |
| Start that run from the Zed AI panel | [Zed launch guide](../tasks/zed-run.md) |
| Set up build tools or select core/dashboard dependencies | [Development stack](development.md) |
| Understand the native Rust stack and resource model | [ADR-0019](adr/0019-resource-efficient-rust-core.md) |
| Understand mode authority and upstream write restrictions | [ADR-0011](adr/0011-stateful-solana-rpc-simulator.md), [ADR-0012](adr/0012-read-through-upstream-with-write-firewall.md), [ADR-0013](adr/0013-managed-surfpool-execution-mode.md) |
| Understand persistent Labs, Journal lifetime, or continuing Cases | [ADR-0015](adr/0015-persistent-lab-first.md), [ADR-0016](adr/0016-continuous-lab-journal.md), [ADR-0017](adr/0017-case-driven-fault-model.md) |
| Implement or validate Solana RPC semantics | [Shared protocol references](../.agents/references/solana-rpc/index.md) |
| Find the reason for another accepted design choice | [Decision index](#decisions) |

## Document authority and maintenance

| Knowledge | Maintained source |
| --- | --- |
| Product definition, users, and boundaries | `product/overview.md` |
| User journeys and command examples | `product/workflows.md` |
| Case status, candidate catalog, and Coverage Claims | `product/cases.md` |
| Custom Case YAML, primitive versions, validation, and provenance | `product/custom-cases.md` |
| Solana RPC compatibility scope and simulator method behavior | `product/solana-rpc-contract.md` |
| Journal event facts, ordering, Findings, persistence, and retention | `product/evidence-and-findings.md` |
| Dashboard behavior, shared CLI/MCP operations, and bounded query schemas | `product/investigation-interfaces.md` |
| Project/config discovery, process ownership, readiness, stop, and recovery | `product/lab-lifecycle.md` |
| Demo consumer and reference behavior | `product/reference-application.md` |
| Application observation adapter, evaluation, and Verdict requirements | `product/checks-and-verdicts.md` |
| Messaging, names, and visual identity | `product/positioning-and-brand.md` |
| Domain vocabulary | Root `CONTEXT.md` |
| Decisions, rationale, alternatives, and supersession | `adr/` |
| Open or deliberately deferred choices | `open-questions.md` |
| External evidence and historical analysis | `research/` |
| Agent procedures and technical reading routes | Root `AGENTS.md` and `.agents/` |
| Design completion and ordered implementation work | `tasks/plan.md` and `tasks/todo.md` |
| Initial toolchain, build layout, and supporting stack | `development.md` and root `rust-toolchain.toml` |

Update the authoritative topic rather than copying a new full product summary into several documents.
A short entry-point summary or an ADR's historical rationale is not a second specification.
Use current code and verification for implementation claims; report disagreements with accepted intent instead of silently changing it.
Keep examples and deferred interfaces distinct from compatibility promises.
Research observations and development skills do not override accepted product decisions.

Use lowercase kebab-case for ordinary Markdown filenames.
Preserve conventional entry-point names such as `README.md`, `AGENTS.md`, `CONTEXT.md`, and `SKILL.md`.
Keep numbered ADR filenames and historical decisions stable; link refinements or supersession explicitly.
When renaming or consolidating a topic, update inbound links and reading routes and account for its retained decisions, limitations, evidence, and open questions.
Do not create a new maintained document for temporary work notes or duplicate definitions.
The initial contracts are recorded; implementation details can be refined during prototype tests.
If the user requests another design interview, use one decision at a time under the project skill.

## Decisions

These documents record design choices, not implementation verification.
Read related later decisions when an earlier ADR is refined.

| ADR | Subject | Relationship |
| --- | --- | --- |
| [0001](adr/0001-local-first-execution.md) | Pre-production local execution | Current environment boundary; read with 0011–0013 |
| [0002](adr/0002-standalone-core-with-optional-adapters.md) | Standalone core and adapters | SDK remains optional; detailed implementation in 0019 |
| [0003](adr/0003-compose-existing-chain-backends.md) | Compose chain backends | Superseded as the default by 0011; optional Surfpool defined in 0013 |
| [0004](adr/0004-fully-local-open-source-product.md) | Fully local open-source product | No registration or hosted control plane |
| [0005](adr/0005-explanation-first-outcomes.md) | Evidence, Findings, and optional Verdicts | Current; persistent workflow clarified by 0015–0016 |
| [0006](adr/0006-human-and-agent-interfaces.md) | Dashboard, CLI, and local MCP | Shared Journal and token-efficient access |
| [0007](adr/0007-declarative-custom-cases.md) | Declarative Custom Cases | Maintained primitives; no executable fault plugins |
| [0008](adr/0008-chaos-run-first-optional-control.md) | Optional Control comparison | Run-first framing superseded by 0015; comparison responsibility remains current |
| [0009](adr/0009-optional-go-application-sdk.md) | Go-first application SDK | Accepted direction; design and implementation deferred |
| [0010](adr/0010-single-solana-http-rpc-path.md) | One Solana HTTP JSON-RPC path | Initial traffic boundary |
| [0011](adr/0011-stateful-solana-rpc-simulator.md) | Non-executing RPC Simulator | Default model; optional execution is 0013 |
| [0012](adr/0012-read-through-upstream-with-write-firewall.md) | Read-through and write firewall | Default mode only |
| [0013](adr/0013-managed-surfpool-execution-mode.md) | Managed Surfpool | Optional exclusive state authority |
| [0014](adr/0014-project-local-workspace.md) | Project-local workspace | Initial discovery and layout refined by the Lab lifecycle contract |
| [0015](adr/0015-persistent-lab-first.md) | Persistent Lab first | Primary workflow |
| [0016](adr/0016-continuous-lab-journal.md) | Continuous Lab Journal | Investigation views share one record |
| [0017](adr/0017-case-driven-fault-model.md) | Continuing Case policy | Initial repetition, sequencing, and recovery defined in Cases and Custom Cases |
| [0018](adr/0018-ronchi-product-name.md) | Ronchi name and positioning | Current working brand |
| [0019](adr/0019-resource-efficient-rust-core.md) | Native Rust stack and resource model | Operational safety defaults exist; measured performance release gates are deferred |

## Research

Read research for evidence or alternatives, not as an implementation task list.
Sources and observations may describe earlier versions or an earlier product snapshot.

- [Tool landscape](research/tool-landscape.md): direct and adjacent tools, community signals, and adoption limits.
- [Human and agent interfaces](research/ai-native-developer-tools.md): dashboard/CLI/MCP patterns and selective evidence access.
- [Post-transaction state options](research/post-transaction-state-options.md): simulation, overlays, execution backends, hydration, and Surfpool integration research.

