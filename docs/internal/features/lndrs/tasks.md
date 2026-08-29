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
