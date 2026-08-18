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

While a turn is running, `Enter` does not discard input. It queues the draft as
a follow-up after the current run settles. `Ctrl+G` sends the draft as steering
guidance before the next model request in the active run.

`Cmd+Enter` on macOS or `Ctrl+Enter` elsewhere also sends steering guidance when
the terminal forwards that chord. The UI shows only queue counts, not queued
prompt text.

## Commands

Press `:` on an empty idle prompt to enter command mode. Command suggestions
appear above the prompt and can be accepted with `Tab` or completed with
`Enter`.

Supported command families include:

| Command                              | Purpose                                              |
| ------------------------------------ | ---------------------------------------------------- |
| `clear`                              | Clear the visible transcript.                        |
| `help`                               | Open help.                                           |
| `bg`                                 | List background processes.                           |
| `bg cancel <id>`                     | Cancel one owned background process.                 |
| `model`                              | Open the model picker.                               |
| `skills`                             | Browse loaded skills.                                |
| `context`                            | Inspect the bounded active context working set.      |
| `compact`                            | Summarize older conversation for continuation.       |
| `context item <id>`                  | Inspect one context item and its recovery state.     |
| `context pin <id-or-path>`           | Keep one context item visible across turn rebuilds.  |
| `context drop <id>`                  | Exclude one item until it is recovered or reset.     |
| `context recover <id>`               | Recover and pin bounded omitted evidence.            |
| `context verify ...`                 | Propose, review, or release a verification relation. |
| `context review <approve \| reject>` | Resolve a pending compaction review.                 |
| `context export <path>`              | Export the bounded redacted model projection.        |
| `context drop --reset`               | Clear all explicit context drops.                    |
| `doctor`                             | Show setup, context, and budget health.              |
| `auth status`                        | Show credential source/status without values.        |
| `config path` / `config show`        | Inspect config paths or redacted effective config.   |
| `setup`, `login`, `logout`           | Choose a provider/model or recover a credential.     |

Slash command forms such as `/model` and `/skills` remain accepted for
compatibility, but `:` command mode is the interactive command entry path.

Prompt templates share the slash-command picker and expand local `.md` or `.j2`
files into complete prompts. See [Prompt Templates](/docs/usage/prompt-templates/)
for file locations, MiniJinja variables, arguments, and bundled commands.

## Context Controls

`/context` opens a ledger of the current working set. See the
[Context guide](/docs/usage/context/) for item states, verification, release,
compaction review, and export.

It shows stable
ids, item kinds, visibility, approximate token costs, source labels, budget
pressure, instruction discovery diagnostics, and compaction review state. It
does not show project-instruction or transcript content, and secret-shaped
values are redacted.

Use `/context pin <id-or-path>` for task-local evidence. Paths must refer to
files inside the workspace. `/context drop <id>` excludes an item from later
selection; `/context recover <id>` removes that exclusion and pins omitted
recoverable detail when necessary. `/context drop --reset` clears all explicit
drops. A failed action leaves the editable prompt unchanged.

`/compact` asks the selected model for a typed continuation summary of a closed
transcript range. The response must preserve the source sequence, source hashes,
recovery handles, protected facts, and any earlier summary references named in
the request. A malformed or incomplete response is rejected before it changes
the model-visible projection.

For a long conversation, compaction retains a recent, complete sequence of user
turns and responses verbatim. A later compaction combines the previous anchored
summary with newly closed history, so it does not repeatedly summarize the
entire transcript. Project instructions, skills, tools, and other durable
context are rebuilt separately and are not entrusted to the summary.

When the covered range contains tool output, failures, permissions, or
unresolved work, the summary waits for `/context review approve` or `/context
review reject`. Approval replaces only the covered transcript entries in later
model requests. Rejection and provider failure preserve the active projection.
The original records remain in the append-only session in every case.

## Path Mentions

Typing `@` in the prompt opens inline file and directory suggestions. Type to
fuzzy-filter paths, move the selection with `Up`/`Down`, and press `Tab` or
`Enter` to insert the selected path. Cursor and deletion keys continue to edit
the path text in the draft while suggestions are open.

## Focused Surfaces

Help, command suggestions, file/model/skill pickers, setup/recovery surfaces,
and detail panes are focused surfaces inside the live shell. `Esc` closes the
active surface before it affects the prompt.

`Ctrl+O` opens the highest-priority available detail: failed tool output,
truncated tool output, latest edit/diff detail, then latest warning or error.

The transcript stays in native terminal scrollback. Use your terminal's normal
selection, copy, and scroll controls while the prompt remains keyboard-driven.
