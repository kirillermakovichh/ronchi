# Tool Landscape and Community Signals

Research snapshot, not a current capability promise or a representative adoption survey. Preserve the context of historical usage and version observations. Current Ronchi scope is defined by the [product overview](../product/overview.md).

This is a qualitative research summary, not a statistically representative survey. Evidence was drawn from project documentation, GitHub issues and discussions, Hacker News, and Reddit. The strongest signal comes from repeated workflow preferences across adjacent tools because the direct blockchain RPC fault-injection projects are still immature.

## Direct blockchain-focused tools

### [blockchain-chaos](https://github.com/ForTheLoveOfPointers/blockchain-chaos)

The closest direct analogue found: an EVM-focused proxy with reproducible YAML scenarios, RPC faults, and simulated reorgs. At the time of research it had 31 commits, 0 stars, and 2 issues. It validates that the idea is emerging, but provides no meaningful community-usage evidence yet.

### [eth-rpc-proxy](https://github.com/aodhgan/eth-rpc-proxy)

A TypeScript EVM HTTP/WebSocket proxy offering forward, fail, hang, and delay behavior with deterministic queues. It is embedded as a library and had no visible adoption at the time of research. It demonstrates a useful low-level mechanism but not a complete developer workflow or explanatory evidence model.

### `@hashspan/faults`

A newly published EVM HTTP fault proxy intended to sit between a client and Anvil. It supports hangs, errors, resets, truncated responses, and wrong results. Its freshness and single-maintainer status make it a useful design reference rather than evidence of established product demand.

### [Solana Bastion](https://github.com/recruitabx/solana-bastion)

A TypeScript Solana resilience SDK with an in-process mock cluster and deterministic chaos transport. Its side-by-side naive-versus-resilient demonstration is strong, but the approach couples tests to a language and client stack. It also had no visible community adoption at the time of research.

### [Attacknet](https://github.com/crytic/attacknet)

A Trail of Bits and Ethereum Foundation project for subjecting blockchain devnets to network and side-channel attacks using Kubernetes, Kurtosis, and Chaos Mesh. It targets node and network resilience rather than off-chain application RPC handling. Its operational weight is justified for protocol infrastructure but inappropriate as the default Ronchi developer loop.

## Adjacent tools with mature usage signals

### [Toxiproxy](https://github.com/Shopify/toxiproxy)

Toxiproxy's durable pattern is a standalone proxy data plane controlled through an HTTP API and optional language clients. Applications use their normal network clients and change only the dependency endpoint. Shopify reports using it in development and test environments since 2014.

Community comments repeatedly value the lack of application rewrites and usefulness for WebSocket and resilience testing. Friction appears around networking, port configuration, and the absence of richer declarative or protocol-aware behavior.

**Lesson for Ronchi:** keep the RPC boundary as the primary interface, separate data and control planes, and provide named protocol Cases rather than exposing only low-level connection faults.

Sources: [Toxiproxy discussion](https://news.ycombinator.com/item?id=37842301), [WebSocket and configuration feedback](https://news.ycombinator.com/item?id=32115791).

### [Testcontainers](https://testcontainers.com/)

Developers praise the ability to start real disposable dependencies from the normal test runner and run the same integration tests locally and in CI. Criticism focuses on Docker availability, Docker-in-Docker, interaction with existing Compose or Kubernetes workflows, opaque failures, resource use, and slow repeated startup.

**Lesson for Ronchi:** provide Testcontainers adapters, but do not force every user or every interactive session through a language SDK and ephemeral container lifecycle. A persistent interactive Lab is important for fast investigation.

Source: [large Testcontainers community discussion](https://news.ycombinator.com/item?id=39531536).

### [WireMock](https://wiremock.org/) and [Microsoft Dev Proxy](https://learn.microsoft.com/en-us/microsoft-cloud/dev/dev-proxy/overview)

WireMock users value stateful scenarios and recording an unreliable dependency for stable CI. Dev Proxy demonstrates a discoverable CLI with ready-made presets and the ability to run an application normally while the proxy changes API behavior.

Common friction includes certificate setup, request interception that silently misses traffic, plugin ordering, and recorded responses that drift from the real dependency.

**Lesson for Ronchi:** prefer an explicit RPC endpoint over a global system proxy, make fault coverage visible, ship built-in Cases, and treat recordings as versioned evidence that can become stale.

Source: [WireMock community discussion](https://news.ycombinator.com/item?id=17924841).

### Local cloud and chain emulators

LocalStack users value cheap, repeatable local integration testing but repeatedly warn that emulator differences can create false confidence. EVM developers commonly use Anvil forks for most scenario testing and reserve public testnets for final infrastructure checks. Solana's traditional test validator has startup, fixture, lifecycle, and WebSocket friction; Surfpool targets this with fast startup, lazy account cloning, and RPC/WebSocket support.

**Lesson for Ronchi:** emulator fidelity limits must be explicit. This research originally suggested integrating an execution engine, but the accepted product boundary is narrower: simulate only the RPC-visible state needed for off-chain reliability analysis and make the absence of real execution explicit.

Sources: [LocalStack 1.0 community discussion](https://news.ycombinator.com/item?id=32193132), [Anvil snapshot concurrency issue](https://github.com/foundry-rs/foundry/issues/11316), [Surfpool comparison](https://github.com/solana-foundation/solana-dev-skill/blob/main/skills/solana-dev/references/surfpool/overview.md), [Solana test-validator WebSocket lifecycle issue](https://github.com/solana-labs/solana/issues/28702).

### [Chaos Mesh](https://github.com/chaos-mesh/chaos-mesh) and Kubernetes chaos platforms

These platforms offer broad infrastructure-level injection but require CRDs, controllers, admission webhooks, RBAC, privileged daemons, and cluster-level operations. That power fits platform and protocol teams, not the shortest local feedback loop for an application developer.

**Lesson for Ronchi:** Kubernetes support is a future packaging adapter, not the product's primary interface or architecture.

## Repeated usability preferences

Across the researched communities, developers consistently prefer:

1. Running the real application with minimal or no code changes.
2. A one-command local start and an immediately visible endpoint.
3. Reusing the project's existing integration-test command.
4. Real disposable dependencies where practical, without pretending they are perfect production replicas.
5. Deterministic reproduction and an explanation of what actually happened.
6. Built-in examples or presets before custom configuration.
7. Docker compatibility without making Docker the product identity.
8. A useful interactive workflow in addition to CI automation.

## Product implication

The strongest opportunity is not merely another chaos proxy. It is a protocol-aware local Lab that combines coherent stateful RPC simulation, controlled fault injection, and a Timeline capable of attributing behavior to the application, RPC Simulator, or Ronchi fault layer. Real execution is outside the default simulator model; optional managed Surfpool is defined separately in [ADR-0013](../adr/0013-managed-surfpool-execution-mode.md).

