# Managed Surfpool execution mode

Ronchi will support Surfpool as an optional external Execution Backend selected through the CLI. In this mode Ronchi starts and owns the Surfpool process or container, waits for its local RPC endpoint, routes all application blockchain reads and writes through that endpoint, injects faults at the Ronchi boundary, and stops only the Surfpool instance it started. Ronchi does not fork or embed Surfpool initially.

Surfpool mode and direct Read-through mode are mutually exclusive state-authority models. Ronchi must not serve account reads directly from a public RPC after Surfpool has executed local transactions, because that would mix pre-execution remote state with mutated local state. Surfpool may use its own configured datasource to hydrate accounts, but this remains behind Surfpool and is not a second application-facing route owned by Ronchi.

## Consequences

- `ronchi up --execution surfpool --case <case>` is the intended conceptual CLI.
- Ronchi manages a pinned compatible native Surfpool binary under the current Project Workspace, verifies its checksum, and reuses it across Runs in that project. Docker is an explicit fallback rather than a prerequisite.
- The normal path follows Surfpool's native project discovery and configuration conventions instead of requiring users to describe programs and accounts again in Ronchi.
- A transaction may be forwarded only to the local managed Surfpool RPC, never to its public datasource.
- Ronchi never funds transaction signers and never invokes a faucet or airdrop automatically. Surfpool execution uses the fee payer state hydrated from its datasource or state created explicitly by the System Under Test; insufficient funds remain an observable application condition.
- Findings may describe local execution and resulting account state only when this mode is active and the relevant evidence was collected.
- Ronchi must pin and report the Surfpool version and distinguish an owned process from any future user-managed attachment mode.
- Each managed Surfpool Lab starts with clean in-memory state by default. State is not reused after the Lab stops; any future disk persistence must be an explicit opt-in.
- Offline state and Surfpool datasource configuration remain packaging decisions rather than reasons to expose two competing state authorities.
