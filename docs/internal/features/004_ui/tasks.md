# UI Usability Tasks

Status: Draft
Captured: 2026-07-03

## P0: Define The UI Contract

- [ ] Audit current renderer/app state and list every visible UI state.
- [ ] Define final transcript row families: user, assistant, reasoning, tool,
      edit, diff, status, error, notice, and cancelled.
- [ ] Define semantic visual treatments for transcript rows, prompt state,
      focused surfaces, diffs, warnings, and errors without relying on color
      changes.
- [ ] Define the compact orientation band fields and truncation rules.
- [ ] Define exact prompt states: idle, drafting, suggesting, running,
      queued, failed, retryable, and cancelled.
- [ ] Implement the supported focused surfaces: command picker, file picker,
      help, tool detail, and diff detail.
- [ ] Implement queued follow-ups as summary-only after submission.
- [ ] Preserve existing keybindings from `usage/keybinds.md` and implement only
      the UI-feature deltas from `plan.md`: running-turn `Ctrl+C`, `Ctrl+O`,
      suggestion `Tab`, and inline `@` file mentions.
- [ ] Render the trust label as
      `local user · workspace-contained tools · no TUI sandbox`.

## P1: Implement View State

- [ ] Add renderer-owned view records for transcript rows, prompt surface,
      orientation band, and focused surfaces.
- [ ] Keep view records free of terminal backend and crossterm types.
- [ ] Map app/runtime events into stable row kinds before formatting rows.
- [ ] Represent running, succeeded, failed, and cancelled tool states in view
      data.
- [ ] Represent edit and diff summary states in view data.
- [ ] Represent queued follow-up state in view data.
- [ ] Represent prompt suggestions for `:` command mode and `@` file mentions.
- [ ] Add narrow-width truncation metadata for status/orientation fields.

## P2: Build Transcript And Tool Rows

- [ ] Render user and assistant rows with stable prefixes and wrapping.
- [ ] Render reasoning rows as distinct live/summarized transcript entries.
- [ ] Render tool start rows with operation, target, and elapsed/status fields.
- [ ] Render live tool output previews with bounded height.
- [ ] Render tool settlement rows for success, failure, and cancellation.
- [ ] Render truncation indicators that distinguish previewed output from stored
      full output.
- [ ] Render file edit summaries with path and operation status.
- [ ] Render diff summaries with changed file and added/removed counts when
      available.
- [ ] Render warning/error rows with enough detail to act on the problem.
- [ ] Keep transcript row construction separate from viewport commit logic.

## P3: Build The Prompt Surface

- [ ] Render the editable prompt with stable cursor placement.
- [ ] Add submit/stop state to the prompt surface.
- [ ] Add queued follow-up summary above or within the prompt surface.
- [ ] Add command suggestions for `:` commands.
- [ ] Keep `Ctrl+P` as the primary file picker entry point.
- [ ] Add inline `@` file mention suggestions for workspace paths.
- [ ] Add draft history navigation that does not conflict with picker focus.
- [ ] Preserve prompt text after failed submit.
- [ ] Add retry affordance for retryable failures.
- [ ] Keep prompt, footer, and orientation visible under streaming pressure.

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
      detail, and diff detail.
- [ ] Add interaction tests for focus transitions and escape behavior.
- [ ] Add tests for queued follow-up rendering while streaming.
- [ ] Add tests for prompt preservation after failed submit.
- [ ] Add tests for resize during running tool output and focused surfaces.
- [ ] Add Unicode snapshots covering combining marks, ZWJ emoji, regional
      indicators, CJK, zero-width characters, long words, and mixed styled
      spans.

## P7: Docs

- [ ] Update public TUI/interaction docs with the final prompt controls.
- [ ] Update keybinding docs with the final keyboard model.
- [ ] Document visible trust/sandbox wording without overstating enforcement.
- [ ] Document tool output preview versus full stored output.
- [ ] Document focused detail surfaces and when they capture scroll.
- [ ] Cross-link notebook research: `ui.md`, `ui-patterns.md`, `pi.md`, and
      `ratatui.md`.

## Validation Commands

- [ ] `cargo fmt`
- [ ] `cargo clippy --fix --allow-dirty --allow-staged`
- [ ] `cargo clippy`
- [ ] `cargo test renderer`
- [ ] `cargo test input`
- [ ] `cargo test`
- [ ] `pnpm --dir docs build`
