---
name: ronchi-docs-maintenance
description: Maintain Ronchi development docs after decisions change, or audit and consolidate duplicated, conflicting, or stale documentation. Use for repository knowledge organization; preserve agreed intent, rationale, and open questions.
license: MIT
---

# Ronchi documentation maintenance

Keep durable development knowledge easy for a future agent to find and interpret.
Use this skill to maintain or consolidate docs, not to start a new product-design session.
Follow the user's requested scope: an audit produces findings; an editing request permits the corresponding edits.

## Existing routes

Read repository-root `AGENTS.md` and the relevant parts of `docs/README.md` before selecting documents.
Use the existing layout:

| Knowledge | Home |
| --- | --- |
| Agent working rules and task routing | `AGENTS.md` |
| Domain vocabulary | `CONTEXT.md`; keep implementation details elsewhere |
| Documentation navigation | `docs/README.md` |
| Product intent and developer experience | `docs/product/overview.md` and `docs/product/workflows.md`; use the index for Cases, the reference relayer, and branding |
| Durable architectural choices and rationale | `docs/adr/` |
| Unresolved or deferred choices | `docs/open-questions.md` |
| External evidence and its limitations | `docs/research/` |
| Active implementation work | The existing plan or task tracker; defaults are `tasks/plan.md` and `tasks/todo.md` |

Inspect additional local documentation rules when the task reaches their directory.
Keep active plans and temporary investigation notes distinct from maintained knowledge. Link active work when needed; keep retired work outside the normal reading route.

## Shared rules

- Give each durable claim one authoritative definition. Update that source and link from other documents. A short navigation summary and an ADR's historical rationale serve different purposes; topic overlap alone is not redundancy.
- Distinguish accepted intent, proposals, implemented behavior, historical evidence, and open questions. An unimplemented decision is not stale merely because code does not exist yet.
- Use code, configuration, tests, and observations to verify current implementation. They do not silently overrule accepted product decisions. Report disagreements and seek a decision only when the evidence cannot resolve the intended behavior.
- Preserve ADR paths and decision history. Mark a replaced decision as superseded and link its replacement. Do not rewrite past observations as current facts.
- Create a document only when it has a durable reader need and a reason to be maintained. Prefer an existing topic or a link. Keep temporary reports out of the canonical index unless they become durable references.
- Use concise titles and task-oriented index descriptions so agents can select relevant documents. Add status or reading guidance where it resolves ambiguity; avoid boilerplate sections in every file.
- For generated documentation, change its maintained source and regenerate when applicable.

## Routine maintenance

Identify the decisions or facts affected by the requested change. Read their authoritative sources and search for dependent references within the repository. Update relevant current documents and navigation without expanding into unrelated cleanup.

When code, tests, or existing docs already convey everything future agents need, leave the docs unchanged and explain that briefly.

## Consolidation

1. Inventory the scoped documents. Identify each document's purpose, authority, status, and overlap. Retain uncertain material while investigating; age and length alone do not justify removal.
2. Propose an old-path to action to destination map for bulk moves, merges, or deletions. Give reasons and obtain agreement before executing that structural plan. Reuse approval already given for the same plan.
3. Before rewriting, record the load-bearing content: decisions, rationale, constraints, conditions, consequences, evidence, important references, and open questions. Use working notes or the agreed plan rather than creating another permanent report by default.
4. Map each item to its destination, authoritative surviving copy, historical location, or explicit reason for removal. A matching heading does not establish that a section is duplicated. Preserve qualifiers and surrounding context when combining claims.
5. Apply only the agreed changes. Preserve retired historical material at an agreed archive location when appropriate; keep ADR history at its stable paths. Do not create an archive hierarchy until there is material to move.
6. Compare the result with the pre-rewrite inventory. Check that retained content supports the same important decisions and that proposed removals are accounted for. Update affected inbound links, relative paths, section references, and the normal index route.

The inventory checks preservation of identified knowledge; it does not prove the inventory was exhaustive. Report unresolved gaps.
No reduction target or minimum number of changes is required. Keeping every document is a valid outcome.

## Verification and report

Use available checks for affected links and references. Inspect context when a claim's meaning depends on its heading or condition; a string match alone is insufficient. State any automated or reader-level verification not performed.

Report the changed locations and reasons, any material removals and their disposition, checks performed, and unresolved contradictions. Keep the response proportional to the task and follow `AGENTS.md`.

## Design references

- [documentation-maintenance](https://github.com/andiedie/skills/blob/main/skills/documentation-maintenance/SKILL.md): agent-oriented knowledge placement and semantic preservation. The instructions above are written for Ronchi's agreed workflow.
- [docs-cleaner](https://github.com/daymade/claude-code-skills/blob/main/daymade-docs/docs-cleaner/SKILL.md): scoped consolidation and evidence gathered before a rewrite. Its MIT notice is preserved in [LICENSE](LICENSE).
