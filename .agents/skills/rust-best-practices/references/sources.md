# Sources and adaptation

Reviewed on 2026-11-02.
The project skill has been substantially rewritten and consolidated.
The original nine chapter files are replaced by task-specific references; the handbook remains available upstream.

## Reviewed material

- Existing project `rust-best-practices/SKILL.md` and `references/chapter_01.md` through `chapter_09.md`: ownership, language idioms, lints, performance, errors, tests, dispatch, typestate, documentation, and pointer semantics.
- [Apollo GraphQL Rust Best Practices](https://github.com/apollographql/rust-best-practices/blob/0727a45244caa460be11d2c446e9b79a518bd8c4/README.md), reviewed revision `0727a45244caa460be11d2c446e9b79a518bd8c4`. The handbook uses Apache-2.0. Preserve that attribution for the adapted language/performance guidance. The full license is in [LICENSES/Apache-2.0.txt](../LICENSES/Apache-2.0.txt). The reviewed tree has no separate NOTICE file. This reviewed revision does not assert that the former local chapter copies were byte-identical to upstream.
- [rust-async-patterns](https://github.com/wshobson/agents/blob/7e2fb559cf4b824b27f1fec49fe88f7511066d26/plugins/systems-programming/skills/rust-async-patterns/SKILL.md) and [details](https://github.com/wshobson/agents/blob/7e2fb559cf4b824b27f1fec49fe88f7511066d26/plugins/systems-programming/skills/rust-async-patterns/references/details.md) from wshobson/agents, revision `7e2fb559cf4b824b27f1fec49fe88f7511066d26`. Selected ideas cover bounded concurrency, channels, task ownership, and cancellation. MIT notice preserved in [LICENSES/MIT-rust-async-patterns.txt](../LICENSES/MIT-rust-async-patterns.txt).
- [oakoss/rust](https://github.com/oakoss/agent-skills/blob/85e3a3919d9e0ec7f7302a5143ec4b3e66f5f6ad/skills/rust/SKILL.md) and its async, CLI, and release references, reviewed revision `85e3a3919d9e0ec7f7302a5143ec4b3e66f5f6ad`. Its frontmatter declares MIT, but the reviewed repository tree contains no license text or copyright notice. No oakoss code examples, configuration templates, or prose are vendored. Related implementation and release notes are locally written against the primary documentation below.

## Deliberate changes

Keep one short entrypoint and load topic references only when needed.
Replace blanket iteration/boxing/dispatch advice with ownership and workload trade-offs.
Remove fixed Copy-size thresholds, one-assertion quotas, and automatic toolchain or dependency updates.
Correct reference Send/Sync semantics and qualify compiler-version-specific advice.
Replace fixed-delay shutdown with owned completion and explicit persistence ordering.
Do not copy unbounded task spawning, success-shaped partial failures, or asynchronous Drop cleanup as production patterns.
Do not add SQLx, Ratatui, Docker, global caches, mandatory release tools, or automatic agent delegation.
The project-specific stack comes from ADR-0019, not from these external sources.

## Primary references

- [Rust Send](https://doc.rust-lang.org/std/marker/trait.Send.html) and [Sync](https://doc.rust-lang.org/std/marker/trait.Sync.html): thread transfer and shared-reference guarantees.
- [Rust trait objects](https://doc.rust-lang.org/reference/types/trait-object.html): dynamic dispatch and dyn compatibility.
- [Tokio graceful shutdown](https://tokio.rs/tokio/topics/shutdown): notification and waiting for completion.
- [Tokio Mutex](https://docs.rs/tokio/latest/tokio/sync/struct.Mutex.html): synchronous versus asynchronous locks.
- [Tokio spawn_blocking](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html): blocking work and abort limitations.
- [Tokio select](https://docs.rs/tokio/latest/tokio/macro.select.html): cancellation safety.
- [clap Command](https://docs.rs/clap/latest/clap/struct.Command.html): parsing, help, and errors.
- [Cargo features](https://doc.rust-lang.org/cargo/reference/features.html), [rust-version](https://doc.rust-lang.org/cargo/reference/rust-version.html), and [Clippy usage](https://doc.rust-lang.org/clippy/usage.html): supported build and lint configurations.
- [Rust 1.81](https://blog.rust-lang.org/2024/09/05/Rust-1.81.0/): stabilization of lint expectations.
- [dist (cargo-dist)](https://axodotdev.github.io/cargo-dist/book/introduction.html), [release-plz](https://release-plz.dev/), and [cargo-deny](https://embarkstudios.github.io/cargo-deny/): current release-tool documentation. Check the selected version before generating configuration.

The license expression covers differently sourced portions, not a claim that the upstream handbook is MIT-licensed.

## MCP implementation reference

The locally written `mcp.md` was reviewed against these sources on 2026-11-02:

- Official [Rust SDK README](https://github.com/modelcontextprotocol/rust-sdk/blob/fe458714e77cbae67498ac9f4da8005a5648fd0d/README.md) and [workspace manifest](https://github.com/modelcontextprotocol/rust-sdk/blob/fe458714e77cbae67498ac9f4da8005a5648fd0d/Cargo.toml), revision `fe458714e77cbae67498ac9f4da8005a5648fd0d`. The reviewed main branch is not a dependency pin or proof of the published version selected by Ronchi. No SDK code templates are vendored.
- MCP `2026-07-28` [versioning and compatibility](https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning), [tools](https://modelcontextprotocol.io/specification/2026-07-28/server/tools), [transport overview](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports), [stdio](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/stdio), and [cancellation](https://modelcontextprotocol.io/specification/2026-07-28/basic/patterns/cancellation).

Check the selected SDK and supported protocol/client versions before applying version-sensitive details.
The reference does not inherit old generator dependency versions, mandatory nightly, fixed transport layouts, authentication setup, or extra prompts/resources.
It adds implementation guidance without choosing final Ronchi tool names, transport packaging, or protocol support promises.
