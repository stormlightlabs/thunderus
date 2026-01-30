---
outline: deep
---

# Architecture

## Crate Responsibilities

### core

Foundation types shared across the workspace.

- `error.rs`: unified `Error` enum
- `approval.rs`: `ApprovalGate`, `ApprovalProtocol`
- `classification.rs`: `ToolRisk`, `Classification`
- `memory/`: tiered memory system
- `session/`: JSONL event log + materialized views
- `task_context.rs`: current task tracking

### providers

LLM adapters implementing `Provider::stream_chat` (streaming tokens and tool calls).
Includes Glm, Gemini, Mock, Replay.

### tools

Tool execution framework.

- `tool.rs`: `Tool` trait
- `registry.rs`: tool registry
- `dispatcher.rs`: approval + execution routing
- builtins: read/write/edit/patch/glob/grep/shell, etc.

### agent

Orchestration between UI, providers, tools. `Agent::process_message()` emits
`AgentEvent` back to the UI.

### store

SQLite-backed memory with FTS5 full-text search. Implements `MemoryRetriever`.

### skills

Plugin system for extending tools. Drivers: shell, WASM, Lua. Discovers
`.thunderus/skills/` and enforces capability permissions.

### ui

TUI app built on Ratatui + Crossterm. Owns the event loop and renders the
transcript, approvals, and drift handling.

### cli

Binary entry point. Parses CLI args, loads config, boots the UI.

## Module Boundaries

| Boundary     | Rule                                                  |
| ------------ | ----------------------------------------------------- |
| core exports | Types only, no behavior that depends on higher crates |
| providers    | Knows core types, nothing about tools or UI           |
| tools        | Knows core types, nothing about providers or UI       |
| agent        | Knows core, providers, tools. No UI dependencies      |
| ui           | Knows everything, owns the event loop                 |
