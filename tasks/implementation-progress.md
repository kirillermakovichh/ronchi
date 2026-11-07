# Current implementation state

Ronchi is a local investigation prototype with a native Rust core and Solid dashboard.
Implementation and installation work is complete at its recorded scope. No new implementation task is selected.
[Tasks](todo.md) own completion status and detailed verification evidence. [Product contracts](../docs/README.md#current-documents) own current behavior.

## Distribution

The current binary release is [v0.1.1](https://github.com/kirillermakovichh/ronchi/releases/tag/v0.1.1) at `662e990`. It includes the yellow dashboard and compact terminal mark.
The maintained installer comes from `main`; native archives come from Releases. [Installation](../docs/development.md#curl-installation-layout) owns PATH setup, updates and removal. The older release tags and assets remain intact.
Homebrew remains deferred.

## Recorded verification

| Scope | Latest recorded evidence | Record |
| --- | --- | --- |
| Native v0.1.1 | Full macOS ARM/Intel and Linux x86_64 matrix; 290 Rust tests; fmt, Clippy, installer fixtures, packaging and installed Simulator/Surfpool gates | [Task 43](todo.md#43-publish-the-yellow-ui-as-v011) |
| Dashboard and terminal | 64 frontend tests, five PTY tests, native browser checks and release assets at their respective change scopes | [Tasks 37](todo.md#37-remove-confirmed-dead-code) and [42](todo.md#42-yellow-identity-and-readme-media); [task 36](todo.md#36-remove-application-checks) records the full ten-flow browser suite |
| Public installation | Anonymous archive checksums, embedded yellow styles/terminal, live upgrade from v0.1.0 and isolated runtime smoke | [Task 43](todo.md#43-publish-the-yellow-ui-as-v011) |
| Automatic PATH | 29 installer fixtures, actual macOS ARM archive smoke and the exact public README command in current/new zsh terminals | [Task 44](todo.md#44-automatic-shell-path-setup) |

These are recorded checks, not tests rerun by documentation maintenance. Local installation checks used temporary projects and shell homes; they did not change user profiles, installations or Labs. Intel/Linux runtime claims are bounded to native CI. [Task 41](todo.md#41-first-native-release) retains the original release, corrective commits and measured CI cache timings. [Task 42](todo.md#42-yellow-identity-and-readme-media) retains the supplied README media provenance.

## Remaining validation

The only open completion item is the [full independent-backend retest](todo.md#remaining-validation) after RPC and dashboard fixes. The earlier Solana-backend trial gives bounded manual evidence; the bundled relayer does not replace that test.
The [normal-down report](history/unconfirmed-down-observation.md) remains unconfirmed. Preserve its original envelope if it recurs.
[Open questions](../docs/open-questions.md) contain deferred product choices, not an active implementation queue.

## Resume

Read `AGENTS.md`, the [documentation map](../docs/README.md) and actual Git status before choosing work.
Use [the working plan](plan.md) for current maintenance scope. Historical run logs do not authorize restarting completed work, delegation, commits or publication.
