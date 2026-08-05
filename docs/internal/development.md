# Multiplexer-Assisted Development

Use a terminal multiplexer when a development task benefits from keeping an
orchestrator, a coding agent, and a long-running command visible at the same
time. Herdr is the preferred tool for agent work because it understands agent
lifecycle state. tmux and Zellij are useful fallbacks for ordinary terminal
processes.

The multiplexer owns terminal layout and process visibility while the orchestrating
agent handles task selection, delegation, review, and the decision to continue.

## Working Layout

Use one orchestrator pane and one worker pane are enough for most
tasks. Add a third pane only for a server, focused test watcher, or
other long-running process that is actively useful.

| Pane    | Typical responsibility                                       |
| ------- | ------------------------------------------------------------ |
| Main    | Orchestrate, inspect changes, and make integration decisions |
| Worker  | Implement one bounded change or investigate one question     |
| Runtime | Run a server, focused tests, logs, or a reproducible failure |

You don't need to fill every available pane, as multiple agents, builds, language
servers, and test processes can make the entire machine unresponsive, so start with
a single worker, wait for it to settle, and add concurrency for independent units
of work on capable resources.

## Operating Rules

- Give each worker one deliverable with its working directory, constraints,
  expected result, and narrow verification command.
- Keep one writer in a shared checkout at a time. If two workers must write in
  parallel, give them separate worktrees and non-overlapping ownership.
- Treat the working tree as user-owned. Do not discard, overwrite, commit, or
  publish changes unless the user explicitly requests it.
- Prefer the least expensive model and effort level that can complete the
  task. Use Terra at high effort for bounded implementation and Luna at xhigh
  for difficult diagnosis, architecture, or final review when that extra
  reasoning is justified.
- Ask workers for concise results. Read recent output instead of repeatedly
  capturing full transcripts, and avoid polling a completed process.
- Run focused checks before broad suites. Do not run duplicate builds or test
  suites in several panes.
- Stop background commands that no longer serve the active task. Close idle
  panes that you created, but never close someone else's pane without asking.
- Use explicit pane or agent identifiers for automation. Visual focus is for
  the human and can change at any time.

## A Bounded Agent Loop

1. Inspect the current workspace and decide whether another process is
   necessary.
2. Open a sibling pane in the same working directory without stealing focus.
3. Start one agent or command and record its pane identifier.
4. Give an agent a narrow prompt that names the deliverable and verification.
5. Wait for meaningful state or output instead of continuously polling.
6. Inspect the worker's result and the shared working tree.
7. Run only the checks needed to establish correctness.
8. Continue the same worker when context is useful; otherwise stop it and close
   the pane.

The orchestrator reviews outcomes rather than accepting an agent's claim that
the task is complete. Repository state and command results are the source of
truth.

## Herdr

Herdr adds workspaces, tabs, stable pane identifiers, and semantic agent states
to normal terminal panes. Use its `agent` commands for recognized coding agents
and its `pane` commands for shells, servers, tests, and other processes.

When controlling Herdr from inside one of its panes, first verify that the
integration environment is present:

```sh
test "${HERDR_ENV:-}" = 1
```

Discover commands with targeted help. Running bare `herdr` launches or attaches
the interface, so do not use it for command discovery.

```sh
herdr --help
herdr pane --help
herdr agent --help
```

Create a sibling pane without moving the human's focus, then use the pane
identifier returned by Herdr:

```sh
herdr pane split --current --direction right --cwd "$PWD" --no-focus
herdr agent start worker --kind codex --pane w1:p2
herdr agent prompt worker "Fix the failing parser test. Keep the change local and run the focused test." --wait --timeout 120000
```

Replace `w1:p2` with the identifier returned by `pane split`. Prefer explicit
identifiers such as `w1`, `w1:t1`, and `w1:p2`; use `--current` only when the
current pane is intentionally the target.

Inspect an agent through the lifecycle-aware commands:

```sh
herdr agent get worker
herdr agent read worker
herdr agent wait worker --timeout 120000
```

For ordinary commands, use pane controls:

```sh
herdr pane run w1:p3 -- cargo test -p thndrs-agent parser
herdr pane read w1:p3 --source recent-unwrapped
```

Herdr reports agents as `idle`, `working`, `blocked`, `done`, or `unknown`.
`blocked` means the worker needs inspection. `unknown` does not mean the work is
finished. If an alternate-screen application has no recoverable history, ask
the worker for a short result in its terminal or, when durable output is truly
needed, a temporary Markdown file.

## tmux

tmux is widely available and reliable for shells and long-running processes,
but it does not know whether a coding agent is working, blocked, or done. The
orchestrator must infer lifecycle from the process and its output.

Create a detached sibling pane in the current directory and capture its stable
pane identifier:

```sh
worker_pane=$(tmux split-window -h -d -c "#{pane_current_path}" -P -F '#{pane_id}')
tmux send-keys -t "$worker_pane" 'codex' Enter
```

Target later interaction and inspection by that identifier:

```sh
tmux send-keys -t "$worker_pane" 'Run the focused parser test and summarize the result.' Enter
tmux capture-pane -p -t "$worker_pane" -S -200
```

Use `split-window -v` for a pane below the current one. Keep captures bounded;
`-S -200` reads recent history instead of dumping the entire pane. When the
process is no longer needed, confirm the identifier and close only that pane:

```sh
tmux kill-pane -t "$worker_pane"
```

Sending text and `Enter` separately is safer when the target program has its
own input editor. Do not enable pane synchronization for agent work: the same
prompt or command could be sent to every pane.

## Zellij

Zellij provides discoverable keybindings, named panes, layouts, and stable pane
identifiers. Like tmux, it manages terminals rather than agent lifecycle.

Open a named pane in the current directory and run a command directly:

```sh
zellij action new-pane --direction right --cwd "$PWD" --name tests -- cargo test -p thndrs-agent parser
```

The command prints the created pane identifier. For an interactive worker, omit
the command after `--` and start the agent in the new shell. Use the action
commands to inspect and target the session:

```sh
zellij action list-panes
zellij action send-keys --pane-id terminal_2 'Ctrl c'
zellij action close-pane --pane-id terminal_2
```

Prefer pane identifiers for cleanup. `close-pane` without `--pane-id` closes
the focused pane, which is fragile when a human is navigating the session.
Avoid synchronized input for the same reason as tmux.

For recurring arrangements, use a checked-in Zellij layout only when the
layout itself is stable and valuable. A one-off task is usually clearer as one
or two explicit `new-pane` commands.

## Choosing a Tool

| Need                                               | Best fit       |
| -------------------------------------------------- | -------------- |
| Start, prompt, wait for, and inspect coding agents | Herdr          |
| Portable terminal multiplexing on an existing host | tmux           |
| Interactive layouts with discoverable controls     | Zellij         |
| One short command or one agent                     | No multiplexer |

A multiplexer should reduce coordination cost. If maintaining the layout,
reading several transcripts, or managing machine load takes more attention than
the task itself, return to one process and one bounded deliverable.
