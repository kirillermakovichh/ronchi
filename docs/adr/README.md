# Architectural decisions

ADRs record rationale and historical choices. Product contracts describe current behavior.
Numbered ADR paths remain stable. Read later refinements when an earlier decision is partly superseded.
Application Checks and business Verdict evaluation are removed; their mentions in older ADRs are historical.
The blockchain WebSocket transport remains deferred. The dashboard uses WebSocket today.

| ADR | Subject | Relationship |
| --- | --- | --- |
| [0001](0001-local-first-execution.md) | Pre-production local execution | Current environment boundary; read with 0011–0013 |
| [0002](0002-standalone-core-with-optional-adapters.md) | Standalone core and adapters | SDK remains optional; detailed implementation in 0019 |
| [0003](0003-compose-existing-chain-backends.md) | Compose chain backends | Superseded as the default by 0011; optional Surfpool defined in 0013 |
| [0004](0004-fully-local-open-source-product.md) | Fully local open-source product | No registration or hosted control plane |
| [0005](0005-explanation-first-outcomes.md) | Evidence and Findings | Current explanation-first model; application Checks retired |
| [0006](0006-human-and-agent-interfaces.md) | Dashboard, CLI, and local MCP | Shared Journal and token-efficient access |
| [0007](0007-declarative-custom-cases.md) | Declarative Custom Cases | Maintained primitives; no executable fault plugins |
| [0008](0008-chaos-run-first-optional-control.md) | Optional Control comparison | Run-first framing superseded by 0015; comparison responsibility remains current |
| [0009](0009-optional-go-application-sdk.md) | Go-first application SDK | Accepted direction; design and implementation deferred |
| [0010](0010-single-solana-http-rpc-path.md) | One Solana HTTP JSON-RPC path | Initial traffic boundary |
| [0011](0011-stateful-solana-rpc-simulator.md) | Non-executing RPC Simulator | Default model; optional execution is 0013 |
| [0012](0012-read-through-upstream-with-write-firewall.md) | Read-through and write firewall | Default mode only |
| [0013](0013-managed-surfpool-execution-mode.md) | Managed Surfpool | Optional exclusive state authority |
| [0014](0014-project-local-workspace.md) | Project-local workspace | Initial discovery and layout refined by the Lab lifecycle contract |
| [0015](0015-persistent-lab-first.md) | Persistent Lab first | Primary workflow |
| [0016](0016-continuous-lab-journal.md) | Continuous Lab Journal | Investigation views share one record |
| [0017](0017-case-driven-fault-model.md) | Continuing Case policy | Initial repetition, sequencing, and recovery defined in Cases and Custom Cases |
| [0018](0018-ronchi-product-name.md) | Ronchi name and positioning | Current working brand |
| [0019](0019-resource-efficient-rust-core.md) | Native Rust stack and resource model | Operational safety defaults exist; measured performance release gates are deferred |
| [0020](0020-installed-native-tool-distribution.md) | Installed Ronchi and private Surfpool | Refines 0014 for executables; runtime data stays project-local |


See the [documentation map](../README.md) for current contracts.
