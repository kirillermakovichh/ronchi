# Explanation-first outcomes

Application Checks and business Verdict evaluation were removed by user decision. References to them below are historical and do not authorize reimplementation. See [current scope](../product/overview.md) and [task 36](../../tasks/todo.md#36-remove-application-checks).

Status: accepted. The earlier paired-run default is superseded by [ADR-0008](0008-chaos-run-first-optional-control.md).

Ronchi will treat causally ordered Journal Evidence, deterministic Findings, and the Timeline as the primary product output, not reduce an Investigation to a conventional test result. A Finding summarizes a known causal pattern and links directly to its supporting evidence. An automated `pass`, `fail`, or `inconclusive` Verdict is produced only when explicit Checks provide enough evidence. This makes the tool useful for interactive investigation, avoids false certainty about unobserved application internals, and requires missing evidence to remain visible.

The primary workflow is a persistent Lab with an armed Case and an independently running System Under Test, as clarified by [ADR-0015](0015-persistent-lab-first.md) and [ADR-0016](0016-continuous-lab-journal.md). Investigations are filtered views over the Lab Journal. An optional bounded Chaos or Control Run may support automation or comparison when the developer or Coding Agent can establish Comparability.

## Considered Options

- A test-runner-first model was rejected because a binary result hides the RPC, simulator, injection, and application transitions developers need to diagnose resilience defects.
- Automatic inference of business correctness was rejected because a proxy cannot reliably identify application intent or uninstrumented internal state.

## Consequences

- Timeline correlation and evidence completeness are core product capabilities rather than optional logging features.
- Findings must be deterministic, evidence-linked, and limited to observable causal claims.
- Findings and Verdicts remain distinct: explaining what happened does not by itself establish business correctness.
- Control-versus-Chaos comparison is optional and must not be implied when only a Chaos Run exists.
- Unverified Comparability is an evidence gap, not permission to assume equivalent application state.
- A Run can be useful without a Verdict.
- The product must distinguish `inconclusive` from both `pass` and `fail`.
- "Full observability" means complete visibility across supported Lab boundaries, not magical access to uninstrumented application internals.
- Every result declares its Observation Boundary and Evidence Gaps; unavailable observations constrain Findings and Verdicts instead of invalidating the entire Run.
- The Timeline records RPC receipt, simulator decision, fault application, and RPC Boundary Result as distinct facts.
- A later RPC request is observable without an SDK; the application's internal decision is observable only through client-side SDK evidence, an Application Checkpoint, or a configured Check.
- Every Observed Application Action identifies its evidence source. Ronchi never converts an RPC Boundary Result into an assumed Client Outcome.
