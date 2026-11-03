# Task 14 completed: embedded dashboard

Task 14 and slices 14a–14f are complete. The final explicit invalid-detail recovery regression passed.
Task 15 is the next task. It has not been started.

## Next session

The user requested one autonomous coordinated run for tasks 15–24.
Use [remaining-implementation-run.md](remaining-implementation-run.md) and [parallel-run-state.md](parallel-run-state.md).
The preparation session did not start implementation workers or change runtime code.
Keep the task-14 verification and unconfirmed observation below as the recorded baseline.

Existing layout, browser authentication, controls, embedded assets, and retained viewer were preserved.
The continuation slice is commit `b430db5`.
It adds bounded Findings pages, exact event/Finding bookmarks, and live/retained `--selection` launch.
The final acceptance slice adds explicit unknown collection outcomes and status inspection,
stale read cancellation, committed scope display, non-queued payload clicks, no-selection UI,
error states, and narrow recovery command wrapping.

| Criterion | Verification | Result |
| --- | --- | --- |
| Findings continuation | Public signed HTTP RPC incidents produce more than five Findings; five-item core cursor pages pin snapshot/scope; controlled transport error keeps the last valid page | Passed |
| Detail bookmarks | Event/Finding links, ordinary reload, fresh bootstrap, exact integer wire models, new retained endpoint, and Lab-name reuse | Passed |
| Polling/cancellation | Native delayed reads; controlled visibility signal; real source accepted-start response loss; status inspection; public terminal cancellation; browser closure preserves ready Lab and armed Case | Passed |
| Payload continuation | Real independent HTTP source and collector; Unicode chunks follow UTF-8 byte offsets; repeated clicks do not enqueue; integer digits stay exact | Passed |
| States/keyboard | Native no-selection, operational error and disconnect; real keyboard detail activation/focus preservation; controlled starting/unsupported/analysis-limit/core-failure DTO branches | Passed with stated coverage boundary |
| Viewer stdout closure | Actual foreground process readiness, pipe closure, observed exit, released listener, unchanged Journal bytes, other Lab survival | Passed |

## Final verification

- `npm --prefix dashboard ci`: passed, lockfile unchanged, audit reported zero vulnerabilities.
- `npm --prefix dashboard run typecheck`: passed.
- `npm --prefix dashboard test -- --run`: 13 tests passed.
- `npm --prefix dashboard run build`: passed.
- `cargo build --locked`: passed with the built embedded assets.
- `npm --prefix dashboard run test:browser`: 8 native Chromium flows passed.
- `cargo fmt --check`: passed.
- `cargo check --locked`: passed.
- `cargo test --locked`: full suite passed, including the real-process viewer test.
- `cargo clippy --locked --all-targets -- -D warnings`: passed.
- `cargo build --release --locked`: passed.

Actual desktop and 390px screenshots were inspected.
The observed long-command overflow was fixed and rechecked.
Screenshots/reports/runtime artifacts remain ignored.
No browser setup was repeated.
The visibility signal is controlled inside real Chromium; it is not a claim about an OS window manager.
Renderer DTO tests do not claim to reproduce native failure lifecycles.
The isolated pre-viewer normal-down failure remains an unconfirmed observation.
See [unconfirmed-down-observation.md](unconfirmed-down-observation.md).
It did not recur in this acceptance run.
No retry or speculative Lab lifecycle fix was introduced.

Keep HTTP-only collection, the 32 KiB cap, and existing evaluation budgets.
Keep Checkpoint C's external-backend item open.
No remote was added and nothing was published.
