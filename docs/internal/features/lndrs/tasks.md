# Landorus Tasks

## LNDRS-1: Establish the frontend application boundary

**Status:** Complete.

Add a versioned frontend-neutral stdio adapter backed by the existing Thunderus
application and harness.

### Acceptance criteria

- [x] `thndrs frontend --stdio` runs without owning or drawing the terminal.
- [x] stdin accepts versioned NDJSON commands.
- [x] stdout contains protocol messages only.
- [x] initialization negotiates the protocol version and returns a bounded
      snapshot.
- [x] requests have stable IDs.
- [x] asynchronous events have monotonic sequence numbers.
- [x] snapshots and events use frontend-specific semantic types rather than
      serialized provider payloads.
- [x] provider credentials and persistence internals do not cross the protocol.
- [x] turn submission uses the existing application/harness lifecycle.
- [x] cancellation uses the existing cooperative cancellation path.
- [x] unexpected frontend disconnection cancels active work.
- [x] output is bounded and sensitive diagnostic/tool text is redacted.
- [x] unsupported future commands fail explicitly.

## LNDRS-2: Bootstrap the Bun/Svelte/OpenTUI frontend

**Status:** Complete.

Create Landorus as a Bun workspace package and prove the Svelte-state →
retained-OpenTUI-renderable integration.

### Acceptance criteria

- [x] the repository uses one Bun workspace and lockfile for `docs` and
      `packages/*`;
- [x] `packages/lndrs` has its own package manifest and TypeScript config;
- [x] `lndrs` is exposed as a local executable;
- [x] `.svelte.ts` modules compile through the Bun Svelte preload plugin;
- [x] Landorus launches an OpenTUI alternate-screen renderer;
- [x] Landorus launches and initializes `thndrs frontend --stdio`;
- [x] the frontend restores the terminal on normal exit and backend exit;
- [x] protocol state can update an existing retained OpenTUI renderable through
      Svelte reactivity;
- [x] protocol framing, client behavior, state, and the initial render shell
      have tests;
- [x] Landorus participates in repository CI.

The existing single-`TextRenderable` transcript is considered scaffolding for
this milestone, not the target transcript architecture.

## LNDRS-3: Build the Stream transcript

**Status:** Complete.

Replace the bootstrap transcript string with the actual conversation-first
Landorus Stream.

### Transcript structure

- [x] replace the single transcript `TextRenderable` with a
      `ScrollBoxRenderable`;
- [x] represent semantic transcript entries as stable child renderables;
- [x] preserve protocol IDs as frontend renderable identities;
- [x] add dedicated presentation for:

  - user;
  - assistant;
  - reasoning;
  - tool;
  - skill;
  - status;
  - error;

- [x] assistant deltas mutate one active assistant block;
- [x] reasoning deltas mutate one active reasoning block;
- [x] tool start and finish events update one block by tool-call ID;
- [x] completed blocks are not recreated when unrelated live events arrive.

### Tool presentation

- [x] render normal tools in a compact collapsed form;
- [x] distinguish running, successful, failed, and cancelled states;
- [x] allow tool arguments/output to be expanded;
- [x] bound visual output consistently with the Rust protocol;
- [x] use richer source/diff presentation only where it improves readability.

### Scrolling

- [x] follow output while the viewport is at the bottom;
- [x] stop auto-following after deliberate upward scrolling;
- [x] resume following after returning to the bottom;
- [x] preserve scroll position across status and usage updates;
- [x] verify long transcripts with viewport culling enabled where appropriate.

### Rendering cadence

- [x] remove full-transcript string regeneration from the live rendering path;
- [x] measure the current `flushSync()`-per-event path under a streaming fixture;
- [x] introduce presentation coalescing if measurements show unnecessary render
      work;
- [x] never drop or merge semantic protocol events in application state;
- [x] keep input and scrolling responsive during dense streaming.

### Layout

- [x] make Stream usable at narrow terminal widths;
- [x] keep the transcript visually dominant;
- [x] avoid permanent sidebars or dashboard chrome;
- [x] establish a minimal visual vocabulary for user, assistant, reasoning,
      tools, errors, and muted metadata.

### Verification

- [x] state tests for streaming block identity;
- [x] render tests for every transcript block type;
- [x] render tests at representative narrow and wide terminal sizes;
- [x] replay fixture for a long tool-heavy run;
- [x] test that historical scrolling is not pulled back to the bottom by new
      deltas;
- [x] test that renderable count does not grow once-per-token.

## LNDRS-4: Implement the composer and active-run interaction

**Status:** Complete.

Make Stream usable for a complete normal agent turn.

### Composer

- [x] replace the placeholder composer with `TextareaRenderable`;
- [x] focus the composer on normal startup;
- [x] support multiline editing;
- [x] define explicit submit and newline bindings;
- [x] handle terminal paste correctly;
- [x] preserve unsent text during unrelated frontend state changes;
- [x] clear submitted input only after the corresponding command is accepted.

### Turn lifecycle

- [x] idle submission sends `turn.submit`;
- [x] active runs visibly distinguish working and stopping;
- [x] the stop action sends `turn.cancel`;
- [x] cancellation remains visibly pending until confirmed by the backend;
- [x] failed turns settle into a stable error state;
- [x] completed turns return focus to the composer.

### Input architecture

- [x] remove the bare global `q` binding before normal text input is enabled;
- [x] define semantic frontend actions independently from physical bindings;
- [x] keep renderer-global key handlers limited to genuinely global actions;
- [x] ensure printable keys are never intercepted while the composer is
      focused;
- [x] introduce `@opentui/keymap` once focus-dependent commands or overlays make
      direct listeners ambiguous.

### Protocol recovery

- [x] track frontend event sequence numbers in `FrontendClient`;
- [x] detect missing or out-of-order events;
- [x] request `state.snapshot` after a detected sequence gap;
- [x] replace local state atomically from the recovery snapshot;
- [x] surface unrecoverable backend termination without corrupting the terminal.

### Verification

- [x] submit → stream → finish integration test;
- [x] submit → cancel → settle integration test;
- [x] composer editing/render tests;
- [x] focus regression test proving ordinary printable keys reach the composer;
- [x] sequence-gap recovery test.

## LNDRS-5: Add progressive-disclosure controls

Add the surrounding controls needed for daily use without turning Stream into
an IDE/dashboard layout.

Secondary controls should appear as overlays, pickers, or temporary inspection
surfaces.

### Capability discovery

- [ ] expose frontend command/capability availability during initialization or
      snapshot state;
- [ ] hide or disable unsupported Landorus actions;
- [ ] distinguish backend command support from provider-specific option
      availability.

### Permissions

- [ ] implement `permission.respond` in the Rust bridge;
- [ ] model pending permission state explicitly in Landorus;
- [ ] open an explicit focused permission surface when requested;
- [ ] support all existing allow/reject semantics;
- [ ] restore sensible focus after settlement;
- [ ] fail closed on backend disconnect.

### Model and reasoning

- [ ] implement backend model selection support;
- [ ] implement backend reasoning-effort selection support;
- [ ] expose model/reasoning selection through a temporary picker;
- [ ] keep the active model and reasoning effort visible in compact status;
- [ ] omit unsupported reasoning controls.

### Context inspection

- [ ] expose normalized context-window usage required by Landorus;
- [ ] show compact context usage in the normal status line;
- [ ] add a temporary context-inspection surface for additional available
      details;
- [ ] expose compaction state where useful;
- [ ] do not reproduce context-selection or compaction policy in TypeScript.

### Command palette

- [ ] introduce a searchable command palette;
- [ ] source palette entries from available semantic frontend actions;
- [ ] show current keybindings where useful;
- [ ] use the palette for infrequent actions instead of permanent chrome.

### Verification

- [ ] permission replay and integration tests;
- [ ] model/reasoning picker render tests;
- [ ] capability-gating tests;
- [ ] context status and inspector tests;
- [ ] focus restoration tests for every overlay.

## LNDRS-6: Add queue and session parity

Implement remaining application controls that materially affect normal
single-agent use.

### Queue and steering

- [ ] implement `queue.submit`;
- [ ] implement `queue.delete`;
- [ ] expose Rust-owned queued items in frontend state;
- [ ] allow active-run input to explicitly target supported steering/follow-up
      semantics;
- [ ] render queued input compactly;
- [ ] provide a temporary queue inspector where needed;
- [ ] never infer settlement independently from the backend.

### Sessions

- [ ] implement `session.new`;
- [ ] implement `session.load`;
- [ ] implement `session.close`;
- [ ] expose session metadata required for a session picker;
- [ ] resume compatible persisted sessions;
- [ ] show truncated-history state when a frontend snapshot cannot contain the
      complete transcript;
- [ ] keep session persistence entirely Rust-owned.

### Explicitly deferred

- [ ] no agent tree;
- [ ] no Fleet view;
- [ ] no subagent UI;
- [ ] no worktree orchestration UI;
- [ ] no multi-session dashboard.

These should become a separate future feature only after corresponding
Thunderus application semantics exist.

### Verification

- [ ] queue/steering integration tests;
- [ ] session create/load/close integration tests;
- [ ] persisted-session replay/render test;
- [ ] tests proving unsupported future orchestration state is absent from the
      frontend model.

## LNDRS-7: Make replay and visual QA first-class

Create the deterministic workflow used to iterate on and polish Landorus.

### Replay

- [ ] define a versioned frontend replay fixture format;
- [ ] add fixtures for:

  - simple turn;
  - streaming;
  - reasoning;
  - multiple tools;
  - failed tool;
  - permission;
  - queued input;
  - cancellation;
  - retry;
  - provider failure;
  - compaction;
  - long transcript;

- [ ] allow Landorus to run without spawning a provider-backed turn;
- [ ] support immediate playback for automated testing;
- [ ] support timed playback for manual streaming inspection;
- [ ] support deterministic terminal dimensions.

### Render QA

- [ ] capture fixed-size OpenTUI character frames for important states;
- [ ] exercise narrow, normal, and wide terminal sizes;
- [ ] verify alternate-screen restoration after exceptions;
- [ ] smoke test macOS and Linux;
- [ ] add Windows testing if Landorus is intended to match the supported
      Thunderus platform matrix;
- [ ] use tmux/VHS/Freeze captures for representative manual visual review where
      useful.

### Performance

- [ ] measure startup time;
- [ ] measure idle memory;
- [ ] measure memory during a long replay;
- [ ] measure CPU during dense streaming;
- [ ] verify input latency remains acceptable while rendering;
- [ ] verify completed transcript history does not cause increasing work for
      every new token.

## LNDRS-8: Evaluate the experiment

Dogfood Landorus on normal coding tasks and decide whether the experiment
should continue.

### Acceptance criteria

- [ ] normal agent tasks can be completed without returning to Ratatui for the
      core interaction loop;
- [ ] transcript scrolling remains predictable;
- [ ] streaming feels stable and responsive;
- [ ] composer input remains responsive during active output;
- [ ] permissions, cancellation, queueing, model selection, and session resume
      are usable;
- [ ] remaining Ratatui parity gaps are documented explicitly;
- [ ] frontend-specific source size is measured;
- [ ] major frontend abstractions are identified and compared with Ratatui;
- [ ] Svelte/OpenTUI projection boilerplate is evaluated from actual code rather
      than anticipated complexity;
- [ ] packaging implications are documented;
- [ ] a recommendation records one of:

  - keep experimental;
  - support Landorus alongside Ratatui;
  - begin gradual frontend replacement;
  - archive Landorus but keep the frontend protocol;
  - archive both.

A custom Svelte/OpenTUI renderer is only a follow-up if the final implementation
demonstrates that retained-renderable synchronization is itself the dominant
source of frontend complexity.
