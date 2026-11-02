# Fully local open-source product

Ronchi will be a fully local open-source product that works without registration, an account, a licensing server, or any hosted control plane. Experiment execution, RPC traffic, signed transactions, Timeline evidence, and artifacts remain on developer-controlled infrastructure. This minimizes adoption and security barriers for financially sensitive systems and makes offline use a real product property rather than a limited fallback mode.

## Considered Options

- A mandatory hosted service was rejected because it would expose sensitive traffic and create an avoidable trust and availability dependency.
- A local data plane with a planned hosted dashboard was rejected from the current product definition because it would distort early architecture and positioning before the local tool proves its value.

## Consequences

- All essential workflows must function offline once required binaries and Fixtures are available locally.
- Authentication, billing, cloud synchronization, and hosted team collaboration are outside the product scope.
- The exact open-source license remains a separate decision.
