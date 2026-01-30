---
outline: deep
---

# Development

Thunderus is a TUI-first, provider-agnostic agent harness in Rust. The UI owns the
runtime loop; the agent orchestrates model calls, tools, approvals, and memory.
Start here, then drill into the deeper docs.

## Start Here

- System overview: [System](/concepts/system)
- Architecture map: [Architecture](/development/architecture)
- Data flow: [Data Flow](/development/data-flow)
- Patterns: [Patterns](/development/patterns)
- Dev workflow: [Workflow](/development/development)

## Quick Start

```bash
cargo build
cargo run --bin thunderus
cargo test
cargo test -p thunderus-core
```

## Crate Map (dependency order)

```text
core       # Types, errors, approval, memory, session
  ↑
providers  # LLM adapters
tools      # Tool trait, registry, builtins
store      # SQLite FTS5 memory persistence
skills     # Plugin system
  ↑
agent      # Orchestration loop, event streaming
  ↑
ui         # TUI rendering, event handling
  ↑
cli        # Binary entry point
```

Lower crates have no knowledge of higher crates. Cross-cutting concerns live in
`core`.

## Key Entry Points

- CLI main: `crates/cli/src/main.rs`
- TUI app: `crates/ui/src/app.rs`
- Agent loop: `crates/agent/src/agent.rs`
- Provider trait: `crates/providers/src/provider.rs`
- Tool trait: `crates/tools/src/tool.rs`
- Approval gate: `crates/core/src/approval.rs`
