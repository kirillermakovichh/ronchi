# Sources and adaptation

Reviewed on 2026-11-02.
These instructions combine selected guidance rather than concatenate upstream skills.

- [cli-ux](https://github.com/swyxio/skills/blob/b77bec0761a3954460bdfa3893e44e22e1d8613d/cli-ux/SKILL.md) from swyxio/skills, revision `b77bec0761a3954460bdfa3893e44e22e1d8613d`. Its MIT notice is preserved in [LICENSE-cli-ux](../LICENSE-cli-ux).
- [agentic-cli-design](https://github.com/tumf/skills/blob/0ec0ba7811b566f04d299d3f9033c64ae8bd34c4/agentic-cli-design/SKILL.md) and its principles/templates from tumf/skills, revision `0ec0ba7811b566f04d299d3f9033c64ae8bd34c4`. Its MIT notice is preserved in [LICENSE-agentic-cli-design](../LICENSE-agentic-cli-design).

The adaptation combines human and machine output, bounded queries, introspection, actionable errors, and proportional process-boundary verification.
It adds Ronchi's persistent Lab lifecycle, shared CLI/MCP semantics, project-local workspace, and distinction between observed evidence and business verdicts.
It removes duplicate checklists, numerical score requirements, blanket idempotency and confirmation rules, prescribed exit-code numbers, OAuth/login guidance, global credentials, and usage-skill installation as a default requirement.
Output field and flag names remain implementation decisions unless an existing project contract defines them.
