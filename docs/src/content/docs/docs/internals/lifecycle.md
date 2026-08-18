---
title: "Request Lifecycle"
---

A submitted prompt travels through the application state machine, a runtime
adapter, an agent worker, and the renderer. The application records the turn
before it starts the worker. The worker emits provider-neutral events. The
application applies those events to `App`. The renderer observes the updated
state on the next scheduled frame.

## Mental Model

```text
terminal input
      │
      ▼
TerminalInput → Action → Msg::Action
                           │
                           ▼
                 update_with_effects
                    │           │
          follow-up Msg       Effect::StartAgent
                                  │
                                  ▼
                    prompt/context + tool catalog
                                  │
                                  ▼
                         background agent run
                                  │
                    AgentEvent through a channel
                                  │
                                  ▼
              EffectResult::Agent → update_with_effects
                                  │
                                  ▼
                         App state and session records
                                  │
                                  ▼
                           semantic views → frame
```

`cli/app` owns state transitions. `runtime` polls input and channels, executes
effects, and schedules frames. `core/agent` runs the provider loop and tool
continuations. `cli/renderer` turns `App` into terminal rows and live surfaces.
No worker mutates `App` directly.

## Submitting a Request

1. `TerminalInput::from_event` decodes the terminal event and
   `translate_input` produces one or more semantic `Action` values.
2. `handle_msg` passes `Msg::Action(Action::Submit)` to
   `update_with_effects`. The submit handler rejects an unresolved setup or
   pending compaction review, routes slash commands, or calls
   `submit_user_turn`.
3. `submit_user_turn` appends an `Entry::User` to the transcript, records input
   history and the session record when persistence is enabled, clears the
   composer, stores the active prompt in `last_input`, increments the turn
   number, and returns `Msg::Agent(AgentEvent::Started)`.
4. The `Started` message changes the run state to `Working`. The update result
   schedules `Effect::StartAgent` with an `EffectRequest` containing the session
   id and turn number. The runtime executes that effect after the pure update.

A prompt submitted while an agent is working is handled by the queue policy.
Follow-up work is queued for the next turn. Steering input is sent to the
active provider run when its channel is available. Internal turns such as
automatic compaction use the same lifecycle without adding a user transcript
entry.

## Running the Agent

`runtime::spawn_agent` starts only when the app is `Working` and no
`AgentSlot` exists. It discovers the workspace root, builds `AgentRunConfig`
with the selected model, authority, reasoning settings, process registry,
artifact store, and skill read roots, then chooses a route:

- An ACP model id creates an ACP `RunHandle`. It receives the active prompt,
  workspace configuration, cancellation, and effective MCP configuration.
- A built-in provider creates the runtime MCP manager, obtains the runtime tool
  definitions, refreshes the context ledger, and builds a `PromptBundle` from
  the prompt, selected context, available skills, transcript, and tool catalog.
  The bundle is lowered to provider messages before the harness starts the
  turn.

The built-in path can run automatic compaction before starting the provider
when the preflight estimate crosses the configured policy threshold. It then
rebuilds the bundle and starts the requested turn. The harness owns the
background `AgentRun`. The runtime keeps its event receiver, cancellation token,
and steering sender in `AgentSlot`.

The worker first emits `Started`. For a built-in provider, the agent loop loads
provider credentials and metadata, sends requests with normalized messages and
tool schemas, streams response events, and repeats provider requests when the
model asks for tools. It checks the cooperative cancellation token before
provider requests and tool dispatch.

A completed response emits `Finished`.

A recoverable provider or execution problem emits `Failed`.

Cancellation emits `Cancelled`.

The interactive loop drains at most `MAX_AGENT_EVENTS_PER_RENDER` events in one
pass. Each event is wrapped in `EffectResult::Agent` and sent through
`handle_msg`, so streamed output and tool activity use the same transition path
as terminal input.

## Handling a Tool Call

The provider-neutral agent loop handles a tool request as a continuation of the
same turn:

1. It emits `AgentEvent::ToolStarted` with the provider id, catalog name, and
   JSON arguments.
2. The application records the running tool entry and request observation.
3. The tool boundary checks authority and, where required, permission. The
   built-in registry dispatches the request. MCP dispatches through the active
   MCP manager. Shell work uses the application process registry.
4. Tool output is converted into model-facing content. Large or display-only
   output can be reduced according to the configured model-reduction policy.
   Bounded evidence may be stored behind an application artifact handle.
5. The worker emits `ToolFinished` with display output, status, and any file or
   shell result. The application finalizes the transcript entry, persists tool
   and side-effect audits, records projection decisions, and refreshes Git
   status.
6. The agent appends the provider-native tool result to its private continuation
   and requests the next provider response. The application does not expose
   provider wire messages through its public library types.

A successful file write marks the turn as having written to the workspace. A
background shell process continues under the process registry after the tool
event. Later process results arrive through tick-driven background effects.

## Updating the Interface

`handle_agent_event` projects semantic events into application state:

- assistant and reasoning deltas extend streaming transcript entries
- status, retry, usage, request-accounting, and model metadata update their
  corresponding runtime or session fields
- permission requests open an overlay and permission outcomes resolve it
- tool events update lifecycle state, audits, artifacts, and Git status
- terminal events finalize streaming entries, persistence, timing, and run
  state

Agent and Git events mark the presentation scheduler dirty and normally use the
configured tick deadline. Input, resize, suspension, and the initial frame
request an immediate draw. `present_if_due` asks `RatatuiSurface` to render
`App`. The renderer does not perform state transitions or persistence. Completed
transcript rows enter terminal scrollback while the composer and status remain
in the mutable live viewport.

A terminal event cannot be lost merely because a frame is due. The loop polls
the earlier of the next tick and presentation deadline, drains bounded event
batches, handles input, and then draws the current state.

## Cancellation and Failure

Escape or the configured cancellation action sends `Effect::CancelAgent`. The
runtime cancels the active slot only when its `EffectRequest` matches the
current request. The provider and tool loop must observe that token and emit
`Cancelled`. The application finalizes partial streaming output, interrupts
active context observations, cancels pending permissions and steering items,
returns to `Idle`, and makes the composer editable again.

A provider failure emits `Failed`. The application records the error, marks the
request observation failed, restores the submitted prompt when appropriate,
opens credential recovery for authentication rejection, and changes the run to
`Error`. The user can edit and resubmit the prompt.

After `Finished`, `Failed`, or `Cancelled`, the runtime settles the worker. It
cancels and joins it during normal settlement. If shutdown cancellation exceeds
the stopping grace period, it detaches the worker instead of blocking the UI.
A worker panic is converted into an agent failure when the event channel closes.

`EffectRequest` identity prevents an old worker's late event from changing a
new turn. The active slot is cleared only for the matching request, and queued
follow-up input is submitted only after the prior turn has settled.

## Boundaries

- `cli/input` translates terminal events
- `cli/app` owns `App`, message handling, transcript projection, persistence
  requests, and lifecycle state. Its update path does not poll channels or do
  terminal I/O.
- `runtime` owns polling, effect execution, worker ownership, cancellation,
  background processes, and frame scheduling.
- `core/harness` and `core/agent` own provider-neutral run delivery and the
  built-in provider/tool continuation. `core/providers` owns provider-specific
  authentication, request conversion, and stream normalization.
- `core/context` and `core/prompt` assemble application context and provider
  messages but do not render the terminal.
- `core/tools` owns the built-in registry and dispatch boundary. `core/mcp`
  owns MCP discovery and connections.
- `cli/renderer` reads `App` and produces rows and frames. It does not mutate
  the transcript, run state, session writer, or agent slot.
- `core/session` owns append-only session records and bounded audit data.

The ACP server has a separate transport loop. It can use the shared harness
and agent boundaries, but terminal polling and Ratatui presentation belong to
the interactive runtime.

## Key Types

- `TerminalInput`, `Action`, and `Msg` — input decoding and semantic messages.
- `App` — session, transcript, composer, overlay, and runtime state.
- `RunState`, `PromptState`, `Effect`, `EffectResult`, and `EffectRequest` —
  lifecycle state and effect protocol.
- `AgentEvent` — provider-neutral stream, tool, accounting, and terminal events.
- `AgentSlot` — the active event receiver, cancellation token, and steering
  sender held by the runtime.
- `PromptBundle` — structured built-in-provider context before lowering.
- `HarnessTurn` and `AgentRun` — background run ownership and event delivery.
- `PresentationScheduler` and `RatatuiSurface` — frame scheduling and terminal
  projection.

## Invariants

- `update_with_effects` is the only interactive state-transition path. Effects
  execute outside it.
- A normal interactive turn has at most one active `AgentSlot`.
- Worker events reach `App` only through `EffectResult::Agent`, and request
  identity filters stale events.
- Cancellation is cooperative. The runtime joins a settled worker or detaches
  it only after the stopping grace period.
- Tool results re-enter the provider loop privately. Provider payload types do
  not cross the public `thndrs-agent` boundary.
- Only finalized model-visible entries are sent in the transcript projection.
  Streaming UI entries are not sent as completed history.
- The renderer observes state and never owns persistence or agent lifecycle.
- Agent event draining is bounded and presentation is coalesced, so provider
  output cannot monopolize input handling.

## Source Map

| Responsibility                         | Primary source                                                                |
| -------------------------------------- | ----------------------------------------------------------------------------- |
| Input decoding and submit actions      | `crates/thndrs/src/cli/app/input.rs`                                          |
| State and message definitions          | `crates/thndrs/src/cli/app.rs`                                                |
| Pure transition and effect derivation  | `crates/thndrs/src/cli/app.rs:update_with_effects`                            |
| Agent event projection and persistence | `crates/thndrs/src/cli/app/agent_lifecycle.rs:handle_agent_event`             |
| Interactive loop and event draining    | `crates/thndrs/src/runtime/interactive.rs`                                    |
| Agent spawning and prompt construction | `crates/thndrs/src/runtime/interactive.rs:spawn_agent`                        |
| Provider/tool continuation             | `crates/thndrs/src/core/agent/run.rs`                                         |
| Background run ownership               | `crates/thndrs-agent/src/run.rs`                                              |
| Provider-neutral events                | `crates/thndrs-agent/src/contracts.rs:AgentEvent`                             |
| Context ledger and selection           | `crates/thndrs/src/cli/app/context.rs` and `crates/thndrs-agent/src/context/` |
| Prompt assembly and lowering           | `crates/thndrs/src/core/prompt/mod.rs`                                        |
| Tool registry and dispatch             | `crates/thndrs/src/core/tools/` and `crates/thndrs/src/core/mcp/`             |
| Frame scheduling and terminal adapter  | `crates/thndrs/src/runtime/mod.rs` and `terminal.rs`                          |
| Transcript projection and drawing      | `crates/thndrs/src/cli/renderer/`                                             |
| Session records and writer             | `crates/thndrs/src/core/session/`                                             |

## Related

- [Codebase tour](/docs/internals/codebase/)
- [Runtime and state](/docs/internals/runtime/)
- [Context assembly](/docs/internals/context/)
- [Providers](/docs/internals/providers/)
- [Tools](/docs/internals/tools/)
- [Sessions](/docs/internals/sessions/)
- [Terminal UI](/docs/internals/terminal-ui/)
