# Cases and coverage

Status: accepted contract with a separate candidate catalog. Simulator `tx-response-lost` is implemented and verified under the scoped Coverage Claim below. The simulator confirmation-delay variants are implemented with the scoped claim below. `provider-timeout-burst` is implemented with the scoped claim below. Other families remain planned; managed Surfpool response loss has the scoped claim below. Names, parameters, and YAML syntax are not stable APIs.
This document owns Case scope and catalog status; [overview.md](overview.md) owns the overall product definition.

## Continuing Case model

One Case is active in a persistent Lab at a time.
It can affect multiple matching interactions until disarmed, replaced, or the Lab stops.
Switching Cases preserves the backend, Lab, and Journal.
Coordinated Faults belong inside one composite Case.

When Ronchi registers an RPC request for processing, it binds that request to the active Case and its current settings before executing the method.
The binding remains unchanged throughout that request's processing.
If no Case is active at registration, that request keeps the no-Case policy even if a Case is activated later.
A Case or settings change applies to requests registered after that change.
For a JSON-RPC batch, bind the Case and settings at registration of the whole batch under the [batch contract](solana-rpc-contract.md#json-rpc-batches).

RPC request registrations and applied Case-control operations share one increasing `registration_seq` within a Lab.
Case-control operations include activation, disarming, replacement, and settings changes.
This shared order determines which Case and settings a registered request uses.
The Journal exposes the numbers and the resulting request-to-policy binding, including when timestamps coincide.
The order covers registration and policy selection; RPC methods can execute concurrently and finish in a different order.
It does not establish a causal relationship between unrelated application actions.
Each Lab start creates an immutable `lab_id`.
Journal events have references `(lab_id, event_seq)`.
`event_seq` orders recorded events; it is separate from `registration_seq` and does not determine policy binding.

With the same controlled initial state, Case configuration, and relevant ordered inputs, fault-selection decisions should reproduce.
That is distinct from reproducing an external backend or live upstream state.
See [ADR-0017](../adr/0017-case-driven-fault-model.md).

An activation has an immutable policy revision and resolved settings.
A settings change creates another revision for later registrations.
Replacing or reactivating a Case starts its transient schedule again.
It does not erase transaction history or the Lab-lifetime `tx-response-lost` identity ledger.
Already registered requests retain their selected policy, timers, and Faults.
A lifecycle schedule assigned at transaction acceptance also survives Case changes.
Disarming a read-changing Case makes newly registered reads use the authoritative view.

The default matcher covers valid, supported calls with a response ID.
Invalid requests, unavailable capabilities, and notifications do not consume a response-loss or timeout selection.
Filters can narrow supported methods and exact transaction signatures.
An absent signature filter means all eligible identities, not a claim about business intent.
Faults never bypass normal format, parameter, mode, or forwarding checks.

For batches, selection uses the policy snapshot for the whole batch.
Elements receive deterministic selection order by array index within that registration.
They can finish in another order.
A held element can prevent the shared HTTP response from completing under the [batch contract](solana-rpc-contract.md#json-rpc-batches).
Other elements retain their own processing evidence and boundary effects.

## Accepted initial boundary

The first catalog targets Solana transaction senders and relayers through one HTTP JSON-RPC path.
It contains five failure families:

| Family | Intended behavior to investigate | Illustrative names from earlier drafts |
| --- | --- | --- |
| RPC unavailability or timeout | Repeated failure, bounded retry, and later recovery | `provider-timeout-burst` |
| Lost submission response | Transaction accepted locally while the successful response is withheld; subsequent signature tracking or resubmission | `tx-response-lost` |
| Unavailable or stale transaction status | A status query returns unavailable data or an older supported lifecycle view | `signature-status-null`, `stale-transaction-read-after-submission` |
| Stalled confirmation | Progress remains below the backend's expected commitment | `tx-confirmation-stalls`, `finality-delayed` |
| Expired recent blockhash submission | A transaction is submitted with an expired hash; rebuilding and reconciliation behavior become observable | `tx-blockhash-expired` |

These are accepted failure families with prototype defaults below, not a release-order promise.
Family names alone do not advertise implementation; the verified claim and public catalog identify the implemented subset.
The names remain illustrative identifiers for the specified variants.
Do not infer another rejection code or terminal status from a name.
The first reference incident remains `tx-response-lost` with the [Withdrawal Relayer](reference-application.md).

### tx-response-lost identity and repetition

The default policy withholds the successful response on the first accepted submission of each eligible new transaction identity while the Case is active.
After that Fault has been applied to a transaction, resubmitting the same signed transaction does not apply the response-loss Fault again.
The Case continues to affect eligible new transaction identities.

The fact that response loss has been applied to an identity is retained for the lifetime of that Lab.
Disarming the Case, replacing it, or later reactivating `tx-response-lost` does not reset that fact.
Reactivation therefore does not cause another response-loss Fault for a transaction identity already affected in the same Lab.

For supported valid signed transactions, Ronchi uses the first signature as the transaction identifier.
Resubmitting identical signed transaction bytes is another submission attempt for the same transaction.
The `recent_blockhash` alone is not a transaction identifier; different transactions can use the same blockhash.
See the official [transaction structure](https://solana.com/docs/core/transactions/transaction-structure#signatures) and [`sendTransaction` contract](https://solana.com/docs/rpc/http/sendtransaction), and the [shared protocol reference](../../.agents/references/solana-rpc/transactions-and-status.md).

In simulator mode, a repeated submission preserves the existing transaction identity and lifecycle history.
It does not create another Simulated Transaction State or restart the lifecycle merely because another RPC request arrived.
The Journal records each submission attempt against that same identity.
In Surfpool mode, Surfpool remains the transaction-state authority; Ronchi does not replace its duplicate-handling result.

Once the response-loss Fault has been consumed for an identity, later attempts follow supported Solana RPC semantics for the current state and request configuration.
An ordinary retry response is not a promise of unconditional RPC success.
The [Solana RPC contract](solana-rpc-contract.md) governs submission options, lookup behavior, supported formats, and explicit capability errors.
Verify exact response shapes for duplicates and expiry against the official method contract during implementation.
This policy does not imply complete validator behavior or real execution in simulator mode.

| Example | Required distinction |
| --- | --- |
| Transaction `A` is accepted and its response is withheld; identical signed bytes are submitted again | One transaction identity, two submission attempts, no second response-loss Fault for that identity |
| A different valid signed transaction `B` uses the same recent blockhash as `A` | A distinct transaction identity, eligible for its own first response-loss Fault |
| The backend changes a transaction message and signs it again, producing another signature | Another transaction to track; whether it represents the same business operation requires separate evidence |

The common matcher and registration rules above apply.
Payload availability and retention follow the Lab's evidence policy.

### tx-response-lost response withholding

After acceptance, Ronchi holds the affected submission request open without sending an HTTP or JSON-RPC response.
The default maximum hold duration is 20 seconds of elapsed real time from the start of withholding.
This value remains subject to prototype validation; it is not a Solana protocol timeout or a stable configuration contract.

When that duration elapses, Ronchi aborts the affected request at the transport boundary without sending an HTTP or JSON-RPC response.
It releases the held request's resources and preserves the accepted transaction and its lifecycle state.

The hold applies to the affected request.
Transaction-state progression and handling of other RPC requests continue independently.
If Ronchi observes client cancellation or a disconnect, it releases the held request's resources without waiting for the full duration.
Releasing those resources does not erase acceptance or restart the transaction lifecycle.

The hold duration is separate from the configurable simulator delay before the transaction becomes successful.
A 20-second hold does not establish whether or when the client experienced a timeout.
Any claimed Client Outcome still requires client-side evidence.

For a request inside a JSON-RPC batch, response loss affects the shared HTTP batch response under the [batch contract](solana-rpc-contract.md#json-rpc-batches).
The Journal records the boundary effect on every affected element, including elements whose processing completed.
The absence of a response to a notification is not itself an injected Fault.

### tx-response-lost Case switching

Disarming or replacing the Case changes the policy for new requests.
A submission request already being held retains its selected Fault and original termination deadline.
The change does not shorten or restart that deadline, send the withheld response, or restart the accepted transaction's lifecycle.
The held attempt remains attributed to the Case that selected its Fault.
Observed client cancellation or disconnect can still end the hold earlier under the withholding policy.

For example, with the default 20-second hold, disarming the Case after 5 seconds leaves the existing request held until its original 20-second deadline unless the client cancels or disconnects first.
The accepted transaction continues its independent simulated progression.

The request binds to its Case at registration under the [continuing Case model](#continuing-case-model).
The Journal exposes the shared registration and Case-control order under that model.
These rules describe disarming and replacement; Lab shutdown follows the Lab lifecycle.

### tx-response-lost default simulated outcome

In the base simulator form of `tx-response-lost`, an accepted eligible transaction reaches a successful Simulated Transaction State by default.
Ronchi withholds the successful submission response while preserving that simulated outcome.
This makes the reference incident an investigation of a missing response for a transaction with a successful simulated outcome.
The state remains evidence of a controlled RPC history, not executed instructions, changed balances, or a completed business operation.

At first acceptance, the simulator records the transaction as pending and starts a configurable processing delay measured in simulator time.
The default processing delay is 2 seconds from that first acceptance.
When that delay elapses, the transaction transitions to a successful Simulated Transaction State with confirmation level `processed`.
The confirmation level then advances automatically to `confirmed` and `finalized` with a configurable delay between levels.
Each confirmation interval defaults to 2 seconds of simulator time.
The normal interactive Lab uses the [accepted 1:1 simulator-time rate](overview.md#simulator-time).

| Elapsed simulator time from first acceptance | Simulated Transaction State | Entry for the signature in `getSignatureStatuses.value` |
| --- | --- | --- |
| 0 seconds | Pending | `null` |
| 2 seconds | Successful | Object with `err: null`, `confirmationStatus: "processed"` |
| 4 seconds | Successful | Object with `err: null`, `confirmationStatus: "confirmed"` |
| 6 seconds | Successful | Object with `err: null`, `confirmationStatus: "finalized"` |

These intervals are Case parameters for prototype validation, not claims about real Solana processing latency, validator voting, or finality.
This progression is independent of backend requests and continues without status polling or resubmission.
Repeated submissions and status queries neither accelerate the transitions nor restart their delays.

For `getSignatureStatuses`, the corresponding entry in `value` is `null` while the accepted transaction is still awaiting processing.
The simulator retains the accepted identity and its pending state internally during that phase.
When the configured processing delay elapses, the status entry becomes an object with `err: null` and `confirmationStatus: "processed"`.
The successful processing outcome is distinct from later confirmation levels.
See the official [`getSignatureStatuses` contract](https://solana.com/docs/rpc/http/getsignaturestatuses).

Standard RPC parameters and status-response fields follow the [verified local RPC profile](solana-rpc-contract.md#verified-local-format-matrix).
The prototype [Custom Case contract](custom-cases.md) exposes bounded primitive parameters.
This default applies to simulator mode; Surfpool retains authority over its actual local execution result.

### tx-response-lost Coverage Claim

This claim records the verified simulator behavior and the accepted remaining mode boundary.
The native CLI/control and HTTP histories described below verify the simulator implementation.
Surfpool behavior remains a requirement for its later implementation, not a verified capability.

| Area | Claim |
| --- | --- |
| Traffic and Fault | One Solana HTTP JSON-RPC path. The Fault targets the first successful acceptance of an eligible new transaction through `sendTransaction`. |
| Submission prerequisite | Simulator-mode submission requires `skipPreflight: true`. A request requiring full preflight receives a capability error before this Case can reach its acceptance-and-response-loss incident. |
| Identity and repetition | [One response-loss Fault per affected identity for the lifetime of a Lab](#tx-response-lost-identity-and-repetition), with separate evidence for each submission attempt. |
| RPC boundary | [Hold the affected request without a response for up to 20 seconds of real time, then terminate it without an HTTP or JSON-RPC response](#tx-response-lost-response-withholding). Observable client cancellation or disconnect can end the hold earlier. |
| Simulator history | Retain acceptance independently of response loss. Follow the [configurable successful processing and confirmation schedule](#tx-response-lost-default-simulated-outcome), with default transitions at 2, 4, and 6 seconds of simulator time. |
| Surfpool history (planned) | Surfpool owns transaction state and the actual local execution result. Ronchi can withhold an observed successful submission response; it does not force execution success or replace the backend's duplicate-handling result. |
| Case changes | [Bind policy at request registration](#continuing-case-model). [Already held requests keep their original deadline](#tx-response-lost-case-switching). Preserve the Lab's record of affected transaction identities across reactivation. |
| Required evidence | Received requests and signatures, selected Case and settings, the shared registration/control order, simulator acceptance or the local backend's acceptance response, applied Faults, RPC Boundary Results, and any observed lifecycle changes and later RPC actions. Each statement identifies its evidence source. |
| Implemented Findings | Two [version-1 rules](evidence-and-findings.md#implemented-version-1-rules) explain observed acceptance, selected response loss, observed withholding, and a later status/transaction request or submission attempt for the same signature. Rules report receipt of the later action; they do not interpret its returned state or infer operation completion. Saved later response facts remain available in Timeline/Evidence Slice. |
| Verdict boundary | A base Investigation can be useful without a Verdict. A Verdict requires configured Checks and sufficient evidence; the [Reference Application](reference-application.md) can supply known intent correlation and application observations. |
| Unavailable conclusions | RPC-only evidence does not establish a Client Outcome, final business-operation state, or shared intent between different signatures. Simulator state does not establish program execution, balance changes, or duplicate economic execution. |

The accepted [RPC contract](solana-rpc-contract.md) defines format coverage, request behavior, Read-through, and the initial clock profile.
The verified local format matrix covers legacy/v0/v1 and explicit nonce/lookup state.
Backend-version compatibility and Surfpool runtime verification remain implementation work.
Any missing required capability must be declared as unsupported or as a limit on the available conclusions.

#### Verified implementation histories

The runtime uses one bounded owner to order RPC registrations and Case changes.
A batch binds one immutable policy; its elements select in index order.
Only a new accepted signed identity with a response ID and matching signature filter can consume selection.
A notification may create protocol state without consuming a Fault; an existing lifecycle is never treated as a new acceptance on retry.
The SQLite ledger persists selected identities without eviction across disarm and reactivation.
Acceptance, required signed bytes, and `fault.response_loss_selected` commit atomically.
The implementation separately records `fault.withholding_started`, prepared source results, and `rpc.boundary` after actual transport closure.
A committed selection without a later observation does not establish that withholding was observed.

The default real hold is 20,000 ms; bounded settings can lower it.
HTTP tests use explicit short profiles and an independent controlled simulator clock.
A Rust-only scheduling pause establishes a no-Case registration before a later arm without relying on sleeps; no remote pause control exists.
Tests read the socket to prove no headers/body arrive and then observe EOF.
They cover independent 2/4/6 progression, concurrent retries, distinct signatures sharing a hash, signature filters, notifications, invalid/preflight errors, disarm/reactivation, and shorter/longer policy replacement.
Batch tests verify two selected identities occupy one held transport, retain other processing evidence, and give each element a shared boundary effect.
Capacity tests reject new admission without creating an identity and release the slot after observed peer closure.
A real SQLite ledger-write failure rolls back acceptance and selection together without beginning a hold.
Normal drain retains the original deadline; force cancellation preserves identity.
A native process history stops while the transport is still held and verifies finalized normal shutdown versus interrupted force shutdown after completion evidence settles.
Legacy/v0/v1 nonce histories retain exact bytes and advance the nonce once despite response loss and retry.

Future evidence capacity is reserved before irreversible acceptance: one start record plus prepared-result and boundary records for every element.
The hold slot remains owned until transport closes and its completion evidence settles.
An ordinary Case-capacity error is separate from a fatal Journal error.
The simulator does not execute programs, forward writes, infer a client timeout, or establish a business outcome.
The Findings engine, Custom YAML engine, and MCP adapter remain their own tasks.

Checkpoint B is reproducible with:

```bash
cargo test --locked --test lab_lifecycle checkpoint_b_native_http_loses_response_then_tracks_original_signature_without_restart -- --nocapture
```

That history starts a native Lab, arms a short explicit hold, sends an independently signed transaction over HTTP, observes EOF without a response, and queries the original signature until processed.
It verifies unchanged Lab/process identity and reopens acceptance/selection/withholding/transport evidence after force stop.
Client receipt remains `unknown` in Journal evidence; the test's socket observations do not create application or economic conclusions.

### RPC unavailability or timeout

The `provider-timeout-burst` default targets `sendTransaction`, `getLatestBlockhash`, `getSignatureStatuses`, and `getTransaction`.
Other supported methods remain available unless an explicit method filter includes them.
Each activation repeats a 15-second real-time cycle: 5 seconds of outage, then 10 seconds of recovery.
The first cycle starts at activation, even without traffic.
Registration time selects the interval; its right boundary belongs to the next interval.

During outage, Ronchi does not dispatch a selected call to the simulator, Read Upstream, or Surfpool.
It holds the request without a response for 20 seconds of real time, then terminates the transport.
Observed cancellation or disconnect releases it earlier.
Consequently, a selected submission has no acceptance from that attempt.
An earlier attempt for the same signature may already have been accepted.
During recovery, newly registered calls dispatch normally.
Held requests retain their deadline across recovery, disarming, or replacement.

The Journal records cycle position, selection, non-dispatch, hold start, and actual termination or cancellation.
A Finding can explain that Ronchi prevented dispatch and later received another RPC attempt.
It cannot call the client result a timeout without client evidence.
This variant does not reproduce an upstream timeout after ambiguous acceptance, rate limiting, DNS failure, or an entire provider outage outside the selected HTTP path.

### Implemented timeout Coverage Claim

`provider-timeout-burst` is available in simulator and managed Surfpool modes.
The continuing policy freezes its cycle at whole-batch registration.
Outage and recovery do not depend on polling or later dispatch time.
The default outage/recovery intervals are 5000/10000 ms.
The optional `timeout` settings accept `outage_ms` and `recovery_ms` in 1..120000 ms and up to 32 supported method names.
An empty method list uses the maintained default four methods.
CLI equivalents are `--outage-ms`, `--recovery-ms`, and repeated `--rpc-method`.
`hold_ms` remains 1..20000 ms and defaults to 20000 ms.
`max_held` remains 1..16 shared HTTP exchanges; a batch consumes one slot.
An exact signature filter selects calls containing one of its eligible signatures.
Methods without a signature do not match a nonempty signature filter.

Before selecting a hold, Ronchi checks request shape, supported parameters, mode capabilities, signed input, and required local simulator ALT/nonce state.
Surfpool retains authority over state-dependent results that require execution or a source read.
The pre-dispatch validator does not contact the source to learn those results during outage.
Invalid calls, notifications, missing Read Upstream capabilities, and unknown methods do not consume a hold.
Capacity failure returns explicit `case_capacity_exceeded` before dispatch.
Earlier accepted transactions continue their own lifecycle while another attempt is held.

`rpc.dispatch_prevented` records the original binding, cycle position, method, signatures when available, and intended authority.
Its held attempt has no `rpc.response_prepared` result and no new transaction acceptance.
`fault.withholding_started` and `rpc.boundary` retain the actual shared-transport history.
Cancellation preserves reserved evidence and releases the hold after its owned start commit settles.
A required evidence write failure fails the writer instead of leaving cleanup waiting indefinitely.

The version-1 `provider-timeout-burst.later-rpc-attempt` Finding requires selected non-dispatch, hold-start, actual transport boundary, and a later same-method registration inside the selected scope.
Its `RpcRequest` scope contains the original request and method without inventing an accepted transaction.
The statement describes receipt of the later request.
It does not establish that the application retried because of this Fault or observed a timeout.
Missing support produces an Evidence Gap or no match.

Focused histories cover exact cycle-coordinate boundaries, no-traffic recovery, repetition, delayed batch dispatch after replacement, shared batch withholding, malformed/unsupported calls, simultaneous capacity admission, disconnect, cancellation during a gated start commit, and required-write failure.
Simulator histories also verify preflight/ALT/nonce capability checks and preserved earlier acceptance.
A fake Read Upstream verifies zero dispatch during outage and one allowlisted read after disarm.
Native pinned Surfpool 1.7.0 histories verify held reads and signed submission without backend dispatch, then authoritative recovery and retained Finding inspection.
These checks do not establish production-provider fidelity or an unobserved Client Outcome.

### Unavailable or stale transaction status

These variants target `getSignatureStatuses` and `getTransaction` for identities first accepted under the current activation.
They do not change authoritative lifecycle state.
Each read records the authoritative source view separately from the returned boundary view.
Signature arrays retain their size, order, and unaffected entries.
Unknown signatures and already-null source results do not become injected Faults merely because the boundary returns `null`.

For `signature-status-null`, the default masking window lasts 10 seconds from first acceptance.
Simulator mode measures it in simulator time.
Surfpool mode measures it in real time from Ronchi's observation of a successful submission response.
Within that window, Ronchi replaces an available selected status entry or transaction result with `null`.
After the window, reads return the current authoritative view.
Each new eligible identity gets its own window; retries do not restart it.

For `stale-transaction-read-after-submission`, Ronchi retains the first available status view actually observed for an eligible identity.
It freezes that view for 10 seconds from capture, using the same mode-specific clock.
The simulator can capture its own lifecycle transition; Surfpool requires a collected backend response.
Until a snapshot exists, Ronchi returns the normal source result and records no stale Fault.
If Surfpool's first observed status is already finalized, this variant cannot manufacture an earlier status.
Ronchi records a stale Fault only when the retained view changes the boundary result.

The frozen view governs both lookup methods.
`getTransaction` remains `null` if that view has not reached the method's requested supported commitment.
If retained content is available at that commitment, Ronchi serializes the actual retained transaction under the RPC format contract.
It never copies another signature's result or invents execution metadata.
Source slots, query context, and snapshot provenance remain distinct in evidence.
If a supported query constraint cannot use the snapshot, the call follows its normal constraint handling and records why no stale Fault applied.

Disarming ends masking for new requests; registered reads keep their selection.
The authoritative transaction continues progressing throughout both variants.
Findings can explain that an available source result was hidden or that an older observed lifecycle view was returned.
They can link a later query or same-signature resubmission.
Neither variant establishes rejection, an abandoned intent, a client retry policy, or a real consensus regression.

### Verified null and stale lookup claim

Task 19 implements `signature-status-null` and `stale-transaction-read-after-submission` in both modes.
Both retain the default 10-second windows above.
Optional typed `status` settings can lower `max_keys` from 1,024 and `max_snapshot_bytes` from 4 MiB.
The CLI exposes the same settings as `--status-max-keys` and `--status-max-snapshot-bytes` on `case arm`.
The byte bound charges each admitted key 1 KiB and accounts for serialized snapshot content beyond its reserved allowance.
One captured status entry is limited to 16 KiB.
These bounds are provisional resource defaults, not measured memory or throughput claims.
Capacity errors occur before selecting a new Fault.
Existing acceptance and lifecycle records remain authoritative.

A registration retains its activation state until that exchange finishes.
Replacing or disarming the Case changes later registrations.
It does not change a registered read's selection.
Duplicates preserve their original acceptance window.
An identity first accepted under an older activation is not selected by rearming the Case.
Signature filters preserve unselected entries.
Read notifications can observe a source view but never record a changed response Fault.

The simulator records its first available processed view in the committed lifecycle transition.
Its capture timestamp is the actual simulator observation time, separate from the scheduled transition time.
Surfpool captures only a collected `getSignatureStatuses` entry.
A `getTransaction` result alone does not establish exact backend finality.
Until a status snapshot exists, transaction reads keep the authoritative result and record `snapshot_unavailable`.
If the first collected backend status is finalized, Ronchi retains that finalized view.
It does not manufacture an earlier commitment.

`rpc.lookup_source` records source authority, source identity, query context, entry index, and availability before a boundary transform.
`fault.lookup_changed` links that source fact to the selected replacement and any captured view.
Prepared per-entry result digests and the separate transport boundary confirm the recorded output.
Version negotiation, transaction encoding, supported commitments, unavailable recent history, and ordinary query errors keep their base handling.
A frozen transaction lookup is `null` until the captured view satisfies its requested commitment.
At an eligible commitment, Ronchi uses the same signature's actual retained content.

The version-1 `rpc.lookup-view.changed` Finding requires acceptance under the selected activation, an available source view, a changed lookup, its prepared digest, and a completed server boundary within the selected scope.
A stale Finding also requires its actual captured source fact within that scope.
The Finding does not establish client receipt, rejection, business intent, retry policy, or production consensus.

Controlled-clock HTTP histories verify per-identity recovery, source progression, mixed order, unknown identities, snapshot provenance, query constraints, replacement, filtering, notifications, and resource bounds.
A controlled backend HTTP adapter verifies a confirmed snapshot followed by a finalized source view.
Pinned native Surfpool verification uses an explicit isolated funded snapshot.
It verifies actual transfers during masking, recovery without window restart, missing-snapshot passthrough, genuine first finality capture, retained Findings, and owned cleanup.

### Stalled confirmation

In simulator mode, `tx-confirmation-stalls` assigns a schedule to each new transaction first accepted under its activation.
The transaction becomes successful at `processed` after the normal 2-second processing delay.
It remains `processed` for 30 extra seconds beyond the normal confirmation schedule.
It reaches `confirmed` at 34 seconds and `finalized` at 36 seconds from acceptance.
The `finality-delayed` variant instead reaches `processed` at 2 seconds and `confirmed` at 4 seconds, then reaches `finalized` at 36 seconds.
All durations use simulator time.

Each eligible identity receives one schedule.
Resubmission and polling do not restart it.
New identities receive independent schedules while the Case remains active.
At the declared recovery time, progression resumes without a request or application action.
Disarming or replacement leaves previously assigned schedules intact.
Later submissions use the newly bound policy.

Status fields and commitment-gated transaction availability derive from that authoritative schedule.
Holding confirmation after processing does not expire an already processed transaction when its recent blockhash ages out.
The Journal records the assigned schedule and actual transitions.
A Finding can state how long simulated commitment remained below the requested level and identify later RPC actions.
It cannot establish real validator finality, application failure, or an unobserved client deadline.

Surfpool cannot provide this authoritative lifecycle variant through a response filter.
It remains unsupported there unless a future maintained backend capability can actually control confirmation.
A stale boundary view in Surfpool is the preceding family, with that family's narrower claim.
Infinite stalls and arbitrary commitment regressions are outside these initial variants.

#### Verified simulator confirmation-delay claim

`tx-confirmation-stalls` and `finality-delayed` are available through the shared Case catalog and arm operation.
An optional `confirmation` object contains `processing_ms`, `confirmation_interval_ms`, `extra_delay_ms`, and `hold_at` (`processed` or `confirmed`).
Each interval is positive and at most 120,000 ms.
The default schedules are the 2/34/36-second and 2/4/36-second variants above.
The shared signature filter limits eligible new identities.
Notifications do not select this Fault.
Duplicate identities keep their assigned schedule.

Acceptance and `fault.confirmation_schedule_assigned` commit in one atomic group.
The assigned deadlines and autonomous `transaction.transition` events remain readable after Case replacement.
The version-1 `sim.confirmation-delay.observed` Finding requires selected simulator acceptance, assignment, held commitment, and recovery transition facts.
Its statement gives the observed scheduled interval in simulator time.
Its references remain inside the requested Lab, scope, and snapshot.
Omitting assignment or transition support prevents that Finding.
It does not establish an application deadline, application failure, or validator finality.

Controlled-clock HTTP histories verify autonomous recovery, commitment-gated lookup, duplicate and disarm preservation, signature filtering, notifications, interval bounds, and no expiry after processing.
Managed Surfpool capability verification remains part of the managed-mode integration.


### Expired recent blockhash submission

The `tx-blockhash-expired` default deliberately reuses an observed `getLatestBlockhash` result.
The matcher uses the method's default commitment, including an omitted commitment.
An explicit commitment filter can select another supported view.
Ronchi captures one genuine result with its blockhash, `lastValidBlockHeight`, source context, and authority.
It returns that same tuple on subsequent matching calls while the source block height advances.
It never changes signed transaction bytes or compares a public height to a simulator height.

The tuple becomes expired only when the same authority and commitment view report `blockHeight > lastValidBlockHeight`.
At equality it has not expired under this rule.
Ronchi continues returning the expired tuple until the first matching submission using it, or for 10 real-time seconds after the expiry observation, whichever occurs first.
It then starts a 10-second real-time recovery interval in which reads return fresh source results.
The next matching blockhash read after recovery starts another capture cycle.
If traffic never uses the expired tuple, the Investigation reports that no expired submission was observed.

If a request's supported query constraints exclude the cached context, Ronchi uses normal constraint handling and records a skipped selection.
If no genuine tuple or source-height evidence is available, this Case cannot claim an expired-hash incident.
The Case does not accelerate Lab time, invent a protocol validity duration, or guarantee an immediate incident after activation.

In simulator mode, a new validly signed submission with `skipPreflight: true` can receive its signature and RPC acceptance even though its recent blockhash is already expired.
The simulator records processing as prevented by expiry; it does not assign the successful 2/4/6-second schedule.
`getSignatureStatuses` remains `null`, and `getTransaction` returns `null` because the transaction was never processed.
An internal `transaction_expired` diagnostic is not a fabricated synchronous Solana rejection or an execution error in a processed status.
Unsupported full preflight still receives the base capability error.

In Surfpool mode, Ronchi preserves the actual submission response and collected local lifecycle result.
It cannot promise a particular rejection code, acceptance, or expiry outcome from merely returning the cached tuple.
Durable-nonce lifetimes are not eligible for this recent-blockhash Fault.
Already processed identities retain their state on resubmission; expiry does not retroactively fail them.
Disarming releases the cache for new reads but cannot refresh bytes already signed by the backend.

Required evidence links the original tuple, authority, later source height, returned stale tuple, submitted signature and blockhash, and observed admission, processing, and boundary results.
A Finding can explain that a submission used a provably expired recent blockhash and report the mode's observed result.
A different replacement signature is a separate observed transaction; shared economic intent still requires application evidence.

### Mode support and limits

This matrix defines intended variants, subject to implementation verification.
Read-through does not change local transaction authority in simulator mode.

| Variant | RPC Simulator | Managed Surfpool | Unavailable claim |
| --- | --- | --- | --- |
| `provider-timeout-burst` | Pre-dispatch hold and termination | Same boundary policy before backend dispatch | Provider internals or client-observed timeout |
| `tx-response-lost` | Accepted identity and controlled successful lifecycle; withhold response | Withhold an observed successful submission response; preserve backend state | Forced execution success in Surfpool |
| `signature-status-null` | Hide available local lookup data | Hide collected backend lookup data | Missing data proves rejection |
| `stale-transaction-read-after-submission` | Retain an actual local earlier view | Retain an actually collected backend view | Manufacture a prior view or real reorg |
| `tx-confirmation-stalls`, `finality-delayed` | Assign the bounded delayed lifecycle above | Authoritative stall unsupported | A response filter controls backend finality |
| `tx-blockhash-expired` | Reuse observed tuple; record admission separately from expiry-prevented processing | Reuse observed backend tuple; preserve actual backend result | Guaranteed backend rejection, nonce expiry, or public execution |

Initial operational ceilings are provisional defaults: 1,024 transient identity keys per activation and 16 concurrently held HTTP requests per Lab.
A batch occupies one held HTTP request, with evidence for each affected element.
Case settings may lower a ceiling; they cannot exceed the effective core or Lab limit.
The durable `tx-response-lost` affected-identity ledger is not a transient cache and must not evict identities to reset eligibility.
If transient Case state or hold capacity is exhausted, Ronchi returns an explicit resource error before selecting a new Fault.
Journal writer failure or exhaustion of its evidence admission budget instead fails and stops the Lab under the [evidence contract](evidence-and-findings.md#persistence-bounds-and-failure).
It does not silently turn the call into an unaffected success or erase an existing transaction.
Already accepted submissions and assigned lifecycles remain recorded.
All real-time waits use a monotonic clock; simulator intervals use the declared simulator clock.
These bounds support prototype implementation and are not performance release gates.

### Minimal implementation histories

Verification uses controlled clocks, fake upstreams, and isolated local execution.
It must cover each Fault's matching and nonmatching calls, recovery boundary, cancellation, Case switch, duplicate identity, and concurrent batch selection.
For status masking, compare source evidence with the changed boundary result and include unknown signatures.
For stalls, advance time without polling and verify transitions and commitment-gated lookup availability.
For expiry, cover heights immediately below, equal to, and above the validity bound, plus a durable nonce and an already processed identity.
For every Finding, remove one required fact and check that the stronger conclusion disappears or reports an Evidence Gap.
Successful finite histories establish those checks, not universal compatibility or economic correctness.

## Coverage Claim

Every implemented Case publishes:

- precise protocol and failure semantics, including supported variants;
- required simulator or Execution Backend capabilities and versions;
- Observation Boundary and Journal Evidence required for its Findings;
- supported Findings and any generic or application-supplied Checks for a Verdict;
- known limitations and unavailable conclusions.

If a required capability is absent, report the Case as unsupported or its conclusions as limited.
Do not silently substitute a different experiment.
Simulator Findings describe the controlled RPC history, not executed programs or changed balances.
Local execution claims require the optional Surfpool mode and relevant execution evidence.
Neither mode proves production consensus, validator topology, or universal provider fidelity.

An RPC history must stay coherent unless the Case explicitly probes malformed or inconsistent provider behavior.
An intentional stale boundary response does not justify losing the authoritative internal state or the record of the applied Fault.
Precise Solana semantics matter; later EVM adapters must not force unlike concepts into misleading shared behavior.

## Custom Cases

The [Custom Case contract](custom-cases.md) defines the prototype YAML v1 shape, primitive catalog, validation, limits, and provenance.
Built-in and Custom Cases share the same binding, evidence, mode, and recovery contracts.
Custom Cases have visibly distinct provenance and contain no arbitrary executable fault code.
If the installed primitives cannot express a behavior, a maintained core release or accepted contribution must add that primitive.
See [ADR-0007](../adr/0007-declarative-custom-cases.md).

## Later candidates

The following ideas are retained for evaluation, not included in the accepted initial boundary:

| Candidate | Purpose or limitation |
| --- | --- |
| `tx-submit-rate-limited` | Submission rate limits, retry budgets, and ambiguous acceptance |
| `provider-rate-limit` | Method-specific status, header, and body shapes |
| `partial-batch-response` | Completeness and request-ID correlation for incomplete or reordered replies |
| `malformed-or-truncated-response` | Parsing and response validation after partial transport success |
| `provider-disagreement` | Inconsistent views, quorum, or fallback; multi-provider routing is deferred |
| `websocket-hard-disconnect` | Reconnect, resubscribe, and gap recovery; WebSocket is deferred |
| `websocket-silent-gap` | Liveness detection and polling fallback on a live but incomplete stream |
| `event-duplicate` | Idempotent event handling |
| `event-reordered` | Ordering assumptions and buffering |
| `confirmation-regresses` | A simulated status regresses or disappears; this is not proof of a real consensus reorg |

The catalog is extended through protocol-aware fault, evidence, and Finding semantics.
The Reference Application is not turned into a separate implementation for each new Case.

## Candidate Packs

Packs are proposed groupings, not a committed first-release matrix:

| Pack | Intended grouping |
| --- | --- |
| `transaction-sender-resilience` | Submission ambiguity, confirmation stalls, expiry, stale status, and later rate-limit cases |
| `relayer-resilience` | Sender behavior plus durable intent reconciliation, externally driven restarts, and application-level Checks |
| `indexer-resilience` | Disconnects, gaps, duplicates, ordering, reorg-shaped histories, and backfill; outside the initial HTTP scope |
| `wallet-resilience` | Provider disagreement, stale balances, ambiguous submission, and terminal user-visible state |
| `exchange-deposits` | Duplicate events, delayed finality, provider disagreement, and canonical-history reconciliation |

The sender/relayer grouping reflects the first user wedge; it does not move later failure types into the initial five families.
The reference Withdrawal Intent remains correlation context without real SOL or SPL transfer execution in default mode.

## Deferred Exploration

Constrained random fault selection is a possible later discovery feature.
It does not replace named Cases.
Any such feature would need recorded choices/seeds and a way to reduce a useful discovery into a reproducible Case.
A seed alone would not control external state or concurrent request ordering.
No Exploration API or implementation is selected here.

## Managed Surfpool response-loss claim

`tx-response-lost` is available in simulator and managed Surfpool modes.
The two confirmation-delay Cases remain simulator-only.
Surfpool arm and initial-Case selection reject those confirmation capabilities before changing policy.

In Surfpool mode, Ronchi validates signed input and forwards submission only to its owned native backend.
A successful backend response must match the first signature independently decoded from the signed input.
Ronchi durably records that signed identity and its source authority before returning or withholding a successful response.
The acceptance and once-per-identity response-loss selection commit atomically.
Duplicates still contact Surfpool and retain its actual result or preflight rejection.
They never restart the accepted identity or consume another response-loss Fault.
Disarming, replacing, and reactivating the Case preserve this identity memory.

This acceptance establishes a backend RPC response, not a promise of successful execution.
Account effects and status require actual later backend evidence.
Ronchi records no invented simulated processing or commitment transitions in this mode.
The current response-loss Findings remain simulator-specific and report their source/mode limitation for Surfpool evidence.
A core source response does not establish a Client Outcome.
