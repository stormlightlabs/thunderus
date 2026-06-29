# TUI

## Layout

The screen is split into a fixed sidebar and a main workbench. The main area is
vertical: transcript on top, prompt pinned near the bottom, and footer/status
metadata on the final line.

## Sidebar

The sidebar is 22 columns wide on normal terminal sizes. It shows the app title,
session list, and compact run status.

## Transcript Panel

The transcript fills the available main area and shows the newest entries. It
uses separate rows for user messages, assistant text, reasoning, tools, status,
and errors.

## Prompt Line

The prompt line has a top divider and an explicit state marker. Editable prompts
use `▌ ▶`. Submitted, streaming, stopped, and errored states use compact status
icons and labels.

## Status Line

The footer shows model, search mode, and current working directory. Long working
directories are truncated from the left when needed.

## Narrow Width Behavior

The sidebar hides on narrow terminals before critical prompt and status text
wraps. The transcript then uses the full width.

## Banner

The empty transcript can show a `thndrs` banner when there is enough space. It
falls back to plain placeholder text when the terminal is too narrow.

## Palette

The default TUI palette is based on Iceberg dark. Catppuccin Mocha is included
as an alternate palette for future selection support.
