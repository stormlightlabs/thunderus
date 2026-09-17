---
title: "Interactive TUI QA"
---

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

`-e` preserves ANSI colors and `-N` preserves trailing styled spaces. Use
bounded output, check picker open/close plus narrow and short resizes, then stop
the process and remove the session:

```sh
tmux send-keys -t "$qa_session":0.0 C-d
sleep 0.2
tmux send-keys -t "$qa_session":0.0 C-d || true
tmux kill-session -t "$qa_session" 2>/dev/null || true
```

## Session fixtures

Checks that need a populated transcript load a generated fixture instead of
sending a prompt, so the same check twice shows the same frame twice. The
generator constructs `SessionRecord` values and serializes them, which means a
change to the record enum breaks the build rather than a QA run:

```sh
cargo run --example session_fixtures -- target/tui-fixtures/sessions
tmux new-session -d -x 100 -y 30 -s "$qa_session" \
  "./target/debug/thndrs --session-dir target/tui-fixtures/sessions"
tmux send-keys -t "$qa_session":0.0 '/resume picker-open' Enter
```

One session per scenario, named for the scenario, so the `/resume` argument and
the check are the same word: `picker-open`, `streaming-mid-tool`,
`tool-output-truncated`, `permission-prompt`, `error`, `narrow-60-cols`,
`short-16-rows`, `no-color`, and one per built-in theme. Theme sessions come
from the `Theme` enum, so a theme added there adds a fixture.

The content is invented, so nothing generated needs a redaction pass, and
`target/` is already ignored, so nothing generated is committed.

Regenerate before each pass. Resuming a session appends to it, and a run also
writes its own session into the directory it was pointed at, so a fixture
directory carried over from the previous pass is no longer the transcript that
pass captured. `--ephemeral` is not the way around that: it refuses `/resume`.
