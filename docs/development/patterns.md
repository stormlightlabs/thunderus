---
outline: deep
---

# Patterns and Conventions

## Errors

Single `Error` enum in `core/src/error.rs`. Subsystems map errors into the shared type;
CLI layers add context.

Blocked commands are modeled separately (e.g., `BlockedCommandError`) to preserve
user-facing intent.

## Builders

Builders are used for complex configuration (e.g., `ChatRequest`, `ApprovalRequest`).
Expect `build()` to validate and return `Result<T>`.

## Trait Objects for Extensibility

Extension points are trait objects passed through `Arc`:

- `Arc<dyn Tool>`
- `Arc<dyn Provider>`
- `Arc<dyn ApprovalProtocol>`
- `Arc<dyn MemoryRetriever>`

## Streaming

Providers return `Pin<Box<dyn Stream<Item = StreamEvent> + Send>>` so the agent can
consume SSE, NDJSON, mock, or replay streams behind a common interface.

## Session Immutability

Session events are append-only JSONL. Never mutate historical events; append new
events for state changes.

## Workspace Boundaries

Tools must ensure paths stay within workspace roots before operating on files.

## Naming

- Traits: nouns (`Provider`, `Tool`, `ApprovalProtocol`)
- Implementations: prefix + noun (`GlmProvider`, `ShellTool`)
- Events: `ToolCall` vs `ToolResult`

## Testing

- Mock provider for deterministic agent tests
- Replay provider for regression tests
- `insta` snapshots for UI
- Integration tests in `tests/`
