# Ronchi

**Make hidden failures visible.**

An open-source, local-first reliability lab for blockchain applications.
Ronchi is designed to show how a real backend handles controlled Solana RPC failures through an explanatory Timeline and evidence-linked Findings.

**Project stage:** native lifecycle prototype. Persistent `up`, `status`, and `down` controls use a bounded durable Journal. Seven local bootstrap RPC methods are implemented. Durable transaction admission, lifecycle lookup, legacy/v0/v1 encodings, explicit lookup/nonce state, parsed projection, and local fee quotes are implemented. Cases and the dashboard remain unimplemented. The API is not stable.

Build with `cargo build --locked`.
Use `cargo run --locked -- --help` for available commands.
Use `ronchi schema lab.up` for the current request and response schemas.
`lab.status` and `lab.down` also have offline schemas.
See [startup configuration](docs/product/lab-lifecycle.md#implemented-configuration-surface) for supported settings.

A lifecycle-only Lab can use dynamic loopback ports:

```bash
ronchi up --rpc-listen 127.0.0.1:0 --control-listen 127.0.0.1:0
ronchi status --json
ronchi down
```

The readiness result lists implemented capabilities.
The RPC listener supports the [local bootstrap profile](docs/product/solana-rpc-contract.md#implemented-bootstrap-profile).
Other capabilities return explicit JSON-RPC errors.

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
