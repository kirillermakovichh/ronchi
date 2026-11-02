# Local MCP with rmcp

Load this reference for Rust MCP implementation, protocol behavior, cancellation, or client compatibility.
Use [ronchi-cli-design](../../ronchi-cli-design/SKILL.md) for shared operation semantics, evidence projections, and token-efficient output.
This reference does not choose final MCP tool names, transport packaging, or a daemon lifecycle.
Reviewed sources and SDK revision are recorded in [sources.md](sources.md).

## Version and SDK contract

Inspect the repository's selected `rmcp` version, lockfile, compiler requirement, and enabled features before using an API.
Use that version's documentation and official examples instead of copying an older server generator.
The reviewed upstream supports both modern `2026-07-28` and legacy protocol behavior; this does not commit Ronchi to supporting every version.
Match version/capability handling to the versions Ronchi actually supports.
Legacy initialization and modern per-request metadata/discovery differ; do not hard-code one lifecycle as universal.
Delegate wire framing and protocol handling to the SDK where available.
Do not add nightly Rust, enable all features, or upgrade dependencies solely because a skill template does so.

## Shared core and state

Use the same Lab operations and query models as the CLI and dashboard.
Keep MCP result envelopes distinct from CLI output envelopes.
Pass explicit Lab and evidence identifiers when needed; do not rely on a connection as the implicit identity of the active Lab.
Keep transport lifetime, a tool invocation, and the persistent Lab lifetime separate.
An attaching MCP client disconnect must follow the documented ownership contract rather than accidentally stopping unrelated Lab work.
If the MCP process owns a Lab, define its shutdown and persistence behavior explicitly.

Expose only accepted product operations: Lab/Case control and bounded evidence inspection.
Do not add arbitrary shell execution or backend supervision.
Add tools, resources, prompts, subscriptions, or newer protocol extensions only when the workflow needs them and clients can use them.
No model, account registration, cloud service, or authentication workflow is required merely to expose local MCP.

## Schemas and results

Derive request/result schemas from maintained Rust types where practical.
Verify generated schemas against the selected protocol and SDK version.
Describe a tool's purpose, accepted inputs, relevant effects, and recovery information clearly enough to use without reading the whole manual.
Avoid duplicate tools that differ only in output verbosity when a bounded projection suffices.
Declare only capabilities the server actually implements.

Use structured results compatible with the target clients.
Respect the applicable `outputSchema`/`structuredContent` rules; they differ across protocol versions.
Where a client/version needs text compatibility content, keep it bounded and consistent with structured data.
Do not unconditionally duplicate an entire evidence payload as both prose and JSON.
Apply pagination, filters, and field projection through shared queries before loading unrequested payloads.
Preserve correlation and Evidence Gaps required to interpret an Evidence Slice.

Keep protocol errors distinct from failures while executing a valid tool call.
Use the SDK's protocol-error mechanism for malformed requests or unsupported tools as appropriate to the spec.
Use tool-result error signaling such as `isError` for operation failures where the protocol requires it.
Do not treat protocol errors as invisible to all clients or mark operation failures as successful empty results.
An injected RPC error recorded by Ronchi is not automatically an MCP tool-execution error.
Annotations such as read-only, destructive, and idempotent hints must describe actual effects; they do not enforce access or retry safety.

## Transport and lifecycle

For stdio, stdout contains valid MCP messages only, including during startup and failures.
Write diagnostics to stderr.
Do not print CLI banners, readiness prose, or standalone CLI JSON into the protocol channel.
Handle input EOF and transport shutdown through the selected ownership contract.
Do not require an infinite tool invocation merely to keep an independently managed Lab running.
Return or expose readiness separately from the Lab's eventual end when the workflow requires it.

If using Streamable HTTP, use the applicable SDK binding and the project's local network boundaries.
Do not add deprecated standalone SSE transport or remote hosting as a default from an old template.
Do not confuse SSE framing within Streamable HTTP with a separate legacy transport.
Verify the selected binding rather than implementing a competing JSON-RPC framing layer.

Cancellation targets an in-flight request, not an automatic rollback of completed side effects.
Use the cancellation mechanism for the supported protocol version and transport.
For the reviewed modern binding, stdio uses a cancellation notification and Streamable HTTP uses response-stream closure.
Propagate cancellation to cancellable query/work tasks while respecting already accepted Lab mutations and Journal writes.
If a mutation outcome is unknown after timeout or disconnect, offer state inspection rather than a blind retry.
Do not advertise cancellation of already-started blocking work as guaranteed.

## Focused verification

Use a real SDK client/transport boundary for affected protocol contracts.
Check discovery or legacy initialization as applicable, capability declarations, tool listing/calls, schemas, and relevant errors.
When changed, exercise cancellation, EOF, client reconnect, pagination, and mutation completion uncertainty.
For stdio changes, use a process-level check to detect stray stdout and incorrect process exit behavior.
Use isolated Lab/workspace fixtures and fake or local endpoints.
The MCP Inspector is an optional development check; it is not a runtime dependency or permission to modify a user's client configuration.
Report actual client, SDK, and protocol versions tested; do not infer broad compatibility from compilation alone.
