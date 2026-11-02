# Transactions and status

These notes summarize selected official method semantics reviewed on 2026-11-02.
Check the linked method contract for exact wire types and supported configuration.
Use [modes-and-providers.md](modes-and-providers.md) when applying these facts to Ronchi.

## Submission and identity

[`sendTransaction`](https://solana.com/docs/rpc/http/sendtransaction) accepts a signed transaction and returns its first signature when the RPC accepts it.
That success does not guarantee processing or confirmation.
The method forwards the transaction without changing it; its signature identity comes from the signed input, not an RPC-generated business identifier.
Preflight may verify signatures and simulate execution unless skipped.
Keep preflight rejection distinct from later execution failure and from a transport failure.
Verify `encoding`, `skipPreflight`, `preflightCommitment`, `maxRetries`, and `minContextSlot` against the method's current contract when supporting them.
Do not assume the non-executing simulator performs real preflight execution.

Resending the same signed transaction preserves its transaction identity.
Changing its message, such as replacing its blockhash, and signing again creates another signed transaction to track.
Different signatures alone do not establish whether application business intent is the same.
A lost submission response leaves the caller without that response; it does not prove rejection or non-execution.
Keep the observed RPC Boundary Result separate from any client-side interpretation.

## Status queries

[`getSignatureStatuses`](https://solana.com/docs/rpc/http/getsignaturestatuses) returns a status entry or null for each supplied signature.
By default it searches the recent status cache; `searchTransactionHistory` requests a broader history search.
Its documented configuration is not a generic `commitment` selector.
Check the method-specific schema instead of adding parameters used by other calls.

Treat a null entry as unavailable status from that query context, not a confirmed failed or never-submitted transaction.
Keep a returned status's `err`, `slot`, `confirmations`, and `confirmationStatus` meanings distinct.
In particular, null `confirmations` inside an existing status is different from a null status entry.
An execution error and a confirmation level answer different questions; finality is not proof of successful application execution.
Preserve request order and method limits when implementing the response array.

## Case implications

Track accepted transaction identity separately from whether Ronchi returned the submission response.
Model later status availability independently from that earlier boundary response where the Case requires it.
Do not invent a new transaction record merely because the same signed transaction is submitted again.
Do not replace an unknown outcome with a terminal failed state for convenience.
Exact retention, status progression, and simulated execution outcomes remain mode- and Case-specific contracts.
Use explicit Journal Evidence before making a Finding about their relationship.

For confirmation and expiry context, see the official [confirmation guide](https://solana.com/developers/cookbook/transactions/confirmation) and [blockhash-and-commitment.md](blockhash-and-commitment.md).
