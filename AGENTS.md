# thndrs

`thndrs` is a minimal coding agent built from explicit prompts, project
instructions, local tools, and durable sessions.

## Start here

- Read the [codebase tour](docs/src/content/docs/docs/internals/codebase.md) before
  changing module ownership or crate boundaries. Follow its links for the
  subsystem you change.
- Follow the [development workflow](docs/src/content/docs/docs/development/workflow.md)
  and [testing guide](docs/src/content/docs/docs/development/testing.md).
- For terminal interface work, load the
  [TUI design skill](.agents/skills/tui-design/SKILL.md). Use the
  [tmux TUI QA skill](.agents/skills/tmux-tui-qa/SKILL.md) when verification
  needs a real pseudo-terminal.

## Repository rules

The working tree is user-owned. Treat Git as read-only unless the user requests
another Git operation. The user may change Git state while agents work, so
track files changed by your own actions instead of relying on Git status.

Keep `thndrs-agent` provider-neutral and independent of filesystem discovery,
session persistence, terminal I/O, transport, and provider wire payloads. The
`thndrs` application owns those adapters. The TUI and ACP server are sibling
frontends over shared application code.

For Rust code (writing-rust & reviewing-rust skills):

- Use `//!` module docs and `///` docs for exported or important symbols.
- Use enums, structs, and newtypes for meaningful values.
- Keep parsing, policy, rendering projection, and validation pure where
  practical. Isolate filesystem, network, process, environment, and terminal
  effects.
- Return `Result` for recoverable failures. Production `unwrap`, `expect`,
  `panic!`, `todo!`, and `unimplemented!` need a documented invariant.
- Prefer concrete helpers to traits. Add traits only at real external or runtime
  boundaries.
- Test behavior, errors, state transitions, serialization, and side effects
  with deterministic fakes where practical.

## Verification

Run the narrowest relevant check first. For Rust changes, finish with:

```sh
cargo fmt
cargo clippy --workspace --fix --allow-dirty --allow-staged
cargo clippy --workspace
cargo test --workspace
```

Run `cargo clean` when build artifacts need pruning.

Changes to the documentation site require `bun run --cwd docs build`. Root-level
Markdown and files under `docs/internal` do not require a site build. Keep the
internals and development docs current when a change affects architecture,
runtime behavior, or contributor workflow.
