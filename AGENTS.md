# thndrs

`thndrs` is a minimal coding agent. Its default behavior is an explicit prompt,
project instructions, local tools, and durable sessions.

## Development

The working tree is user-owned. Treat Git as read-only unless a user requests a Git
operation. The user may change Git state while agents work. Track files changed by
your own actions so do not use Git status as the sole record of agent work.

## Workspace

- `thndrs-agent`: provider-neutral agent loop, contracts, and context control.
- `thndrs`: CLI/TUI application and ACP server mode.

`thndrs-agent` is a reusable leaf library.

The `thndrs` application composes it with application adapters own filesystem discovery,
session persistence, terminal I/O, and transport, which stay in application adapters.

Provider wire payloads do not appear in public library APIs.

## Code Style & Quality

Module order:

1. constants
2. traits
3. enums and impls
4. structs and impls
5. exported functions
6. private functions
7. tests.

Use `//!` module docs and `///` docs for exported or important symbols.

- Use enums, structs, and newtypes for meaningful values.
- Keep parsing, policy, rendering projection, and validation pure where
  practical. Isolate filesystem, network, process, environment, and terminal
  effects.
- Return `Result` for recoverable failures. Production `unwrap`, `expect`,
  `panic!`, `todo!`, and `unimplemented!` need a documented invariant.
- Prefer concrete helpers to traits. Traits mark real boundaries such as
  storage, process execution, providers, clocks, or tool executors.
- Tests cover behavior, errors, state transitions, serialization, and side
  effects with deterministic fakes where possible.

## Checks

For Rust changes, run the narrowest relevant test, then:

```sh
cargo fmt
cargo clippy --workspace --fix --allow-dirty --allow-staged
cargo clippy --workspace
cargo test --workspace
```

Documentation in the doc site changes require `pnpm --dir docs build`.
Ensure documentation stays up to date as feature work is completed.

Root level & `docs/internal` files don't require a docs build.
