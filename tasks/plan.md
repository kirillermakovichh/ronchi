# Curl distribution with managed Surfpool

Status: implemented and locally verified. GitHub-hosted target verification and publication remain separate actions. Task 40 in [todo.md](todo.md#40-curl-installation-and-bundled-surfpool) is the only completion checklist.
The agreed research notes remain outside the repository at `/private/tmp/ronchi-installation-plan.md`.

## Contract

Ship Ronchi under MIT with a compatible pinned Surfpool executable in the same native installation.
Support curl installation and update by rerunning the installer. Homebrew, npm and self-update commands are outside this task.
Prepare release builds for macOS arm64, macOS x86_64 and Linux x86_64; publish support claims only after target checks pass.
Keep GitHub publication separate from implementation. Do not commit, push a tag, publish a release or create external repositories without a new request.

Use one versioned installation directory with a stable user-local `ronchi` symlink. Keep old versions so running Labs retain their executable paths during upgrades.
Keep Surfpool private to that installation; never search PATH for it or replace an independently installed copy.
The user explicitly requested removal of existing local Surfpool installations before implementing this task. Do not delete Labs, Journals or chain state.
Ronchi and Surfpool executables may be shared across projects. All runtime data remains under the selected project's `ronchi/`.
Resolve installed Surfpool relative to the canonical Ronchi executable. Preserve explicit project-local archive installation for source builds and existing users.
Verify the official pinned Surfpool archive and executable on use; do not trust a mutable release manifest to establish upstream identity.

## Ordered slices

1. Add MIT metadata and a shared pinned artifact catalog. Resolve and validate an installed companion before falling back to the existing project-local tool path. Prove actual cross-project startup and rejection of tampered companions.
2. Add deterministic native packaging and a POSIX shell installer. Bundle the official Surfpool archive, extracted executable and required distribution notices. Use versioned directories, checksummed downloads and atomic symlink replacement; do not touch project data.
3. Add GitHub Actions build/package/smoke workflows for the selected targets. Build dashboard assets before Cargo. Test release artifacts without publishing; gate tagged publication on build and install checks. Keep runtime dependencies off the user's machine.
4. Verify installation/update/failures against local HTTP release fixtures and the real macOS artifact. Run focused Rust checks and applicable regressions. Update maintained contracts, README and progress with accurate publication status.

## Design choices and checks

A small explicit release/installer layer replaces the stock cargo-dist shell layout because the private pinned companion includes an archive and notices as well as two executables. Do not introduce an additional distribution framework unless it removes demonstrated complexity.
Use GitHub-hosted artifacts and a checked-in installer; no domain or hosted service is required.
Installation must reject unsupported platforms, corrupt archives and unsafe archive entries without replacing a working installation.
Installer failure and concurrent updates must preserve the previous executable. Reinstalling an identical version must not mutate files used by live Labs.
Check PATH messaging without claiming to edit the current parent shell. Document removal separately from Lab-data deletion.
Actual target smoke tests, not a cross-compiled file, establish platform compatibility. macOS minimum version and Linux runtime compatibility must be explicit in release documentation.
Keep dependency licenses/notices with distributed binaries. Surfpool remains an independent executable under Apache-2.0.
