# UI Usability Plan

Status: Draft
Owner: thndrs maintainers
Captured: 2026-07-03

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

### Rendering

- Keep transcript formatting separate from viewport commit logic.
- Keep prompt rendering separate from command/file picker rendering.
- Keep focused-surface rendering bounded by terminal height.
- Treat terminal width changes as row-rebuild events.
- Keep backend escape handling mechanical.

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
- cancellation and retry states.

## Validation

Before implementation is considered done:

- snapshots prove all row families at normal and narrow widths;
- terminal manual QA covers ordinary terminals plus tmux/herdr/Zellij where
  supported by the rendering-engine milestone;
- docs explain the visible controls without describing unsupported behavior;
- no new UI state is added without a test that shows how it renders while idle,
  running, failed, and narrow.
