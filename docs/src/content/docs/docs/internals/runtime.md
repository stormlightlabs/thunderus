---
title: "Runtime and State"
---

The interactive runtime coordinates terminal input, application state, agent
work, background processes, and Ratatui frames. It keeps those concerns
separate: the application updates state, the runtime executes effects, and the
renderer projects state into terminal output.

## Mental Model

The runtime uses an Elm-style update loop. A terminal event becomes a semantic
`Action`, and the action becomes a `Msg`. `update_with_effects` applies that
message to `App` without doing I/O. It returns pure follow-up messages and a
list of effects for the runtime to execute. Effect completions come back as
messages and enter the same update path.

```text
terminal event
      │
      ▼
TerminalInput → Action → Msg
                          │
                          ▼
              update_with_effects(&mut App, &Msg)
                    │                    │
             follow-up Msg          Effect requests
                    │                    │
                    └────────┬───────────┘
                             ▼
                      runtime executor
                             │
                    EffectResult / AgentEvent
                             │
                             └── back to Msg
```

`App` is the source of truth for the current session, transcript, prompt,
overlays, queue, run state, context ledger, and runtime measurements. The
renderer does not mutate those values. The background agent sends semantic
events through `AgentRun`; it does not call into `App` directly.

## Responsibilities

`cli/app` owns the state machine and state transitions:

- `App` holds session, transcript, composer, overlay, setup, queue, context,
  usage, git status, and run state.
- `Msg` is the input vocabulary for actions, ticks, agent events, effect
  results, transcript clearing, quitting, and git-status changes.
- `update_with_effects` applies one message and derives effects or a follow-up
  message.
- The child modules split transition logic by event family: input and queues,
  slash commands, onboarding, context operations, and agent lifecycle.

`runtime` owns the application adapters around that state machine:

- `interactive_loop` polls terminal input, drains agent and git events, runs
  ticks, and decides when to draw.
- `execute_effect` starts, cancels, and settles agents, drains process results,
  and performs terminal operations.
- `spawn_agent` builds the application run configuration, prompt bundle, tool
  catalog, and harness turn.
- `terminal.rs` owns raw mode, bracketed paste, keyboard enhancements,
  suspension, the inline viewport, and the Ratatui surface.

## State Transitions

A normal turn follows these transitions:

1. `translate_input` converts a `TerminalInput` into one or more semantic
   actions. `input::handle_action` edits the composer or submits a turn.
2. Submission records the user turn and changes the run state from `Idle` to
   `Working`. `update_with_effects` creates an `EffectRequest` containing the
   session id and turn number, then emits `StartAgent`.
3. The runtime executes `StartAgent`. `spawn_agent` selects the ACP route or
   the built-in harness, constructs context and provider messages, and stores
   one `AgentSlot` with its event receiver, cancellation token, and steering
   channel.
4. The loop drains a bounded batch of events. Each event becomes an
   `EffectResult::Agent` message and is accepted only when its
   `EffectRequest` matches the active request.
5. `handle_agent_event` updates the transcript, usage, context accounting,
   permission surfaces, tool records, and session writer. A `Finished`,
   `Failed`, or `Cancelled` event returns the run to an editable state.
6. When the run settles, the runtime waits for or detaches the worker as
   appropriate and clears the active agent slot. A queued follow-up may then
   submit through the same path.

`Tick` advances UI deadlines, refreshes foreground process output, expires
status feedback, progresses cancellation grace periods, polls OAuth recovery,
and drains completed background processes. Git status changes use a separate
watcher and update `App` through `GitStatusChanged`.

## Effects

The update path requests effects; it does not perform them. The current effect
set is:

| Effect | Runtime operation |
| --- | --- |
| `StartAgent` | Spawn the selected ACP or built-in agent run |
| `CancelAgent` | Set the active run's cooperative cancellation token |
| `SettleAgent` | Wait for the worker, or detach it after a stop timeout |
| `DrainBackgroundProcesses` | Move completed shell-process results into `App` |
| `ShutdownProcesses` | Stop application-owned background processes on exit |
| `ClearTerminal` | Clear the mutable terminal surface |
| `SuspendTerminal` | Temporarily restore shell control and resume the TUI |

Each effect is identified by an `EffectRequest` when it belongs to a turn.
Agent completions carry the same identity. Stale events from an earlier run
are ignored instead of changing the current session.

## Agent Events

The application event vocabulary is defined by `cli::app::AgentEvent`. It
covers more than assistant text:

- `Started`, status, usage, provider capacity, retry, completion, failure, and
  cancellation describe run and provider state.
- `RequestStarted` and `RequestAccounting` record serialized request details,
  usage, timing, and context snapshots.
- Reasoning and assistant deltas append to streaming transcript entries.
- `ToolStarted` and `ToolFinished` update tool lifecycle state, persist audits,
  refresh git status, and attach file or shell results.
- Permission requests open an application overlay; resolutions update the
  transcript and session records.
- Model metadata and ACP session events update their corresponding picker or
  session state.

`agent_lifecycle::handle_agent_event` is the mutation boundary for these
background events. It finalizes streaming entries and persistence when a run
finishes, restores editable input after failure where appropriate, cancels
pending permissions and tools on interruption, and can return a follow-up
message for compaction or queued input.

## Presentation Scheduling

The runtime separates state changes from presentation. User actions, resizes,
suspension, and the initial frame request an immediate draw. Agent events and
git-status updates mark the surface dirty but use the configured tick interval
as a presentation deadline. `PresentationScheduler` coalesces those updates so
a burst of streamed events produces one frame rather than one terminal write
per event.

The scheduler also tracks whether a full repaint is required. A full repaint
clears Ratatui's mutable inline viewport; ordinary draws update the current
frame. The event loop waits for the earlier of the next tick and the scheduled
presentation deadline. It drains at most `MAX_AGENT_EVENTS_PER_RENDER` events
before returning to input and rendering, which prevents a busy provider from
starving keyboard handling.

`RatatuiSurface` projects the app into an inline viewport. Completed transcript
rows are inserted into native terminal scrollback, while the composer, status,
and other live surfaces remain in the mutable viewport. The terminal session
uses raw mode and bracketed paste, but deliberately does not use the alternate
screen, preserving normal terminal scrollback and selection behavior.

## Boundaries

- `cli/app` owns state and pure transition decisions. It does not read keys,
  poll channels, spawn workers, or draw frames.
- `runtime` owns polling, channels, worker lifecycle, process effects, and
  terminal control. It does not implement provider wire conversion or render
  individual widgets.
- `core/agent` and `core/harness` own application-side agent orchestration.
  The runtime starts them through the harness boundary and receives events.
- `cli/renderer` reads `App` and creates semantic views and rows. It does not
  change transcript, prompt, or run state.
- `core/session` owns append-only records. Runtime and lifecycle code request
  persistence through the session writer; rendering does not write session
  files.

The ACP server has its own transport loop in `server/`. It can reuse the shared
application and agent boundaries, but terminal polling and Ratatui presentation
are TUI-only runtime responsibilities.

## Key Types

- `App` — all mutable interactive application state.
- `Msg` — the message vocabulary consumed by the update path.
- `Action` — semantic input actions after terminal decoding.
- `UpdateResult` — pure follow-up message plus effect requests.
- `Effect`, `EffectResult`, and `EffectRequest` — effect protocol and request
  identity.
- `RunState` and `PromptState` — run lifecycle and prompt presentation state.
- `AgentEvent` — normalized events from a background agent run.
- `AgentSlot` — the runtime's active event receiver, cancellation token, and
  steering channel.
- `PresentationScheduler` — dirty-state and frame-deadline coordination.
- `InteractiveSurface` and `RatatuiSurface` — terminal adapter boundary.

## Invariants

- `update_with_effects` is the single state-transition path used by the
  interactive runtime. Effects are executed outside that function.
- At most one `AgentSlot` is active for an interactive turn.
- Agent events can mutate the app only through the effect-result path, and
  request identity filters stale completions.
- Cancellation is cooperative. The runtime waits for a worker during normal
  settlement and detaches it only after the configured stopping grace period
  expires.
- The runtime drains agent events in bounded batches and coalesces background
  redraws. Input handling can therefore continue while a provider streams.
- The renderer observes `App`; it does not own application state or persistence.
- Terminal cleanup restores raw-mode, cursor, keyboard-enhancement, and
  bracketed-paste state when the surface exits or is suspended.

## Source Map

| Responsibility | Primary source |
| --- | --- |
| State and message definitions | `crates/thndrs/src/cli/app.rs` |
| Pure update and effect derivation | `crates/thndrs/src/cli/app.rs:update_with_effects` |
| Input translation and action handling | `crates/thndrs/src/cli/app/input.rs` |
| Agent event mutation and persistence | `crates/thndrs/src/cli/app/agent_lifecycle.rs` |
| Main interactive loop | `crates/thndrs/src/runtime/interactive.rs` |
| Effect execution and agent spawning | `crates/thndrs/src/runtime/interactive.rs` |
| Frame scheduling | `crates/thndrs/src/runtime/mod.rs:PresentationScheduler` |
| Terminal lifecycle and surface adapter | `crates/thndrs/src/runtime/terminal.rs` |
| Agent harness boundary | `crates/thndrs/src/core/harness/mod.rs` |
| Provider-backed run orchestration | `crates/thndrs/src/core/agent/` |
| Terminal projection and drawing | `crates/thndrs/src/cli/renderer/` |
| Session records and writer | `crates/thndrs/src/core/session/` |

## Related

- [Codebase tour](/docs/internals/codebase/)
- [Request lifecycle](/docs/internals/lifecycle/)
- [Context assembly](/docs/internals/context/)
- [Terminal UI](/docs/internals/terminal-ui/)
- [Sessions](/docs/internals/sessions/)
- [Development workflow](/docs/development/workflow/)
