# Product baseline and development plan

Status: all eight initial design outcomes are recorded. This file owns design decisions; [todo.md](todo.md) records runtime implementation and scoped verification.
Current position: follow the next unchecked task in [tasks/todo.md](todo.md).
Use the [implementation handoff](implementation-handoff.md) to start a coding session and follow the local commit workflow.
For the authorized autonomous completion of tasks 15–24, use the [remaining implementation run](remaining-implementation-run.md) and its [assignment ledger](parallel-run-state.md).

This plan preserves the completed discussion topics and the implementation handoff.
Items 1–3 record user-approved decisions from the interview.
Items 4–8 use the user's autonomous delegation recorded in the [execution brief](design-baseline-brief.md).
Their reversible defaults can change after prototype tests; no further interview is required before coding.
The [product overview](../docs/product/overview.md) and [ADRs](../docs/README.md#decisions) retain their authority.
The [open questions](../docs/open-questions.md) retain the unresolved and deliberately deferred choices.

## Recorded design outcomes

- [x] **1. First useful Investigation.** Describe one concrete result for a real Transaction Sender or Relayer connected without an SDK. Specify what the developer learns, which evidence supports it, and what they investigate next. Distinguish those conclusions from the stronger Demo conclusions supported by known intent correlation and Checks.
  - Outcome: an expected Investigation result and criteria for testing its usefulness.
  - Start with the accepted `tx-response-lost` incident.
  - Recorded: [guidance boundary](../docs/product/overview.md#product-value) and [expected no-SDK result and usefulness criterion](../docs/product/workflows.md#first-no-sdk-investigation).
- [x] **2. Exact behavior of the first Case.** Define `tx-response-lost` matching, repeated submissions, duration, recovery, and Case switching while requests are in flight.
  - Outcome: the first detailed Coverage Claim, including evidence requirements and limitations.
  - Recorded: [Coverage Claim](../docs/product/cases.md#tx-response-lost-coverage-claim), refined by the accepted RPC contract in item 3. Task 7 records simulator runtime verification; Surfpool verification remains pending.
  - Recorded: [transaction identity and default repetition rules](../docs/product/cases.md#tx-response-lost-identity-and-repetition), including preservation of already applied response loss across reactivation within the same Lab. Detailed wire behavior requires implementation verification.
  - Recorded: [default successful simulated outcome and independent processing and confirmation progression](../docs/product/cases.md#tx-response-lost-default-simulated-outcome). The default schedule is null status before processing, then `processed`, `confirmed`, and `finalized` at 2-second intervals with `err: null`. Standard response fields follow the RPC contract.
  - Recorded: [response withholding for up to 20 seconds followed by transport termination without a response](../docs/product/cases.md#tx-response-lost-response-withholding). Transaction identity and lifecycle state survive request termination.
  - Recorded: [request binding and the shared increasing registration/control sequence](../docs/product/cases.md#continuing-case-model), plus [Case-switching behavior for held requests](../docs/product/cases.md#tx-response-lost-case-switching). The [evidence order contract](../docs/product/evidence-and-findings.md#order-time-and-correlation) separates policy selection, committed history, time, and causal links.
- [x] **3. RPC Simulator sufficiency.** Identify the RPC methods and coherent histories required by the first workflow. Define unsupported-request behavior, the Read-through boundary, and the conditions under which an application needs Surfpool.
  - Outcome: a bounded compatibility and capability description for the first workflow.
  - Recorded: [Solana RPC contract](../docs/product/solana-rpc-contract.md), including all accepted decisions indexed below.
  - Recorded: [1:1 simulator-time rate](../docs/product/overview.md#simulator-time) and [local clock/validity profile](../docs/product/solana-rpc-contract.md#clock-and-validity-profile).
  - Implementation still needs exact schemas, pinned compatible versions, full format/encoding verification, and validation against independently running backends.
- [x] **4. Evidence and Findings.** Define the facts required for each Finding, correlation rules, and event ordering. Address incomplete evidence and failures inside Ronchi. Establish initial persistence, retention, deletion, redaction, and storage-bound policies.
  - Outcome: an evidence contract with supported Finding examples and explicit Evidence Gaps.
  - Recorded: [Evidence and Findings](../docs/product/evidence-and-findings.md).
- [x] **5. Human and Coding Agent investigation paths.** Walk through the same example in the dashboard, CLI, and MCP: summary, Finding, then Evidence Slice. Define useful response content and behavior when matching traffic or required evidence is absent.
  - Outcome: a concrete investigation flow with bounded machine responses over shared evidence.
  - Recorded: [Investigation interfaces](../docs/product/investigation-interfaces.md), including public Check operations.
- [x] **6. Persistent Lab behavior.** Define native startup, readiness, attachment to a running Lab, stop, and abnormal termination. Specify what happens to the Journal and processes owned by Ronchi.
  - Outcome: explicit Lab lifecycle and resource-ownership behavior.
  - Recorded: [Persistent Lab lifecycle](../docs/product/lab-lifecycle.md).
- [x] **7. Reference Application and Checks.** Define Withdrawal Intent states, transaction-identity persistence, and relayer recovery. Specify observable behavior for the `unsafe` and `resilient` profiles. Determine generic and application-supplied Checks and the evidence needed for a Verdict.
  - Outcome: observable reference acceptance behavior and bounded Verdict criteria.
  - Recorded: [Reference Application](../docs/product/reference-application.md) and [Checks and Verdicts](../docs/product/checks-and-verdicts.md).
- [x] **8. Remaining Cases and Custom Cases.** Specify the other four initial failure families. Use concrete Cases to identify shared Fault Primitives, then define YAML, configuration validation, and version compatibility.
  - Outcome: precise coverage for the initial families and a declarative composition contract grounded in those examples.
  - Recorded: [Cases and coverage](../docs/product/cases.md) and [Custom Cases](../docs/product/custom-cases.md).

The expected result guides Case semantics.
Case semantics guide simulator capabilities and evidence requirements.
The evidence model guides the investigation interfaces.
The implementation queue follows those dependencies and can be refined when a prototype exposes a better boundary.

## Implementation approach

Use one native Rust executable and the stack in [ADR-0019](../docs/adr/0019-resource-efficient-rust-core.md).
Start with project configuration, a durable Journal, and persistent Lab control.
Build a coherent local RPC history before adding the first lost-response Fault.
Deliver a bounded CLI Investigation before the reference Demo, dashboard, and MCP adapters.
Then complete Read-through, managed Surfpool, the remaining initial families, and Custom Cases.
The ordered tasks, acceptance criteria, dependencies, and checkpoints live only in [tasks/todo.md](todo.md).
Small prototype slices do not narrow the accepted format, method, or Case scope.

| Risk | Initial response |
| --- | --- |
| Protocol libraries omit an accepted format or lifetime mechanism | Pin a verified matrix early; retain unsupported existing formats as implementation gaps |
| Shutdown, saturation, or backend ambiguity loses evidence | Exercise committed acceptance, reserved capacity, fail-stop behavior, and interrupted recovery |
| Findings overstate RPC-only observations | Remove required facts in negative tests; preserve source authority and Evidence Gaps |
| Defaults consume too many resources or confuse users | Measure representative runs and revise provisional limits and interface details |

## Solana RPC decisions

Status: all nine decisions accepted. In item 2, the user replaced the restricted-format recommendation with coverage of all formats already present on Solana.
The authoritative behavior lives in the RPC contract; this table preserves the discussion index.

| No. | Decision | Accepted source |
| --- | --- | --- |
| 1 | `sendTransaction` preflight | [Submission and preflight](../docs/product/solana-rpc-contract.md#submission-and-preflight) |
| 2 | All current transaction formats and encodings, including `jsonParsed` and durable-nonce handling | [Transaction formats and encodings](../docs/product/solana-rpc-contract.md#transaction-formats-and-encodings) |
| 3 | Local protocol clock and blockhashes | [Clock and validity profile](../docs/product/solana-rpc-contract.md#clock-and-validity-profile) |
| 4 | Request parameters | [Request parameters and errors](../docs/product/solana-rpc-contract.md#request-parameters-and-errors) |
| 5 | Signature lookup and retained history | [Transaction lookup and history](../docs/product/solana-rpc-contract.md#transaction-lookup-and-history) |
| 6 | Read-through allowlist | [Read-through allowlist](../docs/product/solana-rpc-contract.md#read-through-allowlist) |
| 7 | Bootstrap methods | [Local methods](../docs/product/solana-rpc-contract.md#local-methods) |
| 8 | Fees | [Fee queries](../docs/product/solana-rpc-contract.md#fee-queries) |
| 9 | JSON-RPC batches | [JSON-RPC batches](../docs/product/solana-rpc-contract.md#json-rpc-batches) |

Standard field types, method defaults, and standard error meanings follow official method contracts.
Verification of those details remains implementation work, not an additional product vote.

## Later topics and their triggers

| Topic | When to discuss | Scope to preserve |
| --- | --- | --- |
| CI and Control comparisons | After the interactive workflow and Checks are defined | Bounded Run lifecycle, parallel execution, cleanup, wrappers around existing commands, and evidence for Comparability and Control transparency |
| Chaincase and replay | After the state and evidence needed for reproduction are defined | Versioned inputs, selected evidence, state restoration versus transport-only replay, and artifact compatibility and migration |
| Optional SDK | After checking which useful observations are missing without it | Client wrapping, Application Checkpoints, correlation, local transport, and integration ergonomics; the Go-first direction remains accepted |
| Distribution | Before public release | Initial native platforms, open-source license, public naming and registry checks, and delivery adapters; Testcontainers language selection follows the integration need |
| Numerical resource budgets | After an executable core and representative workloads exist | Measurements and release gates from [ADR-0019](../docs/adr/0019-resource-efficient-rust-core.md); the accepted stack and resource-efficiency priority remain in force |

## Validation throughout prototyping

Start product validation with the first working prototype.
Observe the transition from the bundled Demo to an independently running stateful backend.
Assess setup effort, usefulness without an SDK, result clarity, repeat use, and the maintenance burden of Cases and integrations.
Compare the usefulness and total investigation cost of dashboard, CLI, MCP, and the Fault Primitive catalog using observed work.
Keep demand and adoption uncertain until evidence supports them.

## How to continue

Start the next unchecked implementation task when development is requested.
Use the relevant project skills and the task's authoritative sources.
Make reversible implementation choices without reopening the full product interview.
Record a changed default and its reason in its authoritative document when prototype evidence warrants it.
Update `CONTEXT.md` only when a domain term is resolved.
Use an ADR when the decision warrants one under the project `domain-modeling` guidance.
Update the open questions when a choice is resolved or deliberately deferred.
Mark development tasks complete only after their acceptance and verification steps pass.
Design completion does not imply runtime verification or product demand.

## Foreground terminal experience

The user requested a polished animated terminal after verifying foreground `up` and Ctrl-C.
Implement this as task 25 in [todo.md](todo.md).
Keep the open third-party validation in Checkpoints C/E unchanged.

The terminal should resemble a small optical instrument panel.
Use the existing parallel-band identity, theme-native text, muted labels, and one coral Fault accent.
Use the terminal's own monospace typeface and background.
Keep the layout left aligned, with a compact wordmark and no full-screen takeover.
Body text inherits the terminal foreground; previews use warm white `#E8E4DF` on `#1B1D21`.
Muted labels use ANSI 245 (`#8A8A8A`); the Fault signal uses ANSI 209 (`#FF875F`).
The background is never painted by Ronchi.

1. Add a TTY-only Ronchi header, startup spinner, readable RPC URL, and short dashboard hyperlink.
2. Show elapsed time, the committed Journal watermark, the current Case, and held RPC requests from authenticated status.
3. Animate shutdown until the owned Lab exits, then show actual Journal completion or interruption.
4. Verify native PTY lifecycle, narrow widths, reduced motion, plain output, and JSON compatibility.

Use Console for terminal width and styling.
Use Indicatif for bounded inline redraws.
Drive animation from the existing Tokio runtime, without a separate ticker thread.
Poll authenticated status at most once per second, without running Findings or scanning the Journal.
Status-display failures do not change Lab health; terminal disconnection follows normal ownership shutdown.
Missing status must appear as unavailable rather than a zero count or a healthy state.

TTY detection selects presentation only.
JSON, detached startup, redirected output, `TERM=dumb`, and CI retain simple output.
`NO_COLOR` suppresses color and emphasis.
`up --no-animation` keeps a static styled introduction and completion message.
Never use raw mode, capture normal keyboard input, clear scrollback, or enter an alternate screen.
Dashboard links remain reusable until Lab stop; show the command for printing the same link.

Sources inspected before implementation:

- [Console](https://github.com/console-rs/console): terminal styles, width measurement, and ANSI handling.
- [Indicatif](https://github.com/console-rs/indicatif): inline progress and spinners.
- [Ratatui](https://github.com/ratatui/ratatui): broader terminal UI alternative, unnecessary for this scoped panel.
- [CLI Guidelines](https://clig.dev/#output): TTY-aware animation and intentional color.
- [NO_COLOR](https://no-color.org/): opt-out convention.
- [Terminal hyperlinks](https://iterm2.com/feature-reporting/Hyperlinks_in_Terminal_Emulators.html): OSC 8 hyperlink format and compatibility.

## Lab-lifetime dashboard access

The user approved a permanent link for the life of each Lab after finding the 60-second limit disruptive.
Task 26 supersedes the original task-14 browser expiry defaults.
Keep the fragment wire field `ticket` and the existing authenticated launch route for compatibility.
The link returns the same random launch key on every request.
Its exchange returns one distinct random browser key, with HttpOnly, SameSite=Strict and the existing Lab/port scope.
Keep both keys in the runtime only; no authorization pool or expiry timers are needed.
Stop invalidates access; a new Lab or viewer gets fresh keys.
Retained viewers apply the same rule for their own process lifetime and keep their read-only boundary.
Preserve Host, Origin, bearer separation, fragment removal, and credential-free evidence bookmarks.
The trade-off is explicit: a copied full launch URL grants access until its Lab stops.

## Human dashboard navigation

The user approved one interface combining a guided Lab overview, readable Finding investigation, and an RPC inspector.
Task 27 extends the implementation queue without closing independent-backend validation in Checkpoints C/E.
The product UI remains English.
Research notes, source studies, concept HTML and preview screenshots remain local and are excluded from commits.

Navigation has five destinations: Overview, Observations, Traffic, Case and Checks.
The Journal is an advanced Traffic view.
Overview explains first connection, current traffic, continuing Case state and the next investigation action.
Finding detail separates supported facts, unknown application outcome, developer guidance and supporting evidence.
Traffic lists RPC exchanges with observed boundary results and opens Overview, Request, Response and Records tabs.
Raw request bodies were not retained and must never be reconstructed as observed wire content.
Raw payloads require explicit selection and retain bounded byte continuation.

Extend the shared query core with investigate.requests and a request_id filter.
Each list row anchors one rpc.registered event and projects bounded related metadata at the same watermark.
Use the existing request index, response limits, cancellation and opaque cursor binding.
Missing boundary evidence means unknown, not pending, success or client receipt.
The projection adds no storage schema or writer work.

Page links preserve Lab identity, filters, snapshot and selected event or Finding.
Old bookmarks remain valid.
Back navigation restores the previous selection.
Opening detail or continuing a page pins the view; refreshing explicitly returns to the latest snapshot.
Hidden pages do not initiate background Investigation queries.
Hidden browser tabs suspend polling.
A current runtime Case is labelled separately from historical Case context.
Case authoring remains independent of catalog loading.
Retained Journals allow reading and Check evaluation, with live mutations disabled.

Implementation order: bounded core, navigation/overview, observation detail, inspector/controls, verification.
Check each slice through focused tests.
Finish with native browser flows, full Rust/UI checks, visual desktop/narrow review and an embedded release build.
