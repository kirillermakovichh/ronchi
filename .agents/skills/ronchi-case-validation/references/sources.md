# Sources and scope

Reviewed on 2026-11-02.
This skill is locally written for Ronchi's accepted contracts.
No upstream skill prose, code examples, scripts, or agent-orchestration configuration is vendored.

- [proptest-praxis](https://github.com/inanna-malick/agent-skills/blob/fa2ce23d66a155833e227860699da18044341b3e/skills/proptest-praxis/SKILL.md), reviewed revision `fa2ce23d66a155833e227860699da18044341b3e`: independent models, interacting operation histories, counterexample minimization, and reproducibility informed the approach.
- [Proptest Book](https://proptest-rs.github.io/proptest/intro.html): check the selected framework's generation, shrinking, and failure-persistence behavior when used.
- [proptest-state-machine](https://docs.rs/proptest-state-machine/latest/proptest_state_machine/) (reviewed documentation for 0.9.0): the reviewed runner documents sequential state-machine histories; it does not establish concurrent coverage. Recheck the selected version before relying on this limitation.
- [Shared Solana references](../../../references/solana-rpc/index.md): protocol facts and their official sources.
- Project `CONTEXT.md`, ADR-0005, ADR-0006, and ADR-0011 through ADR-0017: observation limits, modes, forwarding, persistence, and continuing Cases remain defined by the project.

The skill does not inherit repository-wide bug hunting, a requirement to find defects, discovery branches, mandatory PR creation, automatic multi-agent work, or open-ended investigation from the external methodology.
It does not require a new test dependency or establish deferred Case, concurrency, or replay contracts.
