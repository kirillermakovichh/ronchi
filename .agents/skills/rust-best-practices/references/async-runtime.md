# Tokio runtime and resource management

Adapted from selected `rust-async-patterns` guidance and checked against Tokio documentation.
The examples in source skills are not production templates.
See [sources.md](sources.md) for reviewed revisions and license notices.

## Runtime and blocking work

Use the application's existing Tokio runtime rather than creating nested runtimes.
Enable dependency features required by actual code; do not copy `features = ["full"]` as a default.
Feature selection alone does not prove a runtime memory improvement.
Choose runtime thread configuration from actual workload needs, not a rule that single-threaded is always cheaper overall.

Keep synchronous SQLite, long CPU work, and blocking I/O off async worker threads.
Follow ADR-0019's dedicated batched rusqlite writer instead of adopting SQLx or a task per write.
For other finite blocking work, use `spawn_blocking` when appropriate.
Bound admission separately for large CPU workloads.
Once started, a `spawn_blocking` closure cannot normally be aborted; design cooperative exit or bounded completion where required.

## Tasks and capacity

Give each long-lived task an owner and a completion path.
Track task results with a `JoinSet`, retained handles, or another appropriate lifecycle mechanism.
Inspect task panics and returned errors according to the module's failure policy.
Do not turn failed children into a successful aggregate by merely logging them.
Detached tasks are appropriate only when their lifetime and failure policy are deliberate.

Limit concurrent work before creating an unbounded population of waiting tasks.
A semaphore inside every spawned task can limit active work while still allowing unbounded task accumulation.
For finite concurrent streams, use bounded concurrency and process results incrementally when possible.
Do not collect an unbounded result stream into a `Vec`.
Acquire resource permits before consumption and release them when the protected resource is actually returned.
Avoid asynchronous cleanup in `Drop` as the only mechanism required for correctness.

## Channels and evidence

Use bounded channels when queued work can grow with input traffic.
Bound queued bytes or payload sizes as well as message count.
Define capacity behavior and channel-closure errors.
Use `watch` for replaceable latest state, not complete history.
Handle `broadcast` lag explicitly; it is not a durable Journal transport by itself.
Do not silently lose evidence because a diagnostic subscriber or a writer is slow.
Distinguish queue admission, database commit, and any promised durability acknowledgment.

## Locks

Prefer state ownership and narrow critical sections where they simplify concurrency.
A synchronous mutex can protect short data-only operations that neither block for long nor cross an await.
Never hold a blocking mutex guard across `.await` in the async path.
A Tokio mutex is designed to allow its guard across `.await`, but doing so can serialize work and requires a deliberate reason.
Do not replace every mutex with an async mutex or RwLock without examining access patterns.
Avoid holding shared state locks during unrelated upstream network waits or Journal queries.

## Cancellation and partial effects

For each cancellable operation, identify effects already performed when its future is dropped.
Dropping a request future does not prove that an upstream transaction was never submitted.
Check cancellation safety for operations used in `select!`, especially partial reads, writes, and reservations.
Keep accepted state transitions and evidence consistent when a caller disconnects.
Do not automatically retry a timed-out mutation with an unknown outcome.

## Shutdown

Detect the shutdown cause, notify owned tasks, and wait for completion.
Stop new admission before shutting down consumers required by in-flight producers.
After producers stop, drain and commit accepted Journal work according to its persistence contract.
Use explicit completion signals or joined tasks; fixed sleeps are not completion evidence.
If a shutdown deadline expires, report what remains incomplete and follow the chosen force-stop policy.
Await aborted async tasks when their termination matters.
Close and wait for managed Surfpool according to its ownership contract.
Do not treat token cancellation or a dropped handle as proof that processes exited or files were flushed.

## Focused verification

When affected, exercise saturation, receiver closure, task failure, cancellation during effects, or shutdown during writes.
Use controlled synchronization and Tokio test time where appropriate rather than relying on arbitrary sleeps.
Do not use a tracing span entered across await; instrument the future or use an async-aware instrumentation mechanism.
