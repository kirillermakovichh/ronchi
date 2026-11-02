# Sources and adaptation

Reviewed on 2026-11-02.
This is a project adaptation, not an unmodified upstream installation.

- [architecture-foundation](https://github.com/majiayu000/spellbook/blob/b20e693fdbdba0598f4538537f5dc48c4754728c/skills/architecture-foundation/SKILL.md) and its [Rust reference](https://github.com/majiayu000/spellbook/blob/b20e693fdbdba0598f4538537f5dc48c4754728c/skills/architecture-foundation/references/rust-architecture-patterns.md from majiayu000/spellbook, revision `b20e693fdbdba0598f4538537f5dc48c4754728c`. MIT notice preserved in [LICENSE](../LICENSE).
- Accepted Ronchi architecture remains defined by the repository's ADRs, not by the external skill.

The adaptation keeps explicit ownership, effects, lifecycle, backpressure, errors, and contract-level verification.
It narrows the scope to Ronchi's long-running developer tool.
It omits UI/ECS/rendering catalogs, generic plugin architecture, mandatory layer/crate layouts, a second specification template, and automatic roadmap generation.
Use the existing project decision and planning workflows instead.
