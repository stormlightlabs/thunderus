---
outline: deep
---

# Contributing

We welcome contributions! Do your best to make sure your contributions emphasize
reviewability, safety, and clear diffs.

## Build and Test

```sh
cargo build
cargo test
```

For faster feedback:

```sh
cargo check
```

Formatting and linting:

```sh
cargo fmt
cargo clippy
```

## Project Structure

- `crates/agent`: Agent orchestrator and event loop.
- `crates/cli`: CLI entry point.
- `crates/core`: Configuration, approvals, sessions, errors.
- `crates/providers`: Provider-neutral types.
- `crates/tools`: Tool execution framework.
- `crates/ui`: TUI components.
