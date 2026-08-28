# Landorus Tasks

## LNDRS-1: Define the frontend protocol boundary

**What to build:** Add a versioned, frontend-neutral stdio protocol backed by
the existing Rust application core and harness. Establish snapshots, commands,
responses, and asynchronous events without importing Ratatui state or exposing
provider payloads.

**Acceptance criteria:**

- [ ] `thndrs frontend --stdio` starts without entering terminal raw mode or
      drawing UI.
- [ ] stdin accepts versioned NDJSON frontend commands.
- [ ] stdout contains protocol messages only; diagnostics use stderr.
- [ ] Initialization negotiates a protocol version and returns a bounded
      frontend snapshot before live events.
- [ ] Commands and responses have stable request identifiers where a direct
      result is required.
- [ ] Existing `AgentEvent` values are projected into a separate serialized
      frontend event vocabulary.
- [ ] Provider-native payloads, credentials, and internal session writer state
      never cross the protocol.
- [ ] Turn submission uses the shared context, prompt, provider, tool, MCP, and
      session paths rather than a second agent loop.
- [ ] Cancellation uses the existing cooperative cancellation token.
- [ ] The protocol supports clean shutdown and detects unexpected peer
      termination.
- [ ] A second frontend does not need to import `cli/app`, `runtime`, or
      Ratatui renderer modules.
- [ ] Shared behavior found only in `cli/app` is moved to an appropriate
      frontend-neutral owner rather than duplicated for Landorus.

**Verification:**

- Rust protocol serialization and schema tests.
- Fake-provider integration test covering handshake, snapshot, turn start,
  streaming output, tool lifecycle, terminal outcome, and shutdown.
- Tests proving stdout remains valid protocol when diagnostics occur.
- Tests for malformed commands, unsupported versions, peer disconnect, and
  cancellation.

## LNDRS-2: Bootstrap the Svelte/OpenTUI frontend

**What to build:** Create `packages/lndrs/` as a Bun/TypeScript application
using Svelte 5 runes for reactive state and `@opentui/core` for imperative
terminal rendering.

**Blocked by:** LNDRS-1 for live integration. The frontend shell and replay
transport may begin independently.

**Acceptance criteria:**

- [ ] `packages/lndrs/` has an isolated package manifest, TypeScript
      configuration, formatting/linting commands, and tests.
- [ ] `bun run dev` starts an OpenTUI alternate-screen application and restores
      the terminal cleanly on normal exit, Ctrl+C, error, and backend exit.
- [ ] Landorus can spawn `thndrs frontend --stdio` and complete the protocol
      handshake.
- [ ] Svelte reactive state lives in `.svelte.ts` modules where appropriate.
- [ ] No React, Solid, DOM runtime, browser renderer, or custom Svelte
      reconciler is required.
- [ ] OpenTUI renderables are created through `@opentui/core` and retained
      across ordinary state updates.
- [ ] One small prototype proves that a protocol state change can update an
      existing OpenTUI renderable through the chosen Svelte-reactivity
      integration.
- [ ] Shutdown disposes reactive effects, protocol streams, the child process,
      and the OpenTUI renderer without leaving the terminal corrupted.
- [ ] The package exposes a `lndrs` executable or equivalent local launch
      command.

**Verification:**

- Unit test for protocol framing and typed message parsing.
- State test for snapshot initialization and incremental updates.
- OpenTUI render test for the initial shell.
- Manual smoke test on macOS and at least one Linux environment before this
  milestone is considered complete.

## LNDRS-3: Implement transcript and live run rendering

**What to build:** Render the normal conversation and active agent run from
frontend snapshots and semantic events.

**Blocked by:** LNDRS-2.

**Acceptance criteria:**

- [ ] User turns and assistant responses render as distinct transcript blocks.
- [ ] Assistant deltas update one active assistant block instead of allocating
      one view per token.
- [ ] Reasoning has a distinct presentation and streams into one active
      reasoning block.
- [ ] Tool start/update/finish events modify one stable tool block identified by
      tool-call id.
- [ ] Failed and cancelled tools remain visibly distinct from successful tools.
- [ ] Markdown output uses OpenTUI's Markdown renderable where appropriate.
- [ ] Source and diff output use appropriate OpenTUI renderables where they
      materially improve readability.
- [ ] Completed transcript content remains stable when a new run begins.
- [ ] Long transcripts scroll correctly and do not require rebuilding all
      completed renderables for every live delta.
- [ ] Auto-follow remains enabled while the user is at the bottom and stops
      pulling the viewport downward after the user deliberately scrolls into
      history.
- [ ] Resize events preserve usable layout across narrow and wide terminals.
- [ ] Failure, cancellation, and normal completion return the live surface to a
      settled state.

**Verification:**

- Replay fixtures for simple, reasoning-heavy, tool-heavy, cancelled, and
  failed turns.
- Fixed-size render snapshots for representative transcript states.
- A stress fixture with a long transcript and many streaming deltas that does
  not exhibit unbounded renderable growth or obvious input stalls.

## LNDRS-4: Implement prompt, steering, queue, and permissions

**What to build:** Make Landorus capable of controlling a complete interactive
turn rather than acting as a read-only event viewer.

**Blocked by:** LNDRS-3.

**Acceptance criteria:**

- [ ] The composer supports editable multiline input.
- [ ] Idle submission sends a new user turn through the frontend protocol.
- [ ] Input submitted during a run can target either steering or follow-up
      queue behavior according to the current frontend action.
- [ ] Queue state is rendered from Rust-owned queue semantics rather than
      independently inferred by Landorus.
- [ ] Queued items can be inspected and deleted where the Rust application
      allows it.
- [ ] Escape or the chosen stop binding requests cancellation without locally
      pretending the run has already stopped.
- [ ] The UI displays stopping state until the backend confirms settlement.
- [ ] Permission requests interrupt normal input with an explicit decision
      surface.
- [ ] Allow/reject responses map to existing Rust permission semantics.
- [ ] Backend disconnect while a permission is pending fails closed.
- [ ] Input focus returns to a sensible location after permission settlement,
      cancellation, and turn completion.
- [ ] Keybindings are translated into semantic frontend actions before any
      protocol command is sent.

**Verification:**

- State tests for idle submit, steering, follow-up queueing, deletion,
  cancellation, and permission settlement.
- Replay fixtures for queue and permission states.
- Fake-provider integration tests for submit → tool → permission → continue and
  submit → cancel → settle.

## LNDRS-5: Add session, model, and context controls

**What to build:** Add the minimum surrounding application controls required for
Landorus to function as a daily Thunderus frontend.

**Blocked by:** LNDRS-4.

**Acceptance criteria:**

- [ ] Landorus can create a new session and resume an existing compatible
      session.
- [ ] Session selection uses Rust-provided session metadata rather than reading
      JSONL files directly.
- [ ] The active provider/model is visible.
- [ ] Model selection uses a backend-provided option list and command.
- [ ] Reasoning effort is visible and selectable where supported.
- [ ] Context usage is displayed from normalized backend accounting.
- [ ] Compaction activity and relevant recoverable diagnostics are visible.
- [ ] Active skill/context information needed for normal operation can be
      inspected without reproducing context-selection policy in TypeScript.
- [ ] Backend configuration changes refresh the relevant frontend snapshot or
      emit explicit state updates.
- [ ] Unsupported controls are disabled or omitted based on backend
      capabilities rather than failing after selection.

**Verification:**

- State and render tests for model, reasoning, session, and context states.
- Integration test loading a persisted session and starting a new turn.
- Integration test changing model/reasoning configuration before a turn.

## LNDRS-6: Make replay and parity tests first-class

**What to build:** Establish a shared deterministic fixture suite so Landorus
can be developed and compared with the Ratatui frontend without live provider
calls.

**Blocked by:** LNDRS-3. Expand alongside LNDRS-4 and LNDRS-5.

**Acceptance criteria:**

- [ ] A versioned frontend fixture format is documented.
- [ ] Fixtures exist for:
  - simple turn;
  - reasoning;
  - multiple tools;
  - failed tool;
  - permission;
  - steering;
  - follow-up queue;
  - cancellation;
  - provider failure;
  - compaction;
  - long transcript.
- [ ] Landorus can launch directly against a fixture without spawning a live
      provider.
- [ ] Rust projection tests can generate or validate the same protocol
      messages consumed by the TypeScript tests.
- [ ] Fixture playback supports deterministic terminal dimensions.
- [ ] Replay can run faster than real time for automated tests.
- [ ] Replay can optionally preserve event timing for manual streaming and
      animation inspection.
- [ ] A protocol change that invalidates fixtures fails tests explicitly rather
      than silently ignoring unknown semantic data.

**Verification:**

- Landorus unit/render suite runs without network credentials.
- Rust and TypeScript CI both exercise the shared fixture corpus.
- At least one end-to-end fake-provider trace round-trips through capture and
  replay with equivalent final frontend state.

## LNDRS-7: Polish and evaluate the experiment

**What to build:** Bring Landorus to realistic daily-use quality, measure it
against the Ratatui frontend, and record whether the experiment should
continue.

**Blocked by:** LNDRS-4, LNDRS-5, and LNDRS-6.

**Acceptance criteria:**

- [ ] Common interaction paths are usable without visible flicker or terminal
      corruption.
- [ ] Streaming remains responsive during tool-heavy and long-output runs.
- [ ] Input remains responsive while assistant output is arriving.
- [ ] Completed transcript content does not cause obviously increasing work on
      every new delta.
- [ ] Startup time, idle memory, active memory, and CPU during representative
      streaming are measured against the Ratatui frontend.
- [ ] Cross-platform behavior is checked on macOS, Linux, and Windows or the
      current supported platform matrix is explicitly narrowed.
- [ ] The implementation records approximate frontend-specific source size and
      major abstraction count for comparison with Ratatui.
- [ ] Adding one representative new frontend surface is used as a qualitative
      maintainability test.
- [ ] Remaining behavioral gaps are listed explicitly rather than hidden behind
      "parity" language.
- [ ] A short evaluation records one of:
  - keep experimental;
  - promote to supported alternative;
  - begin replacement work;
  - archive Landorus but retain the frontend protocol;
  - archive both.
- [ ] A custom Svelte/OpenTUI renderer is proposed only if repeated imperative
      synchronization has become a demonstrated maintenance problem.

**Verification:**

- Daily-use dogfood through representative coding tasks.
- Recorded performance measurements for equivalent fixture workloads.
- Final architecture review confirming that provider, tool, authority,
  persistence, and context behavior remains Rust-owned.
