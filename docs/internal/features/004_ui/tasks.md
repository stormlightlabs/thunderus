# UI Usability Tasks

Status: Draft
Captured: 2026-07-03
Updated: 2026-07-07

## P0: Define The UI Contract

- [x] Audit current renderer/app state and list every visible UI state.
- [x] Read `docs/src/content/docs/notebook/iocraft.md` and the iocraft
      `scrolling.rs`, `form.rs`, and `table.rs` examples as adoption evidence.
- [x] Define final transcript row families: user, assistant, reasoning, tool,
      edit, diff, status, error, notice, and cancelled.
- [x] Define semantic visual treatments for transcript rows, prompt state,
      focused surfaces, diffs, warnings, and errors without relying on color
      changes.
- [x] Define semantic theme tokens above `Palette`: transcript row roles,
      prompt roles, focused surfaces, setup forms, tables, statuses, and diffs.
- [x] Define the compact orientation band fields and truncation rules.
- [x] Define exact prompt states: idle, drafting, suggesting, running,
      queued, failed, retryable, and cancelled.
- [x] Define the supported focused surfaces: command picker, file picker, help,
      tool detail, diff detail, transcript lens, setup form, and structured
      table surface.
- [x] Decide and document the iocraft integration mode: direct iocraft-backed
      bounded surfaces through a renderer adapter.
- [x] Implement queued follow-ups as summary-only after submission.
- [x] Preserve existing keybindings from `usage/keybinds.md` and implement only
      the UI-feature deltas from the [plan](./plan.md): running-turn `Ctrl+C`, `Ctrl+O`,
      suggestion `Tab`, and inline `@` file mentions.

## P1: Implement View State

- [x] Add renderer-owned view records for transcript rows, prompt surface,
      orientation band, and focused surfaces.
- [x] Keep view records free of terminal backend and crossterm types.
- [x] Map app/runtime events into stable row kinds before formatting rows.
- [x] Represent running, succeeded, failed, and cancelled tool states in view
      data.
- [x] Represent edit and diff summary states in view data.
- [x] Represent queued follow-up state in view data.
- [x] Represent prompt suggestions for `:` command mode and `@` file mentions.
- [x] Add narrow-width truncation metadata for status/orientation fields.
- [x] Add view records for setup/recovery forms: fields, focus index,
      validation errors, hidden/secret values, submit/cancel actions, and
      completion state.
- [x] Add view records for semantic tables: header, rows, column alignment,
      width policy, selected row when applicable, and narrow fallback text.
- [x] Add a renderer adapter boundary for iocraft surfaces that accepts
      semantic view data plus theme/width/height and returns existing `Row`
      values.
- [x] Ensure iocraft never owns app state or writes directly to stdout/stderr
      inside the TUI.

## P2: Build Transcript And Tool Rows

- [x] Render user and assistant rows with stable prefixes and wrapping.
- [x] Render reasoning rows as distinct live/summarized transcript entries.
- [x] Render tool start rows with operation, target, and elapsed/status fields.
- [x] Render live tool output previews with bounded height.
- [x] Render tool settlement rows for success, failure, and cancellation.
- [x] Render truncation indicators that distinguish previewed output from stored
      full output.
- [x] Render file edit summaries with path and operation status.
- [x] Render diff summaries with changed file and added/removed counts when
      available.
- [x] Render warning/error rows with enough detail to act on the problem.
- [x] Keep transcript row construction separate from viewport commit logic.
- [x] Implement an iocraft-backed transcript lens based on `scrolling.rs`:
      fixed-height content, keyboard scroll, clipped-content indicator, and
      optional mouse capture only while focused.
- [x] Keep committed transcript rows native-scrollback-friendly even if the
      transcript lens is implemented with iocraft.
- [x] Add table-aware rendering for markdown tables and structured tool output,
      using `table.rs`-style column widths, alignment, header separators, and
      narrow fallback behavior.

## P3: Build The Prompt Surface

- [x] Render the editable prompt with stable cursor placement.
- [x] Add submit/stop state to the prompt surface.
- [x] Add queued follow-up summary above or within the prompt surface.
- [x] Add command suggestions for `:` commands.
- [x] Keep `Ctrl+P` as the primary file picker entry point.
- [x] Add inline `@` file mention suggestions for workspace paths.
- [x] Add draft history navigation that does not conflict with picker focus.
- [x] Preserve prompt text after failed submit.
- [x] Add retry affordance for retryable failures.
- [x] Keep prompt, footer, and orientation visible under streaming pressure.
- [x] Decide not to replace the main prompt editor with iocraft `TextInput`
      in this feature; setup forms may use `TextInput`, but the main prompt
      must keep existing cursor, Unicode, history, hidden-input, mention, and
      suggestion behavior.

## P4: Build Focused Surfaces

- [ ] Implement a command picker with keyboard navigation and stable
      descriptions.
- [ ] Implement a file picker with path truncation and fuzzy-match display.
- [ ] Implement help as a bounded focused surface, not a permanent panel.
- [ ] Implement `Ctrl+O` detail priority: failed tool output, truncated tool
      output, latest edit diff, then latest warning/error detail.
- [ ] Implement tool output detail for failed or truncated output.
- [ ] Implement diff detail as unified diff with a per-file summary header.
- [ ] Make focused surfaces capture scroll/navigation while focused.
- [ ] Make `Esc` close focused surfaces and return focus to the prompt.
- [ ] Add tiny-height behavior for each focused surface.
- [ ] Implement setup/recovery forms using the `form.rs` pattern: reusable
      fields, Tab/BackTab focus cycling, validation rows, hidden secret fields,
      multiline support where needed, submit, and cancel.
- [ ] Implement structured table surfaces using the `table.rs` pattern for
      command help, model/provider lists, tool inventories, diagnostics, and
      markdown tables.
- [ ] Implement a `Canvas`-to-`Row` conversion path that
      preserves text, background, foreground, weight, underline, dim, selected,
      muted, warning, error, and diff semantics.
- [ ] Use `element!`/`component` only behind the focused surface adapter; do not
      call `render_loop` or `fullscreen` from the `thndrs` TUI.

## P5: Interaction And Runtime Wiring

- [ ] Wire keyboard events through one update path.
- [ ] Make `Enter` submit or accept the focused selection based on focus state.
- [ ] Make stop/cancel settle into stable transcript rows.
- [ ] Make queued input visible while a turn is running.
- [ ] Make queued follow-up input run after the current turn.
- [ ] Make `Ctrl+T` toggle the running input target between steering and
      follow-up.
- [ ] Make command/file suggestions disappear predictably after submit, escape,
      or invalid draft state.
- [ ] Ensure mouse/wheel input affects only focused bounded surfaces.
- [ ] Ensure resize rebuilds rows from semantic view state.
- [ ] Route transcript lens scroll keys and wheel events only while the lens is
      focused.
- [ ] Route setup form field changes through app messages so form state remains
      inspectable and testable.
- [ ] Route table/list selection events through the same focus model as command
      and file pickers.

## P6: Tests

- [ ] Add transcript snapshots for every row family at normal width.
- [ ] Add transcript snapshots for every row family at narrow width.
- [ ] Add prompt snapshots for idle, drafting, suggesting, running, queued,
      failed, retryable, and cancelled states.
- [ ] Add orientation band snapshots for normal, narrow, and tiny widths.
- [ ] Add tool output snapshots for running, success, failure, cancellation,
      truncation, long paths, JSON, diffs, and compiler/test failures.
- [ ] Add edit and diff summary snapshots.
- [ ] Add focused-surface snapshots for command picker, file picker, help, tool
      detail, diff detail, transcript lens, setup form, and structured table
      surface.
- [ ] Add interaction tests for focus transitions and escape behavior.
- [ ] Add tests for queued follow-up rendering while streaming.
- [ ] Add tests for prompt preservation after failed submit.
- [ ] Add tests for resize during running tool output and focused surfaces.
- [ ] Add Unicode snapshots covering combining marks, ZWJ emoji, regional
      indicators, CJK, zero-width characters, long words, and mixed styled
      spans.
- [ ] Add theme snapshots for every built-in theme covering at least one
      transcript row, one focused surface, one setup form field, one table, and
      one diff.
- [ ] Add transcript lens tests for keyboard scroll, clipped content, optional
      mouse capture, narrow width, tiny height, and native-scrollback
      preservation.
- [ ] Add setup form tests for Tab/BackTab focus cycling, hidden secret input,
      multiline input, validation errors, submit, cancel, and tiny-height
      layout.
- [ ] Add table tests for markdown parsing, percent/fixed/flexible columns,
      numeric alignment, long cells, no-color fallback, narrow fallback, and
      Unicode cell content.
- [ ] Add canvas/mock-terminal tests proving events and rendered canvases
      convert deterministically into `Row` snapshots.

## P7: Docs

- [ ] Update public TUI/interaction docs with the final prompt controls.
- [ ] Update keybinding docs with the final keyboard model.
- [ ] Document visible trust/sandbox wording without overstating enforcement.
- [ ] Document tool output preview versus full stored output.
- [ ] Document focused detail surfaces and when they capture scroll.
- [ ] Document setup/recovery forms and how hidden credentials are handled.
- [ ] Document markdown table rendering and narrow fallback behavior.
- [x] Document the iocraft adoption decision and the reasons for preserving
      native transcript scrollback.
- [ ] Cross-link notebook research: `ui.md`, `ui-patterns.md`, `pi.md`, and
      `ratatui.md`, `iocraft.md`.
