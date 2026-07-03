# Rendering Engine Hardening Tasks

Status: Draft
Captured: 2026-07-03

## P0: Prevent Regressions

- [x] Move transcript-entry row construction out of `src/renderer/region.rs`
      into a focused transcript module.
- [x] Add direct row snapshots for user, assistant, reasoning, tool, status,
      error, and startup banner entries.
- [x] Add normal-width and narrow-width transcript snapshots for every `Entry`
      variant.
- [x] Add tool row fixtures for running, ok, failed, highlighted, truncated,
      and path-shortened output.
- [x] Add width-epoch tests proving committed rows reset and replay after
      terminal width changes.
- [x] Add tests proving mutable live rows are not committed as stable history.
- [x] Add backend tests for scroll-region insertion and reset.
- [x] Add backend byte-output fixture for history insertion followed by live
      diff rendering.
- [x] Add prompt Unicode snapshots for combining marks, ZWJ emoji, regional
      indicators, CJK, zero-width characters, long words, and explicit newlines.
- [x] Add transcript, picker, footer, and backend Unicode fixtures for the same
      width classes where relevant.

## P1: Implement Advanced Live Surfaces

- [x] Introduce a small renderer-owned view projection from `App` plus terminal
      dimensions.
- [x] Keep the view projection free of crossterm types and terminal writes.
- [x] Add view projection tests for idle, working, streaming, accessory, narrow,
      and tiny-height states.
- [x] Make live-region composition use explicit surface priority instead of
      incidental append order.
- [x] Represent Pi-style execution in renderer state with visible status,
      running tool output, failure, cancellation, and transcript rows.
- [x] Preserve footer and prompt visibility under streaming pressure.
- [x] Add tests for active picker plus streaming output.
- [x] Add tests for queued prompt summary plus running tool output.
- [x] Add tests for focused detail pane plus running tool output.
- [x] Add tests for tiny-height clipping with prompt, footer, accessory, and
      live tail present.

## P2: Improve Tool And Transcript Rendering

- [x] Add fixtures for compiler or test failure output.
- [x] Add fixtures for search result output.
- [x] Add fixtures for JSON output.
- [x] Add fixtures for diff output.
- [x] Add fixtures for long absolute paths and workspace-relative path display.
- [x] Add row grouping metadata needed for transcript navigation.
- [x] Add display truncation indicators that distinguish stored output from
      rendered output.
- [x] Add cancelled-tool rendering if the app exposes cancelled tool state.
- [x] Keep `wrap_text` and `wrap_spans` renderer-owned; do not add Textwrap for
      this milestone.
- [x] Add wrapping fixtures for CJK, emoji, URLs, prose, long words, mixed
      styled spans, and terminal-cell clipping.
- [x] Keep prompt storage `String`-backed; do not add Ropey for this milestone.
- [x] Add long-prompt stress tests for 10 KB, 100 KB, and 1 MB prompts with
      edits at start, middle, and end.
- [x] Add timing assertions or benchmark notes that separate prompt editing
      cost from visual wrapping and display-width measurement cost.

## P3: Terminal Compatibility

- [ ] Run manual QA at 80x24.
- [ ] Run manual QA at narrow width.
- [ ] Run manual QA at short height.
- [ ] Verify prompt wrap and cursor movement in a real terminal.
- [ ] Verify multiline prompt editing in a real terminal.
- [ ] Verify file picker and command suggestions in a real terminal.
- [ ] Verify streaming assistant text in a real terminal.
- [ ] Verify running tool output in a real terminal.
- [ ] Verify native scrollback with mouse or trackpad.
- [ ] Verify wheel capture only inside active focused surfaces: picker, help,
      model/skill picker, command picker, and long tool detail pane.
- [ ] Verify unfocused transcript history remains usable through native
      terminal scrollback.
- [ ] Verify resize during and after streaming.
- [ ] Verify tmux behavior as part of the release target.
- [ ] Verify herdr behavior as part of the release target.
- [ ] Verify Zellij behavior as part of the release target.
- [ ] Record terminal name, shell, dimensions, multiplexer, command, and result
      in a status file.

## Validation Commands

- [x] `cargo test renderer`
- [x] `cargo test input`
- [x] `cargo test`
