---
outline: deep
---

# Architecture

Thunderus is organized as a Cargo workspace with eight specialized crates.

## Crate Overview

```sh
crates/
├── agent/      # Orchestration loop
├── cli/        # Binary entry point
├── core/       # Configuration, sessions, memory, approvals
├── providers/  # LLM adapters (GLM, Gemini)
├── skills/     # On-demand capabilities
├── store/      # SQLite FTS5 memory store
├── tools/      # Tool framework and builtins
└── ui/         # TUI rendering
```

## Data Flow

User input enters through the TUI event handler, which dispatches to the agent
loop. The agent coordinates:

1. Provider calls for streaming LLM responses
2. Tool execution through the dispatcher
3. Memory retrieval for context augmentation
4. Approval gating for risky operations

Events flow back to the UI as `AgentEvent` variants (Token, ToolCall, ToolResult,
ApprovalRequest, MemoryRetrieval, Error, Done).

## Approval System

Three approval modes control what executes without confirmation:

- **ReadOnly**: Only read operations allowed
- **Auto**: Safe operations proceed; risky ones prompt
- **FullAccess**: All operations proceed

Tool risk is determined by the `CommandClassifier` in `thunderus-tools`.

## Session Storage

Sessions are recorded as append-only JSONL event logs. Each event has a
monotonic sequence number, timestamp, and typed payload. This format supports
replay, auditing, and trajectory inspection.

## Memory Tiers

Memory is organized into four tiers with different lifetimes:

- **Core**: Always-loaded project knowledge (FACTS, DECISIONS, PLAYBOOKS)
- **Semantic**: Entity-relationship facts and ADR-like decisions
- **Procedural**: Playbooks and repeatable workflows
- **Episodic**: Session recaps by date

The Memory Gardener consolidates, deduplicates, and validates memory over time.

## Provider Abstraction

The `Provider` trait defines a streaming interface. Implementations exist for
GLM-4.7 (OpenAI-compatible SSE) and Gemini (Google native JSON).
The adapter handles request/response format translation so the agent sees a uniform
`StreamEvent` stream.
