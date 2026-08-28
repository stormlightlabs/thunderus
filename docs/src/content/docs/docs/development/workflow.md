---
title: "Development Workflow"
---

## Formatting

Use `cargo fmt` before submitting code changes.

### Testing

Run the full test suite:

```sh
cargo test
```

For snapshot-specific work, run:

```sh
cargo insta test
```

## Snapshots

Use `cargo insta review` to inspect changed TUI snapshots and accept only
intentional visual changes.

## Debugging

Use `--print-prompt` to inspect prompt assembly without making a provider call.

## Checks

### TUI

For UI work, check both normal and narrow snapshot states. When possible, also
run the TUI in a real terminal to inspect color and spacing.

### Release

Before release work, run formatting, tests, snapshots, and package checks.

## Multiplexer-Assisted Development

Use a terminal multiplexer when a task benefits from keeping an orchestrator, a
coding agent, and a long-running command visible at the same time. Herdr is the
preferred tool for agent work because it reports agent lifecycle state. tmux
and Zellij are useful fallbacks for ordinary terminal processes.

The multiplexer owns terminal layout and process visibility. The orchestrating
agent handles task selection, delegation, review, and the decision to continue.

### Working Layout

One orchestrator pane and one worker pane are enough for most tasks. Add a third
pane only for a server, focused test watcher, or another useful long-running
process.

| Pane    | Typical responsibility                                       |
| ------- | ------------------------------------------------------------ |
| Main    | Orchestrate, inspect changes, and make integration decisions |
| Worker  | Implement one bounded change or investigate one question     |
| Runtime | Run a server, focused tests, logs, or a reproducible failure |

Do not fill every available pane. Multiple agents, builds, language servers,
and test processes can make the machine unresponsive. Start with one worker and
add concurrency only for independent work when the machine has capacity.

### Operating Rules

- Give each worker one deliverable with its working directory, constraints,
  expected result, and focused verification command.
- Keep one writer in a shared checkout at a time. Give parallel writers separate
  worktrees and non-overlapping ownership.
- Treat the working tree as user-owned. Do not discard, overwrite, commit, or
  publish changes unless the user explicitly requests it.
- Match the model and effort level to the task. Use Terra at high effort for
  bounded implementation and Luna at xhigh for difficult diagnosis,
  architecture, or final review when the extra reasoning is justified.
- Ask workers for concise results. Read recent output instead of repeatedly
  capturing full transcripts, and avoid polling a completed process.
- Run focused checks before broad suites. Do not run duplicate builds or test
  suites in several panes.
- Stop background commands that no longer serve the task. Close idle panes you
  created, but never close someone else's pane without asking.
- Use pane or agent identifiers for automation. Visual focus belongs to the
  human and can change at any time.

### Agent Loop

1. Inspect the workspace and define one deliverable.
2. Open one worker pane without stealing focus and record its ID.
3. Give the worker the deliverable, constraints, and focused check.
4. Wait for useful output, then review the files it changed.
5. Run the smallest check that proves the change.
6. Reuse the worker when its context helps; otherwise close the pane.

The user may change Git state during the task. Track files from tool calls and
worker reports. Use Git only as read-only supporting evidence.

### Herdr

Use `agent` commands for recognized agents and `pane` commands for other
processes. Verify control first:

```sh
test "${HERDR_ENV:-}" = 1
```

```sh
herdr --help
herdr pane --help
herdr agent --help
```

```sh
herdr pane split --current --direction right --cwd "$PWD" --no-focus
herdr agent start worker --kind codex --pane w1:p2
herdr agent prompt worker "Fix the failing parser test. Keep the change local and run the focused test." --wait --timeout 120000
```

```sh
herdr agent get worker
herdr agent read worker
herdr agent wait worker --timeout 120000
```

For ordinary commands:

```sh
herdr pane run w1:p3 -- cargo test -p thndrs-agent parser
herdr pane read w1:p3 --source recent-unwrapped
```

Inspect `blocked`; do not treat `unknown` as complete. If alternate-screen
history is missing, ask the worker for a short result.

### tmux

tmux does not report agent state. Capture the pane ID and bounded output.

```sh
worker_pane=$(tmux split-window -h -d -c "#{pane_current_path}" -P -F '#{pane_id}')
tmux send-keys -t "$worker_pane" 'codex' Enter
```

```sh
tmux send-keys -t "$worker_pane" 'Run the focused parser test and summarize the result.' Enter
tmux capture-pane -p -t "$worker_pane" -S -200
tmux kill-pane -t "$worker_pane"
```

#### Interactive TUI checks

A TUI needs a real pseudo-terminal for input timing, cursor placement, native
scrollback, and resize events. A dedicated tmux session provides that terminal
without taking over the developer's active pane, and lets you send keys, change
the dimensions, and inspect bounded captures from one repeatable scenario. Use
`--ephemeral` for these checks so the QA run does not add session artifacts.

Use a named, dedicated session and keep the size explicit so a resize can be
repeated and compared:

```sh
cargo build -p thndrs
qa_session="thndrs-qa-$$"
tmux new-session -d -x 100 -y 30 -s "$qa_session" \
  "./target/debug/thndrs --model <model> --ephemeral --tick-rate-ms 100"
```

Exercise the state transition in the same pane:

```sh
tmux send-keys -t "$qa_session":0.0 'hello' Enter
sleep 2
tmux capture-pane -p -e -N -t "$qa_session":0.0 | tail -30
tmux resize-window -t "$qa_session":0 -x 80 -y 30
tmux capture-pane -p -e -N -t "$qa_session":0.0 | nl -ba | tail -30
tmux send-keys -t "$qa_session":0.0 '/model'
tmux capture-pane -p -e -N -t "$qa_session":0.0 | tail -30
tmux send-keys -t "$qa_session":0.0 Escape
```

Check streaming output, word wrapping, stale rows, composer anchoring, picker
open/close, and a short-height resize. `capture-pane -e` keeps ANSI colors;
`-N` keeps trailing styled spaces that affect full-cell backgrounds. Bound text
inspection with `tail` or `-S` rather than dumping the whole pane.

For visual review, capture the complete visible frame and render it with
[Freeze](https://github.com/charmbracelet/freeze):

```sh
mkdir -p .sandbox
capture=.sandbox/tui-smoke.ansi
screenshot=.sandbox/tui-smoke.png
tmux capture-pane -p -e -N -t "$qa_session":0.0 > "$capture"
freeze "$capture" -o "$screenshot"
```

Keep the raw ANSI capture until review is complete. Compare it with the PNG so
a screenshot-renderer defect is not mistaken for a TUI defect. In particular,
if background-only cells are present in the raw capture but broken in Freeze,
use VHS or a native terminal screenshot for that review.

Stop the application and remove the session when the check is complete:

```sh
tmux send-keys -t "$qa_session":0.0 C-d
sleep 0.2
tmux send-keys -t "$qa_session":0.0 C-d || true
tmux kill-session -t "$qa_session" 2>/dev/null || true
```

Send text and `Enter` separately for programs with their own input editor. Do
not synchronize agent panes.

### Zellij

Zellij provides named panes and layouts but does not report agent state.

```sh
zellij action new-pane --direction right --cwd "$PWD" --name tests -- cargo test -p thndrs-agent parser
```

```sh
zellij action list-panes
zellij action send-keys --pane-id terminal_2 'Ctrl c'
zellij action close-pane --pane-id terminal_2
```

`close-pane` without an ID closes the focused pane. Avoid synchronized input.

### Choosing a Tool

| Need                                | Use            |
| ----------------------------------- | -------------- |
| Agent lifecycle and stable pane IDs | Herdr          |
| Portable terminal multiplexing      | tmux           |
| Named interactive layouts           | Zellij         |
| One short command or agent          | No multiplexer |

If the layout costs more attention than the task, return to one process.
