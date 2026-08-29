# Landorus

Landorus (`lndrs`) is an alternative terminal frontend for Thunderus built with
Bun, TypeScript, Svelte 5, and OpenTUI.

The architecture experiment has already established the important boundary:
Thunderus can expose a frontend-neutral application protocol while keeping
providers, tools, permissions, sessions, context management, and agent
lifecycle behavior in Rust.

The next phase is product work. Landorus should become a useful, polished
conversation-first coding-agent TUI rather than accumulate more experiment
scaffolding.

## Current baseline

The `x/lndrs` branch already has:

- a versioned `thndrs frontend --stdio` NDJSON application boundary;
- a Bun workspace package in `packages/lndrs`;
- Svelte rune state in `.svelte.ts` modules;
- an OpenTUI retained render tree;
- a scrolling semantic transcript with stable blocks;
- multiline prompt editing, submission, cancellation, follow-up queueing, and
  steering;
- permissions, model/reasoning selection, context inspection, sessions, and
  queue controls;
- deterministic replay fixtures, render tests, integration tests, and
  performance coverage.

Those are foundations. They should now support feature work rather than become
work items of their own.

## Goal

Landorus should optimize for three things:

1. **Actual feature work.** Add useful agent-facing surfaces such as richer
   transcript rendering and focused inspection.
2. **UI/UX quality.** Make streaming, tools, permissions, composer behavior,
   overlays, status, focus, and narrow-terminal layouts feel deliberate.
3. **Svelte-authored views.** Express application structure primarily through
   `.svelte` markup compiled into OpenTUI renderables instead of hand-building
   the full tree imperatively and synchronizing it through a separate
   projection layer.

Landorus does not need to copy Ratatui visually. It should preserve Thunderus
semantics while using the frontend stack to simplify UI implementation.

## Non-goals

This phase does not:

- move provider, tool, permission, session, queue, or context policy into
  TypeScript;
- make the frontend protocol a public compatibility guarantee;
- introduce subagents, Fleet, Workbench, or dashboard surfaces;
- add a permanent sidebar that reduces transcript width;
- reproduce every Ratatui view solely for parity;
- build a general-purpose Svelte/OpenTUI framework;
- emulate a browser DOM in the terminal;
- turn replay, benchmarking, source-line counts, or architectural comparison
  into standalone milestones.

## Application boundary

The dependency direction remains:

```text
                 Ratatui
                    │
                    │
Landorus ── frontend protocol ──► Thunderus application core
                                  │
ACP / other surfaces ─────────────┤
                                  │
                                  ▼
                              thndrs-agent
```

### Rust owns

- provider setup and requests;
- prompt and context construction;
- model continuation;
- tool execution and MCP;
- permissions and authority;
- workspace containment;
- queue semantics;
- sessions and persistence;
- compaction;
- cancellation;
- normalized usage and diagnostics.

### Landorus owns

- terminal layout and visual hierarchy;
- transcript presentation;
- prompt editing and focus;
- scrolling and navigation;
- local disclosure and selection state;
- dialogs, pickers, and inspectors;
- presentation timing and animation;
- translation of UI actions into semantic frontend commands.

## Svelte markup and compiler bridge

The current implementation uses Svelte primarily for rune state:

- `svelte-plugin.ts` calls `compileModule` for `.svelte.ts`;
- `root.ts`, `transcript.ts`, and `overlay.ts` build OpenTUI renderables
  imperatively;
- `projection.svelte.ts` mutates those retained renderables from `$effect`
  blocks.

That should change.

Landorus should author application views as `.svelte` components and compile
their markup into OpenTUI renderables.

Svelte does not currently provide an official OpenTUI rendering target, so
Landorus should own a deliberately small compile-time bridge rather than a DOM
shim or a general framework binding.

The bridge should:

- use `svelte/compiler` to parse `.svelte` source;
- keep `compileModule` for reusable `.svelte.ts` rune modules;
- translate template structure into OpenTUI renderable creation;
- generate reactive property and text updates for Svelte expressions;
- preserve stable renderable instances across reactive updates;
- support component composition and the control flow Landorus actually needs;
- emit useful diagnostics for unsupported markup;
- remain private to `packages/lndrs` until there is evidence it should be
  generalized.

Initial compiler support only needs:

- nested components/renderables;
- static and expression-backed properties;
- text and interpolated text;
- `{#if}` blocks;
- keyed `{#each}` blocks;
- component composition;
- event handlers;
- renderable/component references;
- lifecycle cleanup.

Do not chase complete Svelte DOM compatibility. Extend the compiler only when a
real Landorus component requires it.

The UI should converge toward component boundaries such as:

```text
src/ui/
├── App.svelte
├── Stream.svelte
├── Composer.svelte
├── StatusLine.svelte
├── blocks/
│   ├── UserBlock.svelte
│   ├── AssistantBlock.svelte
│   ├── ReasoningBlock.svelte
│   ├── ToolBlock.svelte
│   ├── SkillBlock.svelte
│   └── NoticeBlock.svelte
├── overlays/
│   ├── OverlayFrame.svelte
│   ├── CommandPalette.svelte
│   ├── PermissionPrompt.svelte
│   ├── ModelPicker.svelte
│   ├── SessionPicker.svelte
│   ├── QueueInspector.svelte
│   └── ContextInspector.svelte
└── inspect/
    ├── ToolOutput.svelte
    ├── SourceView.svelte
    └── DiffView.svelte
```

Imperative OpenTUI code remains appropriate for narrow leaf adapters where a
specialized renderable or lifecycle API is awkward to express in markup.

## UI direction

Landorus remains a **single-session, conversation-first interface**.

The normal screen has three persistent regions:

```text
┌─────────────────────────────────────────────────────────────┐
│                                                             │
│ Stream                                                      │
│                                                             │
│ conversation + compact agent activity                       │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│ composer                                                    │
├─────────────────────────────────────────────────────────────┤
│ contextual status / hints                                   │
└─────────────────────────────────────────────────────────────┘
```

There is no permanent sidebar, dashboard, Workbench, or Fleet mode.

Secondary information appears through transient overlays or focused
inspection.

### Visual language

The interface should be restrained and terminal-native rather than a collection
of bordered panels.

Use a small semantic palette:

- **blue** for Landorus identity, active work, focus, and primary selection;
- **yellow** for the user, attention, queued input, and permission emphasis;
- terminal-aware neutral text for normal structure;
- green and red only for success and failure semantics.

Centralize these values as theme tokens. Do not scatter literal color values
through components.

Spacing and hierarchy should carry more of the design than borders.

## Stream

The Stream is the primary interface and should become richer than styled plain
text.

### Messages

- user and final assistant output should be immediately scannable;
- reasoning should remain secondary and quieter;
- assistant Markdown should preserve useful structure;
- code blocks should use OpenTUI code rendering where it improves readability;
- diffs should use a dedicated diff treatment instead of raw unified text when
  possible;
- errors and exceptional states should have more weight than routine status.

### Tool activity

Routine tool use should stay compact:

```text
● read   src/ui/Composer.svelte
● edit   src/ui/Composer.svelte                     +24 -11
● test   packages/lndrs                              18 passed
```

Running activity may use a subtle animated indicator. Completed tools settle to
quiet success/failure states.

Expansion should reveal arguments and a useful output preview. Long output,
source, or diffs should open a focused inspector rather than expanding the
Stream indefinitely.

Keyboard navigation must provide the same disclosure path as the mouse.

### Streaming and scrolling

Streaming should update the active semantic block in place. Presentation can be
frame-coalesced while semantic events remain lossless.

The Stream should:

- follow output while the user is at the bottom;
- stop following after deliberate historical scrolling;
- show when newer output exists below the viewport;
- return to live output with one command;
- preserve position while overlays open and close;
- remain responsive with long histories.

A small streaming cursor, pulse, or spinner is appropriate while work is
active. Animation must never delay content delivery or input.

## Composer

The composer should feel like the primary control surface rather than a fixed
five-row box.

It should:

- grow within a bounded height as the draft becomes multiline;
- preserve drafts across overlays, failed submissions, and inspection;
- distinguish normal submit, queued follow-up, and steering modes clearly;
- make cancellation discoverable while a run is active;
- surface queued input without requiring the queue inspector;
- keep key hints short and mode-specific;
- restore focus predictably after every transient surface.

The footer should not concatenate every state into one long sentence. Show only
high-value persistent state and move detail into overlays.

## Overlays

Command palette, permissions, model/reasoning selection, session selection,
queue inspection, and context inspection should share a consistent overlay
language.

A common overlay shell should own:

- responsive width and height;
- title and optional description;
- search/filter input where useful;
- selection and empty states;
- contextual key hints;
- focus trapping and restoration;
- escape/settlement behavior.

Permission prompts are exceptional and should interrupt clearly without looking
like a generic picker.

Overlays must remain usable at 42×16 as well as normal terminal sizes.

## Focused inspection

Landorus should add focused inspection for information too dense for the
Stream:

- long tool output;
- source/code output;
- diffs;
- detailed errors or diagnostics.

Inspection temporarily replaces or overlays the Stream. Closing it restores the
previous scroll position, composer draft, and focus.

This is a real feature surface, not a dashboard mode.

## Motion and presentation cadence

Terminal animation should communicate activity rather than decorate idle
screens.

Useful examples include:

- an active-run spinner or pulse;
- a streaming cursor;
- a brief state change when a tool settles;
- restrained overlay focus/selection feedback.

Animations must:

- stop when their state is no longer active;
- use deterministic clocks in render tests;
- avoid forcing a permanent high-FPS render loop;
- preserve input latency and terminal correctness.

## Interaction architecture

`AppState` can remain the semantic frontend state while it remains
understandable.

Interaction code should move away from direct ownership of imperative view
classes. Prefer semantic actions plus component callbacks/references so the UI
tree can change without rewriting protocol behavior.

Keybindings should remain layered:

1. focused editor/control behavior;
2. overlay/inspector behavior;
3. Stream navigation and disclosure;
4. global application commands.

Bare printable global bindings must never steal composer input.

## Replay-driven development

The existing replay corpus is infrastructure, not the product roadmap.

Use it to develop and regress:

- simple and streaming turns;
- reasoning;
- tool-heavy runs;
- permissions;
- queueing and steering;
- cancellation and failures;
- compaction/context pressure;
- long transcripts;
- narrow and wide layouts.

When a new UI state matters, extend an existing fixture or add the smallest
fixture that exercises it.

## Testing bar

The frontend should keep three levels of coverage:

- **state tests** for protocol-to-state semantics;
- **component/render tests** for compiled Svelte views at deterministic terminal
  sizes;
- **integration tests** for the Rust bridge and complete interaction paths.

Tests should protect user-visible behavior, compiler correctness, focus,
scrolling, and terminal cleanup. Avoid measurement work that does not protect or
unlock a product decision.

## Definition of a strong Landorus frontend

This phase is successful when:

- the application UI is primarily authored in `.svelte` markup;
- the Stream renders assistant output, tools, errors, code, and diffs clearly;
- useful detail can be inspected without cluttering normal conversation;
- composer, permissions, queueing, sessions, model selection, and cancellation
  are comfortable from the keyboard;
- active work feels alive without becoming noisy;
- 42×16, 80×24, and 120×30 layouts all remain intentional;
- normal coding tasks do not expose obvious UI dead ends;
- replay and performance tests remain guardrails rather than the work itself.
