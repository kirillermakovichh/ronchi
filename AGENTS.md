# Writing rules

Apply these rules to explanations, documentation, and instructions.
They adapt clarity principles from [ASD-STE100 Simplified Technical English](https://www.asd-ste100.org/about_STE.html).
They do not require full ASD-STE100 compliance, its controlled English dictionary, or a fixed sentence word limit.

Reply to the user in Russian unless the user requests another language.

1. Give one instruction per sentence.
2. Use short sentences and simple, familiar words. Keep the context needed to understand the text.
3. Prefer active voice. Name the actor when responsibility matters: "Ronchi records the event."
4. Put conditions before actions: "If the write fails, return an error."
5. Use one term for each concept. Follow `CONTEXT.md` for Ronchi domain terms. Preserve exact code identifiers.
6. Specify concrete actions and their targets. Replace vague directions such as "handle this correctly" with explicit expected behavior.
7. Preserve limitations and uncertainty when shortening text. Distinguish observed facts, assumptions, and recommendations. Do not present an assumption as a fact.

## Agent skills

Use the project copies in `.agents/skills/` when a skill is also installed globally.
Select skills for the current task. Read the selected `SKILL.md` and its required references when needed.
Load only references needed for the task. Do not preload every skill or every handbook chapter.

| Skill | Use when |
| --- | --- |
| [grill-with-docs](.agents/skills/grill-with-docs/SKILL.md) | The user requests a design interview with documentation. Discuss one decision at a time and record agreed decisions. |
| [architecture-foundation](.agents/skills/architecture-foundation/SKILL.md) | Designing or reviewing runtime architecture, state ownership, lifecycle, or contracts across modules. |
| [codebase-design](.agents/skills/codebase-design/SKILL.md) | Designing or revising module interfaces, dependencies, or testability. |
| [ronchi-case-validation](.agents/skills/ronchi-case-validation/SKILL.md) | Validating Cases, simulator histories, fault sequencing, Coverage Claims, or Findings against observed evidence. |
| [ronchi-cli-design](.agents/skills/ronchi-cli-design/SKILL.md) | Designing or reviewing CLI behavior for humans and agents, structured output, queries, errors, or persistent Lab commands. |
| [rust-best-practices](.agents/skills/rust-best-practices/SKILL.md) | Writing, reviewing, or optimizing Rust; load only the relevant language, async, CLI/control, MCP, verification, or release reference. |
| [sqlite](.agents/skills/sqlite/SKILL.md) | Designing or reviewing Journal schema, SQL, transactions, WAL/checkpoints, migrations, backup, or recovery. Use Rust guidance for driver and runtime integration. |
| [tdd](.agents/skills/tdd/SKILL.md) | Implementing behavior test-first or building integration tests through public interfaces. |
| [diagnosing-bugs](.agents/skills/diagnosing-bugs/SKILL.md) | Investigating reproducible defects, intermittent failures, or performance regressions. |
| [ronchi-code-review](.agents/skills/ronchi-code-review/SKILL.md) | Reviewing code changes against requirements, relevant decisions, correctness, and resource risks. |
| [planning-and-task-breakdown](.agents/skills/planning-and-task-breakdown/SKILL.md) | Turning agreed requirements into ordered implementation tasks with acceptance criteria and verification steps. |
| [context-engineering](.agents/skills/context-engineering/SKILL.md) | Improving task context, preparing handoffs, or updating project guidance for agents. |
| [ronchi-docs-maintenance](.agents/skills/ronchi-docs-maintenance/SKILL.md) | Maintaining development docs after decisions change, or auditing and consolidating documentation while preserving meaning and history. |

`grill-with-docs` depends on [grilling](.agents/skills/grilling/SKILL.md) and [domain-modeling](.agents/skills/domain-modeling/SKILL.md).
Both dependencies are included in the project.

Ronchi has one domain context: `CONTEXT.md` is the glossary, and `docs/adr/` holds architectural decisions.
Use [docs/README.md](docs/README.md) for task routes and documentation ownership.
Use [docs/product/overview.md](docs/product/overview.md) for current product intent and scope. Read other topics only when the task needs them.
Treat research as evidence and historical context, not as current product requirements.

## Implementation state and documentation

Use [tasks/implementation-progress.md](tasks/implementation-progress.md) for the latest session state.
Keep [tasks/todo.md](tasks/todo.md) as the only task-completion checklist.
The initial implementation run is complete. Do not restart completed work or infer a new assignment from historical task text.
The [documentation map](docs/README.md) routes current contracts, build/verification instructions and historical evidence.
Dashboard behavior lives in [docs/product/dashboard.md](docs/product/dashboard.md).
Application Checks, browser polling and discarded widget controls are removed or superseded. Do not restore them from historical records.
Historical run logs are under `tasks/history/`; their old delegation and commit permissions are not standing authorization.
Use subagents only when the current request or applicable instructions authorize them.

For Solana RPC semantics, use the shared [protocol references](.agents/references/solana-rpc/index.md). Select only the relevant topic. Protocol facts do not silently expand Ronchi's supported capabilities.

`planning-and-task-breakdown`, `context-engineering`, and the shared [Definition of Done](.agents/references/definition-of-done.md) come from [addyosmani/agent-skills](https://github.com/addyosmani/agent-skills/tree/301b02b12bff98e4688e6984af6556802322bbde).
They are pinned to revision `301b02b12bff98e4688e6984af6556802322bbde`; each imported skill includes the upstream MIT license.
For review and ADR guidance referenced by that checklist, use the project's `ronchi-code-review` and `domain-modeling` skills.
