---
title: "Prompting and Input"
---

The TUI prompt is the primary control surface for composing requests, steering a
running turn, queueing follow-ups, and opening short-lived focused surfaces.

## Prompt States

The prompt marker changes with runtime state:

| State        | Meaning                                                            |
| ------------ | ------------------------------------------------------------------ |
| editable     | The prompt is ready for a new request.                             |
| submitted    | The current input has been sent and the agent is starting work.    |
| streaming    | Assistant text or reasoning is arriving.                           |
| running tool | A local tool is active.                                            |
| stopped      | A running turn was cancelled.                                      |
| errored      | The last submit failed or the latest transcript entry is an error. |

If a submit fails before the turn starts, the draft stays in the prompt so it can
be edited and retried.

## Running Input

While a turn is running, `Enter` does not discard input. It records the draft as
either:

| Target    | Behavior                                                           |
| --------- | ------------------------------------------------------------------ |
| steering  | Sends guidance to the active run when the provider can consume it. |
| follow-up | Queues the text as the next turn after the current run settles.    |

Use `Ctrl+T` while running to toggle between `steering` and `follow-up`. The UI
shows only queue counts, not queued prompt text.

## Commands

Press `:` on an empty idle prompt to enter command mode. Command suggestions
appear above the prompt and can be accepted with `Tab` or completed with
`Enter`.

Supported command families include:

| Command                       | Purpose                                            |
| ----------------------------- | -------------------------------------------------- |
| `clear`                       | Clear the visible transcript.                      |
| `help`                        | Open help.                                         |
| `bg`                          | List background processes.                         |
| `model`                       | Open the model picker.                             |
| `skills`                      | Browse loaded skills.                              |
| `doctor`                      | Show redacted setup diagnostics.                   |
| `auth status`                 | Show credential source/status without values.      |
| `config path` / `config show` | Inspect config paths or redacted effective config. |
| `setup`, `login`, `logout`    | Open setup/recovery credential surfaces.           |

Slash command forms such as `/model` and `/skills` remain accepted for
compatibility, but `:` command mode is the interactive command entry path.

## File Mentions

Use `Ctrl+P` to open the workspace file picker. Type to fuzzy-filter files, move
with `Up`/`Down`, and press `Enter` to insert the selected path.

Typing `@` in the prompt opens inline file mention suggestions. `Tab` accepts
the active suggestion and inserts the workspace path into the draft.

## Focused Surfaces

Help, command suggestions, file/model/skill pickers, setup/recovery surfaces,
and detail panes are bounded focused surfaces inside the live shell. `Esc`
closes the active surface before it affects the prompt.

`Ctrl+O` opens the highest-priority available detail: failed tool output,
truncated tool output, latest edit/diff detail, then latest warning or error.

Mouse capture is disabled by default. Native terminal selection and scrollback
remain available unless mouse support is explicitly enabled for overlay testing.
