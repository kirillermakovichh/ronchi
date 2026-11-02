# Read-through upstream with a write firewall

In the default non-executing mode, Ronchi may accept an explicitly configured public, mainnet, testnet, or private Solana RPC endpoint as a Read Upstream during local development, CI, or isolated staging. Only allowlisted read methods may be forwarded. `sendTransaction`, `requestAirdrop`, provider-specific writes, and unknown methods never reach the Read Upstream; transaction lifecycle methods remain under the stateful overlay when required for a coherent Case. This provides real account and chain data without allowing Ronchi to broadcast a transaction. It is not used when a local Execution Backend owns state.

## Consequences

- The CLI remains useful without an SDK: the application changes one RPC URL and the developer supplies the optional Read Upstream.
- Read-through Runs depend on upstream availability, rate limits, current state, and credentials and therefore are not fully hermetic or deterministic.
- Journal Evidence distinguishes the upstream result from the RPC Boundary Result returned by Ronchi.
- A later capture/replay mode may turn selected read-through evidence into a deterministic Fixture.
- Forwarding uses an allowlist rather than attempting to enumerate every possible write method.
