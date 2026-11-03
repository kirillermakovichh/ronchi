# Unconfirmed normal-down observation

On 2026-11-03, one native browser run of
`retained viewer reads the original immutable Journal without restarting a Lab`
failed at `ronchi --project <temporary-project> --json down`.
This happened after ready startup and before viewer startup.
The initial test helper did not include the structured command output in its thrown error.
No retained structured stdout/stderr envelope for that occurrence was found in the repository.
The exact exit code, immutable Lab ID, phase, and resource state are therefore unavailable.
Do not infer a cause from this incomplete record.

Ten focused repetitions subsequently passed.
Later normal-down browser and real-process viewer tests also passed.
These outcomes do not establish a fix.
No retry or lifecycle change was added to hide the observation.

The current browser helpers include command arguments and structured stdout/stderr in errors.
The original retained scenario still uses ordinary `down` without an implicit retry.
On failure it annotates the known Lab ID, operation sequence, and bounded public status fields.
It may force-stop a confirmed ready owned Lab for cleanup, then rethrows the original failure.
Cleanup does not turn that failed acceptance test into success.
The real-process broken-pipe test also records the exit code and immutable Lab ID on normal-down failure.
If the failure recurs, preserve the original failed result.
Record the operation sequence and known process/resource state before owned cleanup.
Remove credentials, tickets, and signing material from retained diagnostic text.
Use the project diagnosing-bugs workflow only after obtaining an observable trigger.
