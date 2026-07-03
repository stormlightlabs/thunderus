# Rendering Engine Hardening Plan

Status: Draft
Owner: TBD
Captured: 2026-07-03

## Background

`thndrs` moved from a Ratatui-owned shell to a direct renderer because inline
coding-agent UIs put unusual pressure on terminal output. The renderer has to
handle editable prompt rows, live streaming text, tool output, pickers, status
rows, native scrollback, resize, Unicode width, cursor placement, and terminal
quirks at the same time.

The current renderer has the right foundation:

- `src/renderer/layout.rs` owns display-width and grapheme-aware wrapping.
- `src/renderer/row.rs` defines `Row`, `Frame`, and `CursorCoord`.
- `src/renderer/backend.rs` translates frames into crossterm writes.
- `src/renderer/live.rs` builds prompt, status, picker, help, and footer rows.
- `src/renderer/region.rs` composes the viewport, commits stable history, and
  keeps the live prompt block bottom-pinned.
- `src/input.rs` stores prompt cursor position as a grapheme index.

Reference review supports this direction. Codex and Pi keep explicit terminal
ownership for agent chat. Ratatui contributes event-loop and snapshot patterns.
Yoga/Gridland fit component-tree layout; this UI is row-first. Rustyline-style
libraries help with input behavior, but they do not own transcript rendering,
tool streams, status rows, and native scrollback together.

## Problem

The renderer can support the current UI, but dogfooding `thndrs` on its own
development requires more than the current happy path.

The next implementation risks are concrete:

- long streaming replies can compete with prompt and footer rows;
- tools can produce wide, structured, colored, or fast-updating output;
- queued follow-ups, pickers, status rows, and focused detail surfaces can all
  want live-region height;
- resize can happen during any of those states;
- scrollback insertion varies across terminal emulators and multiplexers;
- Unicode correctness needs to hold across prompt editing, transcript rows,
  picker labels, paths, backend clipping, and cursor movement;
- `region.rs` still mixes viewport policy with transcript block formatting,
  which makes milestone features harder to add without regressions.

The goal is to harden the implementation so these cases stay boring to change
and test.

## Milestone Outcome

At the end of this milestone, `thndrs` should be usable for its own
development. A contributor should be able to run the TUI inside this repository,
ask it to inspect code, follow tool output, review diffs and test failures,
queue follow-up prompts, use picker surfaces, and continue working through
resize and scrollback without leaving the app because the renderer became
untrustworthy.

## Goals

1. Make the renderer easier to extend without turning it into a widget system.
2. Split viewport policy from transcript block formatting.
3. Add a small pure view layer so advanced live surfaces can be composed before
   terminal output.
4. Strengthen native scrollback, resize, Unicode, cursor, and terminal-backend
   behavior with targeted tests.
5. Support the terminal-agent surfaces required for dogfooding: queued prompts,
   pickers, richer running tool output, focused detail panes, status rows, and
   transcript navigation through native terminal scrollback.

## Advanced Use Cases

### Long Agent Turns

The renderer should handle long assistant streams, reasoning streams, and tool
output without losing the prompt, footer, or cursor. Streaming content should
clip predictably when the terminal is short. Finished content should move into
native scrollback without duplicate rows.

### Tool-Heavy Work

Tool rows need stable formatting for:

- command output;
- search results;
- compiler/test failures;
- diffs;
- JSON;
- long paths;
- truncated output;
- failed and cancelled tools;
- running tools with partial output.

The implementation should make it easy to add a new tool display without
touching viewport commit logic.

### Interactive Live Surfaces

The live region has to arbitrate among prompt rows, command suggestions, file
pickers, model/skill pickers, queued follow-ups, focused detail panes, and
status rows. The renderer needs a simple priority model so these surfaces can
coexist on short terminals.

`thndrs` should follow Pi's permission model for this milestone: the renderer
shows local execution clearly instead of owning a permission workflow. Commands
run under the local user and the surrounding sandbox/project trust model. The
renderer's job is to make execution visible through clear status, tool output,
cancellation/failure rows, and transcript history.

### Resize During Work

Resize should be treated as normal input. The renderer must recompute rows from
semantic state, reset width-dependent scrollback bookkeeping, and restore cursor
placement. It should not try to patch rows wrapped for an old width.

### Multiplexer And Terminal Variance

Native scrollback insertion and final-column behavior vary across terminals and
multiplexers. The backend should keep escape-sequence behavior small, isolated,
and tested with byte-output fixtures. Manual QA should cover the supported
terminal set.

### Non-ASCII Input And Output

The renderer must keep display width and text boundaries separate:

- display width decides cell budgets and cursor columns;
- grapheme boundaries decide edit, wrap, truncate, and backend clipping steps.

This needs coverage for combining marks, ZWJ emoji, regional indicators, CJK,
zero-width characters, long unbroken words, and mixed styled spans.

## Fit Assessments

### Textwrap

Decision: do not adopt Textwrap in this milestone.

The renderer already owns styled spans, padding, prompt cursor placement,
terminal-cell clipping, and Unicode width through its local wrapping helpers.
Textwrap is useful for plain prose, but the dogfooding risk is not generic prose
wrapping. It is keeping styled, mutable, width-sensitive terminal rows correct
while streaming tools, prompt edits, and native scrollback all change around the
same frame.

Adding Textwrap now would create a second wrapping policy without removing the
hard parts: styled span wrapping, grapheme-safe clipping, cursor coordinates,
and backend final-column behavior. The simpler path is to keep
`wrap_text`/`wrap_spans` renderer-owned and add fixtures that prove the current
policy handles CJK, emoji, URLs, long words, and prose.

### Ropey

Decision: do not adopt Ropey in this milestone.

Dogfooding can involve pasted plans, logs, test output, and code snippets, but
prompt storage is not the renderer's main risk. The current `String` plus
grapheme-indexed cursor model is small, understandable, and already lines up
with submitted prompt text, history behavior, mention styling, and rendered
cursor coordinates.

Ropey would help if middle edits in very large prompt buffers were the dominant
cost. For this renderer, visual wrapping and display-width measurement are the
costs that have to be correct on every frame. Changing prompt storage now would
increase the state surface without solving wrapping, clipping, cursor placement,
or transcript rendering. Keep the prompt `String`-backed and add stress tests
for long prompt edits so this decision remains backed by measurements.

### Wheel Scroll

Decision: keep transcript history terminal-native and capture wheel input only
for an active focused surface.

The transcript should use native terminal scrollback because that is the least
surprising behavior for a CLI coding agent. The renderer should not build an
app-owned transcript scroller in this milestone. It should capture wheel or
trackpad input only when focus is inside a bounded surface that clearly owns its
own list, such as a file picker, command picker, model/skill picker, help view,
or tool detail pane.

Supported release targets for this milestone: ordinary terminals, tmux, herdr,
and Zellij. Multiplexer behavior is not a follow-up concern; scrollback,
focused wheel capture, resize, and key handling need to work across those
targets before the milestone is done.

## Proposed Implementation Shape

### Pure View Projection

Add a narrow renderer-owned view projection. It should turn `App` plus terminal
dimensions into plain renderer data before row building or terminal output.

Example shape:

```rust
pub struct RendererView {
    pub transcript: TranscriptView,
    pub live: LiveView,
    pub width: usize,
    pub height: usize,
}
```

This view exists as a staging area for advanced terminal-agent surfaces so the
renderer can decide height, priority, and clipping before writing escape
sequences.

### Transcript Rows

Move transcript-entry row construction into a focused module. That module should
own user, assistant, reasoning, tool, status, error, and startup banner rows.
`region.rs` should own viewport assembly, live-tail clipping, and scrollback
commit bookkeeping.

This split matters because milestone tool displays and transcript card behavior
should not require changes to scrollback insertion logic.

### Live Surface Priority

Make live-region composition explicit. A practical priority order:

1. static footer;
2. prompt rows and cursor;
3. active picker, command surface, help surface, or focused detail pane;
4. queued prompt summary;
5. active tool or assistant live tail;
6. status rows.

The order can change, but it should be represented in code instead of emerging
from append order.

### Width Epochs

Treat each terminal width as an epoch for scrollback commit accounting. On width
change:

- clear visible output;
- reset committed-row counters;
- rebuild rows from transcript entries;
- replay stable rows using the new width;
- place the cursor from the new prompt rows.

### Backend Discipline

Keep `TerminalBackend` mechanical:

- write rows;
- diff rows;
- clear rows;
- insert history lines;
- move/show/hide cursor;
- reset terminal state.

It should not know why a row exists or how much height a surface deserves.

## Implementation Priorities

### P0: Prevent Regressions

These changes make the current renderer safer without changing visible behavior:

- transcript module extraction;
- view projection;
- width-epoch tests;
- backend byte-output fixtures;
- Unicode fixtures across prompt, transcript, picker, and backend.

### P1: Implement Advanced Live Surfaces

These changes make queued prompts, focused surfaces, status rows, and richer
running tool rows part of the milestone:

- live surface priority model;
- live surface height accounting;
- explicit clipping behavior for short terminals;
- direct tests for multiple simultaneous live surfaces.

### P2: Improve Tool And Transcript Rendering

These changes improve tool-heavy dogfooding sessions:

- tool display fixtures for compiler/test output, search output, JSON, diffs,
  and long paths;
- stable row grouping metadata for transcript navigation;
- clear truncation indicators for stored output versus displayed output.

### P3: Terminal Compatibility

These changes reduce surprises outside the default terminal:

- manual QA matrix;
- terminal/multiplexer notes;
- optional backend branches only if a supported terminal needs them.

## Test Strategy

Automated tests should cover implementation behavior, not only screenshots.

Renderer tests:

- row wrapping and padding;
- prompt cursor coordinates;
- live-region composition;
- transcript row construction;
- width change and scrollback replay;
- live-tail clipping;
- picker and footer truncation;
- tool output rendering.

Backend tests:

- final-column avoidance;
- cursor restore after diff writes;
- shorter-frame clearing;
- cursor hide/show transitions;
- scroll-region insertion and reset;
- history insertion followed by live diff render.

Unicode tests:

- combining marks;
- ZWJ emoji;
- regional indicators;
- CJK;
- zero-width characters;
- long unbroken text;
- mixed styled spans.

Manual QA should record terminal name, size, shell, multiplexer, command run,
and observed behavior.

## Success Criteria

- New transcript display behavior can be added without editing scrollback commit
  bookkeeping.
- New live surfaces can be added with a clear height and priority policy.
- Resize during streaming or tool execution does not duplicate rows, lose the
  prompt, or misplace the cursor.
- Unicode input and output remain cell-correct across prompt, transcript,
  picker, footer, and backend writes.
- Native scrollback works in the supported terminal set.
- Renderer tests fail near the source of a regression.

## Resolved Decisions

- Streaming assistant, reasoning, and tool rows remain mutable live rows until
  the entry settles. The renderer should not partially commit a running entry to
  native scrollback because that creates duplicate-row and stale-row failure
  modes on resize.
- Tabs render as spaces inside renderer-controlled rows. Use a fixed four-cell
  tab stop before wrapping so terminal emulator tab settings cannot change row
  width.
- Live-region height uses named internal constants and priority rules, not user
  configuration. If those constants need adjustment, tests should make the
  pressure visible.
- Ordinary terminals, tmux, herdr, and Zellij are the release targets for this
  milestone.
- Transcript navigation for this milestone means native terminal scrollback and
  terminal search, backed by stable transcript row grouping metadata. Do not
  build an app-owned transcript scroller before dogfooding proves native
  scrollback insufficient.
