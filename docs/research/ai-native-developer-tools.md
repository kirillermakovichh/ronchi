# AI-Native Developer-Tool Research

Research note. Observations and recommendations below are evidence and design context, not implementation verification. Current interface and lifecycle decisions are defined by [ADR-0006](../adr/0006-human-and-agent-interfaces.md), [ADR-0015](../adr/0015-persistent-lab-first.md), and the [developer workflows](../product/workflows.md).

This research asks whether a localhost dashboard remains useful when AI coding agents participate in most development workflows. The conclusion is that a dashboard remains valuable for human causal investigation, but a dashboard-only product is not AI-native. Agents need stable machine-oriented access to the same underlying evidence.

## Observed patterns

### Local MCP through stdio

Microsoft Dev Proxy publishes an MCP server for VS Code, Copilot CLI, Claude Code, Cursor, and other MCP-compatible assistants. Its tools expose domain operations and contextual guidance instead of asking the agent to manipulate the Dev Proxy UI. Local setup uses `stdio`.

The official MCP SDKs describe `stdio` as the transport for local, process-spawned integrations. This fits Ronchi's no-registration, no-cloud boundary.

Sources: [Dev Proxy MCP server](https://learn.microsoft.com/en-us/microsoft-cloud/dev/dev-proxy/how-to/use-mcp-server), [MCP TypeScript SDK stdio transport](https://ts.sdk.modelcontextprotocol.io/v2/serving/stdio).

### Dashboard and agent API coexist

Grafana retains dashboards for people while exposing observability capabilities through an open-source MCP server. It supports selective toolsets and read-only operation, which reduces context overhead and limits agent authority.

Source: [Grafana MCP](https://grafana.com/docs/grafana/latest/ai-tools/mcp/), [enable and disable tools](https://grafana.com/docs/grafana-cloud/ai-tools/mcp-servers/oss-mcp/configure/enable-and-disable-tools/).

### Structured non-interactive output is a baseline

Claude Code supports JSON and streaming JSON output in non-interactive mode. Codex documents bounded non-interactive workflows with structured output. A developer tool that only emits human-formatted terminal text creates unnecessary parsing work for agents and CI.

Sources: [Claude Code CLI](https://docs.anthropic.com/en/docs/claude-code/cli-usage), [Codex as a platform](https://developers.openai.com/blog/codex-as-a-platform).

### Browser automation is possible but indirect

Playwright MCP lets agents interact with pages using structured accessibility snapshots rather than pixels alone. This makes dashboards technically accessible to agents, but it remains an indirect interface: the agent must navigate presentation state instead of querying Ronchi concepts directly.

Source: [Playwright MCP](https://playwright.dev/docs/getting-started-mcp).

### Tool scope and read-only access matter

The official GitHub MCP server supports selected toolsets and read-only mode. Grafana follows a similar pattern. AI-native tools should expose bounded domain operations and distinguish observation from mutation.

Source: [GitHub MCP server configuration](https://github.com/github/github-mcp-server/blob/main/docs/server-configuration.md).

## Implications for Ronchi

### One evidence model, several clients

Dashboard, CLI, and MCP should be clients of one local control and evidence model. They must not calculate different causal relationships or Verdicts.

### Progressive disclosure controls token cost

An agent should first receive a compact Lab or Investigation summary containing deterministic Findings, evidence references, identifiers, phase boundaries, faults, evidence gaps, and high-level counts. It then requests an Evidence Slice with only the fields, event types, identifiers, or Timeline interval needed for the current question. Full trace dumps are opt-in.

Required properties include:

- stable identifiers and versioned schemas;
- server-side filtering and pagination;
- server-side field selection;
- bounded default response sizes;
- summaries separate from raw payloads;
- deterministic ordering;
- explicit truncation and continuation cursors;
- machine-readable errors and evidence completeness.

### Domain tools beat raw logs

Useful agent operations are concepts such as listing Cases, inspecting a Run, querying a Timeline, retrieving a specific RPC exchange, comparing Runs, and exporting a reproduction. A generic `get_logs` tool would force the agent to reconstruct domain meaning and waste tokens.

The compact result should include deterministic Findings rather than forcing the agent to rediscover known causal patterns from low-level events. Each Finding must carry evidence references so the agent can request only the facts it needs to verify or continue the investigation.

### Bounded control makes the workflow autonomous

A Coding Agent should be able to start and stop Labs, select built-in Cases, execute Runs, and inspect their evidence without asking a human to operate the dashboard. These controls belong to the Ronchi domain and can remain tightly bounded. Arbitrary shell execution does not: the agent already has a terminal for starting the application or triggering its Workload, and duplicating that capability would increase risk and product scope.

### Ronchi does not embed an AI model

Ronchi provides structured facts and control operations. The developer's chosen Codex, Claude Code, Cursor, Copilot, or other agent performs reasoning. This preserves the fully local product boundary and avoids model-provider coupling.

## Recommended product surface

```text
Ronchi Core and Evidence Store
    |-- localhost web dashboard: human exploration
    |-- CLI text/JSON/NDJSON: shell, scripts, and CI
    |-- local MCP over stdio: coding agents
    `-- local artifacts: durable inspection and replay
```

This is a stronger product than either a dashboard-only tool or an MCP wrapper around raw logs: both human and agent receive a purpose-built view of the same causal evidence.
