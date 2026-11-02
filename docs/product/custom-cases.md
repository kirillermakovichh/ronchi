# Custom Cases

Status: prototype design contract for YAML v1. This is not a released schema or implemented capability.
[Cases and coverage](cases.md) owns built-in defaults, continuing policy, and mode limits.
[ADR-0007](../adr/0007-declarative-custom-cases.md) records why extensions use maintained primitives.

## Boundary

A Custom Case is a bounded declarative program over built-in, versioned Fault Primitives.
The Ronchi core performs matching, sequencing, Fault application, and Finding evaluation.
It accepts no executable plugins, shell commands, scripts, SQL, general expressions, regular expressions, external includes, or network-loaded code.
The artifact cannot add RPC methods, bypass forwarding checks, change state authority, or introduce execution capabilities.
An unavailable primitive requires a maintained core change, not a YAML workaround.

The file, dashboard, and Coding Agent author the same artifact.
Validation produces an inspectable resolved plan before activation.
One Custom Case occupies the Lab's single active Case slot.
Atomic activation and request registration use the shared `registration_seq` order.
A batch binds one complete policy snapshot.
Already registered requests keep their revision, timers, and selected Faults after replacement.

## Small YAML v1 shape

This example hides available transaction lookups for ten simulator seconds after the first matching lookup for each eligible signature.
It then observes one further lookup without changing its result.
It differs deliberately from the built-in `signature-status-null` window, which starts at acceptance.

```yaml
schema: ronchi.case/v1
id: custom.lookup-gap
revision: "1"
description: Hide local lookups, then observe another lookup.
requires:
  primitives:
    - rpc.lookup-null@1
    - observe.rpc@1
  finding_templates:
    - lookup-hidden-then-queried@1
modes: [simulator]
filter:
  methods: [getSignatureStatuses, getTransaction]
  transaction_origin: accepted_under_this_activation
state:
  key: transaction_signature
  scope: activation
  max_keys: 1024
  on_exhausted: error
sequence:
  - id: hide
    using: rpc.lookup-null@1
    until: {elapsed_ms: 10000, clock: simulator}
    next: observe
  - id: observe
    using: observe.rpc@1
    until: {matches: 1}
    next: done
findings:
  - id: hidden-then-queried
    template: lookup-hidden-then-queried@1
    facts:
      hidden: {kind: lookup_hidden, step: hide}
      later: {kind: lookup_received, step: observe}
    same: transaction_signature
    order: [hidden, later]
    within_ms: 60000
guidance: Check how the backend reconciles unavailable status.
```

`schema`, `id`, `revision`, `requires`, `modes`, `filter`, `state`, and `sequence` are required.
`description`, `findings`, `guidance`, and the top-level `repeat` declaration are optional.
Identifiers use ASCII letters, digits, dots, underscores, and hyphens, with at most 64 characters.
Step and rule IDs are unique within the artifact.
Description and guidance contain at most 512 characters each.
Guidance is displayed separately from evidence-backed Finding text.
An author cannot supply a custom factual conclusion through guidance.

## Matching and sequence

`filter.methods` contains exact implemented method names.
Optional `signatures` contains at most 32 exact first signatures.
Optional `commitments` contains supported normalized commitment values for methods that accept that parameter.
Omitted filters impose no additional restriction.
Unknown fields and filters unsupported by a selected primitive are errors.
`transaction_origin` is either `accepted_under_this_activation` or `known_local`.
The first value requires acceptance under this activation; the second permits retained local identities.
Neither value refers to hidden business intent.
In Surfpool mode, eligibility requires recorded backend identity and response evidence.

State keys are either `transaction_signature` or `case`.
The only accepted state scope is `activation`, and `on_exhausted` must be `error`.
The `case` key supplies one schedule for a whole activation, such as a timeout burst.
`transaction_signature` supplies independent schedules for bounded tracked identities.
A multi-signature lookup evaluates each eligible entry without reordering the response array.
The first eligible registration creates a key and enters its first step.
Time-based steps start their clock at entry.
`until` accepts exactly one typed bound: `elapsed_ms` with `clock`, or `matches`.
The allowed clocks are `real` and `simulator`; a primitive can restrict them by mode.
`matches` counts eligible registrations, including a recorded no-change result when the source was already unavailable.
It does not count invalid requests or unsupported capabilities.

At a time boundary, Ronchi advances to the next step before selecting a Fault for that registration.
For a count bound, Ronchi applies the current step to the last selected registration, then advances for the next registration.
Selections and state changes occur in registration order; batch elements use array order within their shared registration.
Method execution and completion remain concurrent.
`next` names a later step, `done`, or the first step with a bounded repeat declaration.
Forward-only sequences are the default.
The only loop form is top-level `repeat: {cycles: N}`, with `1 <= N <= 100`, or `repeat: {while_active: true}`.
A repeating sequence must contain a positive real-time or simulator-time bound.
The validator rejects zero-time cycles and other backward edges.
After `done`, further calls for that key follow the normal RPC contract.

The only user state consists of the declared key, current step, bounded counters, deadlines, and primitive-owned typed data.
There are no user-defined variables, computed keys, payload mutation expressions, or cross-Lab state.
Only the built-in response-loss primitive can use the mandatory Lab-lifetime affected-identity ledger.
Replacing a Custom Case resets activation-scoped sequence state for future registrations.
It cannot clear that ledger or rewrite a transaction lifecycle already assigned at acceptance.

## Initial primitive catalog

`@1` identifies an exact semantic version of a primitive, not a package range.
The installed catalog publishes its argument schema, matching stage, evidence facts, limits, and mode requirements.
The following catalog is the initial implementation target.

| Primitive | Meaning and bounded arguments | Mode |
| --- | --- | --- |
| `rpc.timeout-before-dispatch@1` | Do not dispatch a selected valid call; hold without response, then terminate. `hold_real_ms`, default `20000`. | Both |
| `rpc.withhold-submission-response@1` | After successful acceptance, withhold the successful submission response. `hold_real_ms`, default `20000`; fixed once per identity per Lab. | Both; Surfpool requires observed successful submission response |
| `rpc.lookup-null@1` | Replace available selected status entries or transaction results with `null`; preserve authoritative source evidence. | Both |
| `rpc.lookup-snapshot@1` | Capture the first actually available per-identity lifecycle view, then reuse that view within the sequence step. Return only query-compatible retained content. | Both; no invented prior backend state |
| `sim.confirmation-delay@1` | At first simulator acceptance, assign `processing_ms`, `confirmation_interval_ms`, `extra_delay_ms`, and `hold_at: processed|confirmed`. Defaults `2000`, `2000`, `30000`, `processed`. | Simulator only |
| `rpc.blockhash-reuse@1` | Capture one actual blockhash tuple per selected commitment, reuse it, and record same-authority height-based expiry. Release on first expired-hash submission or bounded expiry window. `expiry_window_real_ms`, default `10000`. | Both; Surfpool submission result remains authoritative |
| `observe.rpc@1` | Observe matching RPC calls without changing their results. No arguments. | Both |

Primitive actions occur only at their declared stage.
An optional step-level `with` mapping supplies exactly the arguments published for that primitive.
Omitted arguments use catalog defaults; unknown arguments are errors.
For example, a submission-response primitive cannot run before acceptance, and a confirmation-delay primitive cannot rewrite an existing lifecycle.
The core validates entry filters against that stage.
It rejects conflicting state-mutating actions for one identity rather than selecting a winner by file order.
Only one step selects an action for a key and registration.
Primitive-owned acceptance callbacks may continue an already assigned lifecycle independently of later steps.
Timeouts and response loss use the shared batch boundary policy.
Notifications cannot consume a missing-response Fault.

The blockhash primitive does not accelerate time or guarantee expiry after a fixed number of seconds.
Its source tuple and height must have one authority and compatible commitment.
It does not apply recent-blockhash expiry to durable nonces.
Simulator acceptance, expiry-prevented processing, and the returned signature remain distinct under the [expired-blockhash contract](cases.md#expired-recent-blockhash-submission).

## Finding rules

YAML v1 selects published, versioned Finding templates and typed facts.
It cannot define arbitrary predicates or free-form factual conclusions.
Each template defines its required fact slots, correlation keys, permitted ordering, output text, and Evidence Gaps.
For `lookup-hidden-then-queried@1`, `hidden` requires an available authoritative lookup, the selected Fault, and a boundary result changed to `null`.
`later` requires a later received lookup for that same signature.
The Finding says those recorded events occurred; it does not claim why the backend queried again.

Fact selectors use exact kinds and step IDs.
`same` supports only a key published by that template, such as `transaction_signature`.
`order` lists required positive facts in Journal order.
`within_ms` bounds their interval using recorded elapsed real time.
The first implementation accepts positive fact patterns only.
Absence tests, arbitrary negation, aggregation, joins across Labs, and application correctness rules are unsupported.

The evaluator uses referenced `(lab_id, event_seq)` evidence and the recorded policy bindings.
It emits at most one Finding per rule, key, and sequence cycle.
If multiple matches exist, it selects the earliest complete ordered fact set by `event_seq`.
Identical evidence and template versions produce the same result.
Missing or redacted required facts prevent the stronger Finding and expose the relevant Evidence Gap.
No rule produces a Verdict.
Client Outcomes require client evidence, and economic claims require the appropriate application and execution evidence.
The initial templates make neither claim.

## Validation, bounds, and errors

Validation parses UTF-8 YAML 1.2 without duplicate keys, tags, anchors, aliases, merge keys, or includes.
The schema permits strings, booleans, bounded integers, arrays, and mappings; it rejects floating-point and implicit timestamp values.
It checks schema fields, resolved versions, capability requirements, filters, sequence reachability, action conflicts, argument bounds, and Finding fact types.
It also checks the selected Lab mode and effective resource limits.
Validation has no RPC, state mutation, or external-network effects.
An invalid replacement leaves the active Case unchanged.
Activation installs the complete validated plan atomically at one `registration_seq`.

Provisional ceilings are 64 KiB per artifact, 8 steps, 8 Finding rules, 32 values per filter, 1,024 transient keys, and 16 held HTTP requests per Lab.
Individual step waits and hold arguments are positive and at most 120,000 ms.
Counters are positive and at most 10,000 matches per step.
A Finding's `within_ms` is at most 300,000.
The effective limit is the lowest of core, Lab, and Case limits.
The built-in `tx-response-lost` default remains 20,000 ms.
Its mandatory durable identity ledger is not subject to transient-key eviction.
These are prototype guardrails, not performance release gates.

Errors have a stable symbolic category, a bounded message, and a YAML field path where available.
Initial categories are `invalid_yaml`, `unsupported_schema`, `unknown_field`, `unknown_primitive`, `incompatible_version`, `unsupported_mode`, `invalid_filter`, `invalid_sequence`, `invalid_finding_rule`, and `limit_exceeded`.
Runtime `case_capacity_exceeded` identifies a key or hold limit before a new Fault is selected.
It does not silently execute a weaker policy, forget an affected identity, or rewrite an accepted result.
If a required source snapshot is not yet available, the primitive records a normal no-change selection with that reason.
If transient Case state cannot be retained, the core reports an explicit resource error rather than applying a weaker policy.
Journal writer failure or exhaustion of its evidence admission budget instead fails and stops the Lab under the [evidence contract](evidence-and-findings.md#persistence-bounds-and-failure).
Operational failure after acceptance preserves the acceptance fact and reports the actual boundary action.

## Provenance and compatibility

Activation records `origin: custom`, the Case ID and revision, a SHA-256 content digest of the canonical parsed artifact, the resolved primitive and template versions, the core compatibility profile, settings, and activation registration.
The Journal retains the resolved plan or an immutable local artifact reference under the Lab's evidence policy.
Evidence and Findings carry the applicable policy revision and source references.
A custom description never grants a maintainer-verified Coverage Claim.
Inspection lists declared coverage, actual supported modes, skipped capabilities, and author provenance separately.

Unknown schema versions and absent exact primitive versions fail validation.
The core does not silently reinterpret a file with a newer primitive.
A semantic change requires a new primitive or template version.
Migration creates a new artifact revision and shows the changed resolved plan before activation.
The digest changes when parsed input changes; YAML comments and key ordering do not change it.
Canonical input is compact UTF-8 JSON with recursively sorted mapping keys, preserved array order, decimal integers, normal JSON string escapes, and unescaped non-ASCII characters.
The canonicalizer does not normalize Unicode strings or insert omitted defaults.
Resolved defaults belong to the separately recorded plan and pinned catalog versions.
This canonical form is part of prototype v1, not a released cross-version compatibility promise.
Reproduction also requires relevant initial state, time, ordered inputs, dependency observations, and recorded versions.
The YAML alone cannot reproduce a changing upstream or an independently running backend.

Implementation verification must include a valid composite sequence, each validation error class, mode rejection, capacity exhaustion, batch binding, reactivation, and incomplete Finding evidence.
It must also verify that response-loss composition cannot reset an identity already affected in the same Lab.
