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

Slash-command suggestions, help, and file/model/skill pickers render as rows in
the live shell instead of floating over the transcript. `Esc` closes the active
accessory first.

## Status Line

The footer shows static model, search mode, token counts, TTFT when available,
and current working directory. Lower-priority fields hide on narrow terminals
before the line wraps. Long working directories are truncated from the left when
needed.

TTFT is client-observed time to first token: the elapsed time from submitting a
local turn to the first semantic model output. While a run is waiting for that
first assistant text, visible reasoning, or tool-call delta, the status line can
show `ttft: pending`. After the first output arrives, it shows a compact value
such as `ttft: 842ms` or `ttft: 1.4s`, and the last completed turn's value stays
visible until the next turn when width allows.

## Resize

Terminal resize reflows prompt and accessory rows and recomputes cursor
placement. Transcript history stays in native scrollback.

## Banner

The empty fullscreen shell can show a `thndrs` banner when there is enough
space. It falls back to plain placeholder text when the terminal is too narrow.

## Palette

The default TUI palette is based on Eldritch minimal. Iceberg dark and
Catppuccin Mocha are kept as alternate palettes for future selection support.
