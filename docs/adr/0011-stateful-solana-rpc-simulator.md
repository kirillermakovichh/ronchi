# Stateful Solana RPC simulator instead of a validator or fork

Ronchi will act as the Solana HTTP JSON-RPC authority for the System Under Test and maintain only the protocol state needed to investigate off-chain behavior: slots and time, recent blockhash validity, submitted transaction identities, and simulated pending, successful, failed, or expired lifecycle states. It will not start a validator, fork mainnet, execute SVM programs, or mutate real account balances. This keeps the product focused on how backend code reacts to controlled RPC histories rather than whether transactions execute correctly on-chain.

## Consequences

- A Run can prove which RPC calls the application made, which lifecycle state the simulator assigned, which fault was applied, what response the application saw, and what the application did next within the active Observation Boundary.
- A Run cannot prove that a transfer occurred, balances changed, a program executed correctly, or duplicate economic execution happened.
- Cases that require real program execution, validator topology, consensus, or authoritative account mutations are unsupported in the primary product model.
- An execution-backed mode may be considered later as a separate capability, but it is not required by the core architecture.

## Considered Options

- A local validator or mainnet fork was rejected because transaction execution fidelity is not needed to test the off-chain retry, timeout, status-tracking, and reconciliation logic in the first product wedge.
- A stateless response mock was rejected because multi-call failures such as lost submission responses, blockhash expiry, and changing transaction status require a coherent history across RPC calls.
