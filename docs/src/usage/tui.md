# TUI

## Layout

The screen is vertical: transcript on top, prompt pinned near the bottom, and
footer/status metadata on the final line.

## Transcript Panel

The transcript fills the available main area and shows the newest entries. It
uses separate rows for user messages, assistant text, reasoning, tools, notices,
and errors. Use PageUp/PageDown or Ctrl+Alt+U/D to jump by 10 lines, and
Ctrl+Alt+Y/E to scroll one line. Up/Down recall prompt history.

Mouse capture is off by default so native terminal text selection works. Start
with `--mouse` to let the TUI handle wheel scrolling.

## Prompt Line

The prompt line has a top divider and an explicit state marker. Editable prompts
use `▌ ▶`. Submitted, streaming, stopped, and errored states use compact icons.
The helper line stays focused on input behavior such as queued steering and
follow-up prompts.

## Status Line

The footer shows run state, model, search mode, token counts, max output tokens,
and current working directory. Lower-priority fields hide on narrow terminals
before the line wraps. Long working directories are truncated from the left when
needed.

## Banner

The empty transcript can show a `thndrs` banner when there is enough space. It
falls back to plain placeholder text when the terminal is too narrow.

## Palette

The default TUI palette is based on Iceberg dark. Catppuccin Mocha is included
as an alternate palette for future selection support.
