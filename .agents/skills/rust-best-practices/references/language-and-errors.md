# Language, interfaces, and errors

Project adaptation of selected Apollo handbook guidance; rewritten and qualified for Ronchi.
See [sources.md](sources.md) for provenance and licenses.

## Ownership and allocation

Accept borrowed data when the operation only needs access during the call.
Use owned values when the callee must retain, transfer, or independently mutate them.
Do not replace a natural ownership transfer with borrowing followed by a clone.
Choose `iter`, `iter_mut`, or `into_iter` according to the required access and ownership.
`into_iter` can move owned elements without cloning them; `Copy` does not make `iter` universally faster.
Use lazy fallback construction when the fallback does meaningful work or allocates.
Introduce `Cow` only when a real borrowed-or-owned path benefits from it.
Do not use fixed byte thresholds as universal rules for passing values or deriving `Copy`.

An array is an inline fixed-size value; its enclosing allocation determines where its storage lives.
Use `Box` for recursive types, deliberate size reduction, ownership, or another concrete allocation need.
Boxing a large variant can reduce enum size while adding indirection and allocation.
Check the relevant trade-off instead of banning internal boxes or prescribing them at every interface.
Use `Arc` for shared ownership across tasks when required; it does not make arbitrary inner mutation safe.
Use `Weak` when a shared-ownership cycle would otherwise retain resources.
Avoid unnecessary `Arc`, locking, or payload copies in a request's lifetime.

## Types and interfaces

Use enums to represent known alternatives and preserve exhaustive handling where useful.
Use newtypes when they prevent meaningful identifier or state confusion.
Use typestate only when it makes important invalid transitions unrepresentable without obscuring runtime behavior.
Prefer a clear interface over a new abstraction that only renames calls.
Extract shared knowledge or a useful named operation; repeated text alone does not justify a helper.

Generics and concrete types enable static dispatch and may allow inlining.
Trait objects support runtime selection and type erasure beyond heterogeneous collections.
Choose between them using API needs, binary size, compilation cost, and the relevant runtime path.
Do not assume either choice is always smaller or faster.
Check dyn compatibility before using a trait object.
For async interfaces, choose `Send` requirements and dispatch deliberately; do not add `async-trait` automatically.

## Send and Sync

`Send` means a value can be transferred across thread boundaries.
`Sync` means shared references can be transferred safely.
For references, `&T` is `Send` when `T` is `Sync`; `&mut T` is `Send` when `T` is `Send`.
`Arc<T>` does not make a non-thread-safe `T` safe to share.
Let compiler checks enforce these contracts rather than adding unsafe implementations to silence errors.

## Errors and state

Return `Result` for expected fallible operations and `Option` for normal absence.
Keep distinctions callers need: timeout, rejection, unsupported method, internal error, and unknown outcome as applicable.
Use typed errors, optionally derived with `thiserror`, at interfaces that require classification.
Use `anyhow` where application-level aggregation is useful and callers do not require a fixed error taxonomy.
Preserve useful source errors while redacting secrets from displayed context.
Convert internal errors into CLI, MCP, or HTTP responses at the appropriate interface.

Use `?` for propagation when it preserves the intended classification and context.
Do not discard a failure with a success-shaped default unless the contract permits that fallback.
Handle malformed RPC input, closed channels, disk failure, and process errors without incidental panics.
Reserve assertions and `expect` for documented internal invariants, not fallible external input.
An unreachable branch must follow from an actual invariant; it is not a placeholder for an unsupported case.
Separate injected failures from Ronchi's own operational failures.
Do not map an uncertain transaction outcome to a confirmed failure or business result.

## Documentation

Document observable behavior, important errors, cancellation, and resource ownership where callers need them.
Use comments for rationale or non-obvious invariants.
Add doc tests when they demonstrate a useful public contract.
Use the project's existing issue and lint conventions; do not introduce a global missing-docs gate or issue requirement for every comment.
