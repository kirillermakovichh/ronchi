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
Notifications execute without JSON-RPC responses; an all-notification exchange returns empty HTTP `204` and never consumes response-loss selection.
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
Durable nonce histories use the explicit local state and advancement rules below.
Missing nonce state returns `state_unavailable`; it does not use recent-hash expiry.
State-availability limits do not narrow the accepted format scope below.
Simulator response-loss policy is implemented under the [Case Coverage Claim](cases.md#tx-response-lost-coverage-claim).

### Local lookup state and JSON projection

`simulator_state` in `ronchi/config.toml` supplies explicit fixture state before Lab startup.
The bounded state is at most 16 KiB, 16 lookup tables, and 32 initialized nonce accounts; each table has at most 256 addresses.
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

### Verified local format matrix

| Behavior | Legacy | v0 | v1 |
| --- | --- | --- | --- |
| Independently signed wire / packet bound | 1,232 bytes | 1,232 bytes | 4,096 bytes |
| `sendTransaction` base58 / base64 | yes | yes | yes |
| Exact binary `getTransaction` round trip | yes | yes | yes |
| JSON / `jsonParsed`, `meta:null` | yes | yes | yes, including `transactionConfig` |
| Client version negotiation | legacy default | explicit max >= 0 | explicit max >= 1 |
| Address Lookup Tables | inapplicable | explicit local state | inapplicable; static addresses |
| Base64 message fee quote | yes | yes | yes, fixed header priority |
| Recent hash lifetime | local height bound | local height bound | local height bound |
| Durable nonce lifetime | explicit nonce account | explicit nonce account, including loaded address | explicit nonce account |

`tests/transaction_history.rs` constructs wire fixtures and signatures without the production decoder.
HTTP histories check invalid input, missing state, version limits, both binary encodings, parsed/fallback projection, lookup order, fee rules, nonce races, and failed persistence.
A native CLI/daemon test also loads nonce configuration, serves a fee quote, stops, and reopens retained state.
This matrix is a local simulator compatibility claim for these methods and fixtures.
It does not establish complete Agave behavior, validator execution, or a stable external API.
Unavailable account/slot-hash state remains explicit; no public write or automatic funding is used.

### Implemented durable nonce model

Supply `[[simulator_state.nonce_accounts]]` with base58 `address`, `authority`, and `nonce` values.
Each entry describes an initialized system-owned nonce account at startup slot zero; optional `slot` must be zero.
Ronchi does not create, authorize, fund, or withdraw nonce accounts through program execution.
A first System `AdvanceNonceAccount` instruction selects nonce lifetime for legacy/v0/v1.
The instruction must reference a writable nonce account, the recent-blockhash sysvar, and its signing authority.
Missing state returns `state_unavailable`; invalid authority/account metadata returns a parameter error before acceptance.
Acceptance retains signed bytes, nonce context, and the original 2/4/6-second deadlines.
It reserves future writes and does not consume the nonce merely by admitting work.

At processing, the submitted value must equal the latest committed account nonce.
The next nonce is SHA-256 of `DURABLE_NONCE` followed by the current local bank blockhash bytes, following the [official SDK nonce state](https://github.com/anza-xyz/solana-sdk/blob/3904641b3dbc94221bf7dc84042bb9acf2b0280a/nonce/src/state.rs).
If that value already equals the account nonce, this bank cannot advance it again.
A successful controlled processing transition and nonce advancement commit atomically.
A competing identity using the consumed value remains unprocessed; it does not acquire a success status or recent-hash expiry.
Identical signed-byte retries retain the original lifecycle and do not consume a second value.
Recent-hash ageing does not expire a nonce transaction or undo processed state.
Other instructions retain the simulator's declared non-execution boundary.

Nonce fee quotes use historical account state at the selected commitment slot.
An obsolete nonce produces a null quote; absent nonce state produces an explicit error.
The base rate remains the local model's 5,000 lamports per signature.
Nonce advancement and historical versions persist in Journal schema 3, including after forced interruption.
This models lifetime protection; it does not establish account balances, fees charged, or application outcomes.

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

### Implemented Read-through profile

Task 16 implements the seven allowlisted reads with `up --read-upstream <url>`, `RONCHI_READ_UPSTREAM`, or `read_upstream` in project configuration.
The outbound HTTP/HTTPS boundary checks each method before sending a request.
It never follows redirects or forwards the incoming headers.
URL user information supplies Basic authorization only to the configured source.
HTTPS uses Rustls and WebPKI trust roots.
Local bootstrap, fee, submission, status, and transaction methods keep local authority, including unknown transaction identities.

Each source call has a 10-second deadline and a 256 KiB response-body limit within the ordinary RPC deadline.
A missing source, failed transport, redirect, malformed envelope, or oversized result returns an explicit capability/source error.
An upstream JSON-RPC error preserves its numeric code.
Ronchi omits arbitrary upstream error text and error data under the typed-facts redaction policy.
These failures never trigger a local invented result or a different source.

The manifest and startup evidence contain an opaque SHA-256 source identity.
The raw endpoint stays in daemon memory and its private launch environment.
Ronchi does not copy it into launch metadata, diagnostic text, or the Journal.
Repeating `up` with the same explicit endpoint resolves the same live Lab.
Changing that endpoint requires a new Lab.

Before forwarding, Ronchi commits a `rpc.upstream_started` source fact.
The `rpc.upstream` result fact retains source authority, request identity, context, and bounded result content separately from the prepared response and transport boundary.
Larger source results use ordered base64-JSON chunks, with at most 24 KiB of source bytes in each chunk.
Chunk representation2 declares `chunk_schema_version:2` and stores encoded content in the retained `payload` field.
Normal queries and exports omit that content.
Explicit payload selection uses the existing bounded previews and byte offsets.
Earlier chunks with a `bytes` field remain readable in their original Journals; readers do not rewrite them.
Endpoint and credential echoes are redacted before Journal admission.
A redacted string or object key can omit related content explicitly.
If the redacted result exceeds its byte budget, the evidence records an omission reason and the original result byte count.
The application still receives the actual source result.
Interrupted source calls retain the known dispatch fact without inventing a completed source result.

Local fake-upstream histories verify the allowlist, mixed batches, notifications, signed local acceptance, unknown local lookups, credential redaction, redirects, byte limits, TLS handshake rejection, and forced shutdown.
They do not establish live provider availability or third-party application compatibility.

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

## Managed backend RPC boundary

Managed Surfpool uses one exclusive local state authority.
The boundary routes its supported methods through that owned backend, including bootstrap methods and local transaction history.
It does not fall back to simulator state or direct Read-through.
Backend responses and source errors are retained with the owned authority identity and bounded chunk metadata.
HTTP clients receive the source's numeric error code, redacted message, and actual `error.data` shape.
Standard preflight fields such as `err` and `logs` remain directly inside `error.data`.
Absent source data remains absent; explicit JSON null remains null.
Safe message strings use the existing overall response-byte cap without a separate message cutoff.
A missing or non-string source message uses a generic diagnostic.
The internal `backend_rpc_error` classification stays inside typed source diagnostics and does not wrap client-visible data.
Source chunks declare `chunk_schema_version:2` and keep their base64 JSON content in opt-in `payload` storage.
Compact `error_code` and `error_classification` metadata support inspection without loading the error body.
Explicit payload offsets can retrieve available chunks; chunks marked redacted retain the existing unavailable-payload policy.
Source internal RPC errors remain distinct from required Journal write failures and do not by themselves fail the writer.
Configured datasource credentials remain redacted from retained source content.
Unknown methods and unsupported parameter shapes fail before network dispatch where Ronchi can classify them.
Surfpool remains responsible for state-dependent preflight, account availability, execution, and native method configuration errors.
Cancellation after dispatch records uncertainty; it does not claim backend rollback.
Core transport limits and batch binding remain unchanged.
