# Reference Application: Withdrawal Relayer

Status: task 10 implementation in progress. Durable intake, foreground execution, submission, and profiles are implemented; the observation endpoint is implemented; full crash acceptance remains in the task queue.
See [workflows](workflows.md) for the Demo entry point and [Checks and Verdicts](checks-and-verdicts.md) for generic observation and evaluation.
The defaults below are reversible choices for the first implementation.

The reference System Under Test is a Solana Withdrawal Relayer.
It gives Ronchi one concrete workflow for testing Cases, evidence, and application-state conclusions.
Simulator-mode outcomes never claim that a withdrawal executed economically.

The Reference Application is bundled with Ronchi but is not the product core. It is an example consumer, the System Under Test for the self-contained Demo, and an acceptance harness that prevents the product from becoming a fault generator with no proof of application-level value.

The Demo must exercise the Reference Application through the same external RPC, control, evidence, Check, dashboard, CLI, and MCP-facing behavior used by real applications. It must not depend on canned Journal Evidence or a private shortcut unavailable to users.

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
SQLite uses version 1, WAL, FULL synchronization, foreign keys, short transactions, and automatic checkpoints.
The database has a 64 MiB page ceiling and retains at most 128 intents with one active intent.
A private 32-byte demo seed lives in `signing.seed` with mode `0600` inside a `0700` reference directory.
The public readiness signer is only its public key.

`POST /intents` accepts `intent_id`, a base58 32-byte `recipient`, and positive integer `amount_lamports`.
Unknown keys are rejected.
The canonical payload digest covers recipient and amount.
Concurrent identical IDs return the same durable intent; another payload returns HTTP 409.
A second active intent returns HTTP 409 instead of creating an unbounded queue.
`GET /intents/{intent_id}` returns committed application state with explicit signature/collection availability.
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
Client response, actual 5-second timeout, transport error, and unknown crash outcome remain separate observations.
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
The generic Check collector, Verdict evaluator, and Demo remain tasks 11–13.

### Read-only observation source

`POST /checks/observe` accepts contract version 1 with `invocation_id`, `check_id`, the pinned `lab_id`, `snapshot_seq`, `scope:{intent_ids:[id]}`, `facts:["intent"]`, and an RFC 3339 `deadline`.
The first source supports one explicit intent and the `intent` fact record; other names/scopes are rejected.
The deadline must be future and at most 30 seconds away; the server also caps each HTTP handler at 5 seconds.
The operation returns `source:reference-relayer`, `schema_version:1`, invocation identity, `observed_at`, optional per-intent `revision`, and per-fact availability.
The `intent` value includes committed state, original/tracked mapping, the complete bounded prepared set, attempts, and retained client/status observations.
Original/tracked signatures and unknown attempt results declare `availability:unavailable` until collected.
A missing intent returns an unavailable fact without a fabricated state, zero revision, or empty observation value.
The same SQLite read transaction supplies the intent row and its identities, attempts, and observations.
`snapshot_relation:application_only` explicitly separates this application snapshot from the caller's Lab evidence prefix.
The endpoint does not send RPC, notify the worker, append observations, mutate state, or return a Verdict.
Signing material and exact signed bytes are excluded from public responses.
A direct HTTP test of this source does not implement the generic collector in task 11.

## Reference Behavior Profiles

The Demo uses one Withdrawal Relayer codebase with named behavior policies:

- `unsafe` creates and submits another transaction for the same Withdrawal Intent after an ambiguous response;
- `resilient` preserves the original transaction identity and reconciles its simulated lifecycle state before deciding what to do next.

The same `tx-response-lost` Case runs against both profiles. Ronchi must report their different observable RPC and application behavior from evidence, not from knowledge of which profile name was selected.

These profiles are intended to demonstrate that Ronchi distinguishes observable unsafe and resilient handling. They are not intended to mimic every application or demonstrate every Case. The broader Case catalog is exercised against real Systems Under Test and extended through protocol-aware Cases, Findings, and optional application Checks.

## Responsibility

The bundled Demo may control its known reference consumer. This does not make the public Ronchi workflow responsible for starting or resetting arbitrary user backends.

The Withdrawal Relayer accepts a durable Withdrawal Intent, turns it into a signed Solana transaction, submits that transaction, tracks its outcome, and reconciles the intent to a terminal state.

Conceptually:

```text
accept Withdrawal Intent
  -> prepare and persist transaction identity
  -> submit transaction
  -> observe signature and commitment
  -> reconcile RPC-visible transaction state with intent state
  -> publish terminal outcome
```

## Boundary

The reference application owns:

- Withdrawal Intent identity and durable state;
- transaction construction and signing;
- submission policy;
- confirmation and expiration policy;
- retry and reconciliation policy;
- an externally observable intent outcome.

Ronchi owns:

- the stateful Solana RPC Simulator and its Simulated Chain State;
- HTTP JSON-RPC observation;
- Protocol Fault injection;
- ordered Timeline evidence;
- generic Checks and invocation of application-specific Checks;
- persistent Lab and Journal lifecycle, plus optional Run and reproduction-artifact workflows.

## First reference incident

The first Withdrawal Intent supplies a stable business identifier for correlation. The Reference Application may build a syntactically valid signed Solana transaction, but Ronchi does not execute its instructions or mutate balances.

The initial incident is `tx-response-lost`:

1. The Withdrawal Relayer submits a signed transaction.
2. The RPC Simulator derives its signature and assigns it a configured Simulated Transaction State.
3. Ronchi withholds the successful RPC response under the Case's bounded hold policy.
4. The Withdrawal Relayer may observe a timeout; a claimed Client Outcome requires collected client-side evidence.
5. Later status queries expose the configured lifecycle state for the same signature.
6. Ronchi observes whether the Withdrawal Relayer reconciles the original signature, submits another transaction, or remains unresolved.

This incident exposes the difference between a missing submission response and later lifecycle evidence for the same signature. Reference client evidence can additionally establish whether the call timed out.

## Observable behavior goals

**Original transaction reconciliation**:
After an ambiguous response, the relayer continues tracking the original signature before creating another transaction for the same Withdrawal Intent.

**Truthful terminal status**:
An intent's reported status is consistent with the simulator lifecycle state and the relayer's configured policy.

**Eventual resolution**:
After the backend and RPC path recover, the intent must not remain unresolved indefinitely within the bounded reference Run.

**Durable reconciliation**:
Restarting the relayer after an ambiguous submission must not erase the evidence required to reconcile the intent.

## Evidence needed for a verdict

The reference Run should correlate:

- the Withdrawal Intent identifier;
- every prepared transaction identity and signature;
- every submission attempt observed by Ronchi;
- the Simulated Transaction State assigned to each signature;
- each injected Protocol Fault;
- the relayer's reported intent state and durable decision history collected through a configured Check.

Ronchi may find that multiple signed transactions were submitted for one explicitly correlated intent. It must not call that duplicate economic execution: the simulator does not execute transactions or change balances.

## Optional Control comparison

A successful Control Run establishes that the selected Fixture, Workload, relayer, and configured Checks completed without injection. It does not by itself prove semantic transparency or equivalent starting state. An optional Chaos Run uses comparable inputs and adds `tx-response-lost`.

```text
Control Run: intent reaches the expected state without injection
Chaos Run:   relayer reconciles the original simulated transaction after an ambiguous response
```

A failed Control Run makes the comparison inconclusive; it is not evidence of a resilience defect.
A directly established invariant violation can still support an independent Check failure.

A reference comparison must restore equivalent Ronchi-owned Simulated Chain State and separately prepare equivalent Withdrawal Intent and relayer persistence. The reference workflow records its Fixture digest, profile, Workload digest, and fresh database preparation for Comparability.
Each side uses a new Lab and a new reference database initialized with the same logical intent and transaction policy.
A paired Control/Chaos comparison keeps the same profile; comparison of `unsafe` and `resilient` is a separate policy demonstration.
Transaction signatures may differ where a new Lab uses a different local blockhash; comparison uses the declared logical intent and policy.
No reset operates on an arbitrary backend database. Without that evidence, Ronchi may show both Runs but must not attribute their difference to `tx-response-lost`.

## Packaging and configuration

The Reference Application is Rust code shipped in the same native executable as Ronchi.
The Demo starts it as an owned child process in the executable's relayer mode.
An ordinary Demo leaves its Lab running after the bounded reference Workload and relayer finish.
It returns the dashboard URL, immutable `lab_id`, summary, and evidence references.
`--stop-after-demo` explicitly stops only the Demo-owned Lab after collecting Checks; `--json` changes rendering only.
The Demo selects a fresh available `demo` name and free loopback ports; it never reuses or replaces the user's `default` Lab.
If `demo` already exists, allocate `demo-2`, then the next free suffix, and return the selected name.
Each invocation creates fresh Lab and application state; reopening a relayer database is an explicit recovery exercise.
This keeps an external HTTP/RPC boundary without requiring Go, Node, Python, or a database server.
It uses a separate SQLite database under the Demo's Project Workspace artifacts.
The Reference Application never writes the Lab Journal directly.
The Demo selects `unsafe` or `resilient` with `ronchi demo tx-response-lost --profile <profile>`; the default is `resilient`.
The selected profile is persisted at database creation and cannot change while recovering that database.
Profile names are setup metadata; Check rules evaluate observations rather than trusting those names.

Initial limits are one active intent, one in-flight RPC call per intent, a 5-second client request deadline, and 1-second status polling.
The target commitment is `finalized`; the bounded Demo observation horizon is 60 seconds of real time from intent acceptance.
The Case retains its independent default successful lifecycle at 2/4/6 seconds of simulator time and response hold of up to 20 seconds of real time.
A client timeout may end the held connection earlier; only retained client-side evidence establishes that timeout.
Simulator submissions explicitly set `skipPreflight: true` and `maxRetries: 0`.
The reference fixture may use one valid signed transaction format; the core retains its full [format contract](solana-rpc-contract.md#transaction-formats-and-encodings).
The Demo's narrower transaction generator does not narrow legacy/v0/v1, `jsonParsed`, or durable-nonce support requirements.

## Durable state and identity

| Intent state | Durable meaning and next action |
| --- | --- |
| `accepted` | Unique intent and immutable payload are committed; prepare the first transaction |
| `prepared` | Exact signed bytes, signature, validity inputs, and original identity are committed; a submission may begin |
| `submitting` | An attempt-start record is committed before the HTTP call; completion may be unknown after a crash |
| `reconciling` | Follow the policy-selected identity and record status observations until the terminal policy is satisfied |
| `succeeded` | The tracked signature reached the configured commitment with no reported transaction error; the resilient profile requires the original identity |
| `failed` | A definitive transaction failure or policy-defined validity failure is observed; retain its evidence |
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
A repeated intent ID with the same payload returns the existing intent; a different payload returns a conflict.
The sender commits signed bytes and identity before any network submission.
A failed identity commit prevents submission.
The sender never regenerates a signature merely because its response is missing.
The deliberately unsafe policy below is the explicit demonstration exception.

SQLite uses WAL, `synchronous=FULL`, enabled foreign keys, and versioned migrations.
One owner serializes writes; no database transaction remains open across an HTTP call.
A committed state transition is the application persistence boundary under local filesystem guarantees.
An uncertain database write outcome stops new submissions until the database is reopened and reconciled.
A storage failure remains an application infrastructure failure, not a false terminal withdrawal outcome.
Private signing material stays outside Check responses, Journal Evidence, and exports.

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

## Public application observations and Checks

The reference HTTP API accepts an idempotent intent with `POST /intents` and exposes committed state with `GET /intents/{intent_id}`.
It exposes the read-only `POST /checks/observe` operation defined by the generic HTTP observation adapter.
The returned revision identifies a consistent SQLite read of intent, prepared transactions, attempts, and observations.
Explicit `intent_id` to signature mappings supply business correlation; Ronchi does not infer it from RPC timing.
The Check response identifies `original_signature`, `tracked_signature`, prepared/submitted identities, current state, target commitment, and relevant client/status observations.
It reports these as application-source facts, not trusted simulator transitions or SDK Application Checkpoints.
The optional SDK is unnecessary for this path.

The Demo configures three versioned requirements: at most one prepared identity per intent, terminal success tied to original-signature finality, and resolution within the declared horizon.
The unsafe profile must fail the identity requirement after its second committed preparation.
The resilient profile can pass only when observed intent success and committed simulator evidence establish finality for its original signature, with required complete evidence.
Crash-recovery acceptance additionally verifies that restart retains that original identity and resumes reconciliation.
A deadline with authoritative observed unresolved state fails the resolution requirement; an unreachable Check endpoint makes it inconclusive.
A missing signature mapping, lost evidence, or unknown final boundary cannot become a pass by default.

Real native SOL or SPL execution remains outside this default simulator-based reference incident.
It requires the optional [Surfpool execution mode](../adr/0013-managed-surfpool-execution-mode.md) and separate execution-backed requirements.

## Implementation acceptance

Exercise both profiles through actual HTTP RPC requests and the generic Check adapter.
Crash the relayer before submission, after attempt-start commit, and after RPC acceptance before local result commit.
Reopen the file-backed database and verify idempotent intent intake, preserved bytes/signature, and evidence-backed recovery.
Verify that Check facts cannot alter the simulator history or manufacture an economic execution claim.
The implementation must establish these outcomes; this design document does not report passing tests.
