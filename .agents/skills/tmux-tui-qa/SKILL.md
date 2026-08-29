---
name: tmux-tui-qa
description: Exercise the thndrs terminal UI in a real pseudo-terminal with tmux, including input, streaming, picker transitions, resize behavior, ANSI capture, screenshots, and cleanup. Use for hands-on TUI verification when snapshots cannot establish cursor, reflow, scrollback, timing, color, or terminal-lifecycle behavior.
compatibility: Requires tmux. Freeze or VHS is optional for rendered visual evidence.
---

# tmux TUI QA

Use tmux as a controllable pseudo-terminal, not as a substitute for focused state, layout, and snapshot tests. Read the repository's [testing guide](../../../docs/src/content/docs/docs/development/testing.md) before choosing the evidence needed for a change.

## Define the scenario

Name the behavior and transitions under review before starting the TUI. Select only the states implicated by the change, plus the narrow or short size most likely to expose a regression.

For renderer work, usually check:

- startup and editable composer;
- submitted and streaming output;
- the changed picker, overlay, permission, error, or cancellation state;
- return from that state;
- normal, narrow, and short dimensions.

Run focused automated checks first. Use tmux for behavior that depends on a real terminal: input timing, cursor placement, reflow, native scrollback, resize events, ANSI color, and terminal cleanup.

## Start an isolated terminal

Build the current binary and check dependencies:

```sh
command -v tmux
cargo build -p thndrs
```

Choose a unique socket name and reuse it for every command in the run. A separate tmux server prevents the check from changing or killing the user's sessions. Keep the terminal size explicit and use an ephemeral thndrs session:

```sh
qa_socket=thndrs-qa-<unique-id>
qa_target=qa:0.0

tmux -L "$qa_socket" new-session -d -x 100 -y 30 -s qa \
  "./target/debug/thndrs --model <configured-model> --ephemeral --tick-rate-ms 100"
```

Do not attach to the session or type into the user's active pane. Inspect the visible pane and wait for the expected startup state with a bounded loop rather than an arbitrary long sleep:

```sh
for attempt in $(seq 1 100); do
  frame=$(tmux -L "$qa_socket" capture-pane -p -t "$qa_target")
  printf '%s\n' "$frame" | grep -q 'Editable' && break
  sleep 0.1
done
printf '%s\n' "$frame" | tail -30
```

If startup does not reach the expected state, inspect one bounded capture and the pane process before deciding whether the problem is the application, configuration, authentication, or the QA setup:

```sh
tmux -L "$qa_socket" capture-pane -p -t "$qa_target" -S -60
tmux -L "$qa_socket" list-panes -t "$qa_target" \
  -F '#{pane_dead} #{pane_dead_status} #{pane_current_command}'
```

## Exercise transitions

Send literal text and special keys separately. This avoids tmux interpreting input text as key names and matches applications that own their input editor:

```sh
tmux -L "$qa_socket" send-keys -t "$qa_target" -l 'hello'
tmux -L "$qa_socket" send-keys -t "$qa_target" Enter
```

Wait only as long as the transition needs, then capture bounded evidence:

```sh
tmux -L "$qa_socket" capture-pane -p -e -N -t "$qa_target" -S -40
```

Resize the same window so the running application receives a real resize event:

```sh
tmux -L "$qa_socket" resize-window -t qa:0 -x 80 -y 30
tmux -L "$qa_socket" capture-pane -p -e -N -t "$qa_target" | nl -ba | tail -30

tmux -L "$qa_socket" resize-window -t qa:0 -x 80 -y 16
tmux -L "$qa_socket" capture-pane -p -e -N -t "$qa_target" | nl -ba | tail -20
```

Open and close any affected surface in the same pane. For example:

```sh
tmux -L "$qa_socket" send-keys -t "$qa_target" -l '/model'
tmux -L "$qa_socket" send-keys -t "$qa_target" Enter
tmux -L "$qa_socket" capture-pane -p -e -N -t "$qa_target" | tail -30
tmux -L "$qa_socket" send-keys -t "$qa_target" Escape
```

Check the transition, not only the final frame. Look for stale rows, blank gaps, mid-word wrapping, hidden choices, composer movement, cursor displacement, lost transcript content, and incomplete cleanup.

## Capture visual evidence

`capture-pane -e` preserves escape sequences and `-N` preserves trailing spaces, including styled cells used for full-row backgrounds. Store every screenshot under `.sandbox/screenshots/`, including Freeze, VHS, and native-terminal screenshots. Save the complete visible frame when visual inspection is needed:

```sh
mkdir -p .sandbox/screenshots
capture=.sandbox/tui-qa-<state>.ansi
screenshot=.sandbox/screenshots/tui-qa-<state>.png

tmux -L "$qa_socket" capture-pane -p -e -N -t "$qa_target" > "$capture"
freeze "$capture" -o "$screenshot"
```

Inspect the raw ANSI capture as well as the rendered image. Freeze is a renderer, not the terminal under test. If background-only cells or other terminal behavior are correct in the ANSI capture but wrong in Freeze, use VHS or a native terminal screenshot; do not alter thndrs to fit the screenshot tool.

Use VHS when the deliverable needs a repeatable scripted recording or a sequence of screenshots. Keep tmux for quick interactive diagnosis and explicit cell-size resize checks.

## Clean up before ending the turn

Kill every application, recorder, and tmux server started by the QA run, including after a failed or interrupted check. Do not finish the turn while any QA process remains. Stop the application, then remove only the isolated tmux server created for this run:

```sh
tmux -L "$qa_socket" send-keys -t "$qa_target" C-d
sleep 0.2
tmux -L "$qa_socket" send-keys -t "$qa_target" C-d 2>/dev/null || true
tmux -L "$qa_socket" kill-server 2>/dev/null || true
```

Confirm cleanup before responding:

```sh
if tmux -L "$qa_socket" has-session -t qa 2>/dev/null; then
  echo "QA tmux session is still running" >&2
  exit 1
fi
```

Failure from `has-session` is the expected result. If another process was started outside that isolated tmux server, stop it separately and verify that it exited. Keep captures only while they are useful for review. Never include prompts, credentials, or private transcript content in committed or shared artifacts.

## Report the result

State:

- the behavior and transitions exercised;
- terminal dimensions used;
- focused automated checks run before the live check;
- capture or screenshot paths, when retained;
- defects found or the specific behavior observed as correct.

A successful build or a single polished screenshot does not establish interactive correctness.

## Sources

The workflow follows tmux's documented pseudo-terminal, stable target ID, detached-session, explicit-size, resize, literal-key, and capture facilities. Freeze documents piping `tmux capture-pane` output into its renderer. VHS provides scripted typing, waits, terminal dimensions, screenshots, and recordings. Ratatui recommends fixed-size `TestBackend` snapshots for deterministic render checks; those remain the first line of verification.

- [tmux manual](https://man.openbsd.org/tmux.1)
- [Freeze: screenshot TUIs](https://github.com/charmbracelet/freeze#screenshot-tuis)
- [VHS command reference](https://github.com/charmbracelet/vhs#vhs-command-reference)
- [Ratatui snapshot testing](https://ratatui.rs/recipes/testing/snapshots/)
