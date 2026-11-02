# Case-driven fault model

Ronchi uses named, reproducible, protocol-aware Cases as its mandatory primary fault-selection model. Purely random fault injection was rejected as the core because it weakens reproducibility, causal attribution, and RPC-semantic coherence. A future constrained Exploration mode may select from safe Fault Primitives, record its decisions, and promote useful discoveries into Cases, but it is deferred and does not shape the initial workflow.

Only one Case is active in a Lab at a time. Switching Cases does not restart the Lab, System Under Test, or Continuous Lab Journal. Multiple coordinated Faults must be expressed as one composite Case so their relationship and Findings remain explicit.

An active Case is a continuing fault policy and may inject its declared Faults across multiple matching interactions until disarmed, replaced, or the Lab stops. Cases are not one-shot test injections; exact repeat, burst, sequencing, and recovery semantics are deferred.
