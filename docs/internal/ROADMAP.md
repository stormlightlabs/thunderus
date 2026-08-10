# Product and Architecture Roadmap

This is the canonical product direction for `thndrs`. It consolidates the
former harness-instance and Quiver feature plans, the internal backlog, and the
UI research completed on 2026-08-10. [`TODO.md`](TODO.md) is the executable
backlog; this document explains the sequence and the decisions behind it.

## North Star

`thndrs` should be a quiet, transcript-first coding agent that works equally
well as a daily-driver TUI, a composable headless process, and an ACP server.
The default interface should make the current task, agent state, authority,
queued instructions, and durable result obvious without turning the terminal
into a dashboard.

The product has three reinforcing roles:

1. **Interactive agent.** A focused conversation, a capable prompt editor, a
   navigable transcript, progressive disclosure for tool activity, and
   complete review and recovery workflows.
2. **Dispatchable process.** The same executable can run once with JSONL or
   remain available over ACP, with stable identity and the same provider,
   permission, workspace, and session semantics as the TUI.
3. **Small harness.** A foreground session can supervise a bounded number of
   explicit child `thndrs` processes and can load trusted external capabilities
   through Quiver without absorbing their runtimes into `thndrs-agent`.

This is not a multiplexer, IDE, plugin host, or general workflow engine. Herdr,
the user's terminal, ACP clients, and external tools remain first-class peers.

## What the Research Says

The useful commonality among Grok Build, Amp, Factory Droid, Claude Code,
Codex, and Pi is not their decoration. It is their interaction model:

- the transcript is the durable work surface;
- streaming and tool lifecycle updates settle into stable entries rather than
  producing an ever-growing activity log;
- details are available in place but routine success stays compact;
- the composer supports history, modes, command discovery, queued follow-ups,
  steering, and interruption without requiring a separate screen;
- pickers, help, permissions, details, and settings are temporary focused
  surfaces over the conversation;
- session identity and recovery are product features, not persistence details;
- interactive and headless operation share the agent/session core;
- current operational state is always visible, while account and diagnostic
  detail is available on demand.

There are two different kinds of evidence in this comparison. Grok Build,
Codex, and Pi expose source, so their module boundaries and testing techniques
can inform our architecture. Amp, Factory Droid, and Claude Code expose product
behavior and official documentation; they inform interaction requirements, not
internal implementation claims.

### Grok Build

[Grok Build](https://github.com/xai-org/grok-build/) is the closest source-level
reference. Its repository separates the composition root, interactive pager,
headless/runtime shell, tools, and workspace concerns. Within the pager it
separates app/event-loop/effects, normalized input, modal views, and a
first-class scrollback domain. The scrollback itself has distinct block,
entry, state, layout, rendering, search, selection, and sticky-position
modules. Its pager depends directly on Ratatui and Crossterm and has a separate
presentation-primitives crate rather than placing another UI framework between
semantic state and Ratatui.

Its supporting crates are narrow and purpose-specific: text wrapping, Unicode
width and segmentation, ANSI conversion, and first-party textarea and inline
rendering helpers. The pager also implements small Ratatui widgets directly,
including a one-row status bar with left, center, and right content. Grok's
appearance system remains more decorated than the direction chosen here; it
uses borders and selection boxes in some focused surfaces. We should copy its
ownership boundaries and direct rendering path, not its frame decoration.

Useful references:

- [repository and crate map](https://github.com/xai-org/grok-build/)
- [pager application modules](https://github.com/xai-org/grok-build/tree/main/crates/codegen/xai-grok-pager/src/app)
- [scrollback modules](https://github.com/xai-org/grok-build/tree/main/crates/codegen/xai-grok-pager/src/scrollback)
- [input modules](https://github.com/xai-org/grok-build/tree/main/crates/codegen/xai-grok-pager/src/input)
- [direct Ratatui status-bar widget](https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-pager/src/views/status_bar.rs)
- [pager dependencies](https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-pager/Cargo.toml)

The lesson is not to reproduce Grok's size. It is to give transcript, input,
effects, and overlays real boundaries before they become large collections of
conditionals.

### Codex and Pi

The [Codex TUI source](https://github.com/openai/codex/tree/main/codex-rs/tui/src)
organizes durable history cells, live streaming, the bottom pane, overlays,
diffs, keymaps, app events, and resume selection as separate concerns. Pi's
[TUI package](https://github.com/badlogic/pi-mono/blob/main/packages/tui/README.md)
uses components, differential rendering, overlays, a terminal abstraction, and
a virtual terminal for tests. Pi also keeps its
[agent session API](https://github.com/badlogic/pi-mono/blob/main/packages/coding-agent/docs/sdk.md)
usable without the interactive UI.

For `thndrs`, these reinforce three choices: history and live activity need
different representations, input should become semantic actions before it
mutates state, and terminal behavior needs a deterministic backend rather than
only string snapshots.

### Amp, Factory Droid, and Claude Code

[Amp's manual](https://ampcode.com/manual) documents editable queued prompts,
steering after the current step, immediate interruption, previous-message
editing, a command palette, configurable keys, and collapsible details.
[Claude Code's interactive-mode documentation](https://code.claude.com/docs/en/interactive-mode)
documents transcript detail, rewind, background tasks, mode cycling, rich line
editing, direct shell input, and a unified command/skill/plugin menu.
[Factory Droid's quickstart](https://docs.factory.ai/cli/getting-started/quickstart)
documents a full-screen interface with explicit modes, direct shell input,
approval surfaces, shortcuts, and workflow commands; its
[headless mode](https://docs.factory.ai/cli/droid-exec/overview) is read-only by
default and keeps structured automation separate from the interactive shell.

These are behavior targets. We should adopt the coherence—one composer, one
transcript, explicit modes, inspectable queues, and complete workflows—without
copying product-specific key bindings or adding every peer feature.

### Ratatui

Ratatui is an immediate-mode rendering library, not an application
architecture. Its own guidance presents
[The Elm Architecture](https://ratatui.rs/concepts/application-patterns/the-elm-architecture/)
and [component architecture](https://ratatui.rs/concepts/application-patterns/component-architecture/)
as compatible organization patterns, and recommends pure view projection for
TEA. Its [event-handling guidance](https://ratatui.rs/concepts/event-handling/)
also calls out the scaling limit of one raw-key match and the value of
centralized event capture followed by message passing.

`thndrs` should therefore keep its TEA-like model/update/view flow and use
small state-owning domains where local behavior is substantial. Ratatui should
render those projections directly; it should not become the owner of product
state or provider events.

"Direct Ratatui" does not mean reimplementing text handling. Keep Crossterm and
the existing Unicode width and segmentation utilities, and add another focused
crate only when a concrete editor, wrapping, ANSI, or clipboard requirement
justifies it. Do not add another general component or layout framework.

## Current Baseline

The roadmap starts from working code, not a greenfield redesign.

Already present:

- `thndrs-agent` is a provider-neutral leaf library; provider wire payloads and
  terminal, filesystem, session, and transport adapters stay in `thndrs`.
- The interactive application has `App`, `Msg`, and one `update` mutation path.
- `RendererView` and `SemanticUiView` project application state for rendering.
- The alternate-screen renderer owns a complete frame and uses Ratatui with a
  test backend.
- Transcript entries, live activity, prompt input, pickers, details, queue
  state, onboarding, sessions, JSONL, and ACP all exist in some form.
- Tool execution and background process ownership have explicit lifecycle and
  cancellation behavior.
- Session resume and naming, headless execution, JSONL streaming, piped input,
  and ephemeral sessions are implemented.
- ChatGPT Codex, OpenCode Zen, and OpenCode Go are the first-class routes;
  Umans has been retired.

The main structural liabilities are concentrated rather than pervasive:

- `App` owns unrelated UI, session, auth, process, audit, permission, and queue
  state, so small features increase field coupling.
- raw terminal input still reaches large mode-specific mutation branches;
- transcript projection, custom rows, Ratatui drawing, and an iocraft
  canvas-to-row adapter form parallel presentation layers;
- completed tool activity does not yet have one stable block identity and
  complete progressive-disclosure behavior;
- full-screen terminal ownership is not matched by complete in-app search,
  selection, copy, and anchored-follow behavior;
- queued input is visible only as a summary and cannot be reliably inspected
  and edited;
- process-instance and Quiver contracts are planned but not implemented.

The highest-churn files and bug-fix clusters overlap `cli/app`, input,
transcript/rendering, core agent lifecycle, session handling, and server
handlers. Repository history available for this study is short, so churn is a
directional signal, not a quality score. It supports the same conclusion as
the code flow: narrow the boundaries around state, input, rendering, and
transport events before adding orchestration.

## Architecture Direction

### Keep one application architecture

The target flow is:

```text
Crossterm / provider / process / timer events
                    |
             normalized Event
                    |
             Action / AppMsg
                    |
       update(model) -> Effects to execute
                    |
          pure semantic projections
             /             \
      Ratatui widgets   JSONL / ACP adapters
             |
       terminal backend
```

The boundaries have distinct responsibilities:

- **Event capture** decodes terminal and runtime events. It does not mutate
  application state.
- **Keymap and focus routing** translate a normalized key or mouse event into a
  product action for the active mode/surface.
- **Update** is the only application mutation path. It returns explicit effects
  for filesystem, provider, process, session, clipboard, and terminal work.
- **Semantic projections** expose only the state a surface needs. They are pure
  and do not know about terminal cells or provider payloads.
- **Ratatui widgets** own layout and rendering for bounded UI regions. They do
  not own agent or session state.
- **Transport adapters** consume stable semantic agent/session events, not TUI
  views. JSONL and ACP are peers of the TUI, not render targets.

This preserves the best part of the current design and removes translation
layers that do not protect a real boundary.

### Split state by product domain, not by widget

Keep a top-level `AppModel` (the current `App`, renamed only if useful) and move
cohesive fields and invariants into concrete sub-state:

- `SessionState`: active session, persistence, resume/fork/export metadata;
- `TranscriptState`: stable blocks, viewport anchor, selection, search, details;
- `ComposerState`: editor buffer, history, mode, attachments, queued prompts;
- `OverlayState`: the single focused picker/modal/detail/help surface;
- `RuntimeState`: active agent run, provider status, permissions, processes;
- `InstanceState`: bounded child specifications and lifecycle, when introduced;
- `OnboardingState`: setup flow until it can be removed from the steady-state
  application model.

These are structs and enums, not service traits. Introduce traits only for real
effect boundaries such as session storage, providers, process execution,
clocks, clipboards, or terminal control.

### Make the transcript a model, not rendered history

Introduce stable transcript block identifiers and explicit block kinds. A
block owns its semantic lifecycle and compact/detail projections:

- user prompt;
- assistant prose;
- reasoning summary;
- tool call with `queued`, `running`, `succeeded`, `failed`, or `cancelled`;
- edit/diff;
- permission request and decision;
- status/error/system notice;
- child-instance activity and result.

Provider and tool events update a block in place. Rendering computes wrapped
lines and visible ranges for the current width, but layout caches are derived
and invalidated by width/content/style generation. Session records preserve
semantic events, not terminal rows.

Routine successful reads and searches should settle to a one-line summary.
Edits, failures, permission decisions, verification, and unexpected effects
stay prominent. Expanded detail must remain bounded and deterministic.

### Use Ratatui as the only bounded-screen renderer

Port the focused surfaces currently rendered through iocraft to direct Ratatui
widgets, using existing semantic view types and snapshots to prove parity.
Remove the iocraft canvas-to-row adapter and dependency after the last surface
moves. Retain the custom row/style types only where they are a useful
presentation primitive shared by transcript tests or non-terminal output; do
not keep them merely to mirror Ratatui's `Line`, `Span`, `Style`, and `Buffer`.

This should be a sequence of small parity changes, not a renderer rewrite. The
alternate-screen driver, terminal cleanup, resize handling, and full-frame
ownership stay intact throughout.

The dependency tree supports this choice but is not the main reason for it.
`iocraft 0.8.3` has 61 packages in its normal dependency subtree in the current
lockfile, including its macro crate, `taffy`, `generational-box`, `futures`,
`regex`, and a second `unicode-width` version. Some are already shared by the
application, so removing iocraft will not remove all 61 packages. The decisive
cost is architectural: focused surfaces currently render into an iocraft
canvas, convert that canvas into custom rows, and then draw those rows through
Ratatui. One renderer and one layout vocabulary are easier to change and test.

### Use a borderless visual language

The normal frame, composer, focused surfaces, pickers, permissions, help, and
details use no decorative borders or box-drawing chrome. Separate regions with
spacing, alignment, restrained background bands, typography, and color. Show
focus with the cursor, selection background, text weight, or an accent glyph.
Do not spend terminal cells enclosing content.

This rule applies to application chrome, not source material. Diffs, tables,
tool output, and user content may contain their own rules or box-drawing
characters. Monochrome mode must preserve hierarchy through spacing and text
attributes rather than color alone.

### Treat full-screen ownership as a product commitment

The existing daily-driver study found that alternate-screen operation removes
native scrollback, terminal search, and ordinary selection/copy. Grok succeeds
in full-screen mode because it implements these affordances itself.

Keep one full-screen architecture and complete it:

- searchable transcript with visible match count and navigation;
- keyboard and mouse text selection where terminal support is reliable;
- explicit copy action with actionable clipboard failure;
- visible anchored-away and follow-latest states;
- stable content while streaming or updating tool blocks;
- suspend/resume, resize, crash cleanup, narrow-width, Unicode, and monochrome
  behavior covered by deterministic and real-terminal tests.

Do not maintain separate inline and full-screen renderers without evidence that
the complete full-screen workflow still fails a real user need.

### Make the composer the control center

The composer should remain available while the agent works. It should unify:

- normal prompts and multiline editing;
- history and editing a prior prompt into a new turn;
- slash-command discovery;
- direct shell mode, if adopted, with unmistakable authority and output;
- queued follow-ups, targeted steering, reorder/edit/delete/send-now;
- interruption and cancellation without losing queued work;
- file mentions and image attachments when provider capability permits;
- explicit plan/read-only and write-capable modes once sandbox policy exists.

The queue is durable application state. Its items need stable identifiers,
target, order, bounded preview, audit state, and explicit settlement. A transient
list of strings is not sufficient once steering and children exist.

### Keep overlays sparse and focus explicit

At most one focused overlay is active. Help, command palette, session picker,
model/provider picker, transcript details, permissions, queue editor, review,
and instance picker share consistent open/close/focus behavior. They should not
each add a new top-level input mode and raw-key match.

The default frame remains transcript + composer + restrained operational
status. Instance controls use a compact list and drill-down; they do not create
per-child panes or a permanent dashboard.

### Make the status line useful and configurable

Keep one borderless status row visible below the composer. Its content is a
pure projection of application state, with configurable ordered left and right
segments chosen from a typed set such as run state, active tool, model route,
authority, workspace, session, queue, viewport anchor, and child count. The
default stays sparse. Configuration controls which known segments appear and
their order; it does not execute commands or interpolate arbitrary templates.

Each segment declares its minimum width and priority. At narrow widths the
renderer removes optional segments, then truncates eligible values without
wrapping. Current run state, permission waits, failures, and authority must not
be displaced by cosmetic context. Unknown, stale, unavailable, and zero remain
distinct. Account, quota, token, and diagnostic detail belongs in `/status` or
`/usage`.

### Unify instance identity, not transport implementations

One executable keeps three roles:

- `thndrs`: foreground TUI;
- `thndrs run --jsonl`: one-shot, protocol-clean process;
- `thndrs acp serve`: long-lived ACP server.

JSONL and ACP may have different wire protocols, but they map to the same local
instance identity, model route, absolute workspace, session policy, authority,
and lifecycle. The TUI supervisor owns child pipes, process groups,
cancellation, timeout, bounded evidence, and cleanup. Children never share the
parent transcript or inherit write/delegation authority implicitly.

Start with one read-only child. Add bounded concurrency only after cancellation,
failure isolation, permission visibility, and durable result handles are
proven. Provider-reported capacity can inform routing only when a supported and
reliable account API exists; `unknown` must never be treated as zero or safe.

### Make Quiver the external capability boundary

Quiver is a registry and policy layer for independently installed tools called
arrows. "Arrow" is the canonical product term; do not introduce "bolt" in
public CLI or configuration names. Quiver is not a second plugin runtime.

An arrow has a declarative TOML or JSON manifest, optional agent-authored
overlay, explicit argv operations, declared effects, health, enablement, and
scope. Project arrows shadow global arrows by name. Trusted manifest fields own
entrypoints and authority; an overlay can add bounded learned context but cannot
change execution, effects, permissions, or trust.

The default context receives only compact identity/status/capability metadata.
Full docs and learned notes load explicitly. Invocation remains contained,
timed, bounded, permission-aware, and auditable. `mccabre` is the first
read-only vertical slice; artifact writing and remote authority remain out of
scope for v1.

## Delivery Sequence

### Milestone 1: Establish the UI foundation

Split cohesive application state, normalize input into actions, introduce
explicit effects where mutation is currently mixed with I/O, and establish
stable transcript blocks. Port focused surfaces to direct Ratatui, adopt the
borderless frame and configurable status line, and remove iocraft only after
parity. Use the existing test suite as the baseline and add characterization or
performance evidence only for the boundary being changed.

The exit condition is architectural: one event/update/projection flow, one
bounded-screen renderer, no user-visible regression, and deterministic tests at
the semantic, widget-buffer, and terminal-driver boundaries.

### Milestone 2: Close the daily-driver gaps

Complete transcript search/selection/copy/follow, stable progressive disclosure,
an inspectable/editable queue, accurate operational status, structured review,
and real-terminal quality. Run the same implementation, diagnosis, review,
failure-recovery, and resume workflows repeatedly until the high-severity study
findings are closed or explicitly accepted.

### Milestone 3: Make `thndrs` a small process harness

Settle the instance contract and common identity, validate real ACP packaging,
prove provider routes through TUI/JSONL/ACP, supervise one read-only child, then
add bounded concurrency and sparse controls. Recursive delegation stays off
until authority and lifecycle invariants have direct tests.

### Milestone 4: Ship Quiver v1

Implement pure manifest discovery, explicit enablement and health, compact
context projection, bounded operations, the read-only `mccabre` arrow, and the
public setup/trust documentation. Quiver follows the sandbox/permission model;
it does not invent a parallel one.

### Milestone 5: Expand platform and safety deliberately

Add session fork/export, image prompts, additional provider routes, project
trust, sandbox backends and approvals, context inspection/diff/lineage,
request-projection capture, and versioned evaluations as independent vertical
slices. A writing child in an isolated worktree comes only after read-only
instances and OS-enforced write containment are proven.

### Horizons that need a concrete trigger

Do not schedule these merely because the architecture can accommodate them:

- a repository-map arrow needs evidence that bounded, targeted derived context
  beats existing search, plus an explicit cache/index lifecycle;
- a memory arrow needs a facts model, provenance, retention and forgetting,
  and user-controlled writes;
- Quiver sandbox execution waits for the shared application sandbox boundary;
- jobs, watch triggers, or a local daemon wait for a demonstrated durable
  workload;
- first-class `ocaat` support needs separately permissioned local reads and
  remote writes;
- write-capable children wait for isolated workspaces and explicit Git
  authority; remote transports, automatic model routing, a public instance SDK,
  or a permanent instance cockpit each need an observed use case that ACP,
  JSONL, manual routing, and sparse controls cannot meet.

## Quality Gates

Every milestone must preserve these invariants:

- `thndrs-agent` remains provider- and UI-neutral.
- provider payloads do not enter public library APIs or session-domain models.
- rendering and projection are pure where practical; terminal, process,
  provider, filesystem, clipboard, and network effects stay isolated.
- raw input never bypasses focus, mode, and permission policy.
- transcript, queue, session, and child lifecycle transitions are deterministic
  and reject invalid transitions.
- protocol stdout is clean; diagnostics are bounded, redacted, and routed to
  stderr or durable audit records as appropriate.
- unknown capacity, diff, authority, or completion state is displayed as
  unknown rather than inferred.
- full-screen cleanup works on success, error, cancellation, panic, suspend,
  resume, and resize.
- focused tests precede workspace checks; real-terminal and provider smokes are
  reserved for behavior deterministic tests cannot establish.
