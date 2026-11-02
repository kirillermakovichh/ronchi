# Persistent Lab first

Ronchi's primary product is a persistent local Lab connected to an independently running backend, not a test runner. A developer or Coding Agent arms a Case, drives or uses the backend through its normal interfaces, and investigates the fault, Timeline, Findings, and Evidence Gaps through the Continuous Lab Journal defined by ADR-0016. Ronchi does not own the backend lifecycle or require a terminal `pass` or `fail`; bounded Runs, tests, CI wrappers, Checks, and Control comparisons are optional workflows over the same core.
