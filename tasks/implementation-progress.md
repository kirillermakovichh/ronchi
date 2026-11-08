# Current implementation state

Ronchi is a local investigation prototype with a native Rust core and Solid dashboard.
Active work: [v0.1.2 publication](todo.md#49-publish-dashboard-and-installer-updates-as-v012). [Tasks](todo.md) own completion status and verification evidence; [product contracts](../docs/README.md#current-documents) own current behavior.

## Published and local state

The latest verified public binary release is [v0.1.1](https://github.com/kirillermakovichh/ronchi/releases/tag/v0.1.1) at `662e990`. It includes the yellow dashboard and compact terminal mark. The maintained installer comes from `main`; native archives come from Releases. [Installation](../docs/development.md#curl-installation-layout) owns PATH setup, updates and removal. Homebrew remains deferred.

The prepared v0.1.2 source adds the response-loss activity summary, sub-tenth RPS display, compact RPC copy icon and RPC total. The local native executable embeds all these UI changes. README uses the supplied no-follow-up screenshot. Public release assets have not been replaced.

The mock loop, Surfpool Lab/child and temporary UI preview were stopped at the user's request. Their Journals remain readable. Local launch metadata under `/private/tmp/` is historical. The user authorized committing/pushing the current UI/README/docs and publishing v0.1.2. `mock_backend/` remains excluded, untracked and unignored.

## Recorded verification

| Scope | Recorded evidence | Owner |
| --- | --- | --- |
| Public v0.1.1 | Full macOS ARM/Intel and Linux x86_64 matrix; 290 Rust tests; fmt, Clippy, packaging and installed Simulator/Surfpool gates; anonymous asset checksums, colors and live upgrade | [Task 43](todo.md#43-publish-the-yellow-ui-as-v011) |
| Local UI | 72 frontend tests at the latest revision; native Chromium checks of the summary, live updates, clipboard and counters; earlier checks keep their own scopes | [Task 46](todo.md#46-response-loss-follow-up-summary) |
| Local README and final binary | Supplied image preserved byte-for-byte; updated case story; confirmed process shutdown; final embedded release build after shutdown | [Task 47](todo.md#47-readme-no-follow-up-showcase-and-preview-shutdown) |
| Public installer | 29 fixture tests, actual macOS ARM archive smoke and exact public README command in current/new zsh terminals | [Task 44](todo.md#44-automatic-shell-path-setup) |

These are recorded checks, not tests rerun by documentation maintenance. Intel/Linux runtime claims are bounded to native CI. [Task 41](todo.md#41-first-native-release) retains the initial release, CI fixes and measured cache timings. [Tasks 36](todo.md#36-remove-application-checks), [37](todo.md#37-remove-confirmed-dead-code) and [42](todo.md#42-yellow-identity-and-readme-media) retain earlier browser, terminal and supplied-media evidence.

## Remaining work

The only open completion item is the [full independent-backend retest](todo.md#remaining-validation). Neither the bundled relayer nor the local mock replaces that test.
The [normal-down report](history/unconfirmed-down-observation.md) remains unconfirmed; preserve its original envelope if it recurs.
[Open questions](../docs/open-questions.md) hold deferred product choices, not an active implementation queue.

## Resume

Read `AGENTS.md`, the [documentation map](../docs/README.md), [working plan](plan.md) and actual Git status before choosing work.
Historical run logs do not authorize restarting completed work, delegation, commits or publication.
