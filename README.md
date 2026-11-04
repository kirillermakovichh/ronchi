# Ronchi

**Make hidden failures visible.**

An open-source, local-first reliability lab for blockchain applications.
Ronchi is designed to show how a real backend handles controlled Solana RPC failures through an explanatory Timeline and evidence-linked Findings.

**Project stage:** local investigation prototype. Persistent Labs, a durable bounded Journal, the embedded dashboard, CLI/MCP queries, application Checks, built-in Cases, and bounded YAML Custom Cases are implemented. Allowlisted Read-through has a write firewall. Optional pinned Surfpool supplies local execution. The API is not stable.

The simulator supports the declared transaction encodings, lifecycle lookups, explicit lookup-table/nonce state, parsed projections, and fee quotes.
See the [RPC contract](docs/product/solana-rpc-contract.md) for compatibility limits and the [development queue](tasks/todo.md) for verification and remaining external validation.

Prepare the embedded assets before building a fresh checkout:

```bash
npm --prefix dashboard ci
npm --prefix dashboard run build
cargo build --locked
```

Use `cargo run --locked -- --help` for available commands.
Use `ronchi schema lab.up` for the current request and response schemas.
`lab.status` and `lab.down` also have offline schemas.
See [startup configuration](docs/product/lab-lifecycle.md#implemented-configuration-surface) for supported settings.

Start a Lab in the terminal:

```bash
ronchi up
```

Ronchi prints its RPC URL and dashboard link after readiness.
Keep this terminal open and start your backend in another terminal with that RPC URL.
Press Ctrl-C to stop the Lab and retain its Journal.
Ronchi does not stop your backend.
In an interactive terminal, Ronchi shows an animated Lab panel with the current Case and Journal watermark.
Use `ronchi up --no-animation` for a static panel.
Set `NO_COLOR=1` to disable color and emphasis.
JSON, CI, and redirected output stay plain.

For scripts or background use, select `--detach` explicitly:

```bash
ronchi up --detach --rpc-listen 127.0.0.1:0 --control-listen 127.0.0.1:0
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
