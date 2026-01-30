---
outline: deep
---

# Development Workflow

## Build, Test, Lint

```bash
cargo build
cargo build --release
cargo check
cargo test
cargo test -p thunderus-core
cargo clippy
cargo fmt
```

## Run

```bash
cargo run --bin thunderus
cargo run --bin thunderus -- --help
```

## Add a Tool

1. Implement `Tool` in `crates/tools/src/builtins/`.
2. Register in `ToolRegistry::with_defaults()`.
3. Add unit tests (same file or `tests/`).

## Add a Provider

1. Implement `Provider` in `crates/providers/src/`.
2. Handle provider-specific request/stream parsing.
3. Wire into CLI/provider selection.

## Add a Skill (Plugin)

Create `.thunderus/skills/my_skill/SKILL.md` with permissions and metadata, then add the
driver file (`main.lua`, `plugin.wasm`, or `run.sh`).

## Debugging

Tracing is enabled via `RUST_LOG`.

```bash
RUST_LOG=debug cargo run --bin thunderus
RUST_LOG=thunderus_agent=trace cargo run --bin thunderus
```

Session logs live at `.thunderus/sessions/*/session.jsonl`.

## Common Changes

### Modify the event schema

1. Update `Event` in `crates/core/src/session/events.rs`.
2. Update serialization and any materialized views.

### Change approval flow

1. Update `ApprovalGate` or `ApprovalProtocol` in `crates/core/src/approval.rs`.
2. Update UI approval handling in `crates/ui/src/event_handler/approval_mode.rs`.

### Add a UI component

1. Create component in `crates/ui/src/components/`.
2. Add state to `AppState` if needed.
3. Wire into render function in `crates/ui/src/app.rs`.
4. Handle input in appropriate event handler mode.
