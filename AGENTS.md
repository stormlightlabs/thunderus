# AGENTS.md

This document helps agents work effectively in the Thunderus codebase.

## Project Overview

Thunderus is a high-performance coding agent harness built in Rust. It aims to replicate the rigorous workflows of "Claude Code" and the "Codex CLI" as a standalone, provider-agnostic TUI tool.

**Key Design Principles**:

- **Harness > Chat**: TUI workbench, not conversational UI
- **Shell-First**: "Anything in the shell" is usable, but gated by approval modes
- **Mixed-Initiative**: Seamless collaboration - agent pauses when you type/edit
- **Diff-First**: All edits are reviewable, reversible, and conflict-aware

For detailed implementation roadmap, see `doc/ROADMAP.txt`.

## Project Structure

This is a Cargo workspace with the following crates:

```sh
.
├── Cargo.toml                 # Workspace configuration
├── README                     # Project overview
├── doc/
│   ├── ROADMAP.txt           # Detailed implementation roadmap
│   └── art.txt
└── crates/
    ├── cli/                  # Binary entry point (thunderus)
    ├── ui/                   # TUI library (thunderus_ui)
    ├── core/                 # Core library (thunderus_core)
    ├── tools/                # Tools library (thunderus_tools)
    ├── providers/            # Providers library (thunderus_providers)
    └── store/                # Store library (thunderus_store)
```

### Crate Responsibilities

- **cli**: Binary entry point, command-line interface
- **ui**: TUI implementation using Ratatui (planned)
- **core**: Core types, provider harness, agent loop
- **tools**: Tool implementations (file operations, shell, etc.)
- **providers**: LLM provider adapters (GLM-4.7, Gemini)
- **store**: Session storage, JSONL event logs, Markdown views

## Build and Development Commands

```bash
# Build the entire workspace
cargo build

# Build with optimizations
cargo build --release

# Run the CLI binary
cargo run --bin thunderus

# Run tests across all crates
cargo test

# Run tests for a specific crate
cargo test -p thunderus-core
cargo test -p thunderus-cli

# Run tests with output
cargo test -- --nocapture

# Check code without building
cargo check

# Format code
cargo fmt

# Run clippy linter
cargo clippy

# Update dependencies
cargo update
```

## Rust Configuration

- **Edition**: Rust 2024
- **Workspace resolver**: Version 2

Current workspace members in `Cargo.toml`:

```toml
[workspace]
resolver = "2"
members = ["crates/cli", "crates/core", "crates/providers", "crates/store", "crates/tools", "crates/ui"]
```

## Dependencies

Currently active dependencies (as of initial scaffold):

**thunderus-core**:

- `serde = "1.0.228"` (features: derive)
- `toml = "0.9.11"`

All other crates currently have no external dependencies.

## Code Patterns and Conventions

### Crate Names

- Workspace crates use kebab-case directory names (`core`, `ui`, etc.)
- Package names in `Cargo.toml` follow `thunderus-{crate_name}` pattern
- Binary name: `thunderus` (from `crates/cli`)

### Module Structure

Standard Rust patterns apply:

- Library code in `src/lib.rs`
- Tests in nested `#[cfg(test)] mod tests` blocks
- Use `use super::*` to import parent module items in tests

### Testing Pattern

All crates follow the standard Rust testing convention:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
```

## Development Status

This project is in early scaffold phase. See `doc/ROADMAP.txt` for the complete implementation plan covering:

1. Repo Scaffold + Config + Session Store
2. Provider Harness + Tool-Calling Core Loop
3. TUI Harness (Composer UX, Keybinds, Panels)
4. Approvals + Sandboxing (Shell-First, Still Safe)
5. Diff-First Editing + Git Integration
6. Markdown + JSONL Hybrid VCS
7. Mixed-Initiative Collaboration
8. Extensibility + Hardening

## Planned Key Technologies

Based on the roadmap:

- **TUI**: Ratatui for terminal interface
- **Providers**: GLM-4.7 and Gemini adapters
- **Storage**: JSONL event logs + Markdown materialized views
- **Diff**: Git apply-based patch system
- **Integration**: Model Context Protocol (MCP) for extensibility

## Important Context for Agents

1. **Workspace-based**:
   This is a Cargo workspace - commands apply to all crates by default
2. **Early stage**:
   Code is mostly scaffold - expect placeholder implementations
3. **Provider-agnostic design**:
   All provider interactions should go through the abstraction layer in `crates/providers`
4. **Shell-first philosophy**:
   Tool implementations should be shell-gated with approval modes
5. **Diff-first editing**:
   File edits should generate and apply patches, not raw writes (with safe escape hatch)
6. **Mixed-initiative**:
   The TUI design supports user interrupt and reconciliation at any point

## File Organization Notes

- The `.agent/` directory is planned for repo-local session storage (not yet implemented)
- Configuration will use `config.toml` with profiles (not yet implemented)
- Session logs will use JSONL format with materialized Markdown views (not yet implemented)

## When Working on This Codebase

- Always test across workspace: `cargo test`
- Use `cargo check` for faster feedback during development
- Follow the workspace structure - add dependencies to the appropriate crate's `Cargo.toml`
- Consult `doc/ROADMAP.txt` for context on feature implementation order
- The project emphasizes safety: approvals, sandboxes, and reversible operations are core to the design
