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
