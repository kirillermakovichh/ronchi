# Install Ronchi with its compatible Surfpool executable

Status: accepted. Refines ADR-0014 for installed executables; its project-local runtime-data rule remains in force.

The curl distribution installs Ronchi and a pinned private Surfpool together so users do not repeat manual archive setup in every project. Both executables and their distribution notices live in a versioned installation shared by projects. Journals, chain state, configuration and process logs remain in each project's `ronchi/` directory. Runtime startup never downloads a tool or selects an arbitrary executable from PATH.

Ronchi locates the companion relative to its canonical executable and verifies the official archive and extracted executable. An invalid installed companion is an error, not a reason to silently select a project copy. Source builds retain the explicit project-local archive installer when no distribution companion exists.

A stable user-local symlink selects a versioned release. Updates leave old versions intact so active Labs keep their executable paths. Cleanup of old installed releases is explicit and requires those Labs to be stopped. No self-update command or Homebrew publication is part of the initial curl distribution.
