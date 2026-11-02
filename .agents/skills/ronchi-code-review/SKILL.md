---
name: ronchi-code-review
description: Review Ronchi changes against requirements, project decisions, correctness, and resource risks. Use when asked to review files, local changes, commits, branches, or pull requests.
license: MIT
---

# Ronchi code review

Produce an evidence-based review. Work in one agent by default.
A review request authorizes analysis and relevant local checks. Apply fixes or publish review comments only when requested.

## Establish scope and context

- Use the target the user names. If no target is given, review local changes, including relevant untracked files. If there are none, review the last commit and state that scope.
- For local changes, distinguish staged, unstaged, and untracked files. For a branch or commit range, resolve the comparison refs before reviewing.
- A comparison such as `git diff <base>...HEAD` covers committed changes only. Include local edits when requested. For a single commit, compare that commit with its parent.
- If Git history is unavailable, review the named files and disclose that no historical comparison was possible. Ask only when the missing scope prevents useful review.
- Use the task or referenced issue/spec as the requirements source. Read affected callers, dependencies, and tests to understand the actual execution path.
- Use repository-root `AGENTS.md` for working rules, `CONTEXT.md` for domain terms, and relevant `docs/adr/` decisions. Use `docs/README.md` to locate only the product documents needed for this change.
- If a requirement is unavailable or unresolved, state that gap. Continue the checks that do not depend on it.

## Review three areas

**Requirements:** Check missing or partial behavior, incorrect implementations, unintended behavior changes, and features beyond the agreed scope. Cite the requirement or accepted decision behind a mismatch. Treat proposals and open questions as unresolved choices.

**Correctness:** Trace input validation, error propagation, state transitions, retries, cancellation, concurrent access, and persistence where relevant. Check affected callers and whether tests can detect the actual failure pattern. Evaluate test gaps against concrete risk.

**Resources and complexity:** Follow the resource model in `docs/adr/0019-resource-efficient-rust-core.md`. Look for unbounded buffers, queues, tasks, or retained Journal data; blocking work on the RPC path; unnecessary payload copies or parsing; lock contention; and excessive disk writes. Consider the supported workload. Judge abstractions by the responsibility they encapsulate and the complexity they remove. Rust enums, repeated matches, or a small wrapper are not defects by themselves.

Use relevant existing local checks when they can resolve uncertainty. Preserve their actual results and limitations.
Performance claims need measurements or a demonstrable mechanism under a concrete workload. Do not invent resource budgets or treat shorter code as proof of efficiency.

## Require evidence

- For each confirmed defect, identify the precise location, triggering conditions, consequence, and supporting code or check. Cite a requirement when the finding concerns compliance.
- Verify that the affected caller or execution path exists. Report a pre-existing issue only when the change exposes or worsens it, and explain that relationship.
- Separate confirmed defects, hypotheses needing verification, and optional improvements. An architectural preference alone is an optional suggestion.
- Suggest the smallest complete correction that preserves required behavior, validation, and error handling.
- Do not impose a finding quota. An empty findings list is a valid result. Preserve uncertainty instead of claiming that untested behavior is proven safe.

## Report

Lead with the highest-impact confirmed issue. For each item, give a severity, `file:line`, trigger, consequence, evidence, and concise correction. Scale the explanation to the issue.

Group confirmed defects, items needing verification, and optional improvements separately; omit empty groups. State the review scope and material checks performed or unavailable. If no confirmed defects were found, say so without implying exhaustive verification.

Follow the language and brevity rules in `AGENTS.md`.

## Sources

Adapted from [Matt Pocock's code-review](https://github.com/mattpocock/skills/blob/main/skills/engineering/code-review/SKILL.md) and [DietrichGebert's ponytail-review](https://github.com/DietrichGebert/ponytail/blob/main/skills/ponytail-review/SKILL.md). Both sources use the MIT license; notices are preserved in [LICENSE](LICENSE).
