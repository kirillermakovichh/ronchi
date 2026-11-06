# Reference Application: Withdrawal Relayer

Status: implemented bundled consumer and Demo workload, with durable recovery verified on macOS.
This document owns relayer behavior, storage and recovery. [Workflows](workflows.md#first-proof-bundled-demo) owns Demo orchestration.
The defaults below are reversible prototype choices.

The relayer accepts a durable Withdrawal Intent, signs and submits a transaction, then reconciles a terminal state.
It owns business identity, signing, submission policy and persistence. Ronchi owns the RPC boundary, Cases and Journal.
Both profiles use the same external RPC/control interfaces as other applications; no canned trace or private evidence path is used.
The bundled Demo may manage this known child. Ronchi does not manage arbitrary user backends.
The default simulator executes no programs and changes no balances. Simulated finality is not economic execution.

## Implemented reference process

```bash
ronchi --project /path/to/project --lab-id L1 reference relayer --profile resilient --name reference --listen 127.0.0.1:0 --json
```

This foreground process never starts or stops a Lab.
It prints one readiness line with its loopback HTTP address and remains running until SIGINT or SIGTERM.
The immutable Lab ID is required; the default reference workspace name is `reference`.
The process uses `ronchi/reference/<name>/relayer.sqlite` in the selected Project Workspace.
Its separate owner lock prevents two relayer processes from writing this database.
One bounded 16-command storage owner serializes reads and writes outside Tokio workers.
Queue saturation rejects HTTP admission; a worker/storage failure stops new actions and preserves committed state for explicit recovery.
SQLite uses version 1, WAL, FULL synchronization, foreign keys, short transactions, and automatic checkpoints.
The database has a 16,384-page ceiling (64 MiB at the default 4 KiB page size) and retains at most 128 intents with one active intent.
WAL files are separate; automatic checkpoints and short application snapshots control their reuse.
A private 32-byte demo seed lives in `signing.seed` with mode `0600` inside a `0700` reference directory.
The public readiness signer is only its public key.

`POST /intents` accepts `intent_id`, a base58 32-byte `recipient`, and positive integer `amount_lamports`.
Unknown keys are rejected.
The canonical payload digest covers recipient and amount.
Concurrent identical IDs return the same durable intent; another payload returns HTTP 409.
A second active intent returns HTTP 409 instead of creating an unbounded queue.
`GET /intents/{intent_id}` returns committed application state with explicit signature/list availability.
An absent intent returns `availability:unavailable`.
The request body limit is 8 KiB; responses are bounded to 256 KiB.
The loopback listener accepts at most 64 connections and rejects browser Origin headers.

Initialization obtains the exact Lab through authenticated public status and pins the public RPC `getGenesisHash` identity.
It does not inspect the Lab Journal or simulator state.
The database persists profile, Lab, local genesis identity, RPC endpoint, and horizon configuration.
`--horizon-seconds` accepts 1..300 (initial default 60); recovery cannot change the stored value.
Profile changes are rejected before serving requests.
Readiness reports `submission:enabled` when the bounded worker is owned by the foreground process.
An accepted intent is durable intake only; it is not transaction acceptance or success.
The worker prepares a legacy signed fixture with a signed intent/generation discriminator.
It commits exact base64 bytes, signature, local blockhash, context slot, and last valid height before attempt-start.
It then commits attempt-start before sending `sendTransaction` with explicit `skipPreflight:true` and `maxRetries:0`.
Each RPC owns its connection task; timeout or shutdown cancels and joins that task.
No SQL transaction spans an HTTP call.
Client response, actual client timeout, transport error, and unknown crash outcome remain separate observations.
Each client RPC deadline is capped at 5 seconds and the remaining original intent horizon.
A null status and submission acceptance leave the intent reconciling.
Only a positive finalized status with `err:null` permits `succeeded`.
A positive status error permits `failed`; missing/expired validity alone does not.
The worker polls at one-second intervals with `searchTransactionHistory:true`.
It validates public status and the pinned genesis before RPC work.
Unsafe prepares generation 1 only after a committed ambiguous Client Outcome for generation 0.
Its signed discriminator guarantees a different message even at the same blockhash.
There are at most two generations and one attempt per generation.
No state transition depends on the active Case name.
The worker reserves terminal headroom within 128 observations per intent and ends explicitly unresolved at its observation limit.
State transitions and their supporting observations share one SQLite transaction.
A storage error stops the worker/process instead of sending after an uncertain commit.
Normal SIGINT/SIGTERM preserves confirmed state and keeps the Lab running.

## Durable state and identity

| Intent state | Durable meaning and next action |
| --- | --- |
| `accepted` | Unique intent and immutable payload are committed; prepare the first transaction |
| `prepared` | Exact signed bytes, signature, validity inputs, and original identity are committed; a submission may begin |
| `submitting` | An attempt-start record is committed before the HTTP call; completion may be unknown after a crash |
| `reconciling` | Follow the policy-selected identity and record status observations until the terminal policy is satisfied |
| `succeeded` | The tracked signature reached the configured commitment with no reported transaction error; the resilient profile requires the original identity |
| `failed` | A definitive transaction error is observed; retain its evidence |
| `unresolved` | The bounded horizon ended without enough evidence; do not claim rejection or success |

`succeeded` describes the reference application's interpretation of simulated lifecycle state.
It does not prove a transfer, account mutation, fee charge, or Business Invariant about economic execution.
A missing response or `null` status is never a definitive failure.
For a recent-blockhash transaction, validity failure requires its stored validity bound and an applicable authoritative status/history query; blockhash expiry alone cannot disprove earlier processing.
The initial reference policy retains uncertainty and ends `unresolved` rather than automatically rebuilding after unobserved processing.
Durable-nonce transactions use their own validity evidence; this reference fixture uses recent blockhashes.

The schema contains unique `intent_id`, canonical intent payload digest, state/revision, original signature, and target commitment.
A transaction table stores exact signed bytes, signature, intent identity, generation number, and validity inputs.
Attempt and observation tables retain attempt start, returned result or recorded Client Outcome, queried signature, and returned commitment/error.
Terminal state updates and their supporting observation commit atomically.
An original signature is immutable after the first `prepared` commit.
A failed identity commit prevents submission.
The sender never regenerates a signature merely because its response is missing.
The deliberately unsafe policy below is the explicit demonstration exception.

A committed state transition is the application persistence boundary under local filesystem guarantees.
An uncertain database write outcome stops new submissions until the database is reopened and reconciled.
A storage failure remains an application infrastructure failure, not a false terminal withdrawal outcome.
Private signing material stays outside public responses, Journal Evidence and exports.

## Profiles and crash recovery

Both profiles use the same construction, signing, storage, HTTP client, and observation code.
They differ only in the persisted policy after an ambiguous first submission.

| Profile | Ambiguity and recovery policy |
| --- | --- |
| `resilient` | Keep the original signature; query it through retained history until finality, definitive failure, or the horizon; optional retransmission uses the exact stored signed bytes |
| `unsafe` | After the first recorded ambiguous Client Outcome, prepare and submit one different signed message for the same intent; then track that replacement without creating further identities |

The unsafe message includes an explicit generation discriminator so it has a different signature even when it shares a blockhash.
Two prepared identities are the bounded deliberate violation; neither implies two economic transfers.
The unsafe profile never creates more than two identities for one intent in this Demo.
Its final state identifies the tracked signature, while its immutable original signature and full prepared set remain observable.
The resilient profile does not mark submission acceptance as success.
It does not submit a replacement identity during the reference incident.

After restart, a `prepared` intent queries its stored identity before a first or repeated send.
A recovered `submitting` intent treats the network outcome as unknown and enters reconciliation.
No attempt-start record proves that the request reached Ronchi.
A crash after a successful response but before its local commit also requires reconciliation.
Already terminal states remain terminal and are returned idempotently.
Recovery preserves stored horizon timing and never silently gives an intent a new full timeout.
If the original Lab is unavailable, recovery reports `unresolved` with that source gap; it does not point the old intent at a new Lab.
The relayer pins the immutable `lab_id` and local network identity in its Demo configuration.

## Demo configuration and observation limits

The native executable contains both Ronchi and the relayer. Demo starts the relayer as an owned child.
`ronchi demo tx-response-lost --profile resilient|unsafe` selects the policy; `resilient` is the default.
Each Demo uses a new Lab, free loopback ports and fresh relayer storage. It never replaces the user's default Lab.
The [Demo workflow](workflows.md#first-proof-bundled-demo) defines names, artifacts, cancellation, stop behavior and reported links.

The reference fixture is one legacy signed message with an intent/generation discriminator.
This narrower generator does not narrow the core's [transaction format contract](solana-rpc-contract.md#transaction-formats-and-encodings).
There is one active intent and one in-flight RPC call, a five-second client deadline and one-second status polling.
Default finality is `finalized`; the application horizon is 60 real seconds from acceptance.
Case lifecycle and withholding use their separate [clock and hold policy](cases.md#tx-response-lost-default-simulated-outcome).
A client deadline can close a held connection sooner; only the relayer's own retained evidence establishes its Client Outcome.
Ronchi does not ingest the relayer database or ordinary HTTP status into the Journal.

The resilient policy aims to retain the original identity, report a truthful terminal state and recover durable reconciliation.
Unsafe deliberately demonstrates one replacement after ambiguity. These are example policies, not generic business rules evaluated by Ronchi.
Manual inspection can correlate intent ID, prepared identities, attempts, RPC lifecycle, Faults and the relayer's own state.
Two different signed identities still do not establish two economic transfers.

## Application inspection

The bundled relayer accepts intents with `POST /intents` and returns committed state with `GET /intents/{intent_id}`.
These routes support workload control and manual inspection.
Demo uses the ordinary GET route only to await a terminal workload state.
Ronchi does not ingest these responses into the Journal or evaluate application requirements.
Its report contains RPC summary, Findings and evidence commands, without a business Verdict.

## Optional Control comparison

A successful Control Run establishes that the selected Fixture, Workload and relayer completed without injection. It does not by itself prove semantic transparency or equivalent starting state. An optional Chaos Run uses comparable inputs and adds `tx-response-lost`.

```text
Control Run: intent reaches the expected state without injection
Chaos Run:   relayer reconciles the original simulated transaction after an ambiguous response
```

A failed Control Run makes the comparison inconclusive; it is not evidence of a resilience defect.
The developer can assess application invariants separately from Ronchi's RPC Findings.

For a manual comparison, restore equivalent Ronchi-owned Simulated Chain State and separately prepare equivalent Withdrawal Intent and relayer persistence. The caller records the Fixture digest, profile, Workload digest and fresh database preparation for Comparability.
Each side uses a new Lab and a new reference database initialized with the same logical intent and transaction policy.
A paired Control/Chaos comparison keeps the same profile; comparison of `unsafe` and `resilient` is a separate policy demonstration.
Transaction signatures may differ where a new Lab uses a different local blockhash; comparison uses the declared logical intent and policy.
No reset operates on an arbitrary backend database. Without that evidence, Ronchi may show both Runs but must not attribute their difference to `tx-response-lost`.

## Verified recovery histories

The acceptance suite runs separate native Lab and relayer processes, public HTTP RPC/control, and temporary file-backed relayer databases.
It verifies concurrent idempotent intake and payload conflict, immutable profile/horizon, private signing material, and both policies under actual response loss.
A retained client timeout comes from the client timer, not a server withholding record.
A null status remains uncertainty; only observed finalized status permits simulated-lifecycle success.
Unsafe prepares exactly one replacement, including at an unchanged local blockhash.
A real SQLite identity-insert failure prevents any submission and fails the relayer process.

| Observed crash boundary | Recovery behavior verified |
| --- | --- |
| Prepared commit, before send | Reopen exact bytes/signature; query saved identity and verify validity before the first send |
| Attempt-start commit, before send | Reconcile an unknown network outcome; create no timeout or replacement; finish unresolved at the original short horizon |
| Public RPC acceptance, before local result commit | Retain the unknown attempt result; reconcile original-signature finality without inventing a Client Outcome |
| Replacement prepared commit | Retain both identities and original mapping; query/send the saved replacement; repeated restarts create no third identity |
| Terminal commit | Return the same terminal snapshot without new RPC work |

Debug builds expose a closed test barrier set through `RONCHI_REFERENCE_TEST_BARRIER`.
Phases are `before_prepare_commit`, `prepared`, `attempt_start`, `rpc_result`, `replacement_prepared`, and `terminal`.
A barrier atomically writes `barrier.json` in the private reference directory and waits for `barrier.release` or shutdown.
These development barriers execute no supplied code and are ignored in release builds.
`GET /intents/{intent_id}` does not release these barriers.
Tests observe these markers before SIGKILL and reopen the same database in a new process.
They do not use an arbitrary sleep as the crash-boundary oracle.

Recovery keeps the stored absolute acceptance deadline.
A caller cannot change the profile or horizon when reopening a database.
An unavailable original Lab or a mismatched public genesis produces unresolved state with a source gap.
If `--lab-id` differs from the stored identity, readiness declares `submission:blocked` and `source_gap:requested_lab_identity_mismatch`.
The API retains the original Lab mapping; the worker never redirects into the requested replacement Lab.
Tests cover a new Lab reusing the exact RPC socket address with both old and new ID selections.
A modified expected-genesis fixture also prevents submission.
SIGTERM during an in-flight submission joins the transport, keeps the attempt outcome unknown, and leaves the Lab healthy.
A subsequent process reconciles that original identity without treating shutdown as a client timeout.

Schema 1 is the only supported reference database version.
Private seed loss, unsupported schema, or storage failure requires explicit recovery; the process does not regenerate an existing signing identity or reset the database.
The fixture is one legacy inert signed message; it does not execute a SOL/SPL transfer.
Process-crash histories do not establish power-loss guarantees beyond SQLite FULL and the local filesystem contract.
Linux/Windows process behavior has not been verified by this slice.
