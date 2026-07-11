---
title: "TUI"
---

## Layout

The TUI keeps a small live shell near the bottom of the terminal: session and
dynamic run status above the input, the input row, and static model/search/token
metadata below it.

Completed transcript entries are written above that shell. The live shell then
redraws only changing rows: active streaming output, dynamic status, prompt
input, help, suggestions, and picker rows.

## Transcript Scrollback

Completed transcript entries are inserted above the live shell into native
terminal scrollback. User messages, assistant text, reasoning, tools, notices,
and errors still render as structured blocks, but there is no app-owned
transcript viewport to page through. Use your terminal or multiplexer scrollback
for wheel/trackpad scrolling and search. Up/Down recall prompt history.

Mouse capture is off by default so native terminal text selection and scrollback
work. Start with `--mouse` only when testing mouse events inside overlays such
as the file picker.

Message blocks use labels above the body and role-specific color. Long paths,
commands, URLs, and diagnostics wrap or truncate intentionally. Syntax
highlighting is reserved for code fences, diffs, snippets, and useful command
output.

## Prompt Line

The prompt area has a top divider and an explicit state marker. Submitted,
streaming, stopped, and errored states use compact icons. The dynamic line above
the input shows the session and current run status, including queued steering
and follow-up prompts. While the agent is running, `Ctrl+T` changes whether
`Enter` sends the current input as steering for the active run or queues it as a
follow-up turn. Help rows show that running-specific binding instead of the idle
`Ctrl+T` transpose binding.

Prompt input supports multiline editing with `Shift+Enter` or `Ctrl+J`,
cursor-aware movement across wrapped rows and explicit newlines, and history
navigation that preserves the current draft when appropriate. Cursor placement
and deletion are Unicode-aware for grapheme clusters, wide characters, CJK text,
emoji sequences, and zero-width marks.

Command suggestions, help, and file/model/skill pickers render as bounded rows
in the live shell instead of floating over the transcript. The command and file
surfaces are rendered through the iocraft adapter, then converted back into the
same row model as the rest of the direct renderer. `Esc` closes the active
accessory first.

The `/context` ledger uses the same bounded surface path. Wide terminals show
item ids, state, token estimates, and labels; narrow terminals fall back to
compact one-line entries. At short heights the surface is clipped before it
can displace the prompt. It displays metadata only, so source text stays out of
the overlay and native scrollback.

## Setup Slash Commands

The CLI commands are canonical, but the TUI exposes safe setup shortcuts:

- `/doctor`: show context source, pin, budget, and compaction review health.
- `/auth status`: show provider credential source/status without values.
- `/config path`: show global and project config paths.
- `/config show`: show redacted effective config and diagnostics.
- `/setup`: open the focused setup surface.
- `/login <provider>`: open hidden credential entry for that provider.
- `/logout <provider>`: open a confirmation surface before removing a stored
  credential.

Slash commands never accept API keys as arguments. `/config edit` is CLI-only;
from inside the TUI it prints the command to run outside the app.

## Status Line

The footer shows static model, search mode, token counts, TTFT when available,
and current working directory. Lower-priority fields hide on narrow terminals
before the line wraps. Long working directories are truncated from the left when
needed.

When the terminal is very wide, the footer can also show the trust boundary:
`local user · workspace-contained tools · no TUI sandbox`. This means tools run
with the local user's authority and thndrs only constrains workspace-scoped tool
paths at the tool layer; it does not claim an OS sandbox.

TTFT is client-observed time to first token: the elapsed time from submitting a
local turn to the first semantic model output. While a run is waiting for that
first assistant text, visible reasoning, or tool-call delta, the status line can
show `ttft: pending`. After the first output arrives, it shows a compact value
such as `ttft: 842ms` or `ttft: 1.4s`, and the last completed turn's value stays
visible until the next turn when width allows.

## Resize

Terminal resize reflows prompt and accessory rows and recomputes cursor
placement. Transcript history stays in native scrollback.

## Detail Surfaces

`Ctrl+O` opens a focused detail surface when there is something actionable to
inspect. The priority is failed tool output, truncated tool output, latest
edit/diff detail, then latest warning or error. Compact transcript rows remain
in native scrollback; detail surfaces are temporary bounded views for reading
more context without permanently expanding the transcript.

Tool transcript rows show a preview. If output is truncated in the transcript,
the full stored output remains available through the detail surface and session
data.

## Tables

Markdown tables and structured command output render as aligned terminal rows
when the terminal is wide enough. Columns use fixed, percentage, or flexible
widths depending on the source data, with right/center alignment where useful
for counts and statuses. On narrow terminals, tables fall back to compact text
rows instead of forcing unreadable wrapped columns.

## Banner

The empty fullscreen shell can show a `thndrs` banner when there is enough
space. It falls back to plain placeholder text when the terminal is too narrow.

## Palette

The default TUI palette is based on Eldritch minimal. Iceberg dark and
Catppuccin Mocha are kept as alternate palettes for future selection support.
