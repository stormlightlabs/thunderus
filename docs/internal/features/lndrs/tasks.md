# Landorus Tasks

## LNDRS-1: Establish the frontend application boundary

**Status:** Complete.

Added the versioned `thndrs frontend --stdio` NDJSON adapter over the shared
application and harness, including bounded semantic snapshots, ordered events,
request IDs, cancellation, disconnect cleanup, redaction, and explicit handling
for unsupported commands.

## LNDRS-2: Bootstrap the Bun/Svelte/OpenTUI frontend

**Status:** Complete.

Added Landorus as a Bun workspace package and local executable. It launches an
OpenTUI alternate screen, initializes the Rust stdio frontend, projects Svelte
state into retained renderables, restores the terminal on exit, and participates
in repository checks and CI.

## LNDRS-3: Build the Stream transcript

**Status:** Complete.

Replaced the bootstrap transcript string with a retained, scrolling Stream of
stable user, assistant, reasoning, tool, skill, status, and error blocks. Live
content updates in place, tool details expand on demand, historical scrolling is
preserved, dense events are coalesced for presentation, and narrow and long
transcripts have deterministic coverage.

## LNDRS-4: Implement the composer and active-run interaction

**Status:** Complete.

Added the multiline OpenTUI composer, semantic submit/newline/cancel bindings,
paste and draft preservation, active-run follow-up behavior, focus restoration,
and sequence-gap recovery through authoritative backend snapshots.

## LNDRS-5: Add progressive-disclosure controls

**Status:** Complete.

Added capability-gated permissions, model and reasoning pickers, compact context
status and inspection, and a searchable command palette. These controls use
temporary focused overlays and restore composer focus when dismissed or settled.

## LNDRS-6: Add queue and session parity

**Status:** Complete.

Added Rust-owned queue submission, steering, deletion, compact queue inspection,
and session create/load/close flows. Persisted history, truncated snapshots, and
unsupported orchestration state remain backend-owned and covered by integration
and render tests.

## LNDRS-7: Make replay and visual QA first-class

**Status:** Complete.

Added the Rust-owned `thndrs-frontend-replay-v1` corpus under
`crates/thndrs/tests/fixtures/frontend-replay`, with immediate and timed
provider-free playback in Landorus. Rust protocol tests and Landorus state,
render, and performance tests consume the same snapshots and events. Fixed-size
character frames cover 42×16, 80×24, and 120×30; matching tmux/Freeze captures
were reviewed.

Replay QA also covers terminal cleanup, Linux/macOS/Windows smoke jobs, startup,
idle and long-replay memory, dense-stream CPU, input latency, and retained-view
work over long completed history.

## LNDRS-8: Compile Svelte markup to OpenTUI

Replace the imperative application-view assembly with a small Landorus-owned
compiler path for `.svelte` components.

Use `svelte/compiler` to parse Svelte markup and lower the template subset
Landorus needs into OpenTUI renderable creation and reactive updates. Keep
`.svelte.ts` rune modules compiled with `compileModule`.

Migrate the root application shell, composer, status line, Stream blocks, and
overlay shell onto the new markup path as part of this task. Do not create a
general-purpose Svelte/OpenTUI framework.

### Acceptance criteria

- [ ] `.svelte` files are loadable through the Bun development/test path;
- [ ] markup can create nested OpenTUI renderables/components;
- [ ] static and expression-backed props compile;
- [ ] interpolated text updates reactively;
- [ ] `{#if}` is supported;
- [ ] keyed `{#each}` is supported for stable transcript/option identity;
- [ ] event handlers and component/renderable refs work;
- [ ] component teardown destroys owned renderables/effects cleanly;
- [ ] unsupported template syntax produces actionable compile diagnostics;
- [ ] `App.svelte`, `Composer.svelte`, `StatusLine.svelte`, Stream block
      components, and the common overlay shell use the markup path;
- [ ] `root.ts` and `projection.svelte.ts` are removed or reduced to narrow
      runtime/compiler glue;
- [ ] existing protocol/state semantics remain unchanged;
- [ ] render tests exercise compiled components at 42×16, 80×24, and 120×30.

## LNDRS-9: Polish the Stream

Turn the functional transcript into the primary polished Landorus surface.

Improve hierarchy, spacing, Markdown/code presentation, reasoning treatment,
tool lifecycle presentation, keyboard disclosure, scrolling behavior, and live
streaming feedback.

### Acceptance criteria

- [ ] blue is used for Landorus/active/focus semantics and yellow for
      user/attention semantics through centralized theme tokens;
- [ ] assistant Markdown is rendered readably;
- [ ] code blocks use syntax-aware presentation where practical;
- [ ] tool rows remain compact by default and expose detail on demand;
- [ ] running tools have a subtle animated state that settles when complete;
- [ ] tool expansion is keyboard accessible;
- [ ] long tool output opens a focused inspection surface rather than expanding
      the Stream indefinitely;
- [ ] deliberate historical scrolling disables follow mode;
- [ ] new output below a scrolled viewport is indicated clearly;
- [ ] one action returns to live output;
- [ ] reasoning remains visible but visually secondary;
- [ ] failure, cancellation, permission, and retry states are visually distinct;
- [ ] long-transcript replay remains responsive and deterministic.

## LNDRS-10: Polish the composer and active-run UX

Make the composer and run-state interaction feel like one coherent control
surface.

### Acceptance criteria

- [ ] the composer grows with multiline drafts up to a sensible maximum height;
- [ ] idle submit, follow-up queueing, and steering have distinct contextual
      affordances;
- [ ] queued input is visible without opening the queue inspector;
- [ ] cancellation is discoverable during active work;
- [ ] draft text survives overlays, inspection, failed submissions, and rejected
      actions;
- [ ] focus returns predictably after every transient surface;
- [ ] footer/status content is concise and mode-specific rather than one
      concatenated status sentence;
- [ ] normal typing never conflicts with global bindings;
- [ ] input remains responsive during dense streaming replay.

## LNDRS-11: Unify overlays and focused inspection

Replace the current fixed generic overlay treatment with responsive reusable
Svelte components and add focused inspection for dense output.

### Acceptance criteria

- [ ] command palette, model picker, reasoning picker, session picker, queue
      inspector, and context inspector share one responsive overlay shell;
- [ ] overlays expose consistent search, selection, empty, help, and escape
      behavior;
- [ ] permission prompts use a distinct interrupting treatment;
- [ ] overlay sizing remains usable at 42×16 and normal sizes;
- [ ] long tool output can open in focused inspection;
- [ ] source/code output can open in focused inspection;
- [ ] diff content has a dedicated inspection presentation;
- [ ] closing inspection restores Stream position, composer draft, and focus;
- [ ] interaction code dispatches semantic actions instead of directly
      orchestrating imperative view classes.

## LNDRS-12: Add restrained motion and interaction polish

Use terminal animation only where it improves perception of active work.

### Acceptance criteria

- [ ] active runs have a small deterministic spinner/pulse or streaming cursor;
- [ ] tool settlement can provide brief visual feedback without persistent
      animation;
- [ ] overlays provide clear focus/selection feedback;
- [ ] animations stop when their semantic state ends;
- [ ] idle Landorus does not maintain an unnecessary high-FPS render loop;
- [ ] replay/render tests can control the animation clock deterministically;
- [ ] motion does not measurably degrade composer responsiveness or terminal
      cleanup.

## LNDRS-13: Close remaining daily-use UX gaps

Use normal coding tasks to identify only concrete workflow gaps and fix the
highest-value ones.

This is not an architecture-evaluation task. Any work added here should result
in a user-visible feature or interaction improvement.

### Acceptance criteria

- [ ] normal coding tasks can be completed without returning to Ratatui for the
      core interaction loop;
- [ ] no common workflow has an obvious keyboard/focus dead end;
- [ ] permissions, cancellation, queueing, model selection, session resume, and
      inspection are comfortable in normal use;
- [ ] remaining parity gaps are documented only when they represent real
      user-facing limitations;
- [ ] new tasks discovered during dogfooding are phrased as concrete feature or
      UX work, not evaluation/measurement work.
