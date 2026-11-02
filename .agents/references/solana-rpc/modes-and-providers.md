# Modes, reads, and provider differences

Reviewed on 2026-11-02.
Use the accepted ADRs as authority for Ronchi modes.
The shared references do not add protocol or provider coverage by themselves.

## State authority

| Mode | Authority and interpretation |
| --- | --- |
| Simulator with optional read-through | Local transaction lifecycle overlay; only explicitly allowlisted reads may use the configured upstream |
| Managed Surfpool | All application blockchain reads and writes go through local Surfpool; its datasource remains behind that boundary |

See [ADR-0011](../../../docs/adr/0011-stateful-solana-rpc-simulator.md), [ADR-0012](../../../docs/adr/0012-read-through-upstream-with-write-firewall.md), and [ADR-0013](../../../docs/adr/0013-managed-surfpool-execution-mode.md).

The non-executing simulator does not mutate account balances or execute programs.
A simulated successful transaction does not imply an upstream balance changed.
In Surfpool mode, bypassing Surfpool for account reads can mix incompatible state authorities.
Do not silently use a public fallback after a local execution-backend failure.
Ronchi does not broadcast writes to the public datasource or automatically fund signers.

## Forwarding and unsupported methods

Only the configured read allowlist grants upstream forwarding permission.
A method sounding read-only does not automatically belong to that list.
Unknown methods, transaction submission, airdrops, and provider-specific writes do not acquire permission through fallback handling.
For batches, inspect relevant individual requests before forwarding effects.
Use a fake upstream to prove which requests crossed the boundary.
Do not use real blockchain writes to verify the firewall.

For related blockhash, height, and transaction-status calls, specify their authority and how the supported history remains coherent.
Do not compare a live upstream validity bound with an unrelated simulator counter without a declared mapping.

Preserve raw upstream results separately from the RPC Boundary Result when an applied Fault changes what Ronchi returns.
A declared stale result or withheld response can differ from authoritative state intentionally.
That distinction must remain visible in evidence and Findings.

## Provider facts and observations

For method support, errors, rate limits, history retention, or node-lag behavior, record the provider, cluster, relevant options, and observation/version context.
Use current official provider documentation or a bounded captured observation for provider-specific claims.
The [Solana RPC overview](https://solana.com/docs/rpc) documents public endpoint rate-limit/blocking responses; it does not guarantee identical behavior across commercial providers.
Do not generalize one timeout, HTTP status, error message, or missing transaction result to every node.
Separate transport errors, JSON-RPC errors, execution errors, and successful responses with missing data.
Use exact versioned error definitions when a Case relies on a particular error code or payload.

## Reproducibility and scope

Live read-through depends on external state, availability, and provider policy; a seed alone does not make it deterministic.
Use controlled fixtures or fake upstreams for deterministic internal checks where appropriate.
Do not promise a product capture/replay format before that contract is decided.
Local Surfpool execution does not reproduce all provider internals or production consensus.
WebSocket, multi-provider routing, Jito, and unrelated external services remain outside the initial HTTP path unless the project explicitly changes that scope.
