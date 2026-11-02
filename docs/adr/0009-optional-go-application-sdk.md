# Optional Go-first application SDK

Status: accepted but deferred while the CLI and no-SDK RPC workflow are defined.

Ronchi will provide an optional first-party application SDK, beginning with Go. The SDK wraps the application's existing blockchain RPC client once; subsequent calls retain the client's normal method shape while the wrapper records call-site and correlation evidence. It also emits structured Application Checkpoints for decisions or state transitions between RPC calls. Per-call closure instrumentation is not the primary UX.

The standalone RPC Simulator remains the primary product interface. Fault injection, RPC evidence, and simulator evidence work without the SDK. SDK evidence describes application behavior and is correlated with, but never substituted for, independently observed RPC and simulator facts.

The SDK emits structured operation and Checkpoint evidence. It does not automatically forward the application's complete log stream. Raw logs may be attached to a Run only as an explicit bounded selection.

## Considered Options

- OpenTelemetry-only instrumentation was rejected as the primary product interface because Ronchi needs a focused developer experience and explicit blockchain/application correlation semantics.
- A mandatory SDK was rejected because it would weaken language independence and make basic fault investigation require application changes.
- No application instrumentation was rejected because a proxy cannot identify code locations, business intent, or internal retry and reconciliation decisions.

## Consequences

- Go is the first supported SDK language; later SDKs may implement the same local evidence protocol.
- Supported RPC clients receive first-party wrappers that preserve their normal calling style.
- The SDK must preserve stable correlation identifiers across wrapped operations, Checkpoints, RPC events, and simulator evidence.
- Source location is useful evidence, but semantic operation names and business identifiers remain explicit application input.
- Structured evidence is the default; raw application logs are opt-in and bounded.
- Exact API design and transport remain open decisions.
