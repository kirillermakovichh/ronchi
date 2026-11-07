# Validate and publish the first native release

Status: authorized by the user. Task 41 in [todo.md](todo.md#41-first-native-release) is the completion checklist.
The user authorized pushing main, running Native distribution, fixing failures, pushing v0.1.0 only after successful checks, verifying public curl installation and updating README.
Homebrew remains deferred. Do not publish an unverified target or bypass failed checks.

## Ordered work

1. Push the verified curl-distribution implementation to main and manually dispatch `.github/workflows/release.yml` at that exact revision.
2. Inspect all three native jobs. Reproduce or explain failures, make the smallest complete fixes, commit and push them, then rerun the affected release validation. Keep tests and release gates meaningful.
3. After the complete matrix passes, tag the verified main revision as `v0.1.0` and push that tag. Monitor the tagged build and publication; do not overwrite an existing version/tag silently.
4. Download published artifacts/checksums and exercise the public curl command in an isolated install prefix. Verify the installed binary, embedded dashboard and private Surfpool. Retain project-local Journal behavior and clean only owned test Labs.
5. Remove README's release-preparation notice only after the public install works. Record actual target evidence, release URL and checks in progress/task 41; commit and push the documentation update.

## Boundaries

Use the existing local GitHub CLI keyring authentication; do not print or store tokens in files or chat.
Keep temporary logs, artifact downloads and test projects outside tracked files.
Do not stop user backends, unrelated services or user Labs to satisfy release checks.
Distinguish a generated workflow, a successful validation run, a successful tagged run and a verified public installation.
