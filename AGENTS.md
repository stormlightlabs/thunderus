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

`cargo clean` regularly to conserve disk space.

Documentation in the doc site changes require `bun run --cwd docs build`.
Ensure documentation stays up to date as feature work is completed.

Keep `docs/src/content/docs/docs/internals` and
`docs/src/content/docs/docs/development` current when changes affect system
architecture, runtime behavior, or contributor workflows.

### Interactive TUI QA

Use a dedicated tmux session for hands-on terminal checks. Build the current
binary first, launch it with a configured model, and keep the session dimensions
explicit so resize behavior is reproducible:

```sh
cargo build -p thndrs
qa_session="thndrs-qa-$$"
tmux new-session -d -x 100 -y 30 -s "$qa_session" \
  "./target/debug/thndrs --model <configured-model> --ephemeral --tick-rate-ms 100"
```

Send input and resize the same pane instead of typing into the active user pane:

```sh
tmux send-keys -t "$qa_session":0.0 'hello' Enter
tmux capture-pane -p -e -N -t "$qa_session":0.0 | tail -30
tmux resize-window -t "$qa_session":0 -x 80 -y 30
tmux capture-pane -p -e -N -t "$qa_session":0.0 | nl -ba | tail -30
tmux send-keys -t "$qa_session":0.0 '/model'
tmux capture-pane -p -e -N -t "$qa_session":0.0 | tail -30
tmux send-keys -t "$qa_session":0.0 Escape
```

`-e` preserves ANSI colors and `-N` preserves trailing styled spaces. For UI
review, save the visible pane and render it with Freeze:

```sh
mkdir -p .sandbox
capture=.sandbox/tui-smoke.ansi
screenshot=.sandbox/tui-smoke.png
tmux capture-pane -p -e -N -t "$qa_session":0.0 > "$capture"
freeze "$capture" -o "$screenshot"
```

Inspect the ANSI capture as well as the screenshot. If background-only cells
look correct in the capture but not in Freeze, use VHS or a native terminal
screenshot instead of changing the TUI to fit Freeze. Use bounded output, check
picker open/close plus narrow and short resizes, then stop the process and
remove the session:

```sh
tmux send-keys -t "$qa_session":0.0 C-d
sleep 0.2
tmux send-keys -t "$qa_session":0.0 C-d || true
tmux kill-session -t "$qa_session" 2>/dev/null || true
```

Root level & `docs/internal` files don't require a docs build.
