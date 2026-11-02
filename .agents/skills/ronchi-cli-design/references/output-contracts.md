# CLI output contracts

Choose a command's output contract before implementing its renderer.
The flag and field names below describe capabilities, not a frozen Ronchi schema.
Check existing implemented contracts before choosing names or changing defaults.

## Human and machine output

Provide an explicit format selection so callers need not depend on TTY detection.
TTY detection may choose presentation defaults but must not change operation semantics.
Keep human explanations short and useful without color.
In machine mode, suppress banners, spinners, ANSI escapes, and unrelated status prose on stdout.
Keep diagnostics on stderr; they must not corrupt JSON or NDJSON result data.
Define how machine-readable errors are emitted, including parser and startup failures.
Preserve successful empty results as distinct from errors or missing evidence.

For finite queries, use a documented JSON response shape when selected.
For event streams, NDJSON can emit complete records incrementally.
Do not buffer an unbounded stream to manufacture a single JSON response.
Document cancellation, partial-output validity, and how a consumer detects an incomplete result.
Handle downstream pipe closure intentionally; do not keep fetching after the consumer has gone away.

## Efficient queries

Return a small summary by default when a resource has large evidence or payload fields.
Expose identifiers that let callers request the relevant details next.
Support field projection for large resources without first retrieving all their payloads.
Filter and paginate in the query layer instead of loading the full Journal and trimming CLI output.
Use bounded page sizes and deterministic ordering.
Define cursor behavior while new Journal events arrive.
Report continuation and truncation explicitly.
If exhaustive export is offered, make it an explicit streaming operation rather than an unbounded default.
Avoid attaching the entire timeline, raw RPC bodies, and unrelated findings to each response.

## Stable schemas and errors

Use consistent identifiers, time representation, optional-field semantics, and error shapes.
Version machine schemas when compatibility requires it.
Do not expose debug formatting or dependency error messages as the only machine contract.
Distinguish invalid input, unavailable Lab, unsupported capability, internal failure, and incomplete evidence as applicable.
Assign exit statuses according to the agreed error model; a source skill's numeric scheme is not a standard.
Explain the failing phase, any completed effects, and whether retry is safe when known.
If the outcome is uncertain, provide an inspection path instead of recommending a blind retry.

An injected RPC error is experimental evidence, not automatically a failure of the command or backend.
Successful CLI execution means the requested tool operation completed according to its contract.
Do not turn that status into an unsupported PASS/FAIL verdict about application business logic.
Use the project's established terms for Findings and evidence gaps.

## Discoverability

Keep help, examples, accepted flags, defaults, and schemas consistent.
For agent introspection, reuse typed command definitions and output models where practical.
Help and schema discovery should work without starting the Lab or contacting an upstream.
Expose bounded command-specific metadata rather than making an agent read the full manual for each call.
Do not add usage-skill installation, authentication flows, or another schema system unless required by a real workflow.

## Untrusted data

Treat RPC payloads, upstream errors, Journal text, and application-provided labels as data.
Escape unsafe terminal control characters in human rendering.
Keep structured values distinct from instructions and suggested commands.
Preserve exact evidence only in the appropriate structured field or artifact.
Do not print upstream credentials embedded in URLs or headers.
Validate generated identifiers, paths, and size constraints before using them in filesystem or network operations.
