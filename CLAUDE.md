# thndrs

`thndrs` is a minimal coding agent. Its default behavior is an explicit prompt,
project instructions, local tools, and durable sessions.

## Writing

Use the `writing-docs` skill for every piece of prose you produce here. That
means documentation, commit messages, pull request bodies, issue bodies, review
comments, and the replies you write in chat. Load it before you write, not
after, and read `references/tells.md` when the prose is longer than a
paragraph.

Nothing checks this and nothing will: style is a judgement a script cannot
make, and a check that cannot be certain would only teach you to route around
it. The rule is blunt instead. Prose written without the skill reads like it,
and the tells it catalogues have each been shipped here at least once.

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

Keep `docs/src/content/docs/docs/internals` and
`docs/src/content/docs/docs/development` current when changes affect system
architecture, runtime behavior, or contributor workflows.

### Interactive TUI QA

Hands-on terminal checks run in a dedicated tmux session. See
[Interactive TUI QA](docs/src/content/docs/docs/development/tui-qa.md) for the
session setup, input and resize commands, and teardown.

Root level and `internal/` files do not require a docs build.

## Workflow

`thunderstorm` is the development loop for this repository. An issue and its
sub-issues are the work; a run is one pass over one of them, and a milestone
groups several without being an issue at all. See
`internal/thunderstorm.md` for statuses, review passes, and branch rules, and
`internal/guides/using-thunderstorm.md` for the operator's view of the loop. Skills and commands live in `.claude/`;
`.agents/skills` is a symlink to `.claude/skills` for harnesses that follow the
AGENTS.md convention. `AGENTS.md` is a symlink to this file.
