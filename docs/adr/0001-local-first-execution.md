# Pre-production-only local execution

Ronchi will run reliability Cases only in pre-production environments: local development, CI, and isolated staging. It must never sit in a production application's RPC traffic path. The primary product uses the stateful RPC Simulator defined by ADR-0011 and may use the read-only upstream defined by ADR-0012; it performs no public-network writes and needs no disposable validator.

## Consequences

- Reports must clearly identify simulated transaction states and avoid presenting them as real chain execution.
- Public RPC endpoints are permitted only as explicitly configured Read Upstreams behind the write firewall from ADR-0012.
- Production deployment and passive production traffic capture are unsupported rather than hidden behind an override.

