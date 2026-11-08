# Documentation map

Start with the smallest document needed for the task.
Current product contracts describe supported behavior and explicit deferred scope; recorded checks remain bounded evidence.
The root [README](../README.md) is the quickstart. [AGENTS.md](../AGENTS.md) owns agent rules and skill selection.

## Current documents

Each topic has one maintained owner. Update that owner instead of copying its specification into setup notes or tasks.

| Need / owner | Document |
| --- | --- |
| Product, users, value, supported environments and limits | [Product overview](product/overview.md) |
| Launch, connect a backend, activate a Case and investigate | [Developer workflows](product/workflows.md) |
| Workspace, Observations, live updates, browser access and RPC metrics | [Dashboard](product/dashboard.md) |
| CLI/MCP operations, shared query semantics, schemas and errors | [Investigation interfaces](product/investigation-interfaces.md) |
| Lab startup/stop, configuration, ownership and managed Surfpool | [Lab lifecycle](product/lab-lifecycle.md) |
| Built-in Cases, Coverage Claims and clearly deferred candidates | [Cases and coverage](product/cases.md) |
| YAML, maintained primitives, validation and Custom Finding rules | [Custom Cases](product/custom-cases.md) |
| Solana methods, formats, clocks, fees, nonce and Read-through | [Solana RPC contract](product/solana-rpc-contract.md) |
| Journal facts, ordering, Findings, storage, redaction and retention | [Evidence and Findings](product/evidence-and-findings.md) |
| Bundled relayer, intent storage and crash recovery | [Reference Application](product/reference-application.md) |
| Messaging, name, visuals and public launch checks | [Positioning and brand](product/positioning-and-brand.md) |
| Install, update, configure PATH and remove Ronchi | [Installation](development.md#curl-installation-layout) |
| Build, dependencies, source layout, verification and commit rules | [Development](development.md) |
| Domain terms | [CONTEXT.md](../CONTEXT.md) |
| Deliberately unresolved or deferred choices | [Open questions](open-questions.md) |
| Published release, local changes, process state and verification | [Implementation progress](../tasks/implementation-progress.md) |
| Completion checklist, task evidence and remaining validation | [Development queue](../tasks/todo.md) |
| Current maintenance scope | [Working plan](../tasks/plan.md) |

Application Checks and business Verdict evaluation are removed.
Historical references do not authorize restoring them.
The blockchain traffic boundary is HTTP JSON-RPC; dashboard WebSocket is a separate implemented browser transport.

## Decisions

The [ADR index](adr/README.md) lists the decisions and their refinement/supersession relationships.
Numbered ADR paths and rationale remain stable. Current topic contracts own implementation details.
The [shared Solana references](../.agents/references/solana-rpc/index.md) supply protocol evidence; they do not expand Ronchi capabilities.

## Research

These sources explain evidence, alternatives and limits. They do not override current contracts.

| Source | Role |
| --- | --- |
| [Tool landscape](research/tool-landscape.md) | Direct/adjacent tools, community signals and adoption limits |
| [Human and agent interfaces](research/ai-native-developer-tools.md) | Dashboard, CLI/MCP patterns and selective evidence access |
| [Post-transaction state options](research/post-transaction-state-options.md) | Simulator/overlay/execution alternatives and hydration |
| [Resource measurement harness](research/resource-measurement-harness.md) | Reproduction method, recorded prototype measurements and limits |

## Historical records

Read these only when investigating prior integration evidence or an unresolved diagnostic observation.
They are not launch instructions or active assignments.

- [Completed implementation run, 2026-11-03](../tasks/history/implementation-run-2026-11-03.md): ownership, commits, integration checks and cleanup at that time.
- [Unconfirmed normal-down observation](../tasks/history/unconfirmed-down-observation.md): original evidence gaps and the required record if it recurs.

## Maintenance

Keep ordinary filenames lowercase kebab-case. Preserve conventional README/AGENTS/CONTEXT/SKILL names and numbered ADR addresses.
When moving or consolidating a topic, update inbound paths and heading links.
Keep one task-completion checklist in `tasks/todo.md`; keep current session notes short.
Delete completed one-off launch instructions after their durable rules and unique evidence have a maintained home.
Keep research, historical observations and current requirements visibly separate.
