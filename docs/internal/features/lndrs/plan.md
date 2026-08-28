# Landorus

Landorus (`lndrs`) is an experimental alternative terminal frontend for
Thunderus. It tests whether a TypeScript frontend built with Svelte 5
reactivity and OpenTUI can provide a simpler, faster-moving presentation layer
than the current Ratatui implementation without changing the agent harness.

Landorus is a sibling frontend, not a second agent implementation. Provider
calls, tools, MCP, context assembly, permissions, sessions, compaction, and
agent lifecycle remain owned by the existing Rust application core and
`thndrs-agent`.

The initial implementation lives under:

```text
packages/lndrs/
```

and communicates with the Rust application through a versioned local stdio
protocol.

## Goals

Landorus should answer three questions:

1. Can Thunderus support a non-Rust frontend through a clean application
   boundary?
2. Is OpenTUI substantially easier to develop and polish than the current
   Ratatui renderer?
3. Can Svelte's reactive primitives provide useful state management without
   requiring a custom Svelte renderer?

The experiment is successful if Landorus can support normal daily agent use
with substantially less frontend-specific machinery while preserving the
behavior and safety guarantees of the Rust application.

## Non-goals

The first implementation does not:

- replace the existing Ratatui frontend;
- implement provider, tool, MCP, context, or session behavior in TypeScript;
- use ACP as its internal frontend protocol;
- embed Rust through Node/Bun FFI;
- introduce a Svelte renderer or OpenTUI reconciler;
- require `.svelte` components;
- reproduce every administrative slash command before the core interaction
  loop is usable;
- make the frontend protocol a public compatibility commitment.

A Svelte/OpenTUI reconciler may become a separate experiment after the
imperative integration has demonstrated where one would remove meaningful
boilerplate.

## Architecture

The dependency direction remains:

```text
                    ┌─ Ratatui frontend
                    │
                    ├─ ACP server
                    │
thndrs-agent ← core ├─ headless surfaces
                    │
                    └─ frontend protocol
                              │
                              ▼
                         Landorus
```

Landorus never calls providers or tools directly.

The Rust side owns:

- provider authentication and requests;
- prompt and context assembly;
- model/tool continuation;
- MCP connections;
- workspace and authority checks;
- permission semantics;
- session persistence;
- queue semantics;
- compaction;
- cancellation;
- normalized diagnostics.

Landorus owns:

- terminal rendering;
- terminal input;
- focus;
- local keybindings;
- scroll position;
- ephemeral view state;
- presentation of transcript, tools, permissions, context, and status;
- translating user interaction into protocol commands.

The frontend protocol is therefore an application adapter between the two
layers, not a remote agent API.

## Rust frontend boundary

Add an application-owned frontend module under a name such as:

```text
crates/thndrs/src/core/frontend/
├── mod.rs
├── command.rs
├── event.rs
├── protocol.rs
└── snapshot.rs
```

The exact module split may change, but frontend-neutral contracts belong in
`core`, not `cli`.

The bridge must use the existing application core and `core/harness`. It must
not create a parallel provider or tool loop.

Behavior currently trapped in `cli/app` that Landorus also requires should be
moved to the narrowest shared owner rather than imported from the TUI. Terminal
editing, focus, pickers, layout, and Ratatui projections remain in `cli`.

This maintains the existing frontend relationship:

```text
frontend
   │
   ▼
thndrs application core
   │
   ▼
thndrs-agent
```

## Transport

The initial transport is newline-delimited JSON over stdio:

```text
lndrs
  │
  ├─ spawn
  ▼
thndrs frontend --stdio
```

Landorus owns the user's terminal. The child process must not enter raw mode,
draw terminal output, or emit human-readable text to stdout.

The bridge contract is:

- stdin: frontend commands;
- stdout: protocol responses and events only;
- stderr: bounded human diagnostics;
- process exit: bridge termination.

The protocol is explicitly versioned from its first message. Version 1 uses one
JSON object per line. Every command contains `version`, a non-empty request
`id`, and a semantic `command` name. The first command must be `initialize` and
must list the versions the frontend accepts. Its response contains the selected
version and the initial snapshot. Later responses retain the request `id`;
asynchronous events use a monotonic `sequence` number instead.

The Rust implementation lives in `crates/thndrs/src/core/frontend/`. It caps an
input line at 1 MiB, a snapshot at the latest 200 transcript entries, each
snapshot text field at 16 KiB, each event text field at 64 KiB, and tool output
at 200 lines. Diagnostic and failure text is redacted and capped at 512 bytes.
A command that exceeds its limit or contains malformed JSON produces a protocol
error. End-of-file without a successful `shutdown` command is an unexpected
peer disconnect and cancels active work.

A local stdio transport keeps the experiment:

- inspectable;
- replayable;
- language-neutral;
- easy to test;
- free of FFI ownership and ABI concerns.

Sockets and daemon mode are deferred until a concrete multi-client or
long-lived-process requirement exists.

## Protocol model

The protocol has three concepts:

### Commands

Commands request application behavior.

Initial commands should cover:

```text
initialize
state.snapshot

turn.submit
turn.cancel

queue.submit
queue.delete

permission.respond

session.new
session.load
session.close

model.select
reasoning.select

shutdown
```

Commands that need a result carry a request identifier. The Rust side returns
a typed success or failure response without relying on event timing to
represent command completion.

### Events

Events describe asynchronous application changes.

Initial event families should cover:

```text
run.started
run.finished
run.cancelled
run.failed

assistant.delta
reasoning.delta

tool.started
tool.updated
tool.finished

usage.updated
context.updated
queue.updated

permission.requested

session.updated
model.updated
status.updated

diagnostic
```

The frontend protocol should project existing normalized application events
rather than serialize provider-native payloads.

Where `AgentEvent` already expresses the required semantic event, lower it into
the frontend protocol. Do not make the serialized Rust enum itself the
long-term wire schema.

### Snapshots

Event streams are not sufficient to initialize or recover a frontend.

The Rust side therefore exposes a bounded `FrontendSnapshot` containing the
current frontend-visible application state, including where applicable:

- session identity;
- workspace;
- selected model and reasoning effort;
- run state;
- transcript projection;
- active tools;
- queued input;
- pending permission;
- context/accounting summary;
- recoverable status.

Initialization returns a snapshot before live events begin.

A frontend may request another snapshot after reconnect or when it detects an
event-sequence mismatch.

## Protocol invariants

The protocol must preserve the same authority boundary as the Ratatui
frontend.

In particular:

- a frontend command cannot grant authority not present in the Rust
  application;
- tool execution never occurs in Landorus;
- provider credentials never cross the protocol;
- raw provider payloads never cross the protocol;
- filesystem containment remains enforced by Rust;
- permission decisions use the existing permission semantics;
- protocol output is bounded where the underlying application output is
  bounded;
- cancellation uses the existing cooperative cancellation path;
- session durability remains owned by `core/session`.

The protocol should be deterministic enough to record as JSONL fixtures and
replay into a frontend without running a model.

## Why not ACP?

ACP remains the interoperability protocol for editors and external agents.
Landorus has a different relationship with Thunderus: it is a native
presentation layer for the complete Thunderus application.

The frontend protocol may therefore expose Thunderus-specific state such as:

- queue state;
- context usage;
- compaction state;
- active skills;
- local session state;
- application diagnostics;
- UI-facing model configuration.

These concepts should not be forced into ACP merely to avoid another small
adapter.

Both transports must still converge on the same application behavior.

## Landorus runtime

The initial frontend stack is:

```text
Bun
TypeScript
Svelte 5 runes
@opentui/core
```

Svelte is used for reactive application state, not as a rendering target.

Reactive modules use `.svelte.ts` where useful:

```text
packages/lndrs/src/
├── main.ts
├── protocol/
│   ├── client.ts
│   ├── commands.ts
│   └── events.ts
├── state/
│   ├── app.svelte.ts
│   ├── transcript.svelte.ts
│   ├── run.svelte.ts
│   └── permissions.svelte.ts
├── views/
│   ├── root.ts
│   ├── transcript.ts
│   ├── tool.ts
│   ├── prompt.ts
│   ├── permission.ts
│   └── status.ts
├── input/
│   ├── keymap.ts
│   └── actions.ts
└── testing/
    └── replay.ts
```

The frontend should not reproduce the current Rust module structure
mechanically. Its modules should reflect frontend responsibilities.

## Svelte and OpenTUI

Landorus deliberately begins with `@opentui/core`, not a framework binding.

OpenTUI owns a retained tree of renderables. Landorus creates those objects
once and changes their properties as application state changes.

Svelte owns reactive state such as:

```text
session
transcript
active run
tools
queue
permission
context usage
selection
```

A small projection layer synchronizes the state that affects each renderable.

Avoid making every field a separate effect. Prefer coarse view projections and
explicit update functions where they are easier to understand.

For example:

```text
FrontendEvent
    │
    ▼
Landorus state
    │
    ├─ transcript projection ──► OpenTUI transcript renderables
    ├─ run projection ─────────► activity/tool surface
    ├─ prompt projection ──────► composer
    └─ status projection ──────► status line
```

The experiment should establish whether this amount of imperative glue is
small enough to keep.

If repetitive tree synchronization becomes a dominant source of complexity,
that evidence can motivate a separate Svelte/OpenTUI binding later.

## Initial interface

The first usable Landorus interface is intentionally narrow:

```text
┌─────────────────────────────────────────────────────────────┐
│ transcript                                                  │
│                                                             │
│ user input                                                  │
│ assistant output                                            │
│ tool activity                                               │
│ reasoning                                                   │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│ prompt / steering input                                     │
├─────────────────────────────────────────────────────────────┤
│ model · context · queue · run status                        │
└─────────────────────────────────────────────────────────────┘
```

It must support:

- streamed assistant output;
- streamed reasoning;
- tool lifecycle display;
- Markdown and code where OpenTUI supports them;
- prompt submission;
- queued follow-up input;
- steering;
- cancellation;
- permission decisions;
- transcript scrolling;
- model/reasoning selection;
- session resume;
- context and usage status.

The visual design does not need to imitate the Ratatui frontend. Behavioral
parity matters more than layout parity.

## Transcript model

Landorus should consume semantic transcript blocks rather than reconstructing
meaning from arbitrary strings.

The frontend model distinguishes at minimum:

```text
user
assistant
reasoning
tool
error
diagnostic
system/status
```

Streaming deltas update the currently active semantic block instead of adding
one renderable per token.

Tool calls retain stable identifiers so start/update/finish events modify one
tool surface.

Large completed output should become stable retained content rather than
remaining coupled to the live run state.

## Rendering and responsiveness

The frontend must not redraw or rebuild the complete transcript for every model
delta.

Streaming updates should:

1. update the active transcript block;
2. mutate only affected OpenTUI renderables;
3. let OpenTUI schedule rendering;
4. preserve scroll behavior when the user is inspecting history.

Landorus may coalesce presentation-only updates over a short frame interval,
but it must never discard semantic protocol events.

Rendering cadence is a frontend concern. Agent event delivery remains lossless.

## Input and keybindings

Landorus owns terminal key events and maps them to semantic frontend actions.

The first keymap should preserve familiar Thunderus behavior where practical,
especially:

- submit;
- newline;
- cancel/stop;
- quit;
- scroll;
- queue target;
- model selection;
- reasoning selection;
- permission allow/reject.

Keymap implementation is not part of the wire protocol. The Rust side receives
semantic commands.

## Replay and frontend development

Recorded protocol streams are a first-class development tool.

Fixtures should cover at least:

```text
simple-turn.jsonl
reasoning.jsonl
tool-heavy.jsonl
permission.jsonl
queued-input.jsonl
cancelled.jsonl
failure.jsonl
compaction.jsonl
long-transcript.jsonl
```

Landorus can run against a replay source:

```text
bun run dev --replay ../../fixtures/frontend/tool-heavy.jsonl
```

or an equivalent command.

Replay must make frontend development possible without credentials, network
access, provider usage, or nondeterministic model output.

The Rust side should also test protocol projection using deterministic
fake-provider runs.

## Testing

Landorus should use three levels of tests.

### State tests

Feed typed protocol events into the Svelte state layer and assert the resulting
frontend state.

### Render tests

Use OpenTUI's testing facilities or an equivalent deterministic render surface
to verify important views at fixed terminal sizes.

### Integration tests

Spawn `thndrs frontend --stdio`, perform a deterministic fake-provider turn,
and verify:

- handshake;
- snapshot;
- streaming;
- tools;
- cancellation;
- permission round trips;
- terminal event;
- clean shutdown.

Protocol fixtures should be reusable by both Rust and TypeScript tests.

## Packaging

During the experiment, Landorus is run from the repository and may require Bun.

It should not complicate the normal Rust release until the experiment has
passed its evaluation criteria.

A future packaged form could be:

```text
lndrs
```

with `thndrs` either discovered on `PATH` or distributed alongside it.

Bundling the Rust backend and JavaScript frontend into one release is deferred.

## Evaluation

Landorus should not replace Ratatui merely because the prototype works.

After the core workflow reaches parity, evaluate it against the existing
frontend on:

- implementation size;
- complexity of state/render synchronization;
- ease of adding a new surface;
- streaming smoothness;
- terminal correctness;
- startup latency;
- steady-state CPU and memory;
- cross-platform behavior;
- test ergonomics;
- packaging complexity;
- maintenance burden.

Possible outcomes are:

1. Landorus remains an experimental alternative frontend.
2. Landorus becomes a supported alternative frontend.
3. Landorus demonstrates a better frontend architecture and eventually replaces
   Ratatui.
4. The experiment is archived, while the new Rust frontend protocol remains
   useful.
5. The experiment is archived entirely if the process boundary or TypeScript
   stack adds more complexity than it removes.

No replacement decision is part of the initial implementation.
