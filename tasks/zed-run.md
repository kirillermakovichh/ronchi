# Starting the remaining run in Zed

## Recommended session

Open the existing Ronchi project in a new agent thread.
Keep a model already available through the user's configured agent/provider.
Use **High** reasoning for the coordinator when that level is supported.
This is a task recommendation for dependency management, integration, and edge cases; it is not a measured speed guarantee.
Start with inherited worker settings so no global model configuration is required.

For the native Zed Agent, use the **Write** profile with reading, editing, terminal, and subagent tools available.
For Codex or another ACP agent, select reasoning in that agent's supported configuration controls.
Native Zed profile and subagent settings do not automatically configure an external agent.
Do not change subscriptions, credentials, global tool permissions, or agent-native config merely to start this project run.

At preparation, the local CLI reports Zed 1.6.3.
The user settings register `codex-acp` and `claude-acp`; the active thread's agent/model was not verified.
Current documentation may describe features absent from that installed version.
The coordinator checks actual delegation tools before dispatch and uses the documented fallback if needed.
The project cap of three workers is a chosen resource policy, not a universal Zed limit.

## Project setup

Root `AGENTS.md` remains the canonical instruction entry point.
No extra `.rules` file shadows it.
No Zed global settings were changed during preparation.
The preparation session did not start workers, create implementation worktrees, or implement tasks 15–24.

Read [remaining-implementation-run.md](remaining-implementation-run.md) for execution policy.
Use [parallel-run-state.md](parallel-run-state.md) for assignment and integration state.
Use [todo.md](todo.md) for acceptance and completion.

## Paste into the new chat

```text
Продолжи Ronchi в текущем репозитории.
Прочитай AGENTS.md и tasks/remaining-implementation-run.md.
Выполни оставшиеся задачи 15–24 автономно по tasks/todo.md.
Ты координатор. Я явно разрешаю субагентов и параллельную реализацию независимых веток.
Проверь доступность инструментов и используй до трёх исполнителей с изоляцией по runbook.
Если delegation недоступен, продолжай последовательно без выдуманных субагентов.
Локальные коммиты и интеграция разрешены. Remote не добавляй, ничего не публикуй.
Сохраняй прогресс в tasks/parallel-run-state.md и не останавливайся после каждого пункта.
Принимай обратимые решения самостоятельно. Не составляй новый общий план вместо реализации.
Не закрывай непроверенные критерии и внешнюю валидацию фиктивными результатами.
Начни с проверки текущего состояния и первой волны из runbook.
```

If a runtime-enforced session boundary requires another thread, use the same text and add that the existing ledger is the resume point.
The ledger, actual worktrees, and Git status identify unfinished work; no new task-by-task handoff is required.

## Official capability references

Checked on 2026-11-03:

- [Zed tools](https://zed.dev/docs/ai/tools): native `spawn_agent` and runtime-dependent tool availability.
- [Zed agent profiles](https://zed.dev/docs/ai/agent-profiles): Write versus read-only profiles.
- [Zed agent settings](https://zed.dev/docs/ai/agent-settings#subagent-models): native subagent settings and inheritance.
- [Zed external agents](https://zed.dev/docs/ai/external-agents): ACP configuration ownership.
- [Zed project instructions](https://zed.dev/docs/ai/instructions): project `AGENTS.md` loading and precedence.
- [Official OpenAI subagent documentation](https://learn.chatgpt.com/docs/agent-configuration/subagents): explicit delegation and reasoning choices.

These references establish capabilities and configuration boundaries.
The task ordering, concurrency cap, and High recommendation are Ronchi workflow choices.
