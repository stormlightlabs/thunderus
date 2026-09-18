---
name: features
last_updated: 2026-09-18
id: 01M2RFP6G4RZZ4BRH6HSYP29BM
---

# Feature plans

Each feature track has one directory holding its design:

```text
features/<feature-name>/plan.md
```

A completed track stays here until it ships; its durable decisions can then
move to `../archive/`.

Write the plan with `/spec-ify`, then file the work with `/decomp`.

## Sequence

| Order | Feature                                | Current boundary                                                                                                         |
| ----: | -------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
|     1 | [MCP](mcp/plan.md)                     | Project-server trust is complete. Resources, lifecycle controls, TUI management, configuration, and distribution remain. |
|     2 | [Lifecycle hooks](hooks/plan.md)       | Builds on the shared lifecycle model; project hooks also depend on project trust.                                        |
|     3 | [Skills](skills/plan.md)               | Owns its project activation rules, diagnostics, supply-chain policy, and packaged discovery.                             |
|     4 | [Providers](providers/plan.md)         | Adds native provider adapters and only then compatible endpoints and account-capacity work.                              |
|     5 | [Image prompts](image-prompts/plan.md) | Uses the provider capability model to route and validate image input.                                                    |

[TUI verification](tui-verification/plan.md) sits outside that sequence. It
changes how interface work is reviewed rather than what ships, so it blocks
nothing above and nothing above blocks it.

[Lndrs](lndrs/plan.md) sits outside it as well. It is an orchestration engine
in its own crate that consumes `thndrs` as a library, so nothing in the table
depends on it and it waits only on [TUI verification](tui-verification/plan.md)
for its interface.

[Transcript](transcript/plan.md) sits outside it too. It decides the
vocabulary the `## UI` items in `../BUGS.md` are cut from, and nothing in the
table depends on that vocabulary.

### Task Index

- [MCP tasks](mcp/tasks.md)
- [Lifecycle hooks tasks](hooks/tasks.md)
- [Skills tasks](skills/tasks.md)
- [Providers tasks](providers/tasks.md)
- [Image prompts tasks](image-prompts/tasks.md)

## v0.2

v0.2 covers MCP. The inline renderer has shipped and its completed feature
track has been removed. v0.2 also includes documented headless JSONL behavior
and the packaged ACP surface.
