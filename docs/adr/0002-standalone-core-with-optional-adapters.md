# Standalone core with optional adapters

The primary Ronchi module will be a standalone process exposing an HTTP JSON-RPC data plane and a separate control interface. The initial traffic scope is further bounded by [ADR-0010](./0010-single-solana-http-rpc-path.md). A local web dashboard, structured CLI, and local MCP server are first-class interfaces over this core. A native binary and container image run the same core; Testcontainers libraries and an optional first-party application SDK are adapters rather than required interfaces. The SDK starts with Go and emits correlated application evidence to the standalone core. Hosted product components are excluded by ADR-0004.

The standalone core provides a useful base Observation Boundary from RPC requests, simulator lifecycle decisions, injected faults, RPC Boundary Results, and later RPC calls. The optional SDK and Checks extend that boundary with Client Outcomes and application state. All interfaces must report the active boundary and Evidence Gaps so deeper integration strengthens conclusions without becoming a hidden prerequisite.

## Considered Options

- An embedded SDK was rejected as the primary interface. An optional first-party SDK is accepted only as an evidence-enrichment interface over the standalone core.
- A container-only identity was rejected because packaging should not dictate local or CI workflow.
- A Kubernetes-first platform was rejected because its operational surface is disproportionate to the local developer use case.

