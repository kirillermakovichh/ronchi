# Solana RPC contract

Status: accepted product contract. This defines intended coverage, not implemented support or a stable wire API.
The [product overview](overview.md#execution-modes) owns product scope and mode authority.
[Cases and coverage](cases.md) owns Fault policies and Coverage Claims.
Use the [shared protocol references](../../.agents/references/solana-rpc/index.md) and official method contracts for protocol facts.

## Local methods

In simulator mode, these methods use one coherent local Simulated Chain State:

| Method | Intended behavior |
| --- | --- |
| `getLatestBlockhash` | Return a local recent blockhash and its last valid block height |
| `sendTransaction` | Accept a signed transaction under the submission contract below |
| `getSignatureStatuses` | Return the local lifecycle view for requested signatures |
| `getBlockHeight` | Return simulated block height at the supported requested commitment |
| `isBlockhashValid` | Evaluate local recent-blockhash validity |
| `getSlot` | Return simulated slot at the supported requested commitment |
| `getTransaction` | Return a retained signed transaction under the lookup contract below |
| `getHealth` | Report local Lab readiness under the declared simulator interpretation |
| `getVersion` | Report the verified local compatibility profile; it does not copy upstream software identity |
| `getGenesisHash` | Return the synthetic local network identity; it does not copy the upstream genesis hash |
| `getFeeForMessage` | Quote the fee for a message under the declared local fee model |

The native Ronchi version and its partial compatibility boundary remain explicit in Lab information.
The implementation must verify the reported compatibility profile before making a compatibility claim.

### Implemented bootstrap profile

Task 4 implements `getHealth`, `getVersion`, `getGenesisHash`, `getSlot`, `getBlockHeight`, `getLatestBlockhash`, and `isBlockhashValid`.
Task 5 adds `sendTransaction`, `getSignatureStatuses`, and `getTransaction`.
The current partial profile is `local-simulator-v1`.
`getVersion` reports `0.0.0-ronchi.<native-version>` and an explicit partial-profile extension.
It does not claim an Agave release or feature set.
The genesis identity and blockhashes derive from a domain-separated local Lab identity.
No public network identity, version, or datasource is copied.

Production time derives from a monotonic clock at rate 1:1.
A controlled time source is injectable through the Rust API for HTTP verification; it is not remotely writable.
The default slot lasts 400 ms and each slot produces one block.
The initial slot and height are zero and remain separate coordinates from elapsed nanoseconds.
The confirmed view trails processed time by 2 seconds; finalized trails by 4 seconds.
These are controlled simulator views, not validator votes or consensus-depth claims.
The default commitment is finalized.
Context checks use the selected view, including `minContextSlot` on slot, height, latest-hash, and validity methods.
A future minimum returns `-32016` with the actual `contextSlot`.

Each generated blockhash remains valid through its issue height plus 150 blocks, inclusive.
A hash not yet present in the selected view is unavailable there.
Validity uses local block height; it does not use elapsed seconds or an unrelated public bound.
The hash encodes its issue height in eight little-endian bytes and authenticates local provenance with 24 domain-separated SHA-256 bytes.
This permits constant-time lifetime lookup without retaining unlimited clock history in memory.

The HTTP request budget is 64 KiB and the batch maximum is 64 elements.
Responses are capped at 256 KiB; requests have a 30-second ordinary deadline.
One HTTP exchange uses one connection in this initial resource profile.
Notifications execute without JSON-RPC responses; an all-notification exchange returns empty HTTP `204`.
Mixed batches retain per-element IDs, errors, internal identities, and Journal records.
Oversized or empty batches are rejected before dispatching elements.
Malformed JSON uses `-32700`; invalid envelopes use `-32600`; invalid method parameters use `-32602`.
Unknown methods use `-32601`.
Known but unimplemented capabilities use `-32000` with `data.code:unsupported_capability`.
No error path forwards requests or executes instructions.

Journal registrations and prepared-response facts precede observed transport completion.
A forced/cancelled or failed transport remains separate from the prepared result.
`http_write_completed` records server transport completion; client receipt and application outcome remain unknown.

Method selection and bank/context semantics were cross-checked against [Agave v3.0.10 RPC source](https://github.com/anza-xyz/agave/blob/v3.0.10/rpc/src/rpc.rs).
The official method-page fetch returned HTTP `429` in this session; the checked-in references and pinned implementation provide the available evidence.
This bootstrap verification does not establish the full accepted format matrix.

### Implemented lifecycle profile

Acceptance commits exact signed bytes and identity before returning the first signature.
Concurrent retries retain one lifecycle and its original deadlines.
One owned actor advances processing, confirmation, and finalization at acceptance plus 2, 4, and 6 simulated seconds without polling.
It retains at most 128 active lifecycles and admits at most 64 queued operations.
Normal stop drains accepted progression; force stop preserves committed state without inventing completion.
An expired local recent hash prevents processing; an unknown hash records unavailable local state instead of claiming expiry.
Processing at the inclusive validity bound succeeds; later ageing does not undo processing.

Submission requires `skipPreflight:true`.
Omitted or zero `maxRetries` is supported; positive values require an unimplemented rebroadcast model and return a capability error.
With skipped preflight, `minContextSlot` uses the processed view; `preflightCommitment` is validated.
Status lookup preserves signature order and duplicates, with at most 256 signatures.
Pending, expired, and unprocessed identities return null status.
The recent cache retains active statuses and 300 finalized slots; `searchTransactionHistory:true` reads retained history.
Transaction lookup defaults to finalized and also accepts confirmed; processed is rejected.
Results retain signed content and return `meta:null` and `blockTime:null`.
Simulator status success describes controlled progression, not program execution.

The selected official modular codec is `solana-transaction 6.0.0` with wincode.
The current verified HTTP fixtures cover legacy and v0, base58/base64 submission, binary round trips, JSON lookup, and client version negotiation.
Independent manually assembled v1 wire fixtures also verify signature identity, binary round trips, version negotiation, and JSON lookup.
V1 JSON uses `recentBlockhash` and `transactionConfig`, matching the [official Agave UI types](https://github.com/anza-xyz/agave/blob/d1a5955b5585eccf15cc627ce09464d1fd0c8c2c/transaction-status-client-types/src/lib.rs).
Wire layout and bounds use official Solana SDK revision `3904641b3dbc94221bf7dc84042bb9acf2b0280a` (modular crates 6.0.0).
Durable nonce histories remain the next task-6 slice.
Missing nonce state returns a capability error; it does not use recent-hash expiry.
These gaps do not narrow the accepted format scope below.
Response-loss policy remains task 7.

### Local lookup state and JSON projection

`simulator_state` in `ronchi/config.toml` supplies explicit fixture state before Lab startup.
The bounded state is at most 16 KiB and 16 lookup tables; each table has at most 256 addresses.
Each table contains `address`, ordered `addresses`, `last_extended_slot`, and `last_extended_slot_start_index`.
Omit `deactivation_slot` for an active table.
New addresses become visible after `last_extended_slot`; older prefix addresses remain visible in that slot.
Deactivated tables require unavailable slot-hash state and return `state_unavailable`.
Missing tables and unavailable indices return the same explicit state error before acceptance.
No lookup-table mutation or upstream fallback is implied.
The startup Journal records supplied state; acceptance retains resolved writable and readonly addresses.
Parsed history uses that retained context after table lifetime changes.

All three formats support JSON and `jsonParsed` lookup with `meta:null`.
Raw JSON preserves compiled indices and signed static addresses.
Parsed JSON orders static keys, all loaded writable keys, then all loaded readonly keys.
It identifies signers, writable accounts, and transaction/lookup-table source.
The local profile uses all-active reserved account keys from the pinned Agave list.
The maintained parsers recognize System transfer/nonce advance and UTF-8 Memo instructions.
Other instructions use the standard partially decoded representation with program ID, resolved account addresses, base58 data, and null stack height.
They do not acquire invented decoded fields or execution results.
The [Agave JSON types and account parser](https://github.com/anza-xyz/agave/tree/d1a5955b5585eccf15cc627ce09464d1fd0c8c2c/transaction-status) define the checked representation.

## Transaction formats and encodings

All transaction formats already present on Solana are in scope.
There is no intentional legacy/v0-only limit.
The official [versioned transaction reference](https://solana.com/docs/core/transactions/versioned-transactions), checked on 2026-11-02, identifies legacy, v0, and v1 and states that v1 is active on mainnet, devnet, and testnet.
The implementation must cover those formats and document its verified version matrix.
An unimplemented existing network format is a gap against this requirement, not an agreed deferral.

Support the RPC encodings defined for each selected method, including applicable `base58`, `base64`, `json`, and `jsonParsed` representations.
Preserve method-specific version negotiation, including `maxSupportedTransactionVersion` where applicable.
Full server-side format coverage does not permit ignoring a client's declared version limit.
Parsing and serialization must preserve transaction identity and content.
Support v0 Address Lookup Table resolution and version-specific message and fee rules where the selected method requires them.

[Durable nonce](https://solana.com/docs/core/transactions/durable-nonces) handling is also in scope as a transaction-lifetime mechanism, not a separate wire format.
Apply its own validity and advancement rules instead of recent-blockhash expiry rules.
Format support does not create missing lookup tables, nonce-account state, application accounts, or funds.
If a required input state is absent, expose that state-availability limit rather than treating the format itself as deliberately unsupported.
Real program execution and its account effects remain under the existing Execution Mode boundary.

## Submission and preflight

Simulator-mode `sendTransaction` requires `skipPreflight: true`.
If the request requires full preflight, including when that requirement follows from an omitted option, return an explicit unsupported-capability error with Surfpool guidance.
Do not report a failed program simulation when no program simulation occurred.
This can require RPC-client configuration in addition to changing the endpoint URL; it requires no Ronchi SDK.

Keep transaction identity, RPC acceptance, subsequent processing, and the RPC Boundary Result distinct.
Follow the supported official submission semantics and the selected Case when evaluating valid inputs, duplicates, expiry, and responses.
The [first Case](cases.md#tx-response-lost-identity-and-repetition) retains one response-loss Fault per affected identity for the lifetime of a Lab.

Simulator-mode `simulateTransaction` returns an explicit unsupported-capability error with Surfpool guidance.
It does not produce an invented execution result or forward the request to the Read Upstream.
In Surfpool mode, submission, preflight, and program simulation use the managed local Execution Backend.
All application-facing blockchain reads and writes remain under that backend's authority.

## Request parameters and errors

Honor the official meanings, types, defaults, and supported combinations of method parameters.
Return an explicit error for behavior outside the declared capability contract.
Do not silently ignore a parameter that requests unsupported behavior.
Use standard JSON-RPC errors for standard protocol errors and distinguish a Ronchi capability limit from a simulated or backend transaction failure.

Enforce `minContextSlot` against the selected local view where the method supports it.
Support explicit `maxRetries: 0` in simulator mode.
Reject explicit positive `maxRetries` until leader-rebroadcast behavior has a declared model.
Omitting the option does not promise simulation of network rebroadcasts or validator topology.
These limits do not replace the independently observed application resubmission attempts recorded in the Journal.

## Transaction lookup and history

Transaction lifecycle lookups stay local in simulator mode, including queries for signatures unknown to the Lab.
They do not automatically fall back to public transaction history.
Return `null` when the requested local status or transaction is unavailable under the method's query rules.
Missing data does not establish rejection, non-execution, or application failure.

For `getSignatureStatuses`, query the recent local status cache by default.
With `searchTransactionHistory: true`, also query retained local history.
Preserve the input signature order and follow the method's supported limits.
Retained history means data actually preserved under the Lab persistence policy; it is not an unlimited-storage promise.
The [evidence contract](evidence-and-findings.md#persistence-bounds-and-failure) defines Journal retention and storage bounds.

For a locally accepted transaction, `getTransaction` returns the retained signed transaction when it is available at the supported requested confirmation level.
It returns `meta: null` for unavailable execution metadata, as permitted by the official [confirmed transaction response structure](https://solana.com/docs/rpc/json-structures#confirmed-transaction-responses).
Its content and availability use the same local state as the other lifecycle methods.
All applicable encodings, including `jsonParsed`, are in scope under the format contract above.
Retrieving the signed payload does not establish execution logs, charged fees, balance changes, or a completed business operation.
Workflows requiring those execution results for local submissions need Surfpool.

## Clock and validity profile

The normal interactive Lab uses the [1:1 simulator-time rate](overview.md#simulator-time).
Keep simulator time, slot, and block height as distinct quantities.
The initial profile has a configurable 400 ms slot and produces one block per slot without skips.
This is a declared simulator simplification, not a real-network timing guarantee.
Generate local blockhashes and derive recent-blockhash validity from local block height and the validity bound.
Do not compare an unrelated public validity bound to a local counter.
Ageing a blockhash does not retroactively fail a transaction already processed with it.

The [Case's default 2/4/6-second confirmation schedule](cases.md#tx-response-lost-default-simulated-outcome) remains accepted.
Commitment labels describe the controlled simulator history; they do not reproduce validator voting or consensus depth.
Derive response context, transaction slots, and confirmation fields from the declared local history and the official field meanings.
Durable-nonce lifetimes use their own protocol state.

## Read-through allowlist

With an explicitly configured Read Upstream, the initial supported read allowlist contains:

- `getBalance`;
- `getAccountInfo`;
- `getMultipleAccounts`;
- `getTokenAccountBalance`;
- `getTokenAccountsByOwner`;
- `getMinimumBalanceForRentExemption`.

`getRecentPrioritizationFees` may also use that explicitly configured read allowlist under the fee contract below.
The local methods listed above retain local authority.
Writes, airdrops, unknown methods, and program simulation never gain forwarding permission through fallback handling.

Without a configured source for an upstream read, return an explicit unavailable-capability error.
Do not invent a zero balance, absent account, or successful read in place of missing source data.
Preserve the actual upstream result and its context as evidence, separately from any RPC Boundary Result changed by a Case.
Identify the source in the Journal and Findings.
Read-through account data does not acquire account mutations from simulated transactions.
If a workflow needs those mutations, it requires local execution.
In Surfpool mode, direct Read-through remains disabled; Surfpool's datasource stays behind its state-authority boundary.

## Fee queries

Provide local `getFeeForMessage` for messages covered by the format contract.
Use an explicit fee model compatible with the selected protocol profile and its version-specific message rules.
The implemented local profile charges 5,000 lamports per required transaction signature and per signature declared by Ed25519/secp256k1/secp256r1 precompile instructions.
For legacy/v0, priority fee is `ceil(CU limit * micro-lamports per CU / 1,000,000)`.
An explicit CU limit is capped at 1,400,000.
The all-active default allocates 3,000 CU per non-migrating builtin and 200,000 per other instruction, capped at 1,400,000.
Duplicate or malformed legacy compute-budget settings return an explicit parameter error.
V1 uses its header's total priority fee in lamports; ComputeBudget instructions do not configure v1, even when their data is invalid.
Arithmetic saturates at the protocol's unsigned 64-bit limit.
The method accepts a canonical base64 message and optional commitment/`minContextSlot` configuration.
Unavailable recent hashes return a null quote; missing required lookup state returns `state_unavailable`.
The context uses the selected local commitment view.
No transaction signature or client version negotiation parameter is required for this message query.
Fee rules were checked against [Agave fee/budget code](https://github.com/anza-xyz/agave/tree/d1a5955b5585eccf15cc627ce09464d1fd0c8c2c/fee) and SIMD-0385 blob `8e9cd5ef29497711e9e50016833e72d6928bbaa7`.
The result is a quote, not evidence that a fee was charged.
The simulator does not deduct it from a real account balance.
See the official [method contract](https://solana.com/docs/rpc/http/getfeeformessage).

Allow `getRecentPrioritizationFees` only through the explicitly configured read allowlist in simulator mode.
Preserve the upstream provenance of those samples.
Without a required source or model, return an explicit error instead of inventing fee data.

## JSON-RPC batches

Support bounded JSON-RPC batches with per-element validation, identifiers, Journal Evidence, and forwarding checks.
Bind the Case and settings when registering the whole batch.
Place that binding in the shared request-registration and Case-control order.
Each element retains its own request identity and recorded observations.
Bound request bytes, element count, and concurrent work under the resource model.

If a response-loss Fault prevents completing the shared HTTP batch response, terminate that response under the selected withholding policy.
For the default `tx-response-lost`, this means holding without a response for up to 20 seconds of real time, followed by transport termination unless an earlier observed cancellation or disconnect ends it.
Record the actual boundary effect on every affected element, including elements whose processing completed.
Do not claim that each affected element was separately selected for a Fault or that a client observed a particular error.
The later `partial-batch-response` candidate remains a separate Case.

Follow [JSON-RPC 2.0](https://www.jsonrpc.org/specification) rules for request IDs, responses, errors, and notifications.
The expected absence of a response to a notification is not itself a Fault.

## Verification and remaining work

Before claiming implementation support, verify the full format/encoding matrix, method response schemas, standard errors, parameter combinations, and relevant node/SDK versions against official references.
Pinning dependencies must not silently narrow the accepted existing-format coverage.
Verify concurrency and ordering through representative histories, including duplicates, expiry, Case changes, and batch response loss.
Use fake upstreams or isolated local execution; public writes are not a test oracle.
User validation must still establish whether this contract is sufficient for independently running backends.
The [evidence contract](evidence-and-findings.md) defines Journal storage, completeness, redaction, and retention.
