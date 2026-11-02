# Resources and verification

Project adaptation of selected Apollo performance, linting, and testing guidance.
See [sources.md](sources.md) for provenance and primary references.

## Resource decisions

Use ADR-0019 for the accepted resource model.
Preserve transparent request/response bytes and parse only what routing or the active Case needs.
Stream unmodified large bodies with backpressure.
If a primitive needs buffering, bound it and specify the behavior when its limit is exceeded.
Do not retain the whole Continuous Lab Journal in memory.
Use bounded queries and batched writes through the dedicated writer.
Check how batching, commit acknowledgment, and recovery interact before changing persistence settings.

Consider total retained memory across concurrency, queues, shared payloads, caches, and query results.
An item-count limit is insufficient if item sizes are unbounded.
Avoid repeated payload clones and intermediate collections without a consumer need.
Reusing connection pools or shared immutable buffers can be useful when their lifetime and limits are clear.
Do not add allocator replacements, SIMD parsers, lock-free structures, or another proxy framework without evidence of a relevant bottleneck.

Explain a concrete risk when reasoning without measurements.
For example, buffering N responses of size B retains approximately N times B payload bytes before other overhead.
Do not present that reasoning as measured throughput or RSS.
Do not invent numerical resource budgets while their definition remains deferred.

## When performance measurement is in scope

Use an optimized build and representative workload.
Record the toolchain, build profile, workload, environment, and relevant variability.
Choose metrics for the claim: latency, throughput, peak/idle memory, CPU, disk bytes or writes, or binary size.
Measure core and Surfpool mode separately.
Use profiles to locate the bottleneck before proposing a broad rewrite.
Use microbenchmarks for local mechanisms; do not treat them as proof of whole-Lab behavior.
Do not impose a universal improvement percentage or start a benchmark project for an unrelated documentation change.

## Toolchain and lints

Inspect the declared toolchain and `rust-version` before using newer language features.
`#[expect(lint)]` requires Rust 1.81 or later; respect the actual supported compiler.
Use a reasoned local lint exception when the alternative would be incorrect or less clear.
Do not suppress warnings merely to make checks pass.

Use the repository's existing check commands and supported feature sets.
When applicable, use `cargo fmt --all -- --check`, focused tests, and Clippy on the affected targets.
Use `--locked` when a lockfile exists and its preservation is required.
Use `--all-features` only if that combination is supported; also check the shipped configuration when it differs.
Do not run `cargo update` or change a pinned toolchain merely because a lint command fails.
Investigate the actual failure and keep dependency updates within task scope.

## Meaningful tests

Verify the observable behavior or invariant at the smallest useful interface.
For relevant changes, cover malformed input, injected versus internal failures, write-firewall behavior, capacity, cancellation, or writer shutdown.
Keep related assertions together when they describe one behavior; one assertion per test is not a universal rule.
Use parameterized cases for meaningful input variation.
Use snapshots when stable, human-reviewable output is the contract; normalize only incidental variability.
Keep expected behavior visible even when fixtures or setup are shared.
Use isolated paths and fake upstreams rather than real blockchain writes.
Do not require every test layer, a coverage quota, or a new testing dependency for each change.
