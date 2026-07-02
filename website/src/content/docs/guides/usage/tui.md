---
title: "TUI"
---

## Layout

The TUI keeps a small live shell near the bottom of the terminal: session and
dynamic run status above the input, the input row, and static model/search/token
metadata below it.

## Transcript Scrollback

Completed transcript entries are inserted above the live shell into native
terminal scrollback. User messages, assistant text, reasoning, tools, notices,
and errors still render as structured blocks, but there is no app-owned
transcript viewport to page through. Use your terminal or multiplexer scrollback
for wheel/trackpad scrolling and search. Up/Down recall prompt history.

Mouse capture is off by default so native terminal text selection and scrollback
work. Start with `--mouse` only when testing mouse events inside overlays such
as the file picker.

## Prompt Line

The prompt area has a top divider and an explicit state marker. Submitted,
streaming, stopped, and errored states use compact icons. The dynamic line above
the input shows the session and current run status, including queued steering
and follow-up prompts.

## Status Line

The footer shows static model, search mode, token counts, and current working
directory. Lower-priority fields hide on narrow terminals before the line wraps.
Long working directories are truncated from the left when needed.

## Banner

The empty fullscreen shell can show a `thndrs` banner when there is enough
space. It falls back to plain placeholder text when the terminal is too narrow.

## Palette

The default TUI palette is based on Eldritch minimal. Iceberg dark and
Catppuccin Mocha are kept as alternate palettes for future selection support.
