# Initial single Solana HTTP JSON-RPC path

Application Checks and business Verdict evaluation were removed by user decision. References to them below are historical and do not authorize reimplementation. See [current scope](../product/overview.md) and [task 36](../../tasks/todo.md#36-remove-application-checks).

The initial Ronchi traffic boundary is one configured Solana HTTP JSON-RPC path between the System Under Test and the Ronchi RPC Simulator. Base observability claims are limited to received requests, simulator lifecycle decisions, applied faults, RPC Boundary Results, and later requests on that path. Client Outcomes and internal application behavior require correlated optional SDK or Check evidence.

WebSocket, fallback and multi-provider routing, Jito and other provider-specific APIs, OS-level traffic interception, and unrelated external services are deferred.

## Considered Options

- Supporting every blockchain access path initially was rejected because each transport and provider interface adds separate interception, correlation, and fidelity requirements.
- OS-level traffic interception was rejected because it is invasive, platform-dependent, and still lacks blockchain semantics.
- A single HTTP JSON-RPC path preserves the core transaction and chain-read use cases while keeping coverage claims precise.

## Consequences

- Ronchi must never imply visibility into blockchain traffic outside the configured HTTP JSON-RPC path.
- The first Case catalog prioritizes transaction submission, blockhash lifetime, confirmation/status queries, and chain reads.
- Deferred transports and provider paths may be added later as explicit capabilities with their own Coverage Claims.
