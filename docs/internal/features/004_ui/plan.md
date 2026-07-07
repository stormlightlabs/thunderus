# UI Usability Plan

Status: Draft
Owner: thndrs maintainers
Captured: 2026-07-03
Updated: 2026-07-07

## Background

The current `thndrs` UI has moved in a Pi-like direction: sparse transcript,
bottom prompt, visible terminal execution, and minimal chrome. That is a good
baseline for a local coding harness, but it risks becoming visually and
behaviorally indistinct from Pi if we stop there.

Reference review points to a more specific direction:

- Pi contributes the right simplicity bias: transcript-first, typed events,
  queued steering/follow-up, visible local execution, and native scrollback.
- Gridland contributes cell-budget discipline, role-aware transcript rows,
  prompt suggestions, and an optional two-panel shell.
- Codex CLI contributes review ergonomics: plans, inline approval, queued input,
  slash commands, file search, prompt history, copy, prompt editor, and MCP
  entry points.
- OpenCode contributes clear agent/mode/permission declarations. `thndrs` does
  not include multi-agent controls in this feature because the current runtime
  does not expose user-switchable agent modes.
- Goose contributes extension/provider breadth and MCP setup patterns, but that
  would be premature as first-screen UI.
- Aider contributes practical coding flow: repo orientation, command help,
  diffs, lint/test feedback, and undo/review loops.
- iocraft contributes a declarative component model, `element!` and
  `component` macros, built-in `View`, `Text`, `MixedText`, `TextInput`,
  `ScrollView`, hooks, Taffy-backed flexbox layout, `Canvas` rendering, and
  mock-terminal testing. Its ideas are especially relevant to bounded prompt
  accessories, setup flows, structured transcript content, and focused
  surfaces, where the current row builder code can become noisy.

The conclusion is not "add everything." The UI should stay small, but it needs
to make the agent easier to operate, inspect, interrupt, and trust.

## Problem

The renderer can display the current app, but usability gaps will become obvious
as `thndrs` handles real coding work:

- the prompt does not yet carry enough state for command discovery, queued
  follow-ups, file mentions, submit/stop, and retry;
- tool output can be hard to scan when output is long, failed, truncated, or
  still running;
- edits and diffs need a reviewable surface before users can trust the agent;
- permission/trust/sandbox state needs honest, compact wording;
- help and commands need to be discoverable without a permanent manual panel;
- the UI needs its own identity rather than copying Pi's exact sparse transcript
  tone;
- richer surfaces must coexist with native scrollback and the row-first
  renderer from the rendering-engine milestone.
- direct adoption of iocraft's fullscreen/render-loop model could conflict
  with `thndrs`' native scrollback and live-region architecture if it replaces
  the renderer wholesale too early.

## Milestone Outcome

At the end of this feature, a user should be able to run `thndrs` in a real
repository, understand the current workspace/model/trust boundary, draft and
queue prompts, discover commands, follow tool execution, inspect failures,
review proposed edits, and recover from cancellation or retry without leaving
the terminal.

The UI should still feel like a terminal tool: dense enough for repeated work,
quiet enough for long sessions, and built from explicit rows and focused
temporary surfaces instead of a dashboard.

## Goals

1. Establish a `thndrs` UI identity that draws from Pi and Gridland but is not a
   Pi clone.
2. Make the prompt a compound control surface for text, command/file
   suggestions, queued input, submit/stop, and status.
3. Make tool execution and edit review legible through stable transcript rows
   and focused detail panes.
4. Preserve native scrollback for the main transcript while allowing bounded
   focused surfaces to capture scrolling.
5. Show workspace, model/provider, session, and trust/sandbox state in one
   compact orientation band.
6. Back the UI with snapshot tests that cover narrow terminals, short
   terminals, Unicode, running streams, failed tools, diffs, and queued input.

## Product Principles

### Quiet, Not Bare

The screen should avoid decorative boxes and filler text, but it should not hide
state. Important state gets a distinct row shape, prefix, separator, indentation,
badge, or focused detail surface. The goal is visually distinct elements, not a
new color theme.

### Scan Before Read

Users should be able to scan a long session by row type:

- prompt;
- assistant;
- reasoning;
- tool running;
- tool success;
- tool failure;
- edit/diff;
- warning;
- cancelled;
- queued follow-up.

### Detail On Demand

Long command output, diffs, search results, and help should have bounded detail
views. The transcript keeps a compact summary and the focused surface handles
navigation.

### Honest Control

Stop, retry, queue, approve, and inspect controls should only appear when the
runtime can honor them. If execution is local-user execution, the UI should say
that plainly instead of implying a stronger sandbox.

### Declarative Where It Clarifies

Use iocraft-style composition where the UI is a bounded control surface with
local focus, layout, and scroll behavior. Keep committed transcript rows as
explicit semantic rows. A declarative surface is a tool for reducing layout
noise, not permission to hide terminal edge cases behind a framework.

## Screen Model

### Transcript

The main transcript remains row-first and native-scrollback-friendly. Stable
rows are committed to terminal history. Mutable live rows remain in the live
region until settled.

Transcript families:

- user prompt row;
- assistant response row;
- reasoning row, live or summarized;
- tool start row;
- tool output preview row;
- tool settlement row;
- edit summary row;
- diff summary row;
- warning/error/cancelled row;
- system/session notice row.

Each row family needs:

- prefix text;
- visual treatment that does not depend on color alone;
- wrapping rule;
- stable snapshot fixtures;
- narrow-width behavior;
- compact and detail forms where relevant.

### Prompt Surface

The prompt lives at the bottom and owns:

- editable input;
- cursor and multi-line wrapping;
- submit/stop state;
- draft history;
- queued follow-up summary;
- command suggestions for `:` command mode;
- file picker entry through `Ctrl+P`;
- inline file mention suggestions for `@`;
- model/provider label;
- inline retry/error hints.

The suggestion surface appears above the input and disappears when it has no
active match.

### Orientation Band

Use one compact status/orientation row for:

- workspace;
- model/provider;
- run state;
- session id or label;
- trust/sandbox wording;
- search/network/tool activity only when active.

This band should stay small enough to survive narrow terminals. If values do
not fit, prefer stable abbreviations and detail-on-demand help over wrapping the
status row into a block.

### Focused Surfaces

Focused surfaces are bounded and temporary:

- command picker;
- file picker;
- help;
- tool output detail;
- diff detail;
- failure detail;

They may capture wheel/scroll keys while focused. They should always show a
clear escape path and return focus to the prompt.

## iocraft Incorporation

The feature incorporates `docs/src/content/docs/notebook/iocraft.md` as an
approved implementation source for richer bounded UI surfaces. The decision is
made: use iocraft directly for focused surfaces, setup/recovery forms,
structured table rendering, and a scrollable transcript/detail lens behind a
small renderer adapter. Keep the committed transcript renderer row-owned until
iocraft can meet the scrollback contract without weakening snapshots or row
grouping.

### Current Research Facts

- iocraft 0.8.3 uses crossterm 0.29, matching `thndrs`' current terminal
  backend dependency.
- iocraft also brings Taffy 0.5, futures, generational-box, regex,
  iocraft-macros, and its own color/component/canvas abstractions.
- Its `ElementExt::render` can render an element to a `Canvas`; `Canvas` can be
  inspected, converted to plain text, or written with ANSI output.
- `ElementExt::mock_terminal_render_loop` plus `MockTerminalConfig` can drive
  terminal events into a component and return a stream of canvases for tests.
- `ScrollView` already models bounded scrolling with keyboard and mouse
  support, which maps well to tool detail, diff detail, and help.
- The `examples/scrolling.rs` pattern combines a column `View`, centered
  instructions, a fixed-height bordered `ScrollView`, keyboard navigation, and
  opt-in mouse capture. This is directly relevant to transcript browsing and
  long-output detail, but it should be bounded and focus-owned rather than
  silently replacing native terminal scrollback.
- The `examples/form.rs` pattern uses reusable focused fields, `TextInput`,
  multiline input, Tab/BackTab focus cycling, focused borders, submit handling,
  and mutable output props. This maps well to setup commands, provider login
  flows, first-run recovery, and multi-step configuration prompts.
- The `examples/table.rs` pattern uses flex percentages, per-column alignment,
  a header separator, alternating row backgrounds, bold/underlined headings,
  and data passed by reference. This is useful for richer markdown tables,
  model/provider lists, tool inventories, command help, search results, and
  structured diagnostics.
- `TextInput` is promising for simple forms and command fields, but the main
  `thndrs` prompt has custom needs: multi-line editing, history, queued
  follow-ups, hidden credential input, mentions, suggestions, streaming state,
  and exact cursor placement.

### Adoption Strategy

Use a three-layer implementation path:

1. **Semantic view records first.** Add renderer-owned transcript, prompt,
   orientation, focused-surface, setup-form, and table records. These records
   remain the app/renderer boundary and are independent of iocraft.
2. **iocraft adapter second.** Add iocraft as the bounded-surface renderer.
   Render iocraft elements to an inspectable `Canvas`, then convert that canvas
   into existing `Row`/`Span` output. The adapter keeps `LiveRegion`, native
   scrollback, `Frame`, `RowGroupId`, and backend escape handling intact.
3. **Surface migration third.** Move help, pickers, setup/recovery forms,
   tool/diff details, transcript lens, markdown tables, diagnostics, and small
   prompt accessories onto the iocraft adapter where it makes the code clearer.
   The main committed transcript stays direct-row until it can preserve the
   current scrollback and snapshot guarantees.

Do not replace the committed transcript renderer with iocraft as the first
move. Transcript rows need stable row grouping, native scrollback, append-only
history behavior, and exact snapshot control. A richer iocraft-backed
transcript lens is acceptable if it is explicitly opened, bounded, and shares
the same semantic `TranscriptView` data as the committed rows.

### Surface Ownership

Use this ownership split:

- **Direct row renderer:** committed transcript rows, live streaming previews,
  final prompt frame assembly, backend cursor movement, terminal resizing, and
  any row that must become native scrollback.
- **iocraft-backed surfaces:** help, command picker, file picker, model picker,
  skill picker, permission prompt, first-run setup choices, setup command
  forms, tool detail, diff detail, transcript lens, markdown tables,
  structured diagnostics, and small inline prompt accessories where the adapter
  keeps state and cursor behavior simple.
- **Main prompt editor boundary:** do not migrate the main prompt editor to
  iocraft `TextInput` in this feature. `TextInput` is approved for setup forms,
  but the main prompt keeps the direct renderer until it can match existing
  cursor, Unicode, hidden-input, history, and suggestion tests.

### Example-Driven Applications

#### Scrollable Transcript And Detail

Use the `scrolling.rs` example as a model for bounded, focus-owned scroll:

- `Ctrl+O` can open a scrollable transcript/detail lens when the latest
  expandable item is long output, a diff, a failed tool, or a transcript group.
- The lens should show a compact title/instruction row, content in a fixed
  height, and a visible clipped-content indicator.
- Arrow/Page/Home/End keys scroll only while the lens is focused.
- Mouse wheel support is optional and must follow the existing
  `--mouse`/`--no-mouse` rules: capture only while a surface needs it.
- The default transcript still commits plain rows to terminal history.

This can improve transcript polish without forcing every transcript interaction
into an internal scroll pane.

#### Setup And Recovery Forms

Use the `form.rs` example as a model for setup commands:

- reusable field rows with label, value, focus state, validation state, and
  hidden/secret mode;
- Tab/BackTab field cycling;
- multiline field support where needed for pasted config or instructions;
- focused border or prefix treatment that does not depend on color alone;
- submit/cancel actions as explicit focused controls;
- output written back through app messages, not directly from an iocraft render
  loop.

This should replace ad hoc first-run/setup prompt rows only after it handles
secret input, cancellation, validation errors, and tiny-height terminals.

#### Structured Markdown And Tables

Use the `table.rs` example as a model for rich structured output:

- parse markdown tables into semantic table blocks instead of preserving them
  only as wrapped plain text;
- allocate columns with fixed, percent, and flexible width rules;
- align numeric/status columns right and text columns left;
- use a header separator and subtle alternating row treatment where the active
  theme has enough contrast;
- fall back to plain wrapped text when a table is too narrow to remain legible.

The table renderer should also be reused for command help, model/provider
lists, tool inventories, and diagnostics so table layout work pays for more
than markdown alone.

### Theme Integration

The existing `Theme` CLI/config value remains the only source of theme
selection. `src/cli/renderer/style.rs` should grow semantic tokens above the
current raw `Palette`, then both direct rows and any iocraft surfaces should
consume those tokens.

Required shape:

```rust
pub struct UiTheme {
    pub palette: Palette,
    pub rows: RowTheme,
    pub prompt: PromptTheme,
    pub surfaces: SurfaceTheme,
    pub status: StatusTheme,
    pub diff: DiffTheme,
}
```

The exact names can differ, but the boundary should not: role-level theme
tokens are produced once from `Theme`, then adapted outward.

Direct rows consume `CellStyle`. iocraft surfaces use an adapter that maps the
same role tokens into iocraft colors, text weights, border styles, padding, and
selected-row styles. iocraft components should receive theme data through
explicit props or context; they should not call global palette functions
directly. This keeps theme snapshots deterministic and makes it possible to
test every built-in theme against both render paths.

### Layout Integration

Use flexbox/Taffy ideas for bounded surface allocation, not for transcript
wrapping:

- transcript wrapping stays in `src/cli/renderer/layout.rs` so row grouping,
  Unicode width handling, and scrollback snapshots stay deterministic;
- focused surfaces may use fixed height, max height, gap, padding, and
  grow/shrink rules modeled after iocraft `View`/Taffy layout;
- tiny-height behavior must be explicit: title row, one body row if available,
  escape hint, and clipped content indicator;
- `ScrollView`-like behavior should apply only while a focused surface owns
  focus or while mouse capture is temporarily enabled;
- table layout should use explicit column specs and truncation rules rather
  than unconstrained wrapping inside every cell;
- form layout should degrade from label/value rows to stacked labels on narrow
  terminals before clipping input values.

### Testing Integration

Keep the existing `Frame::render_styled()` snapshots as the contract for the
direct renderer. Add iocraft canvas/mock-terminal tests for surfaces rendered
through the adapter.

The integration proof must demonstrate:

- themed surface output can be converted into `Row`/`Span` without losing
  selected-row, muted, error, diff, or border semantics;
- keyboard and mouse events can be represented in existing app update messages;
- mock terminal or canvas snapshots cover normal, narrow, tiny, and Unicode
  cases;
- no iocraft render loop writes directly to stdout/stderr in the `thndrs` TUI.

## Interaction Model

### Keyboard

Existing keybindings remain the baseline: prompt submit/newline/history,
command mode, help, file picker, model picker, quit confirmation, and overlay
escape behavior stay aligned with `docs/src/content/docs/usage/keybinds.md`.
This feature changes or adds only these bindings:

- `Ctrl+C`: cancel a running turn. Idle exit remains covered by the global quit
  behavior.
- `Ctrl+O`: open the latest expandable detail surface, preferring the most
  recent failed/truncated tool output or edit diff.
- `Tab`: accept the active prompt suggestion when suggestions are visible.
- `@`: open inline file mention suggestions inside the prompt. `Ctrl+P`
  remains the primary full file picker.

These keybindings supersede the current public keybinding docs for this feature;
implementation includes updating the public docs to match.

### Running State

While a turn is running:

- the prompt remains visible;
- stop/cancel state is visible;
- the user can queue a follow-up;
- new tool output updates a live preview;
- settled tool output commits to transcript history;
- errors and cancellation settle into stable transcript rows.

### Edit Review

When the agent edits files:

- transcript shows a compact edit summary with file path, added/removed counts
  when available, and status;
- `Ctrl+O` opens a focused diff detail surface for the latest edit;
- diff detail uses unified diff output with a per-file summary header;
- failures in applying edits become explicit error rows;
- the UI does not auto-commit or imply git undo.

## Decisions

- **Queued follow-ups:** Submitted follow-ups are summary-only in this feature.
  The prompt can draft a new steering or follow-up message while a turn is
  running, but queued messages are not edited in place after submission.
- **Expandable detail key:** `Ctrl+O` opens the latest expandable detail
  surface. The priority order is failed tool output, truncated tool output,
  latest edit diff, then latest warning/error detail.
- **Diff detail format:** Diff detail uses unified diff as the primary format,
  preceded by a compact per-file summary with path, operation, added lines, and
  removed lines when available.
- **Model selection:** Model/provider selection remains command-driven through
  `:model`. There is no always-visible model switcher.
- **Command syntax:** Command discovery uses the existing `:` command mode.
  Slash commands are not part of this feature.
- **Shell command drafts:** `!` prompt shell commands are not part of this
  feature. Shell execution remains model/tool-mediated.
- **Trust wording:** The orientation band uses the label
  `local user · workspace-contained tools · no TUI sandbox`. This matches the
  current security docs: file tools are workspace-contained, shell commands run
  as the `thndrs` process user, and the TUI is not a security boundary.
- **iocraft adoption:** Use iocraft directly for bounded UI surfaces through a
  renderer adapter. The initial iocraft-backed surfaces are transcript/detail
  lens, setup/recovery forms, structured tables, help, pickers, tool detail,
  and diff detail. Do not use iocraft's fullscreen/render-loop path inside the
  `thndrs` TUI.
- **Transcript ownership:** Preserve the direct row renderer for committed
  transcript history in this feature. iocraft can improve transcript browsing
  through an explicit focused lens, but the default transcript remains
  native-scrollback-friendly.

## Visual Direction

Keep the existing restrained palette. Do not make this feature a color-theme
change. Distinction should come from the structure of the elements:

- stable prefixes for row families;
- indentation and alignment for nested detail;
- compact badges for running, failed, cancelled, and queued state;
- separators only where they group a focused surface or prompt accessory;
- bounded overlays for details;
- spacing that makes transcript groups scannable without becoming card-heavy;
- diff add/remove colors only inside diff contexts, where color carries
  conventional content meaning.

Avoid decorative cards, nested boxes, large banners, and palette churn. Use
short prefixes and spacing to create rhythm. Borders are reserved for focused
temporary surfaces, not every transcript event.

## Implementation Shape

### State

Represent UI-visible events as typed view records before rendering:

```rust
pub enum UiRowKind {
    User,
    Assistant,
    Reasoning,
    Tool,
    Edit,
    Diff,
    Status,
    Error,
    Notice,
}

pub struct PromptSurfaceView {
    pub draft: String,
    pub mode: PromptMode,
    pub queued_summary: Option<String>,
    pub suggestions: Vec<PromptSuggestion>,
    pub status: PromptStatus,
}

pub enum FocusedSurface {
    None,
    CommandPicker,
    FilePicker,
    Help,
    ToolDetail,
    DiffDetail,
}
```

The exact Rust names can differ, but the boundary should stay: app state is
projected into simple renderer-owned view data, then rows are built from that
view.

For iocraft-backed focused surfaces, the adapter boundary is:

```rust
pub struct SurfaceRenderInput<'a> {
    pub theme: &'a UiTheme,
    pub width: usize,
    pub height: usize,
    pub focus: FocusedSurface,
}

pub trait SurfaceRenderer {
    fn render_surface(&mut self, input: SurfaceRenderInput<'_>) -> Vec<Row>;
}
```

The actual names can differ. The important rule is that iocraft stays behind a
renderer adapter and never becomes a second app state owner.

### Rendering

- Keep transcript formatting separate from viewport commit logic.
- Keep prompt rendering separate from command/file picker rendering.
- Keep focused-surface rendering bounded by terminal height.
- Treat terminal width changes as row-rebuild events.
- Keep backend escape handling mechanical.
- Do not use iocraft's fullscreen/render-loop path inside the `thndrs` TUI.
  Render elements to an inspectable canvas or equivalent surface, then convert
  to the existing row/frame contract.
- Keep iocraft usage narrow: text, mixed text, vertical stack, horizontal row,
  bordered surface, selectable list, scroll window, form field, and table are
  enough for this feature.

### Testing

Use snapshot-heavy testing for:

- prompt states;
- transcript row families;
- running and settled tools;
- edit and diff summaries;
- focused surfaces;
- narrow widths;
- tiny heights;
- Unicode and long paths;
- queued follow-ups while streaming;
- cancellation and retry states;
- every built-in theme at least once for direct rows and once for an
  iocraft-backed focused surface.

## Validation

Before implementation is considered done:

- snapshots prove all row families at normal and narrow widths;
- terminal manual QA covers ordinary terminals plus tmux/herdr/Zellij where
  supported by the rendering-engine milestone;
- docs explain the visible controls without describing unsupported behavior;
- no new UI state is added without a test that shows how it renders while idle,
  running, failed, and narrow;
- the iocraft adoption decision remains documented with evidence and clear
  renderer boundaries.
