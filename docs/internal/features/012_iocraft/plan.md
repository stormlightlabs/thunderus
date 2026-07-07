# iocraft Surface Expansion

Status: Draft
Captured: 2026-07-07

## Objective

Use iocraft where it reduces focused-surface layout complexity while preserving
the existing `thndrs` renderer contract: semantic app state becomes bounded
surface rows, committed transcript history stays native-scrollback-friendly,
and iocraft never owns app state or terminal output.

This feature is a refinement of the v0.1 UI work archived in
`docs/internal/archive/v0.1.md`. It should move only the surfaces where
iocraft is a clear simplification, not replace the renderer wholesale.

## Current State

- `src/cli/renderer/view.rs` projects app state into semantic view records and
  routes command picker, file picker, and help focused surfaces through
  `src/cli/renderer/adapter.rs`.
- `src/cli/renderer/adapter.rs` renders iocraft elements to `Canvas`, converts
  them into existing `Row`/`Span` values, applies renderer palette styling, and
  snapshots focused surfaces.
- Permission prompts, first-run recovery, tool detail, diff detail,
  transcript rows, and the main prompt editor still use direct row builders.
- `src/cli/renderer/live.rs`, `region.rs`, `row.rs`, `style.rs`, and
  `layout.rs` remain the source of truth for viewport policy, cursor behavior,
  scrollback commits, row padding, truncation, and Unicode wrapping.
- Public docs already describe focused surfaces, command/file interaction,
  detail panes, tables, and trust wording.

## Success Criteria

- More bounded surfaces use the iocraft adapter only when the rendered output
  matches or improves current behavior.
- Permission/recovery priority is preserved: blocking prompts cannot be masked
  by help, pickers, or optional detail surfaces.
- Adapter theme input is explicit enough that tests do not rely on mutating
  global theme state.
- iocraft output keeps selected, muted, warning, error, diff, clipped-content,
  and table semantics through conversion to `Row`/`Span`.
- Tests cover normal, narrow, tiny-height, Unicode, scroll/clipping, and
  priority behavior for every migrated surface.
- Public and internal docs describe the final boundary without overstating what
  iocraft owns.

## Technical Plan

### Adapter Boundary

Keep the adapter boundary small:

```rust
pub struct SurfaceRenderInput<'a> {
    pub surface: &'a FocusedSurfaceView,
    pub theme: &'a SurfaceThemeView,
    pub width: usize,
    pub height: usize,
}
```

The adapter may add helper types for layouts, line roles, scroll metadata, and
theme resolution, but it should still return `Vec<Row>` and avoid terminal
writes. If a surface needs state changes, route events through app messages and
semantic view updates rather than storing state inside iocraft components.

### Surface Migration Decisions

1. **Model and skill pickers.** They already resemble command/file pickers and
   are the next adapter expansion.
2. **Structured table surfaces.** Consolidate table output for markdown tables,
   command help, model lists, tool inventories, and diagnostics where semantic
   table data already exists.
3. **Diff detail.** Move through iocraft with a fixed title, scrollable body,
   diff-aware line roles, clipping indicators, and tiny-height behavior.
4. **Tool detail.** Move through iocraft with wrapped-row scrolling,
   clipped-above/below indicators, status styling, and output truncation.
5. **Setup/recovery forms.** Enrich `SetupFormView` first, then move recovery
   through iocraft with provider, stage copy, actions, selected action,
   validation, OAuth status, and hidden secret behavior represented
   semantically.

### Theme Plan

Move adapter styling toward explicit role data:

- derive surface colors from `SurfaceThemeView` or a richer `UiTheme` argument;
- keep `style::palette()` use at the caller/theme construction boundary, not
  deep inside generic adapter helpers;
- avoid tests that mutate the global current theme in parallel;
- preserve styled snapshots for selected rows, muted hints, diff lines, errors,
  and warnings.

### Layout Plan

Use iocraft for bounded flex/layout problems:

- fixed title + scrollable body + footer hint;
- selectable lists with selected row background;
- tables with fixed/percent/flexible columns and fallback text;
- forms with label/value/action rows.

Keep direct renderer ownership for:

- committed transcript rows;
- main prompt editor;
- cursor placement;
- terminal resize and scrollback replay;
- Unicode wrapping/truncation primitives.

## Boundaries

Always:

- Preserve existing keybindings and app update paths.
- Keep iocraft behind `src/cli/renderer/adapter.rs`.
- Add or update focused snapshot tests before replacing a direct surface.
- Keep permission and recovery prompts higher priority than optional surfaces.
- Run the Rust verification checklist after code changes.

Approval required:

- Adding new dependencies beyond iocraft.
- Changing the public keybinding model.
- Changing permission or recovery semantics.

Never:

- Call iocraft fullscreen/render-loop APIs from the `thndrs` TUI.
- Let iocraft own `App` state or write directly to stdout/stderr.
- Hide provider secrets, permission decisions, or recovery choices behind a
  surface migration.
- Drop native scrollback grouping or resize replay guarantees.

## Decisions

- Keep the main prompt editor on the direct renderer for this feature.
- Keep committed transcript rows on the direct renderer for this feature.
- Move model picker, skill picker, structured tables, diff detail, tool detail,
  and setup/recovery forms through iocraft in that order.
- Keep permission and recovery priority rules in `view.rs`/live composition,
  not inside iocraft components.
- Use mouse interaction only for focused bounded surfaces and only when it
  respects the existing mouse CLI/config settings.

## Verification

- `cargo fmt`
- `cargo clippy --fix --allow-dirty --allow-staged`
- `cargo clippy`
- `cargo test`
- Focused snapshot tests for every migrated surface.
- Public docs build with `pnpm --dir docs build` if public docs change.
- Manual review of snapshots for clarity, priority, tiny height, and narrow
  width behavior.

## Risks

- iocraft can simplify surface layout while making row-budget behavior less
  obvious; every migration needs explicit height tests.
- Theme role plumbing could become more complex than direct row styles if it is
  over-generalized too early.
- Setup/recovery must carry provider-specific semantics before migration.
- Tool detail correctness depends on wrapped-row scrolling and clipped-content
  indicators, not just visible text.
