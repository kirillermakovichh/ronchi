# Solana RPC development references

Reviewed on 2026-11-02.
Use these shared notes when implementing or validating Solana RPC behavior.
They are selected protocol facts and development guidance, not a complete RPC specification or a shipped Ronchi compatibility claim.

| Topic | Read |
| --- | --- |
| Submission, signatures, retries, missing status, and execution results | [transactions-and-status.md](transactions-and-status.md) |
| Blockhash lifetime, block height, slot, and commitment | [blockhash-and-commitment.md](blockhash-and-commitment.md) |
| Simulator, read-through, Surfpool, and provider-specific behavior | [modes-and-providers.md](modes-and-providers.md) |

Read only the relevant topic.
Use the official method documentation linked beside a fact when implementing its exact parameters, response shape, or edge cases.
If behavior is version-sensitive or sources disagree, check the relevant node/SDK version or official implementation before changing a contract.
Do not import a source skill's preferred JS, Anchor, wallet, signing, funding, or deployment stack.

Keep three kinds of knowledge separate:

1. Protocol facts supported by official documentation or versioned implementation evidence.
2. Accepted Ronchi decisions in the [ADR index](../../../docs/README.md#decisions) and domain terms in [CONTEXT.md](../../../CONTEXT.md).
3. Simplifications, unsupported capabilities, and unresolved choices in the current implementation or Case Coverage Claim.

New documentation upstream does not silently expand Ronchi's supported protocol surface.
Update these references when the relevant protocol support changes; do not fetch an entire external manual for every task.
Provider-specific claims require that provider's documentation or a bounded recorded observation with endpoint/version context.
An observation from one endpoint is not a universal Solana guarantee.
