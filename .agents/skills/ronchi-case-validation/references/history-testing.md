# Testing RPC histories

Start from the affected contract and the selected execution mode.
Use the [shared Solana RPC references](../../../references/solana-rpc/index.md) for protocol facts.
Do not assume the simulator implements every behavior documented for a real node.

## Separate the layers being checked

Distinguish request matching, authoritative simulator/backend state, fault application, RPC Boundary Result, and recorded Journal Evidence.
Check the intended relationship between those layers rather than requiring every returned response to equal current internal state.
A declared stale response or lost response can be the intended Fault.
Its internal state change, applied Fault, and returned boundary behavior must still be recorded coherently.

For the affected behavior, identify:

- supported requests, configurations, transaction identities, and initial state;
- protocol time, block height, slot, and validity inputs that matter;
- permitted transitions and results before, during, and after the Fault;
- continued use after failures, repeated calls, and Case disarming or replacement when relevant;
- invariants that the Coverage Claim actually promises.

Do not choose exact repetition, recovery, or concurrent Case-switch semantics while they remain deferred.

## Model and input generation

Use a small independently expressed model or explicit expected histories.
Sharing a wire decoder may be reasonable; sharing the transition or expiry algorithm under test can hide the same defect in both implementations.
Justify the independence of the expected result for the behavior being checked.

Generate valid interacting operations so histories reach meaningful states.
Reuse created transaction identities and blockhashes where the contract permits it.
Exercise duplicates, absent identifiers, boundary values, and malformed inputs as separate relevant classes.
Include calls before and after a failure, not only the injection point.
Vary initial state and history without forcing every generated run into the same tiny state.
Inspect skipped operations and rejected inputs before increasing generation volume.

Keep sequence length, payload sizes, concurrent work, and test execution bounded.
Use the existing framework when suitable.
`proptest` or `proptest-state-machine` may help; neither is an automatic project dependency.
The reviewed `proptest-state-machine` 0.9.0 documentation supports sequential histories only.
Recheck the selected version before using it for another execution model.

## Time, reproducibility, and concurrency

Control relevant time, state advancement, dependency responses, and operation ordering in a deterministic harness.
Do not use arbitrary sleeps as the only proof that a transition occurred.
A random seed does not control OS scheduling, live read-through data, or a provider's changing state.
Record the initial state, configuration, relevant versions, operation sequence, and any schedule needed to reproduce a failure.
Retain an explicit minimized history as well as a seed; generator changes can change seed behavior.
Keep reproduction artifacts distinct from any promised product Chaincase/replay format.

For concurrent requests, identify allowed orderings before asserting one result.
Use controlled interleavings or a suitable separate concurrency harness when races are the concern.
A passing serialized model test is not evidence that all concurrent executions are correct.
Do not impose a universal ordering or linearizability contract that the project has not accepted.

## Relevant checks

Choose only checks that can demonstrate the changed contract, for example:

- resubmission of identical signed bytes retains transaction identity, while business correlation remains a separate observation;
- status polling and expiry remain consistent with the authoritative mode and declared Faults;
- intentionally withheld boundary responses do not erase already recorded acceptance decisions;
- forwarding rejects disallowed or unclassified methods, including relevant batch elements, without contacting a write-capable public upstream;
- Case changes preserve the persistent Lab and Journal under the accepted lifecycle contract;
- continued requests after rejection, cancellation, or dependency failure observe the promised state.

Use a fake upstream that records received calls when verifying forwarding effects.
If using an execution backend as a comparison, restrict the comparison to semantics it can actually demonstrate.
Local execution is not an oracle for production consensus or every provider's behavior.

## Failure triage

Find the first meaningful divergence and verify the model's assumptions.
Minimize the history while preserving its trigger and failure.
For an implementation defect, retain a readable regression check with explicit expected behavior.
Do not convert an ambiguous model disagreement into a confirmed product defect.
Do not continue searching merely because the scoped checks found no defects.
