# Post-transaction state options

Research note. The options below explain trade-offs. Current state-authority decisions are defined by [ADR-0011](../adr/0011-stateful-solana-rpc-simulator.md), [ADR-0012](../adr/0012-read-through-upstream-with-write-firewall.md), and [ADR-0013](../adr/0013-managed-surfpool-execution-mode.md).

This note evaluates how Ronchi could support backend workflows that read account state after an intercepted transaction. It records research, not an accepted product decision.

## Established facts

- Arbitrary Solana state transitions are program-specific. Correctly updating wallet balances, token accounts, AMM liquidity, tick arrays, fees, oracle-dependent values, and CPI-touched accounts requires executing the actual transaction against the required programs and input accounts.
- Reimplementing those transitions as Ronchi rules would create protocol-specific business logic and cannot provide general Solana coverage.
- Solana `simulateTransaction` executes without committing state and can return post-simulation data for explicitly requested transaction accounts. Agave limits the requested addresses to the number of account keys in the transaction and returns no account data when simulation fails.
- A public simulation still receives the full transaction payload, depends on live upstream state, and does not retain post-state for the next simulation.
- LiteSVM can execute transactions, mutate accounts, load programs, warp slots, and retain transaction history, but it is an in-process program-testing library rather than a drop-in Solana JSON-RPC endpoint.
- Mollusk is a lower-level program execution harness. It requires an explicit account list and deliberately does not provide a validator runtime, Bank, or AccountsDB, making it unsuitable as the primary drop-in backend for Ronchi.
- `solana-test-validator` supplies standard RPC and persistent local execution but requires programs and relevant accounts to be deployed, cloned, or loaded.
- Surfpool already combines a Solana RPC surface, LiteSVM execution, remote account data, local overrides, and protocol-specific scenarios. Its scenario support demonstrates that meaningful manual account overrides still require IDLs, layouts, or protocol-specific adapters.
- Surfpool can start without an application manifest when automatic deployment is disabled. With a datasource configured, an account read that misses local state falls back to the datasource and hydrates the result into the local SVM.
- Before transaction simulation or execution, Surfpool resolves Address Lookup Tables, extracts the transaction account keys, and fetches locally missing accounts through the datasource. When a fetched account is an upgradeable executable program, Surfpool also fetches its ProgramData account. Programs and accounts already present in the datasource therefore do not require a separate manual clone list.
- Lazy hydration does not create state that is absent from both the local SVM and datasource. In particular, an unfunded development fee payer is not automatically made usable, and an application program that is not deployed upstream must still be deployed locally. Explicit local funding or deployment remains necessary for those workflows.

## Options

### 1. RPC lifecycle only

Ronchi intercepts writes, assigns simulated transaction states, and forwards allowlisted reads unchanged. Account reads do not reflect simulated transactions.

Useful for timeout, retry, status tracking, blockhash expiry, and reconciliation behavior. It cannot support workflows whose next decision depends on mutated account state.

### 2. Declarative read overrides

A Case supplies bounded replacement values for selected reads after a transaction. This works for known simple conditions but does not calculate arbitrary protocol state and must not be presented as execution.

### 3. Upstream simulation with a post-state overlay

Ronchi sends the transaction to `simulateTransaction`, requests the post-simulation transaction accounts, and serves those snapshots from a local overlay on later reads. The transaction is not broadcast.

This can support a single transaction followed by account reads when the upstream has all required programs and input state. It is not a general execution environment: the full payload leaves the machine, live state is nondeterministic, failures return no post-state, and a second dependent simulation does not see the first simulation's local overlay.

### 4. Execution-backed adapter

Ronchi places its fault and evidence layer in front of a local execution backend. Surfpool is the strongest existing candidate for a drop-in RPC workflow; a direct LiteSVM integration would require Ronchi to build and maintain an RPC and account-hydration layer that Surfpool already provides. `solana-test-validator` is a more manual alternative.

This mode can support exact post-transaction account reads within the fidelity of the loaded programs, accounts, runtime version, and external inputs. It adds setup and changes the product from RPC-history simulation to local transaction execution.

## Surfpool licensing and integration

The Surfpool repository declares Apache License 2.0 in its root `LICENSE`, GitHub metadata, and Cargo workspace metadata. The license permits use, modification, derivative works, sublicensing, commercial use, and redistribution.

Redistributing a modified fork requires retaining the Apache-2.0 license and applicable copyright, patent, trademark, and attribution notices and marking modified files. If a future upstream distribution adds a `NOTICE` file, relevant notices must also be carried into distributed derivative works. Apache-2.0 does not grant rights to Surfpool or Solana trademarks, product names, or logos beyond reasonable attribution describing the origin of the work.

Ronchi does not need a fork merely to work with Surfpool. The lower-maintenance integration is to treat an installed Surfpool binary or container as an external execution backend, start or connect to it through an explicit CLI mode, and keep the Ronchi fault and evidence gateway in front. A fork is justified only if required backend hooks cannot be added upstream or accessed through public RPC/control interfaces.

## Decision follow-up

Keep RPC lifecycle simulation as the default CLI mode. Surfpool is an optional managed external Execution Backend, not an initial fork. In Surfpool mode all application blockchain reads and writes route through the local Surfpool RPC; Ronchi does not mix Surfpool-mutated state with direct public Read-through. Do not implement protocol-specific balance, pool, tick, or fee transitions in Ronchi core.

## Sources

- [Solana `simulateTransaction`](https://solana.com/docs/rpc/http/simulatetransaction)
- [Agave RPC implementation](https://github.com/anza-xyz/agave/blob/master/rpc/src/rpc.rs)
- [LiteSVM](https://github.com/LiteSVM/litesvm)
- [Mollusk](https://github.com/anza-xyz/mollusk)
- [Solana test validator guide](https://solana.com/developers/guides/getstarted/solana-test-validator)
- [Surfpool](https://github.com/solana-foundation/surfpool)
- [Surfpool transaction account hydration](https://github.com/solana-foundation/surfpool/blob/main/crates/core/src/surfnet/locker.rs#L1980-L2024)
- [Surfpool executable ProgramData hydration](https://github.com/solana-foundation/surfpool/blob/main/crates/core/src/surfnet/remote.rs#L354-L396)
- [Surfpool Scenarios](https://github.com/solana-foundation/surfpool/blob/main/crates/core/src/scenarios/README.md)
