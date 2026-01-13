# AGENTS.md

This document helps agents work effectively in the Thunderus codebase.

## Project Overview

Thunderus is a high-performance coding agent harness built in Rust.
It aims to replicate the rigorous workflows of "Claude Code" and "Codex CLI" as a
standalone, provider-agnostic TUI tool.

**Key Design Principles**:

- **Harness > Chat**: TUI workbench, not conversational UI
- **Shell-First**: "Anything in the shell" is usable, but gated by approvals
- **Mixed-Initiative**: Seamless collaboration - agent pauses when you type/edit
- **Diff-First**: All edits are reviewable, reversible, and conflict-aware

For detailed implementation roadmap, see `doc/ROADMAP.txt`.

## Project Structure

```sh
.
├── Cargo.toml                 # Workspace configuration
├── doc/
│   ├── ROADMAP.txt           # Implementation roadmap
│   └── user-flow.txt         # User flows and testing guide
└── crates/
    ├── agent/                # Agent orchestrator and event loop
    ├── cli/                  # Binary entry point
    ├── core/                 # Core types and infrastructure
    ├── providers/            # Provider-neutral types
    ├── store/                # Session storage (placeholder)
    ├── tools/                # Tool execution framework
    └── ui/                   # TUI library (components ready, not integrated)
```

### Quick Reference: What's in Each Crate

- **agent**: Agent orchestrator, event streaming, approval integration
- **cli**: Command-line interface (start, exec, status), colored output, config loading
- **core**: Configuration, session management, approval system, error handling
- **providers**: Provider types and traits (adapters not implemented)
- **store**: Session storage placeholder
- **tools**: Tool execution framework, registry, dispatcher, command classification
- **ui**: TUI components (header, sidebar, footer, transcript), event handling, syntax highlighting

## Build and Development Commands

```bash
# Build everything
cargo build

# Run the CLI
cargo run --bin thunderus

# Run all tests
cargo test

# Run tests for a specific crate
cargo test -p thunderus-core

# Quick check without building
cargo check

# Format code
cargo fmt

# Lint code
cargo clippy
```

## Development Status

**What's Done**:

- ✅ Workspace scaffold and module structure
- ✅ Configuration system (TOML-based profiles)
- ✅ Session management (JSONL event logs)
- ✅ Approval protocol and gate system
- ✅ Tool execution framework (Tool trait, registry, dispatcher)
- ✅ Command classification with reasoning
- ✅ Provider-neutral types
- ✅ Agent orchestrator with event streaming
- ✅ TUI components (header, sidebar, footer, transcript)
- ✅ Interactive input handling and approval UI
- ✅ Syntax highlighting for code blocks

**What's Next** (see ROADMAP.txt for full details):

- ⏳ TUI integration with CLI (components ready, not connected)
- ⏳ Provider adapters (GLM-4.7, Gemini - types ready, adapters pending)
- ⏳ Core agent loop integration
- ⏳ Text processing tools (Grep, Glob, Read, Edit)
- ⏳ Diff-first editing and git integration

## Key Technologies

- **Language**: Rust 2024
- **TUI Framework**: Ratatui (components implemented)
- **Terminal**: Crossterm for event handling
- **Syntax Highlighting**: syntect + owo_colors
- **Providers**: GLM-4.7 and Gemini (types ready, adapters pending)
- **Storage**: JSONL event logs + Markdown views (planned)
- **Serialization**: serde, serde_json, toml

## Code Conventions

- **Crate names**: kebab-case (e.g., `thunderus-core`)
- **Binary name**: `thunderus`
- **Tests**: Use `#[cfg(test)] mod tests` blocks
- **Test pattern**: `use super::*` at top of test blocks

## Important Context for Agents

1. **Workspace-based**: Commands apply to all crates by default
2. **Provider-agnostic**: All provider interactions use types from `crates/providers`
3. **Approval-centric**: The approval system is the gatekeeper for all operations
4. **Shell-first with guardrails**: Tools and commands are gated by approval modes
5. **Safety-focused**: Approvals, sandboxes, and reversible operations are design goals
6. **Early stage**: Much of the code is foundation; implementation details may change

## Dependencies

**Core crates depend on**:

- `serde` (derive) - serialization
- `serde_json` - JSON handling
- `toml` - configuration
- `chrono` - timestamps
- `thiserror` - error handling

## When Working on This Codebase

- Always test across workspace: `cargo test`
- Use `cargo check` for faster feedback
- Add dependencies to the appropriate crate's `Cargo.toml`
- Consult `doc/ROADMAP.txt` for implementation order and priorities
- The approval system (`thunderus_core::approval`) is ready for integration
