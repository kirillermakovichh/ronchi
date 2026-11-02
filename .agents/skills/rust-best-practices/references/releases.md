# Native binary releases

Load this reference only for packaging, dependency policy, or release work.
These notes are locally written against official tool documentation.
The release tools reviewed in oakoss's skill are candidates, not adopted Ronchi dependencies.
See [sources.md](sources.md) for the reviewed source and primary references.

## Existing contracts first

Inspect current Cargo manifests, lockfile, CI, supported targets, and publishing workflow.
Distinguish local builds and draft configuration from publishing artifacts or changing external registries.
A request to design release automation does not authorize a release.
Reuse established tools unless the task or a demonstrated gap warrants a change.

## Distribution

Preserve the accepted native executable and embedded dashboard model.
Ordinary use must not require Node, Python, a JVM, an external database, or a telemetry collector.
Treat Surfpool as an optional separately managed executable with explicit version and acquisition behavior.
Do not vendor it, bundle it, download it, or change its license handling implicitly.
Keep runtime artifacts in the selected project workspace.

Choose supported OS/architecture targets deliberately.
Verify dynamic linking, platform system requirements, executable permissions, and startup on those targets.
Do not describe a binary as fully static merely because it uses Rustls.
Include required license notices and embedded-asset attribution in the distribution.
Document artifact checksums or integrity mechanisms that the chosen distribution actually provides.

## Optional tooling

| Tool | Consider when |
| --- | --- |
| dist (cargo-dist) | Native release artifacts and installers need maintained cross-platform automation. |
| release-plz | The chosen Cargo versioning, changelog, or publishing workflow benefits from it. |
| cargo-deny | Dependency license, advisory, source, or duplication policy needs automated checks. |

Check current official documentation and the selected version before writing configuration.
Do not copy old configuration keys, tool versions, workflow actions, or target matrices from a source skill.
Do not install tools globally as an incidental effect of editing release configuration.
Do not add TUI, Docker, registry publication, or conventional-commit requirements merely to complete a template.

## Verification

For a changed packaging contract, build and smoke-test the relevant artifact when the environment supports it.
Confirm embedded assets are available without a frontend runtime.
Check that help/version work without starting the Lab.
Check that simulator mode does not require Surfpool.
Use a fresh isolated workspace to detect hidden dependencies on existing developer files.
Report untested targets and distinguish generated configuration from a verified release pipeline.
