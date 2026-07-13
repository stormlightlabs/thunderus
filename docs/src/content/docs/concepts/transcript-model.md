---
title: "Transcript Model"
---

`thndrs` represents a run as typed transcript events rather than one large
rendered transcript string. The same event model feeds the live TUI, native
terminal scrollback, session persistence, inspection, and export.

## Events

User prompts, assistant text, reasoning summaries, tool starts and results,
status notices, errors, and turn completion are kept as distinct events.

This lets the renderer show progress without confusing a partial stream with a
completed answer, and gives session inspection stable records to summarize.

## Persistence

Sessions are append-only JSONL files. They retain replayable transcript content,
tool audit metadata, context evidence, and usage totals, but do not persist raw
provider payloads or full file contents by default. Secret-looking values are
redacted on the diagnostic and inspection paths on a best-effort basis.

## Rendering

The TUI renders completed entries into native terminal scrollback and keeps a
small live prompt region at the bottom. Detail surfaces can expose bounded
additional tool output without turning the transcript into a second viewport.
