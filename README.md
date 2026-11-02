# Ronchi

**Make hidden failures visible.**

An open-source, local-first reliability lab for blockchain applications.
Ronchi is designed to show how a real backend handles controlled Solana RPC failures through an explanatory Timeline and evidence-linked Findings.

**Project stage:** product definition and development guidance. There is no runtime implementation or stable CLI/API in this repository yet. Command examples describe intended behavior.

## Intended use

Start a persistent local Lab, point the independently running backend at Ronchi's RPC endpoint, activate a continuing Case, and investigate the recorded behavior.
Humans use a local dashboard; Coding Agents use compact CLI or MCP queries over the same Journal.
No application SDK, registration, or hosted service is required for the base workflow.

Default mode combines a stateful transaction-lifecycle simulator with optional allowlisted Read-through.
It never broadcasts transactions upstream or claims real execution.
Optional managed Surfpool provides local execution when later account reads must reflect transactions.
Both modes are limited to local development, CI, and isolated staging; Ronchi never enters a production application's RPC path.

## Start reading

| Need | Read |
| --- | --- |
| Product value, user, scope, and limits | [Product overview](docs/product/overview.md) |
| Find the document for a task | [Documentation map](docs/README.md) |
| Understand domain terms | [Domain glossary](CONTEXT.md) |
| Work on the project as an agent | [Agent rules and skill routing](AGENTS.md) |
| See unresolved decisions | [Open questions](docs/open-questions.md) |
| Start development from the recorded baseline | [Development queue](tasks/todo.md) |

The documentation map routes to workflows, Cases, the reference relayer, branding, research, and architectural decisions.
