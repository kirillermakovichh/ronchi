# Finish public installation verification

Task 41 in [todo.md](todo.md#41-first-native-release) is the only completion checklist.
The user authorized pushing main, native CI fixes, the verified `v0.1.0` tag/release, public curl verification and final docs. Homebrew remains deferred.

Manual validation `42404486999` and tagged pipeline `42406451668` passed on all three native targets at `0ca02a0`. The release exists; all three published archive checksums and an actual downloaded ARM installation/live-update flow passed. Requested CI caches have been exercised; timings and retained checks are in task 41.

## Remaining action

GitHub reports the repository as private. An asynchronous question asks the user to authorize making the entire repository/history public. Wait for the answer before changing visibility; elapsed time is not permission.

After explicit permission:

1. Make `kirillermakovichh/ronchi` public using the existing authorized GitHub CLI session.
2. Verify all published checksums and run the anonymous public curl installer in an isolated prefix. Check installed Simulator, embedded dashboard, private Surfpool and same-version reinstall.
3. Apply the prepared release notes, remove README's public-installation notice, close task 41d and push final documentation.

Temporary prepared helpers are `/private/tmp/ronchi-public-release-check.py`, `/private/tmp/ronchi-verify-release-assets.py` and `/private/tmp/ronchi-v0.1.0-notes.md`. Inspect them before use. They are outside tracked files and must not be committed.

Do not print or persist GitHub tokens. Do not change user backends/Labs, overwrite the existing tag/release, or claim anonymous installation from an authenticated asset download. See [implementation-progress.md](implementation-progress.md) for current state.
