# Publish v0.1.2

[Task 49](todo.md#49-publish-dashboard-and-installer-updates-as-v012) owns the checklist. The user authorized commit, push and publication of all current changes except `mock_backend/`. Keep the mock untracked and do not change `.gitignore`.

Prepare/verify the patch version and local release assets. Commit tracked changes and push `main` with the new annotated tag. The existing tagged workflow must pass every native build/test/package/install gate before publication. Preserve earlier releases and do not run a duplicate manual matrix.

After publication, verify anonymous archive checksums, exact README curl installation, installed version/UI and Simulator/Surfpool/reinstall behavior in isolated temporary projects. Update release notes, README and session status from the results, then commit/push final docs. The independent-backend retest and Homebrew remain separate.
