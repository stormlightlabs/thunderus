# Inline Transcript and Ratatui Surface Tasks

## INLINE-1: Render the composer and application views with Ratatui

**What to build:** Turn the composer and focused surfaces into Ratatui-owned
components over the current application state. Provide one live-surface layout
that can place the composer, accessories, and a picker, prompt, detail panel,
or future sidebar-like view without consulting transcript history.

**Blocked by:** None - can start immediately.

**Acceptance criteria:**

- [x] Ratatui owns composer border, padding, wrapping layout, accessories, key
      hints, height, and cursor placement. `App` continues to own prompt state
      and editing behavior.
- [x] Multiline editing, word-boundary navigation, history navigation, queued
      input, mention insertion, bracketed paste, and submit behavior match the
      current composer.
- [x] Pickers, permission/setup prompts, help, context/request details, and
      other current focused surfaces use the shared live-surface layout.
- [x] A new bounded sidebar-like view can be added without changing transcript
      projection, transcript commits, or native history code.
- [x] Transcript rows enter the layout as projected rows. Ratatui components do
      not classify, rewrap, cache, navigate, or select transcript history.
- [x] The layout clips predictably at small terminal sizes and keeps the active
      composer cursor or focused control visible.
- [x] Composer and application views remain keyboard-driven so inline mode can
      preserve native terminal selection and copy.

**Verification:**

- Ratatui `TestBackend` snapshots for empty, single-line, multiline, queued,
  accessory, narrow, and focused-view states.
- Focused behavior tests proving the cursor coordinate uses the layout that
  painted the prompt text.
- An integration test that opens and closes each view class without changing
  projected transcript content.

## INLINE-2: Restore native transcript scrollback

**What to build:** Run the interactive session on the normal terminal screen.
Append completed transcript blocks to native history, keep streaming work in a
mutable tail, and place the Ratatui live surface from INLINE-1 below it. Apply
the inline renderer's operation vocabulary as part of the shared transcript
presentation.

**Blocked by:** INLINE-1.

**Acceptance criteria:**

- [x] Inline mode stays on the normal screen and uses terminal scrollback,
      selection, and copy. It does not enter the alternate screen or enable
      mouse capture by default.
- [x] Ratatui's `Viewport::Inline` and `Terminal::insert_before` support
      composer height changes, transcript commits, and focused-view transitions
      without stale rows or damage to content above the live surface.
- [x] One coordinator owns terminal modes, viewport reservation, ordered
      insertion and drawing, cursor placement, and flushes. Transcript and
      surface modules do not issue independent raw cursor or scroll-region
      writes through `backend_mut()`.
- [x] Stability is decided for semantic blocks rather than inferred from
      wrapped rows, rendered strings, or viewport height.
- [x] The commit checkpoint uses stable block identity and generation and
      survives width, theme, composer-height, and focused-view changes.
- [x] Submitted user entries and finalized assistant or tool blocks are
      committed once, in transcript order.
- [x] Streaming assistant text, running tools, elapsed time, and other changing
      content remain in the mutable tail.
- [x] Finalization inserts the complete block and removes its mutable copy in
      one terminal transaction, with no duplicate or blank intermediate frame.
- [x] A fresh or resumed process hydrates its stable transcript once. Resize,
      compaction, relayout, and in-process resume do not hydrate it again.
- [x] Resize reflows the mutable tail and future commits without purging,
      clearing, or replaying committed terminal history.
- [x] Clearing application history starts a new commit generation and clears
      the live surface without claiming to erase emulator-retained scrollback.
- [x] Recently committed rows are not copied into the live surface to fill
      unused space.
- [x] Transcript semantics map Skill `§`, Run/shell `$`, Search `/`, Read `›`,
      Explore `⌁`, Edit/patch `∆`, Create/write `+`, Delete `−`, Fetch/network
      `↗`, Retry/refresh `⟳`, Tool/MCP `@`, Subagent/parallel `∥`, and
      Warning/blocked `!`.
- [x] Structured tool and transcript metadata determines the operation when
      available. Unknown external capabilities use Tool/MCP, and renderers do
      not parse labels to recover semantics.
- [x] Skill activation uses `§ Skill`. Create, edit, and delete remain distinct
      only when structured change data supports the distinction.
- [x] Operation kind stays fixed across running, success, and failure states.
      Live and committed forms use the same glyph, label, target summary, and
      width-aware layout.
- [x] Every operation symbol has an adjacent readable label and does not depend
      on color for meaning.

If Ratatui's public inline APIs cannot meet the height and insertion criteria,
stop and revise `plan.md`. Do not recreate the former mix of Ratatui draws and
raw scroll-region writes.

**Verification:**

- Pure tests for stable/live projection, exact-once checkpoints, ordered batch
  commits, clear generations, hydration, compaction, wrapping, and width
  changes.
- Table-driven tests for every operation symbol and label, plus representative
  built-in tools, MCP tools, skills, retries, blocked operations, and
  structured create/edit/delete changes.
- Snapshots showing live, succeeded, and failed operations at normal and narrow
  widths, followed by the same committed classification.
- PTY or captured-output tests for dynamic live-surface height, long streaming
  responses, running commands, finalization, resize, native selection, and no
  duplicate ANSI text.
- Manual smoke test in a normal terminal and tmux before INLINE-3 begins.

## INLINE-3: Ship the inline renderer

**What to build:** Harden the inline session across terminal lifecycle events,
make it the default, and remove the alternate renderer's duplicate transcript
viewport. Keep alternate-screen code only for a concrete application view that
requires a full-screen surface.

**Blocked by:** INLINE-2.

**Acceptance criteria:**

- [x] One guard owns raw mode, bracketed paste, keyboard enhancements, cursor
      visibility, suspend/resume, and cleanup.
- [x] Existing normalized key, paste, resize, tick, and cancellation events
      continue through the shared application loop.
- [x] Before suspension or an interactive child process, thndrs settles the
      current draw and leaves the live surface cleanly. Resume redraws mutable
      content without replaying committed history.
- [x] Terminal modes and the cursor are restored after normal exit, error,
      cancellation, panic, suspension, and child-process execution.
- [x] Normal interactive sessions use the inline renderer without an
      experimental selector.
- [x] Alternate-only transcript scroll, selection, mouse capture, viewport
      cache, anchoring, and copy-feedback paths are removed.
- [x] The normal interactive session has one transcript projection and one
      terminal lifecycle path.
- [x] Transcript Markdown, tool output, diffs, activity, failures, compaction,
      and skills retain their intended content apart from the planned operation
      symbols and native-history framing.
- [x] CLI help, configuration, key hints, and user documentation no longer
      promise application-owned transcript scrolling or mouse selection.
- [x] A long session survives streaming output, running tools, multiline paste,
      repeated narrow/wide resize, native copy, every focused-view class, shell
      execution, suspend/resume, cancellation, and exit during active work.

**Verification:**

- Focused lifecycle tests with a recording backend for command order, flush
  boundaries, suspension, resumption, and every cleanup path.
- PTY coverage for resize, bracketed paste, child-process execution, stale-row
  cleanup, and transcript deduplication.
- Focused renderer, runtime, input, and transcript tests, followed by the
  workspace Rust checks required by `AGENTS.md`.
- `pnpm --dir docs build` if public documentation changes.
- `git diff --check` and a final search for the experimental selector and
  obsolete alternate transcript paths.
