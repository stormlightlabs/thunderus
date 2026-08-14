# Feature plans

Each feature track has one directory containing its design and unfinished
implementation work:

```text
features/<feature-name>/{plan,tasks}.md
```

Checked tasks describe completed work. A completed track stays here until it
ships; its durable decisions can then move to `../archive/`.

## Sequence

| Order | Feature                                        | Current boundary                                                                                                          |
| ----: | ---------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
|     1 | [Session context](session-context/plan.md)     | Complete but unreleased. Delivers durable session lifecycle, context inspection, usage accounting, and compaction policy. |
|     2 | [Runtime surfaces](runtime-surfaces/plan.md)   | Defines the provider-neutral run contract, then carries it through JSONL and packaged ACP.                                |
|     3 | [Trust and sandbox](trust-and-sandbox/plan.md) | Establishes shared authority, approval, and sandbox boundaries before more extension surfaces are added.                  |
|     4 | [MCP](mcp/plan.md)                             | Project-server trust is complete. Resources, lifecycle controls, TUI management, configuration, and distribution remain.  |
|     5 | [Lifecycle hooks](hooks/plan.md)               | Builds on the shared lifecycle model; project hooks also depend on project trust.                                         |
|     6 | [Skills](skills/plan.md)                       | Diagnostics, supply-chain policy, and packaged discovery depend on stable trust and runtime boundaries.                   |
|     7 | [Providers](providers/plan.md)                 | Adds native provider adapters and only then compatible endpoints and account-capacity work.                               |
|     8 | [Image prompts](image-prompts/plan.md)         | Uses the provider capability model to route and validate image input.                                                     |

### Task Index

- [Session context tasks](session-context/tasks.md)
- [Runtime surfaces tasks](runtime-surfaces/tasks.md)
- [Trust and sandbox tasks](trust-and-sandbox/tasks.md)
- [MCP tasks](mcp/tasks.md)
- [Lifecycle hooks tasks](hooks/tasks.md)
- [Skills tasks](skills/tasks.md)
- [Providers tasks](providers/tasks.md)
- [Image prompts tasks](image-prompts/tasks.md)

## v0.2

Will be cut **after runtime surfaces** so v0.2 becomes the first
release with a stable external run contract and packaged ACP surface.
