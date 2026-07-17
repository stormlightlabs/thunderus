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

## Prompt History

Use `Up` and `Down` to recall submitted prompts. The TUI keeps the newest 200
entries in its prompt-history buffer and persists them in `.thndrs/input-history.jsonl`
(append-only, capped at 1MiB).

On Unix, thndrs keeps the file readable and writable only by its owner.

When this file is not present, thndrs imports recent user prompts from existing session
files once.

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

| Command                       | Purpose                                               |
| ----------------------------- | ----------------------------------------------------- |
| `clear`                       | Clear the visible transcript.                         |
| `help`                        | Open help.                                            |
| `bg`                          | List background processes.                            |
| `bg cancel <id>`              | Cancel one owned background process.                 |
| `model`                       | Open the model picker.                                |
| `skills`                      | Browse loaded skills.                                 |
| `context`                     | Inspect the bounded active context working set.       |
| `context pin <id-or-path>`    | Keep one context item visible across turn rebuilds.   |
| `context drop <id>`           | Exclude one item until it is recovered or reset.      |
| `context recover <id>`        | Re-enable an item and pin omitted detail when needed. |
| `context review <approve      | reject>`                                              | Resolve a pending compaction review. |
| `context drop --reset`        | Clear all explicit context drops.                     |
| `doctor`                      | Show setup, context, and budget health.               |
| `auth status`                 | Show credential source/status without values.         |
| `config path` / `config show` | Inspect config paths or redacted effective config.    |
| `setup`, `login`, `logout`    | Choose a provider/model or recover a credential.      |

Slash command forms such as `/model` and `/skills` remain accepted for
compatibility, but `:` command mode is the interactive command entry path.

## Context Controls

`/context` opens a bounded ledger of the current working set. It shows stable
ids, item kinds, visibility, approximate token costs, source labels, budget
pressure, instruction discovery diagnostics, and compaction review state. It
does not show project-instruction or transcript content, and secret-shaped
values are redacted.

Use `/context pin <id-or-path>` for task-local evidence. Paths must refer to
files inside the workspace. `/context drop <id>` excludes an item from later
selection; `/context recover <id>` removes that exclusion and pins omitted
recoverable detail when necessary. `/context drop --reset` clears all explicit
drops. A failed action leaves the editable prompt unchanged.

`/compact` asks the selected model for a continuation summary. When the covered
range contains tool output, failures, permissions, or unresolved work, the
summary waits for `/context review approve` or `/context review reject` before
changing the active context. The original session records remain available for
recovery.

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
