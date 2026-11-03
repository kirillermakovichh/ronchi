# Active task 14: embedded dashboard

Task 14 is incomplete. Do not advance to task 15 yet.

The initial embedded/live/session slice is commit `5e846f3`.
The next slice adds public Case/Check controls, streaming response bounds, exact integer display, retained viewer, and startup/Demo launch integration.
The task queue records verified paths and remaining acceptance.

Build the frontend before Cargo:

```bash
npm --prefix dashboard ci
npm --prefix dashboard run typecheck
npm --prefix dashboard test -- --run
npm --prefix dashboard run build
cargo build --locked
npm --prefix dashboard run test:browser
```

Playwright's matching Chromium is installed locally.
Three native browser flows pass.
The suite uses temporary empty projects, real Labs, actual reference recovery, and the public collector/evaluator.
Desktop and 390px screenshots were viewed with `view_image`.
Screenshots and reports remain ignored.
The full Rust suite passed after the viewer and launch changes.
Clippy and release build passed.
The final per-route body-limit correction passed the focused native HTTP regression.

Continue with Findings pagination, detail bookmarks, and the remaining error/cancellation/browser checks listed in task 14.
Do not claim those checks passed from jsdom coverage.
A single retained test failed on ordinary `down` before viewer startup.
Ten focused repetitions then passed.
CLI failure output is now included in the harness error.
No automatic retry was added to Ronchi.
No browser blocker exists.

Do not close Checkpoint C's external-backend item.
Keep the HTTP-only collector, 32 KiB source cap, and existing evaluation budgets.

## Current continuation slice

Findings pagination and event/Finding bookmarks now have focused native coverage.
The tests generate real signed transactions with Node crypto and send them through public HTTP RPC.
They do not write Journal evidence or inject Findings.
The bookmark tests cover fresh tickets, retained endpoint changes, and name reuse.
`--selection` restores a credential-free bookmark query for live or retained launch.
Unsafe integer coordinates preserve exact decimal tokens; fact strings remain strings.
Findings continuation failure has a controlled transport/renderer test.

A real-process stdout-closure viewer test is also written and passed once.
It verifies exit, listener release, unchanged Journal bytes, and another Lab's survival.
It belongs to the next verification slice.
The broader cancellation/payload/state acceptance still remains.
