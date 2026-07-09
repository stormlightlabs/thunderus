# iocraft Surface Hardening And Expansion

Status: Draft
Captured: 2026-07-08

## Objective

Harden the existing iocraft focused-surface adapter before expanding it to more
TUI surfaces.

iocraft should remain a bounded layout/rendering helper behind the existing
direct renderer. It must never own app state, terminal I/O, native scrollback,
cursor placement, prompt editing, or committed transcript rendering.

## Source Review

Reviewed source material:

- `docs/src/content/docs/notebook/iocraft.md`
- `docs/src/content/docs/notebook/ui.md`
- `docs/src/content/docs/notebook/ui-patterns.md`
- `docs/src/content/docs/notebook/ratatui-testing.md`
- `docs/src/content/docs/notebook/text-input-libraries.md`
- `docs/src/content/docs/notebook/yoga.md`
- `docs/src/content/docs/notebook/yoga-gridland.md`
- `src/cli/renderer/adapter.rs`
- `src/cli/renderer/view.rs`
- `src/cli/renderer/live.rs`
- `src/cli/renderer/region.rs`
- `src/cli/renderer/row.rs`
- `src/cli/renderer/style.rs`

Notebook conclusions promoted into this plan:

- terminal-agent transcripts should keep native scrollback;
- focused bounded surfaces are useful for command/file selection, help, diff
  detail, long tool output, and setup forms;
- iocraft's useful role is layout/canvas/testing, not whole-app ownership;
- visual terminal output needs deterministic fixed-size snapshots;
- Unicode boundaries, display width, wrapping, and cursor placement remain
  renderer-owned concerns.

## Current State

The existing code already uses an iocraft adapter more broadly than the older
plan assumed:

- `src/cli/renderer/adapter.rs` is the only module that calls iocraft;
- the adapter renders into an iocraft `Canvas` and converts back into existing
  `Row`/`Span` values;
- command picker, file picker, help, tool detail, diff detail, transcript lens,
  setup form, and structured table variants are already routed through the
  adapter;
- semantic surface data lives in `src/cli/renderer/view.rs`;
- live region, scrollback commits, row widths, cursor placement, prompt rows,
  and transcript row rendering remain direct-renderer responsibilities.

The main risk is no longer whether iocraft can render focused surfaces. The
main risk is losing renderer invariants at the adapter boundary:

- row counts must stay within content and viewport budgets;
- clipped output needs visible indicators;
- theme roles should be explicit and testable;
- selected, muted, warning, error, diff, and table semantics must survive
  canvas conversion;
- Unicode and narrow terminal behavior must remain deterministic;
- optional focused surfaces must not mask permissions, setup recovery, or other
  blocking states.

## Implementation Decisions

- Harden first, then expand.
- Keep iocraft behind `src/cli/renderer/adapter.rs`.
- Keep adapter output as `Vec<Row>`.
- Keep `SurfaceRenderInput` renderer-owned and small.
- Keep committed transcript rows and the main prompt editor direct-rendered.
- Keep permission/setup recovery priority in app/view/live composition, not in
  iocraft components.
- Do not add new dependencies beyond iocraft for this feature without approval.
- Expand to new surfaces only after hardening tickets pass.

## Success Criteria

- Adapter output never exceeds the intended row budget.
- Clipped-above and clipped-below states are visible for scrollable surfaces.
- Theme input is explicit enough that tests do not mutate global theme state.
- Canvas conversion preserves meaningful styles and selected row treatment.
- Normal, narrow, tiny-height, Unicode, and long-line snapshots cover every
  iocraft-rendered focused surface.
- Permission and setup/recovery surfaces cannot be hidden behind optional help,
  picker, table, or detail surfaces.
- The renderer still owns terminal writes, native scrollback, prompt cursor
  placement, resize replay, and transcript commits.
- Expansion tickets have a clear readiness gate instead of migrating surfaces
  because iocraft exists.

## Adapter Contract

The adapter input remains a semantic view record plus explicit render context:

```text
surface
theme
width
height
```

The adapter may add helper structs for line roles, scroll metadata, table
layout, clipping state, and theme resolution. It should still:

- return only `Vec<Row>`;
- avoid crossterm terminal types;
- avoid terminal writes;
- avoid app-state mutation;
- avoid iocraft fullscreen/render-loop APIs;
- avoid storing focus, scroll, selection, or form state inside iocraft
  components.

### Rust Design Constraints

Keep the renderer boundary typed and side-effect free:

- represent clipping as an enum or small struct, not as sentinel strings;
- represent line roles, table width policy, alignment, and surface state as
  typed values in `view.rs`;
- keep row-budget calculation pure and directly unit-testable;
- keep palette lookup and terminal-specific styling at a narrow renderer
  boundary;
- avoid global mutable theme state in tests;
- prefer concrete helper functions over traits unless there are multiple real
  renderer implementations;
- do not introduce new dependencies for wrapping, measurement, or layout until
  fixtures prove the current local helper is insufficient;
- avoid `unwrap`, `expect`, `panic!`, `todo!`, and `unimplemented!` in runtime
  rendering paths unless they guard a documented invariant;
- keep prompt cursor placement, Unicode width calculation, and native scrollback
  outside iocraft components.

## Hardening Areas

### Row Budget And Clipping

Scrollable focused surfaces should distinguish:

- no clipping;
- clipped above;
- clipped below;
- clipped above and below.

The indicator must fit within the surface row budget and must not replace the
surface title in tiny-height cases unless there is no room for both.

Apply this first to:

- tool detail;
- diff detail;
- transcript lens;
- setup/recovery forms where validation or actions overflow.

### Theme Boundary

Theme roles should be renderer-owned semantic inputs:

- text;
- muted;
- selected;
- warning;
- error;
- diff added;
- diff removed;
- table header or title if it proves useful.

Palette lookup should happen at a clear boundary. Tests should be able to
exercise adapter behavior with explicit role data rather than relying on global
theme mutation.

### Surface Semantics

Every focused surface should preserve the semantic meaning needed by tests and
users:

- selected row;
- disabled or muted hints;
- warning and error rows;
- validation errors;
- hidden secret fields;
- diff added/removed lines;
- table alignment and fallback rows;
- long output truncation and scroll state.

The adapter can change layout, but it must not erase these distinctions.

### Priority And Interaction

Focused surfaces are optional unless they represent blocking work. The app/view
composition must prove:

- pending permission prompts outrank help, pickers, and detail surfaces;
- setup/recovery outranks optional surfaces;
- `Esc` closes optional focused surfaces;
- selection and scroll keys mutate app state, not adapter state;
- prompt draft preservation remains unaffected by focused surface rendering.

### Unicode, Width, And Resize

The direct renderer continues to own cell width, wrapping, prompt cursor
coordinates, and resize replay. Adapter tests should still cover:

- CJK;
- emoji;
- combining marks;
- long unbroken paths;
- markdown tables with wide cells;
- tiny and narrow terminals.

## Expansion Gate

Do not add new iocraft-rendered surface families until hardening is complete.

Expansion is allowed when:

- row-budget tests are present for every existing adapter surface;
- clipping indicators exist for scrollable surfaces;
- priority tests prove optional surfaces do not mask blocking states;
- snapshot coverage exists for normal, narrow, tiny-height, Unicode, and
  long-line cases;
- a reviewer can explain what complexity iocraft removed for the next surface.

Potential expansion candidates after the gate:

- model picker and skill picker if they still have direct surface paths;
- richer setup/recovery stages after setup semantics are stable;
- additional structured diagnostic tables;
- future session picker after session navigation is a product feature.

## Testing Plan

Unit and snapshot tests:

- adapter row counts for every surface;
- clipped-above/below indicators;
- clipping state projection from typed data;
- theme role mapping;
- selected-row styling;
- diff added/removed styling;
- table fixed/percent/flexible width behavior;
- setup form hidden secret rendering;
- Unicode and long-line rendering;
- normal, narrow, and tiny-height snapshots.

App/view/region tests:

- permission priority;
- setup/recovery priority;
- optional detail replacement behavior;
- `Esc` closes optional surfaces;
- selection and scroll update app state;
- resize tests for migrated surfaces;
- cursor visibility near prompt chrome.

Manual review:

- inspect snapshots for clarity and row-budget behavior;
- verify no iocraft fullscreen/render-loop APIs are called from the TUI;
- compare detail surfaces against full stored output to ensure no output is
  silently lost.

## Commands

For Rust changes:

```text
cargo fmt
cargo clippy --fix --allow-dirty --allow-staged
cargo clippy
cargo test
```

For public docs changes:

```text
pnpm --dir docs build
```

This planning rewrite changes only internal docs, so no docs build is required
for the planning artifact itself.

## Boundaries

Always:

- keep iocraft behind the adapter;
- keep renderer output as `Row`/`Span`;
- keep adapter helpers pure where possible;
- use typed surface state instead of display-string control flow;
- keep terminal I/O in the direct renderer/backend;
- add or update focused snapshots before changing visible layout;
- preserve permission and setup/recovery priority;
- keep prompt editor and committed transcript rendering direct-rendered.

Ask first:

- adding dependencies;
- adopting additional iocraft runtime APIs;
- changing keybindings;
- changing permission or setup/recovery semantics;
- moving prompt editing, cursor placement, or native scrollback into iocraft.

Never:

- call iocraft fullscreen/render-loop APIs from the `thndrs` TUI;
- let iocraft own `App` state;
- write to stdout/stderr from the adapter;
- hide provider secrets, permission decisions, or setup recovery choices behind
  layout migration;
- parse renderer behavior from styled display text in app logic;
- add dependencies for wrapping/layout without fixture-backed evidence;
- drop native scrollback grouping or resize replay guarantees.

## Deferred Milestones

- New surface migrations after the hardening gate.
- Rich setup/recovery forms after setup and reasoning validation semantics are
  stable.
- A future session picker after session navigation is a product feature.
- Plain-text wrapping library evaluation, if renderer fixtures prove current
  wrapping is insufficient.

## Risks And Open Questions

- iocraft can make layout more declarative while hiding row-budget decisions;
  tests must pin the row contract.
- Theme role abstraction can become more complex than direct row styling if it
  grows before real duplication appears.
- Setup/recovery surfaces carry security-sensitive semantics, especially hidden
  secrets and credential-write confirmation.
- Long tool output correctness depends on scroll/clipping metadata, not just
  visible text.
