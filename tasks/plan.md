# Publish v0.1.1 with the accepted yellow UI

The user authorized a new release so the public curl installer includes the yellow dashboard and current terminal. [Task 43](todo.md#43-publish-the-yellow-ui-as-v011) is the completion checklist. Preserve v0.1.0 and publish the patch as v0.1.1.

1. Bump native/frontend versions without changing dependency versions. Keep target-specific cache reuse across patch-version changes; all checks remain enabled.
2. Verify local frontend/release builds and workflow syntax. Push the release commit/tag. The existing tagged pipeline runs the complete native matrix and publishes only after every gate passes. Do not run a redundant manual matrix.
3. Check all published SHA-256 files. Test anonymous latest curl installation, actual embedded yellow styles and terminal output, Surfpool and updating an isolated v0.1.0 installation. Do not change user installations or Labs.
4. Update release notes, README and progress with the actual new release/results, then commit and push final docs. Homebrew remains deferred.
