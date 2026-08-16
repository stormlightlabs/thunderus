# Feature plans

Each feature track has one directory containing its design and unfinished
implementation work:

```text
features/<feature-name>/{plan,tasks}.md
```

Checked tasks describe completed work. A completed track stays here until it
ships; its durable decisions can then move to `../archive/`.

## Sequence

| Order | Feature                                | Current boundary                                                                                                         |
| ----: | -------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
|     1 | [MCP](mcp/plan.md)                     | Project-server trust is complete. Resources, lifecycle controls, TUI management, configuration, and distribution remain. |
|     2 | [Lifecycle hooks](hooks/plan.md)       | Builds on the shared lifecycle model; project hooks also depend on project trust.                                        |
|     3 | [Skills](skills/plan.md)               | Owns its project activation rules, diagnostics, supply-chain policy, and packaged discovery.                             |
|     4 | [Providers](providers/plan.md)         | Adds native provider adapters and only then compatible endpoints and account-capacity work.                              |
|     5 | [Image prompts](image-prompts/plan.md) | Uses the provider capability model to route and validate image input.                                                    |

### Task Index

- [MCP tasks](mcp/tasks.md)
- [Lifecycle hooks tasks](hooks/tasks.md)
- [Skills tasks](skills/tasks.md)
- [Providers tasks](providers/tasks.md)
- [Image prompts tasks](image-prompts/tasks.md)

## v0.2

Includes documented headless JSONL behavior and the packaged ACP surface.
