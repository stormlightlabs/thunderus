---
Title: Prompt Editing Libraries and Renderer Ownership
Author: Reedline, Rustyline, Rustyline Async maintainers; local reference projects
Date: 2026-07-01
Captured: 2026-07-01
Tags: [terminal-ui, prompt, renderer, ratatui, reedline, rustyline, scrollback]
Sources:
  - https://docs.rs/reedline/latest/reedline/
  - https://docs.rs/rustyline/latest/rustyline/
  - https://docs.rs/rustyline-async/latest/rustyline_async/
---

## Summary

Prompt libraries can inform `thndrs` input editing, completion, history, and multiline
behavior, but they should not own the terminal renderer. The renderer needs one
coherent owner for committed transcript rows, live prompt rows, status rows, tool
streaming, file picker/help surfaces, resize, and scrollback behavior.

The practical direction is a small direct renderer around terminal rows and crossterm,
with rich components built on top of that row model. "Small" should mean narrow
ownership and predictable side effects, not a limited interface.

## Key Ideas

- **Renderer ownership matters more than line-editing feature count:** The main bugs
  we have seen are cursor placement, width changes, truncation, scrollback, and active
  region redraw. Those are renderer ownership problems.
- **Reedline is feature-rich but not a drop-in renderer:** Reedline supports multiline,
  history, completions, keybindings, menus, syntax highlighting, and prompt styling.
  Its docs still list concurrent background output while the prompt is active as a
  future improvement, which conflicts with an agent that streams thinking, tools, and
  status while input remains editable.
- **Rustyline is mature readline-style editing:** Rustyline is strong for a conventional
  prompt with history, hints, validation, and multiline support. It is less natural for
  a custom live transcript/status UI because the normal API centers on reading a line.
- **Rustyline Async is the closest prompt-library fit:** Rustyline Async supports
  writing output above an active prompt while input continues. That helps with streaming
  terminal output, but its prompt UI is still narrower than `thndrs` needs for file
  picking, help, mentions, custom blocks, status, and transcript rendering.
- **Codex and Pi both favor explicit terminal ownership:** Codex keeps tight control
  over terminal history insertion and prompt/status drawing. Pi favors append-to-
  scrollback output plus a small active redraw region. Both models avoid handing the
  whole user experience to a generic prompt library.
- **Ratatui can still be useful as a reference, not the inline owner:** Ratatui's layout
  and widget model are useful for structured full-screen UIs, but our inline/chat
  behavior has repeatedly exposed issues around constrained viewports, mouse scroll,
  resize, and cursor anchoring.

## Claims & Evidence

| Claim                                                                   | Support                                                                                                                                                     | Caveat / Confidence                                                                           |
| ----------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------- |
| A prompt library should not own the `thndrs` terminal renderer.         | `thndrs` needs transcript, live prompt, status, file picker, help, tool streams, resize, and native scrollback to cooperate under one terminal owner.       | High; this directly matches the observed UI failures.                                         |
| Reedline can guide input behavior but is risky as the primary UI owner. | Reedline documents rich editing features, but concurrent background output while the prompt is active is still described as an area for future improvement. | High for avoiding it as renderer; medium for borrowing editing ideas later.                   |
| Rustyline is a poor fit as the main renderer.                           | Rustyline's primary model is readline-style prompt input with history and validation.                                                                       | High; it can still inform history and keybind behavior.                                       |
| Rustyline Async is worth remembering as a fallback.                     | It supports concurrent output above an active prompt through a shared writer.                                                                               | Medium; useful if we choose a simpler prompt, but likely too narrow for the desired UI.       |
| A direct crossterm renderer is the best next step for inline mode.      | It gives explicit control over rows, ANSI styles, cursor position, redraw range, resize invalidation, and scrollback writes.                                | High; implementation must stay disciplined to avoid recreating an oversized widget framework. |

## Important Terms

| Term                 | Meaning                                                                                                                            |
| -------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| Renderer ownership   | The single subsystem responsible for terminal writes, cursor movement, clearing, wrapping, and redraw.                             |
| Committed transcript | Output that has been printed into terminal scrollback and should not be continuously redrawn.                                      |
| Live region          | The rows near the bottom of the terminal that can be cleared and redrawn for prompt, status, picker, and active streaming content. |
| Full-duplex prompt   | A prompt that remains editable while background output can still appear coherently.                                                |
| Row model            | An intermediate representation of styled terminal rows after wrapping, padding, and truncation decisions.                          |
| Resize invalidation  | Recomputing visible rows and cursor placement when terminal width or height changes.                                               |

## Questions for Review

- Why is full-duplex output central for a coding agent?
- Which prompt-library features are worth copying into our own input model?
- How much Unicode grapheme correctness do we need before the direct renderer is acceptable?
- Should Ratatui remain only for alternate-screen experiments and snapshot helpers?
- Where should direct renderer boundaries live: `src/ui/inline.rs`, a new module, or a
  smaller terminal backend abstraction?

## Connections

- Related ideas: Pi's append-to-scrollback TUI, Codex's terminal history insertion and
  reflow model, Gridland-style message blocks, OpenCode-style status and command
  surfaces.
- Related sources: Reedline, Rustyline, Rustyline Async, local `.sandbox/references`.
- Contradictions or tensions: Prompt libraries give mature editing behavior, but the
  desired agent UI needs richer shared rendering than those libraries expose.
- Useful applications: Keep `PromptInput` or a similar internal buffer model; render it
  through our row model; borrow keybinding and completion behavior from established
  prompt libraries without handing over terminal drawing.

## Open Questions

- Should file picker, help, and `@` mentions be rendered as rows inside the same live
  region instead of overlays?
- Should committed transcript blocks be printed immediately to native scrollback, with
  only active streaming content redrawn?
- What is the smallest row diffing algorithm that avoids flicker without becoming a
  custom UI framework?
- Can a future extraction make the renderer independently testable with plain row
  snapshots?

## Takeaways

- Own terminal rendering for inline mode.
- Treat prompt libraries as design references or optional editing backends, not the
  renderer.
- Keep the renderer small in ownership surface: terminal size, row layout, cursor,
  live-region redraw, committed transcript output, and resize invalidation.
- Do not let "small renderer" mean "small UX." Rich UI can be composed from simple,
  deterministic row primitives.
