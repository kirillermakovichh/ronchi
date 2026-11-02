# Blockhash, height, slot, and commitment

Reviewed on 2026-11-02.
Use exact method contracts rather than one global interpretation for every RPC call.

## Validity and progress

[`getLatestBlockhash`](https://solana.com/docs/rpc/http/getlatestblockhash) returns a blockhash and `lastValidBlockHeight`, together with response context.
The validity bound is a block height, not a slot number or a fixed duration in seconds.
For the corresponding recent-blockhash lifetime, the height comparison crosses the bound when current block height is greater than `lastValidBlockHeight`.
Keep the chosen node/state view consistent when evaluating that comparison.
An expired hash prevents new processing under that lifetime rule; it does not retroactively fail a transaction already processed with the hash.

[`getBlockHeight`](https://solana.com/docs/rpc/http/getblockheight) and [`getSlot`](https://solana.com/docs/rpc/http/getslot) expose different quantities.
Do not substitute slot advancement for block-height advancement.
Do not derive an exact expiry instant by multiplying a slot count by an assumed fixed slot duration.
The [confirmation guide](https://solana.com/developers/cookbook/transactions/confirmation) explains recent-blockhash expiry and why commitment or node lag can affect recognition.

[`isBlockhashValid`](https://solana.com/docs/rpc/http/isblockhashvalid) answers validity for the requested commitment and response context.
Preserve that context when correlating it with other calls.
A generic blockhash-not-found rejection alone does not establish why that node did not recognize the hash.
Other transaction lifetime mechanisms require their own verified contract; do not apply recent-blockhash rules to them automatically.

## Commitment

The [official RPC overview](https://solana.com/docs/rpc#configuring-state-commitment) describes these views:

| Value | Meaning |
| --- | --- |
| `processed` | The node's most recently processed block; rollback is still possible |
| `confirmed` | A block directly voted on by a supermajority of active stake |
| `finalized` | The strongest finalized view described by the RPC contract |

Only apply a commitment parameter where the method actually supports it.
Verify method defaults instead of assuming every call accepts the same configuration.
Keep `preflightCommitment`, request commitment, `minContextSlot`, and returned `context.slot` distinguishable in supported request/evidence models.
Do not reinterpret response context as a transaction's execution slot or confirmation count.

## Simulator implications

Keep simulated clock, slot, block height, hash validity, and transaction status distinct wherever the supported Case depends on them.
Document any deliberate simplification in the Coverage Claim.
If a test needs a boundary value, include states immediately before, at, and after the relevant validity bound.
Do not use a fixed sleep as the only evidence that a simulated hash expired.

Simulated commitment progression is a controlled RPC history.
It is not evidence of validator voting, production consensus, or real finality.
Whether a particular regression or stale view is supported must come from the Case and mode contract.
