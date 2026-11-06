# README redesign brief

Status: implemented and verified; task 39 records the evidence. Human + agent presentation with a branded Ronchi parallel-band header.
The user requested a descriptive, attractive README with AI-native positioning, a running terminal example and meaningful dashboard screenshots.
The user wants text descriptions of alternatives, without mockups.
[Task 39](todo.md#39-readme-redesign) is the completion checklist.

## Goal and audience

A developer opening the repository should understand what Ronchi does, why it is useful and how to try it.
Show a concrete investigation result before describing internal architecture.
Make human dashboard use and coding-agent CLI/MCP access visible.
Keep the existing name, parallel-band identity and “Make hidden failures visible.” tagline.

## Required content

- A short product description grounded in the current Solana/local-first scope.
- One worked example: accepted submission, withheld response, later backend RPC actions and actual returned status.
- A real running-terminal example and current dashboard screenshots with explanatory captions.
- A short first-run path using implemented build/start/Demo commands.
- An agent section with real structured CLI examples and the local MCP entrypoint.
- A compact Case overview and Simulator/Surfpool distinction.
- Clear prototype status and links to maintained docs for deeper setup and limits.

AI-native means a first-class structured interface for coding agents over the same Journal as the dashboard.
It does not imply an embedded model, autonomous business-correctness evaluation or an application SDK requirement.
Do not introduce unimplemented installers, cloud dependencies, unsupported platform claims or synthetic benchmark/badge claims.

## Selected direction

The user selected **Human + agent** with the branded parallel-band header from the product-showcase alternative.
Present one Lab and one shared evidence history through two first-class interfaces: dashboard for people, structured CLI/MCP for coding agents.
The user authorized the specification, implementation plan and complete README/media work. Runtime behavior remains outside scope.

Approved narrative:

1. Ronchi brand, existing tagline, concise Solana/local-first descriptor and developer/agent audience.
2. A meaningful Workspace scene with one selected incident, then the two ways to investigate it.
3. One concrete response-loss example that connects applied Fault, backend follow-ups and actual RPC result.
4. A coding-agent workflow grounded in structured summaries and exact evidence references.
5. Running-terminal example and an implemented first-run path, with Demo and backend connection distinguished.
6. Compact fault/mode overview, prototype scope and links to maintained docs.

Keep the README in English. Lead with a real static Workspace scene. Place the terminal example lower. Do not create mockups.

## Media and presentation

Use the existing dark/coral Ronchi identity and readable screenshots at actual README display width.
Capture from a disposable Lab with a known workload; label simulated outcomes accurately.
Prefer a focused Observation detail plus a Workspace overview over a gallery of empty screens.
A short real terminal animation is optional; retain a readable still and copyable commands.
Use original product visuals and real output. Screenshots must not expose launch keys, credentials, personal paths or unrelated backend data.
Keep final referenced README media separate from temporary research and capture scratch files.
Use relative paths, descriptive alt text and captions; retain text explanations when media does not load.

## Implementation plan

1. Capture one response-loss incident in a disposable Simulator Lab through the release executable. Use the same incident for the dashboard and structured CLI examples. Stop only that exact Lab after capture.
2. Produce the original band header and real Workspace, Inspector and terminal media under `docs/assets/readme/`. Keep scratch files and research outside the repository.
3. Rewrite README around the approved narrative. Provide executable source-build, Demo, backend, Case and MCP examples. Link detailed contracts rather than duplicating them.
4. Verify commands against actual help/output. Inspect the rendered README and media at GitHub-like width. Check local links, image dimensions, credentials and whitespace. Record results in task 39.

Dependencies follow this order: verified incident → final media/copy → rendering and link checks.
The completion checklist remains in `todo.md`; this plan does not duplicate it.

## Acceptance and boundaries

A new reader can identify the product, see a meaningful incident and start a local Lab.
Both human and coding-agent workflows use real supported interfaces over one Journal.
The first visual is a real populated Workspace; the selected Observation has readable supporting detail.
The terminal example uses actual captured output; copyable commands remain in text.
Simulated finality is identified as simulated. RPC evidence does not claim business completion.
Relative links and media resolve. Images have descriptive alt text and contain no launch credentials or personal data.
Research and capture scripts are not product assets and remain outside tracked files.
Do not alter runtime behavior or stop user Labs. Commit only at the user’s explicit request. Do not publish.

Existing contracts remain authoritative: [overview](../docs/product/overview.md), [brand](../docs/product/positioning-and-brand.md), [dashboard](../docs/product/dashboard.md) and [interfaces](../docs/product/investigation-interfaces.md).
