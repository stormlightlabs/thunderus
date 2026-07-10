# thndrs

`thndrs` is a minimal coding agent. Its default behavior is an explicit prompt,
project instructions, local tools, and durable sessions with optional memory and
retrieval.

## Development

Feature plans and tasks live in `docs/internal/features/`. They provide product
context, acceptance criteria, and dependencies for work the user has selected.
They do not authorize starting or sequencing work on their own: the user drives
priorities, chooses the active feature or ticket, and decides when to move on.

The working tree is user-owned. Treat Git as read-only unless a user requests a
Git operation. Publishing crates, creating releases or tags, and changing the
existing `thndrs` package metadata require direct user approval.

Dependencies, public API commitments, package boundaries, permissions, session
formats, provider behavior, and work outside the assigned scope require
approval before implementation.

## Workspace direction

- `thndrs-agent`: provider-neutral agent loop and contracts.
- `thndrs-context`: context, memory, prompt-context, and session contracts.
- `thndrs`: CLI/TUI application.
- `thndrs-acp`: ACP server application.

`thndrs-agent` and `thndrs-context` remain independent leaf libraries. The two
application packages compose them. A new shared crate or a dependency between
the libraries needs a real consumer and approval.

Terminal I/O, ACP transport, direct filesystem/shell policy, and UI state stay
in application adapters.

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

Public documentation changes require `pnpm --dir docs build`.

Internal planning changes require a Markdown and diff review.
