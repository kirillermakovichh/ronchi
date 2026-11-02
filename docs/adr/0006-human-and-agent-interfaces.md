# Human and agent interfaces over one local core

Ronchi will provide three first-class interfaces over the same local core and Lab Journal: a web dashboard for human investigation, structured CLI output for shell and CI use, and a built-in local MCP server for AI coding agents. Agents must not need to scrape the dashboard or ingest the complete Journal. Their normal workflow is summary-first, followed by filtered and paginated drill-down into selected evidence.

Token efficiency is part of the product contract. Default machine responses must spend context on causal facts, not presentation markup, repeated metadata, or unbounded logs.

Coding Agents may start and stop Labs, arm built-in Cases, and inspect bounded Journal views through explicit operations. Ronchi will not execute the backend or expose arbitrary shell execution; agents drive the System Under Test through their existing development environment.

## Considered Options

- A dashboard-only product was rejected because browser navigation is indirect, fragile, and context-expensive for coding agents.
- An MCP-only product was rejected because CLI is the most portable automation fallback and humans need a strong causal investigation surface.
- Separate implementations for UI, CLI, and MCP were rejected because they would create inconsistent evidence and semantics.

## Consequences

- All interfaces share stable Run, event, fault, and evidence identifiers.
- Machine responses are bounded, structured, versioned, filterable, and paginated.
- Agents can select fields as well as events, identifiers, and Timeline intervals so unrelated data is not returned.
- Compact summaries are distinct from raw evidence payloads.
- Complete application logs are not a default machine response; only explicitly selected bounded intervals may be attached or retrieved.
- Observation and Lab/Run control are separate, explicit capabilities; arbitrary process control is excluded.
- The MCP server is local, contains no model, and makes no cloud calls.
- Browser automation may test the dashboard but is not the normal agent integration.
