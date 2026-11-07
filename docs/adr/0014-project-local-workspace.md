# One project-local Ronchi workspace

Refined by [ADR-0020](0020-installed-native-tool-distribution.md) for shared installed executables. The original tool-placement trade-off below is historical; runtime data remains project-local.

All persistent Ronchi files will live under `<project-root>/ronchi/`. This includes downloaded managed tools, Surfpool runtime files, generated temporary configuration, Lab Journals, optional Run artifacts, summaries, and process logs. Ronchi will not create a persistent global cache or use OS application-support directories by default. Keeping one visible, deletable directory makes disk use and cleanup explicit for each project.

The conceptual layout is:

```text
ronchi/
├── tools/
│   └── surfpool/<version>/
├── labs/
│   └── <lab-id>/
│       ├── journal/
│       ├── ronchi.log
│       ├── surfpool.log
│       └── surfpool/
└── runs/
    └── <run-id>/             # optional automated workflow
        ├── summary.json
        └── manifest.json
```

The exact internal filenames are not yet a compatibility contract. A future explicit workspace flag may relocate the entire directory, but individual subdirectories are not configured independently.

## Consequences

- Removing `ronchi/` removes every downloaded tool, runtime state, log, and artifact owned by Ronchi for that project.
- Surfpool binaries may be duplicated across projects; this is accepted in exchange for transparent ownership and cleanup.
- Raw RPC payload capture remains opt-in and bounded because project-local storage does not eliminate size or secret risks.
- Git-ignore behavior and automatic retention policy remain separate decisions.
