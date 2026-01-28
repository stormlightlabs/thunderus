# AGENTS.md

This document helps agents work effectively in the Thunderus codebase.

## Overview

Thunderus is a coding agent harness built in Rust.
It replicates the workflows of "Claude Code" and "Codex CLI" as a standalone,
provider-agnostic TUI tool.

### Key Design Principles

- **Harness > Chat**: TUI workbench, not conversational UI
- **Shell-First**: "Anything in the shell" is usable, but gated by approvals
- **Mixed-Initiative**: Seamless collaboration - agent pauses when you type/edit
- **Diff-First**: All edits are reviewable, reversible, and conflict-aware

## Project Structure

```sh
.
├── Cargo.toml                 # Workspace configuration
├── doc/
│   ├── ROADMAP.txt           # Implementation roadmap
│   ├── DESIGN.txt            # UI/UX design guide
│   ├── PROVIDERS.txt         # Provider adapter documentation
│   ├── QA.txt                # User flows and QA testing
│   └── REFS.txt              # External references
├── docs/                      # VitePress documentation site
└── crates/
    ├── agent/                # Agent orchestrator and event loop
    ├── cli/                  # Binary entry point
    ├── core/                 # Core types, memory, approval, session
    ├── providers/            # LLM adapters (GLM-4.7, Gemini)
    ├── skills/               # On-demand capability loading
    ├── store/                # SQLite FTS5 memory store
    ├── tools/                # Tool execution framework
    └── ui/                   # TUI components and rendering
```

### Quick Reference: What's in Each Crate

- **agent**: Agent orchestrator, event streaming, approval integration
- **cli**: Command-line interface (start, exec, status, completions), colored output
- **core**: Configuration, session (JSONL events), approval system, tiered memory, drift detection
- **providers**: GLM-4.7 and Gemini adapters with streaming, mock, replay, retry support
- **skills**: Skill discovery and loading from `.thunderus/skills/`
- **store**: SQLite-backed memory store with FTS5 full-text search
- **tools**: Tool registry, dispatcher, builtin tools (read, write, edit, glob, grep, shell, patch)
- **ui**: TUI with Ratatui (header, sidebar, transcript, footer), themes, syntax highlighting

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

## Key Technologies

- **Language**: Rust 2024
- **TUI Framework**: Ratatui
- **Terminal**: Crossterm
- **Syntax Highlighting**: syntect + owo_colors
- **Providers**: GLM-4.7 and Gemini
- **Storage**: JSONL event logs + Markdown views
- **Serialization**: serde, serde_json, toml, serde_yml

## Important Context for Agents

1. **Workspace-based**: Commands apply to all crates by default
2. **Provider-agnostic**: All provider interactions use types from `crates/providers`
3. **Approval-centric**: The approval system is the gatekeeper for all operations
4. **Shell-first with guardrails**: Tools and commands are gated by approval modes
5. **Safety-focused**: Approvals, sandboxes, and reversible operations are design goals
6. **Early stage**: Much of the code is foundation; implementation details may change

## When Working on This Codebase

- Always test across workspace: `cargo test`
- Use `just check` for complete, fast feedback
- Add dependencies to the appropriate crate's `Cargo.toml`
- Consult `doc/ROADMAP.txt` for implementation order and priorities
