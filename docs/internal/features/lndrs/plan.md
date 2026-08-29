# Landorus

Landorus (`lndrs`) is an experimental alternative terminal frontend for
Thunderus built with Bun, TypeScript, Svelte 5 reactivity, and OpenTUI.

Its purpose is to test whether Thunderus benefits from separating its agent
application core from its terminal presentation layer, and whether OpenTUI
provides a simpler foundation for a polished interactive frontend than the
current Ratatui implementation.

Landorus is a frontend experiment, not a second agent implementation.

## Current status

The experiment has already established its basic architecture:

- the repository is a Bun workspace containing `docs` and `packages/*`;
- Landorus lives in `packages/lndrs`;
- `lndrs` launches a Rust `thndrs frontend --stdio` child;
- the Rust application exposes a versioned NDJSON frontend protocol;
- Svelte runes own reactive frontend state;
- OpenTUI owns retained terminal renderables;
- Svelte effects project state changes onto those renderables;
- provider, tool, permission, context, session, and lifecycle behavior remains
  Rust-owned.

The remaining experiment is primarily a UI and interaction experiment.

## Goal

Landorus should answer:

1. Is a frontend-neutral Thunderus application boundary useful?
2. Is OpenTUI easier to develop and polish than the Ratatui frontend?
3. Can Svelte provide lightweight reactive state without requiring a custom
   Svelte/OpenTUI renderer?
4. Can the resulting frontend become pleasant enough for normal daily coding
   work?

Success requires a usable agent interaction loop, not feature-for-feature
visual parity with Ratatui.

## Non-goals

The current experiment does not:

- replace the Ratatui frontend;
- implement provider or tool behavior in TypeScript;
- use ACP as its native frontend protocol;
- use Bun or Node FFI to embed Rust;
- implement a custom Svelte renderer;
- introduce agent orchestration or subagents;
- implement a Fleet/dashboard interface for hypothetical future orchestration;
- reproduce Ratatui layout choices merely for parity;
- make the frontend protocol a public compatibility guarantee.

A Fleet-like orchestration surface should only be reconsidered after Thunderus
has actual multi-agent or multi-instance semantics worth visualizing.

## Repository structure

```text
thunderus/
├── package.json
├── bun.lock
├── docs/
├── packages/
│   └── lndrs/
│       ├── bin/
│       ├── src/
│       ├── tests/
│       ├── package.json
│       └── svelte-plugin.ts
└── crates/
    └── thndrs/
        └── src/core/frontend/
```

The root Bun workspace owns JavaScript dependency installation and quality
commands. Landorus remains independently runnable from its package directory.

The Rust Cargo workspace remains authoritative for the agent implementation.

## Application boundary

The dependency direction is:

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

Landorus never invokes providers, tools, or session storage directly.

### Rust owns

- provider setup and requests;
- prompt and context construction;
- model continuation;
- tool execution;
- MCP;
- permissions and authority;
- workspace containment;
- queue semantics;
- sessions and persistence;
- compaction;
- cancellation;
- normalized usage and diagnostics.

### Landorus owns

- terminal layout and visual hierarchy;
- transcript rendering;
- prompt editing;
- focus;
- scrolling;
- keybindings;
- local selection state;
- disclosure state;
- dialogs and pickers;
- presentation timing;
- translating UI actions into semantic frontend commands.

## Transport

Landorus owns the user's terminal and launches:

```text
thndrs frontend --stdio
```

Communication is versioned NDJSON over stdio.

The transport remains intentionally local and simple:

```text
Landorus
   │
   │ stdin: commands
   │ stdout: responses + events
   │ stderr: diagnostics
   ▼
Thunderus
```

The bridge must never enter raw terminal mode or write presentation output to
stdout.

Sockets, daemons, and multi-client synchronization are deferred until a real
requirement exists.

## Protocol model

The frontend protocol consists of:

- commands for semantic application actions;
- direct responses for command completion;
- ordered asynchronous events;
- bounded snapshots for initialization and recovery.

The frontend consumes normalized Thunderus concepts rather than provider-native
payloads.

Protocol commands may be named before their implementation exists, but the UI
must not expose unavailable behavior.

Initialization exposes supported commands separately from provider-specific
model and reasoning options. Landorus uses this state to omit unsupported
controls instead of discovering support through failed commands.

Landorus should track event sequence numbers. A detected gap should request a
fresh snapshot rather than attempting to infer missing state.

## Frontend stack

Landorus uses:

```text
Bun
TypeScript
Svelte 5 runes
@opentui/core
```

Svelte is a state system, not the rendering target.

`.svelte.ts` modules are compiled by the Bun preload plugin using Svelte's
`compileModule`.

OpenTUI renderables are created imperatively and retained.

A custom Svelte/OpenTUI renderer should only be considered if repeated
projection boilerplate becomes a demonstrated maintenance problem.

## UI direction

Landorus is a **single-session, conversation-first interface**.

The normal screen has three persistent regions:

```text
┌─────────────────────────────────────────────────────────────┐
│                                                             │
│ transcript stream                                           │
│                                                             │
│ user / assistant / reasoning / tools / status               │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│ prompt                                                      │
├─────────────────────────────────────────────────────────────┤
│ model · context · queue · run state                         │
└─────────────────────────────────────────────────────────────┘
```

There is no permanent sidebar.

There is no dashboard mode.

There is no Workbench mode.

There is no Fleet mode while Thunderus remains a single-agent harness.

Additional information should use progressive disclosure:

- expanded tool blocks;
- command palette;
- model/reasoning picker;
- session picker;
- permission dialog;
- context inspector;
- queue inspector;
- diff/source inspection where useful.

These surfaces should temporarily overlay or replace part of the Stream rather
than permanently reducing transcript width.

## Stream design

The transcript is the primary interface.

Its default presentation should remain quiet:

```text
you
Refactor the prompt renderer and verify it.

landorus
I'll isolate the prompt state first, then update the focused snapshots.

› read   src/ui/prompt.rs
› edit   src/ui/prompt.rs                     +38 -21

landorus
The renderer is separated and the focused tests pass.
```

Operational detail is visible but compressed.

A tool call should normally occupy one stable line or compact block. Its
arguments, output, error details, and other metadata are available through
expansion.

Reasoning should be visually distinct but secondary to final output.

Run state should be obvious without turning every agent action into dashboard
chrome.

Exceptional states deserve greater visual weight:

- permission required;
- retrying;
- cancellation;
- failure;
- context pressure;
- queued input;
- completed changes ready for inspection.

## Transcript rendering model

The current bootstrap renders the transcript as one text projection. That is
only a prototype.

The production experiment should use a retained hierarchy:

```text
ScrollBox
├── UserBlock
├── AssistantBlock
├── ReasoningBlock
├── ToolBlock
├── AssistantBlock
└── ...
```

Each semantic transcript item has a stable protocol ID and a stable OpenTUI
renderable.

Streaming must mutate the currently active block rather than reconstruct the
complete transcript.

Tool lifecycle events update one tool block by tool-call ID.

Completed blocks should remain untouched by subsequent streaming whenever
possible.

The transcript scroll surface should:

- follow new output while positioned at the bottom;
- stop following after deliberate historical scrolling;
- resume following when the user returns to the bottom;
- preserve scroll position across unrelated state changes;
- support viewport culling for large histories where appropriate.

Markdown, code, and diff renderables should be used where they materially
improve readability, not merely because OpenTUI provides them.

## Presentation cadence

Protocol event delivery is lossless.

Rendering does not need to occur once per protocol event.

Landorus may coalesce presentation updates over a short frame interval,
especially during high-frequency assistant or reasoning deltas, provided that:

- semantic state receives every event in order;
- cancellation and permission state remain responsive;
- final rendered state is exact;
- input latency does not noticeably increase.

Optimization should be driven by replay fixtures and measurements rather than
arbitrary animation delays.

## Composer and focus

The composer uses OpenTUI's multiline textarea facilities rather than a custom
text editor.

It should support:

- multiline editing;
- submit;
- explicit newline insertion;
- paste;
- queued follow-up input when supported;
- steering when supported;
- cancellation;
- preservation of unsent input.

Bindings belong to semantic frontend actions.

Simple global shortcuts may initially use renderer key events. Once multiple
focus contexts or overlays exist, commands should move to a layered keymap so
focused editors, dialogs, and global actions cannot conflict.

Bare printable global bindings must not intercept normal composer input.

## Overlays and inspection

Secondary application controls should be transient surfaces.

### Command palette

The palette provides discoverability for commands such as:

```text
session.new
session.open
model.select
reasoning.select
context.inspect
queue.inspect
transcript.bottom
quit
```

Only supported commands appear.

### Permissions

Permission requests interrupt the normal interaction loop with an explicit,
focused decision surface.

The frontend never infers permission settlement.

### Context

Normal context display remains compact:

```text
74k / 128k · 58%
```

A context inspector may expose additional normalized backend information such
as compaction threshold, active skills, or retained context when the Rust
application provides it.

Landorus must not reproduce context-selection policy.

### Review and inspection

Diffs, source output, long tool output, or diagnostics may open into a focused
inspection surface.

Inspection is temporary. Returning from it restores the conversation and input
state.

## State organization

The bootstrap currently uses one `AppState`. Keep this until the state becomes
difficult to reason about.

Split state by responsibility only when useful, for example:

```text
state/
├── app.svelte.ts
├── transcript.svelte.ts
├── composer.svelte.ts
└── overlay.svelte.ts
```

Do not mechanically mirror the Rust application module structure.

Similarly, evolve views according to actual UI responsibilities:

```text
views/
├── root.ts
├── transcript.ts
├── blocks/
│   ├── user.ts
│   ├── assistant.ts
│   ├── reasoning.ts
│   └── tool.ts
├── composer.ts
├── status.ts
└── overlays/
```

The exact split should emerge from implementation rather than being created
preemptively.

## Replay-driven development

Deterministic protocol replay is the preferred UI development workflow.

Representative fixtures should cover:

```text
simple-turn
streaming
reasoning
tool-heavy
permission
queue
cancelled
failure
retry
compaction
long-transcript
```

Replay should support:

- deterministic terminal dimensions;
- immediate playback for automated tests;
- optional original timing for manual streaming inspection;
- running without provider credentials or network access.

The same semantic fixture corpus should be usable by Rust protocol tests and
Landorus state/render tests where practical.

## Testing

Landorus uses three levels of tests.

### State tests

Apply snapshots and frontend events to state and verify semantic results.

### Render tests

Use OpenTUI's deterministic test renderer at fixed terminal sizes.

Important render tests should cover:

- narrow terminals;
- normal terminals;
- long wrapped output;
- active streaming;
- expanded tools;
- scrolled history;
- composer focus;
- permission overlays.

### Integration tests

Spawn the Rust frontend bridge against deterministic agent behavior and verify
the complete interaction path.

The TypeScript frontend should not require live provider credentials for CI.

## Evaluation

Landorus should be evaluated only after it supports normal daily agent use.

Compare it with Ratatui on:

- frontend-specific implementation size;
- state/render synchronization complexity;
- streaming smoothness;
- input responsiveness;
- terminal correctness;
- startup latency;
- steady-state CPU and memory;
- long-transcript behavior;
- test ergonomics;
- cross-platform behavior;
- packaging complexity;
- ease of adding a new UI surface.

Valid outcomes remain:

1. keep Landorus experimental;
2. support both frontends;
3. begin gradual replacement of Ratatui;
4. archive Landorus but retain the frontend protocol;
5. archive the entire experiment.

The experiment should optimize for learning, not for guaranteeing that Landorus
wins.
