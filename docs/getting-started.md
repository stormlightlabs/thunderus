---
outline: deep
---

# Getting Started

Thunderus is in active development. The TUI, approval system, tool framework,
providers, and memory retrieval are wired end-to-end, with ongoing refinement as
the product stabilizes.

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

When you first run Thunderus, it will automatically create a config file if one
doesn't exist. The config file is searched in the following locations:

1. `.thunderus/config.toml` in the current directory
2. `~/.config/thunderus/config.toml` (or `$XDG_CONFIG_HOME/thunderus/config.toml`)

You can also specify a custom config path with the `--config` flag.

At minimum, set `working_root` and select a provider profile. The GLM and Gemini
adapters are wired into the runtime, so model and API credentials are used directly by
the CLI.

## First Session Walkthrough

1. Launch the CLI.
2. Open a workspace within your configured `working_root`.
3. Ask for a small change (e.g., "summarize README" or "find all TODOs").
4. Approve read-only commands when prompted.
5. Review diffs before accepting any edits.

## TUI Coverage

The TUI runs from the CLI and provides approvals, diffs, and transcript views.
Some panels and flows will continue to evolve, but core interaction loops are
available today.
