---
outline: deep
---

# Getting Started

Thunderus is in active development. The TUI components, approval system, and tool
framework exist today, while some integrations (provider adapters, full TUI
wiring) are still in progress. Sections marked **Planned** describe upcoming work.

## Prerequisites

- A Rust toolchain with 2024 edition support (latest stable is recommended).
- `cargo` available on your PATH.

## Build and Run (Local)

```sh
cargo build
cargo run --bin thunderus
```

This builds the CLI and launches the current entry point. If you prefer a
prebuilt binary, run the compiled target directly:

```sh
./target/debug/thunderus
```

## Configure a Profile

Copy `config.example.toml` to `config.toml` and update the paths and provider
settings.

```sh
cp config.example.toml config.toml
```

At minimum, set `working_root` and select a provider profile. Provider adapters
are still evolving, so treat credentials and model configuration as **Planned**
until the adapter for your provider is wired in.

## First Session Walkthrough

1. Launch the CLI.
2. Open a workspace within your configured `working_root`.
3. Ask for a small change (e.g., "summarize README" or "find all TODOs").
4. Approve read-only commands when prompted.
5. Review diffs before accepting any edits.

## Planned: Provider Connections

Provider adapters (GLM and Gemini) have types and configuration schemas in place,
but runtime integration is still in progress. Until adapters are complete, treat
provider usage as **Planned** and rely on local-only or mock flows.

## Planned: Expanded TUI Coverage

The TUI runs from the CLI today, but some features (inspector wiring, deeper
navigation, and richer session tooling) are still in progress. Expect occasional
placeholder views as the UI expands.
