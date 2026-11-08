# Current implementation state

Ronchi is a local investigation prototype with a native Rust core and Solid dashboard.
No implementation task is active. [Tasks](todo.md) own completion status and verification evidence; [product contracts](../docs/README.md#current-documents) own current behavior.

## Published state

The latest verified public release is [v0.1.2](https://github.com/kirillermakovichh/ronchi/releases/tag/v0.1.2), tagged at `2afc14a`. It includes the response-loss activity summary, exact RPC total, sub-tenth rate display, compact RPC copy feedback, automatic installer PATH setup and the updated README screenshot/story. [Task 49](todo.md#49-publish-dashboard-and-installer-updates-as-v012) records publication and public verification.

The maintained installer comes from `main`; native archives come from Releases. [Installation](../docs/development.md#curl-installation-layout) owns PATH setup, updates and removal. Older releases remain intact. Homebrew remains deferred.

The user authorized committing/pushing all intended changes except `mock_backend/` and publishing this release. The mock remains local, untracked and unignored; do not stage it. Demo processes are stopped and their Journals are retained. Metadata under `/private/tmp/` is historical; release checks used separate disposable projects and did not change user installations or Labs.

## Recorded verification

| Scope | Evidence | Owner |
| --- | --- | --- |
| Native v0.1.2 | Complete macOS ARM/Intel and Linux x86_64 matrix; 290 Rust tests, 29 installer tests, fmt, Clippy, packaging and installed Simulator/Surfpool gates | [Task 49](todo.md#49-publish-dashboard-and-installer-updates-as-v012) |
| UI v0.1.2 | 72 frontend tests and ten real Chromium flows with the pinned Surfpool fixture enabled; downloaded embedded UI checked for summary, RPC total, clipboard and live update | [Task 49](todo.md#49-publish-dashboard-and-installer-updates-as-v012) |
| Public installation | Anonymous checksums, exact README command, current/new zsh PATH, live upgrade from v0.1.1, installed runtime smoke, tamper rejection and reinstall | [Task 49](todo.md#49-publish-dashboard-and-installer-updates-as-v012) |
| Supplied README media | Original screenshot preserved byte-for-byte; selected-history no-follow-up story; prior demo shutdown and final embedded build | [Task 47](todo.md#47-readme-no-follow-up-showcase-and-preview-shutdown) |

Checks have their recorded scope. Intel/Linux runtime claims come from native CI, not this macOS host. [Task 41](todo.md#41-first-native-release) retains initial release/CI cache evidence; [task 43](todo.md#43-publish-the-yellow-ui-as-v011) retains v0.1.1; [tasks 44](todo.md#44-automatic-shell-path-setup) and [46](todo.md#46-response-loss-follow-up-summary) retain installer and intermediate UI evidence.

## Remaining work

The only open completion item is the [full independent-backend retest](todo.md#remaining-validation). Neither the bundled relayer nor the local mock replaces that test.
The [normal-down report](history/unconfirmed-down-observation.md) remains unconfirmed; preserve its original envelope if it recurs.
[Open questions](../docs/open-questions.md) hold deferred product choices, not an active implementation queue.

## Resume

Read `AGENTS.md`, the [documentation map](../docs/README.md), [working plan](plan.md) and actual Git status before choosing work.
Historical run logs do not authorize restarting completed work, delegation, commits or publication.
