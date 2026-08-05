---
title: Harness Instances And Daily Driver
status: Draft
captured: 2026-08-05
---

## Objective

Make `thndrs` useful in two forms that share one agent behavior:

1. a foreground coding agent that feels good enough to use all day; and
2. a harness instance that another harness, or another `thndrs` process, can
   start, observe, steer, stop, and resume.

An orchestrated agent is a real child process with its own context, transcript,
session, model, working directory, and lifecycle. The parent does not call a
hidden in-process subagent loop. External harnesses and parent `thndrs`
instances use the same process boundary.

The normal foreground model may be
`chatgpt-codex/gpt-5.6-sol`. Child instances may use any supported ChatGPT
Codex, OpenCode Zen, OpenCode Go, or configured ACP model. Model selection
remains an explicit instance property; the orchestrator does not silently
choose a model.

Umans is no longer a supported product route. Remove it from first-run setup,
provider readiness, model discovery, documentation, live smokes, and new
instance contracts. Existing Umans configuration should fail with one
actionable unsupported-provider message during the transition. Never delete a
stored credential automatically.

## Consolidated Feature Work

This pair becomes the active product plan for the foreground harness and its
process boundary:

- The ACP packaging work folds in completely. Real-client validation,
  protocol-clean stdio, registry metadata, and the rule against speculative
  transports belong to dispatchable instances.
- The unfinished release-candidate provider and workbench gates fold in with a
  changed provider direction: ChatGPT Codex and OpenCode replace ChatGPT Codex
  and Umans.
- Completed context optimization and observability are part of the product
  baseline. Request accounting, bounded evidence and recovery, deterministic
  reduction, reviewed range compression, exports, replay fixtures, and
  benchmarks feed instance status and daily-driver review.
- [Quiver](../quiver/plan.md) remains separate. Toolchain extensibility should
  not block foreground use or instance dispatch.
- Parking-lot subagent supervision is replaced by child `thndrs` processes.
  Worktree-isolated writing children remain deferred until read-only instances
  are proven.

## Product Direction

### One executable, three roles

`thndrs` already contains the three required roles:

- interactive TUI for a human operator;
- `run --jsonl` for a bounded one-shot process;
- `acp serve` for a long-lived, controllable agent process.

This feature turns those modes into one coherent instance contract. A caller
chooses the narrowest interface that fits:

| Need                                                             | Interface            |
| ---------------------------------------------------------------- | -------------------- |
| One prompt, wait for completion                                  | `thndrs run --jsonl` |
| Long-lived session, streaming, permissions, cancellation, resume | `thndrs acp serve`   |
| Human foreground work                                            | `thndrs` TUI         |

ACP is the primary control protocol for supervised instances. JSONL remains a
simple process/event interface, not a second interactive protocol. Herdr can
continue to supervise terminal processes and panes; it does not need special
knowledge of `thndrs` internals.

### Instance orchestration

A parent `thndrs` starts a child executable and talks ACP over stdio. The child
may itself use a built-in provider or a configured external ACP agent. The
parent owns the child process, but the child owns its agent loop and session.

```text
human or harness
       |
       | ACP or JSONL
       v
thndrs instance (cwd, model, session, policy, capacity)
       |
       | provider request or configured ACP delegation
       v
ChatGPT Codex / OpenCode / external agent
```

Self-dispatch uses the same shape:

```text
foreground thndrs
       |
       | spawn + ACP stdio
       v
child thndrs --cwd <workspace> --model <model> acp serve
```

The parent receives bounded semantic updates and a final summary. Child
transcripts do not merge into the parent transcript. A child cannot recursively
delegate unless the user or project instructions explicitly grant that
authority and the configured depth/concurrency limits permit it.

### Daily-driver experience

Provider support is not the main blocker. ChatGPT OAuth, Sol/Terra/Luna,
OpenCode models, reasoning controls, tools, queued steering/follow-up, sessions,
focused detail panes, and deterministic renderer tests already exist.

The current interaction model has several likely sources of friction:

- the implementation enters an alternate screen even though the product notes
  favor a transcript-first, native-scrollback workflow;
- queued steering and follow-up text cannot be inspected or edited, only
  counted;
- durable sessions can be inspected and resumed from commands, but there is no
  TUI session picker;
- diff detail exists, but review is not yet a complete workflow with an
  explicit read-only review command and structured findings;
- tests prove components, but there is no short dogfood protocol that records
  where real work becomes slower or less trustworthy than Codex or Pi.

These are hypotheses, not a license for a visual rewrite. Begin with a bounded
side-by-side workflow study. Fix observed interaction breaks in vertical slices
and retain the current quiet visual language where it works.

## User Stories

- As a developer, I can use Sol in a normal `thndrs` session without fighting
  transcript navigation, queued input, change review, or session recovery.
- As a developer, I can start Terra or Luna explicitly for a comparison or
  review without changing my global Sol default.
- As a harness author, I can dispatch `thndrs` with an exact cwd, model, prompt,
  session policy, and safety policy, then consume stable semantic events.
- As a subscription user, I can see current remaining capacity and reset timing
  for the active ChatGPT or OpenCode account without confusing it with session
  token consumption.
- As a parent `thndrs` instance, I can delegate a bounded independent task to a
  child `thndrs`, inspect its state, steer or stop it, and receive a concise
  result.
- As a Herdr user, I can run `thndrs`, Codex, and Pi as ordinary pane processes
  and choose which harness, if any, controls another.

## Success Criteria

### Daily driver

- A five-workflow dogfood protocol records concrete friction against the
  current `thndrs`, Codex, and Pi experiences without scoring aesthetics.
- The main transcript supports terminal-native search, selection, copy, and
  useful history, or a documented alternative proves better in the actual
  workflow study.
- A running turn exposes queued input contents and lets the user remove or edit
  an item before delivery.
- A user can inspect changes, verification, failures, and full bounded tool
  evidence without leaving the current task.
- A user can find and resume a recent valid session from the TUI without losing
  the current draft.
- Cancellation, provider failure, pending permission, active model, cwd,
  session, and instance state are always distinguishable.
- Remaining subscription capacity is visible in `/usage`, instance status, and
  a quiet orientation surface. Stale or unavailable provider data is labeled.

### Dispatchable instances

- A caller can launch an instance with an explicit cwd and any configured
  supported model, including ChatGPT Codex and OpenCode model IDs.
- ACP covers long-lived control: initialize, prompt, semantic streaming,
  permissions, cancellation, session load/resume/close, and clean shutdown.
- JSONL covers one-shot execution with a versioned event vocabulary and stable
  exit status.
- stdout stays protocol-clean in ACP and JSONL modes. Diagnostics use stderr.
- Instance identity, parent identity, depth, model, cwd, session, lifecycle,
  and final outcome are typed and auditable without storing secrets.
- Account-capacity snapshots use provider-reported windows, balances, limits,
  and reset timestamps. The harness never estimates remaining subscription
  capacity from tokens consumed in the current session.

### Self-dispatch

- A foreground parent can supervise bounded read-only child instances through
  the same ACP client used for external agents.
- Children have isolated context and transcripts, explicit models, independent
  cancellation, and bounded summaries.
- Concurrency and delegation depth are explicit limits. Parent cancellation
  settles every owned child before the parent completes.
- A child cannot write merely because its parent can write. Write-capable child
  instances require separate authorization and an isolated workspace strategy.

## Current State

- `thndrs run --jsonl` already provides a versioned, provider-neutral, one-shot
  event stream and supports ephemeral runs.
- `thndrs acp serve` already exposes stdio negotiation, prompt streaming,
  cancellation, permissions, sessions, MCP configuration, and protocol-clean
  stdout.
- `acp:<name>` already lets `thndrs` dispatch a configured external ACP agent.
- `HarnessTurn` owns one in-process provider run and exposes semantic events,
  steering, permissions, execution hooks, and cancellation.
- The TUI already queues steering and follow-ups, renders semantic transcript
  rows, and has tool/diff detail surfaces.
- Sessions are append-only JSONL with inspection, export, and command-line
  resume support.
- Known model configuration already includes ChatGPT Codex Sol, Terra, and Luna
  plus OpenCode Zen and OpenCode Go routes.
- Request accounting records provider-reported input/output/cache usage when a
  response supplies it, but no account-capacity model currently reports how
  much ChatGPT or OpenCode subscription usage remains.

The missing architectural unit is a supervised `thndrs` process represented as
an instance, rather than another provider turn represented as a subagent. The
missing product work is a coherent foreground workflow, not another provider.

## Subscription Capacity

Subscription capacity and request consumption are different values:

- request accounting says what a completed provider request consumed;
- account capacity says what the provider reports as remaining, used, reset,
  balance, or plan allowance for the authenticated account.

Add one provider-neutral `AccountCapacitySnapshot` with explicit provenance and
freshness. It can represent multiple rate-limit windows, percentage used or
remaining, reset timestamps, monetary/credit balance, plan label, and a
provider-supplied limit state. Every field is optional because providers expose
different shapes. Unknown is never rendered as zero.

For ChatGPT-managed Codex access, current Codex interfaces expose primary and
secondary rate-limit windows with used percentage, window duration, and reset
time; optional plan, workspace credit, and reset-credit data may also be
present. `thndrs` should fetch the equivalent account data through a supported
ChatGPT/Codex account boundary available to its OAuth route. If no supported
boundary is available to this independent client, stop and document the gap
rather than scrape the web dashboard or infer capacity from session tokens.

For OpenCode Go, remaining subscription allowance and its reset are the primary
capacity values. For OpenCode Zen, credit balance and configured monthly limits
are the relevant values. Use an authenticated API documented or shipped by
OpenCode. If the API omits a value, render the known provider state plus a link
to the OpenCode console and label the missing value unavailable.

Expose capacity in four places:

- `/usage` for full provider detail and refresh;
- `/status` for a compact active-provider summary;
- the quiet orientation surface when remaining capacity crosses a configurable
  warning threshold or a limit is reached;
- ACP/JSONL instance metadata so a supervising harness can avoid starting work
  on a depleted account.

Refresh on explicit request, successful authentication, instance startup, and
provider capacity notifications when supported. Use a bounded cache to avoid
spending quota or adding startup latency on every render. Show observation time
and stale state. Never persist account email, tokens, or raw account responses.

## Instance Contract

### Specification

An instance specification contains:

- executable and protocol version;
- exact model ID;
- absolute working directory;
- durable new session, resumed session, or ephemeral policy;
- reasoning and search configuration;
- read-only or explicitly approved write authority;
- delegation depth and concurrency budget;
- bounded environment allowlist and safe configuration sources;
- optional parent instance ID and task label.

Credentials remain child-owned configuration. The parent may allow the child to
resolve its normal credential store, but it never copies access tokens into the
prompt, event stream, instance specification, or session metadata.

### Identity and lifecycle

Each supervised child has an opaque local instance ID and one lifecycle:

```text
starting -> ready -> running -> waiting_permission -> running -> completed
                                     |                |          |
                                     v                v          v
                                  stopping --------> failed / cancelled
```

Transitions are driven by process state and protocol events, not inferred from
assistant prose. Unexpected EOF, protocol corruption, and child exit are typed
failures. A child result contains status, bounded summary, session handle,
changed-path metadata when available, verification evidence, and failure
diagnostics.

### Authority

The first self-dispatch slice is read-only. It is useful for repository
orientation, research, diagnosis, and review without introducing concurrent
workspace mutation.

Writing children are a later expand-contract step. They need a separate
workspace or worktree, explicit user authorization, containment checks, change
inspection, and separate apply/cleanup actions. A child never writes into the
parent's active checkout concurrently by default.

### Parent interaction

Keep instance controls sparse and transcript-oriented:

- list active/recent instances;
- open one instance's bounded status and latest events;
- steer, stop, or close exactly one instance;
- surface a child's permission request while another view is focused;
- insert the settled child summary into the parent as a typed result.

Do not build a multi-pane dashboard inside the TUI. Herdr and the terminal
already provide process layout. `thndrs` needs legible lifecycle and control,
not another multiplexer.

## Daily-Driver Study

Before changing the terminal ownership or interaction model, run the same five
workflows in current `thndrs`, Codex, and Pi:

1. orient in an unfamiliar repository and ask a follow-up while work is active;
2. make a bounded edit, inspect the diff, and run verification;
3. recover from a failed command and steer the active turn;
4. interrupt, exit, and resume the task;
5. delegate a read-only investigation to another harness instance.

Record only observable friction:

- extra keystrokes or commands;
- unclear current state;
- hidden or hard-to-recover evidence;
- lost draft or queued input;
- broken terminal selection/search/scrollback;
- unclear change review;
- unreliable cancellation, resume, or process cleanup.

The study produces an ordered friction ledger. Each UX ticket must link to one
ledger item and close it with a deterministic test plus a short human terminal
check. Avoid a broad redesign based on screenshots or preference alone.

## Development Bootstrap

Codex can supervise `thndrs` as an ordinary process while this plan is being
implemented. Keep one writer in the shared checkout at a time:

1. give `thndrs` one ticket or one bounded investigation with an exact model,
   working directory, expected result, and verification command;
2. wait for that run to settle;
3. inspect its events, workspace changes, and verification output;
4. let Codex review or continue the work only after `thndrs` has stopped.

Use JSONL for machine-readable one-shot runs and the TUI when a route can ask
for permission. Current native headless tools can write without an interactive
approval step, so a read-only instruction is a working convention rather than
an enforced authority boundary. Do not run Codex and `thndrs` as concurrent
writers in the same checkout.

Inside Herdr, start `thndrs` in a normal sibling pane and coordinate it as a
process. Herdr owns terminal layout and visibility; Codex owns delegation,
review, and the decision to continue. This bootstrap path must not create a
Herdr dependency in `thndrs`.

## Verification Strategy

- Pure tests cover instance specification, lifecycle transitions, bounds,
  summaries, and redaction.
- Fake ACP clients/servers cover process startup, negotiation, semantic events,
  steering, permissions, cancellation, session operations, malformed protocol,
  EOF, and cleanup.
- Black-box tests launch the real `thndrs` executable in JSONL and ACP modes.
- Renderer snapshots cover the instance list/detail, queued-input editor,
  session picker, permission handoff, and revised transcript/prompt states.
- A small PTY suite covers terminal ownership, resize, raw-mode restoration,
  Ctrl-C, selection/scrollback behavior, and child cleanup.
- Live Sol/Terra/Luna and OpenCode checks remain opt-in. Objective commands and
  workspace state determine task success; model review is supplemental.

For Rust changes:

```sh
cargo fmt
cargo clippy --workspace --fix --allow-dirty --allow-staged
cargo clippy --workspace
cargo test --workspace
```

Public documentation changes also require:

```sh
pnpm --dir docs build
```

## Boundaries

Always:

- use a real process boundary for orchestrated harness instances;
- keep ACP and JSONL stdout protocol-clean;
- make cwd, model, session policy, authority, depth, and concurrency explicit;
- cap child output, retained events, runtime, tool calls, and summary size;
- settle owned child processes during cancellation and shutdown;
- preserve credential redaction and existing workspace containment;
- distinguish provider-reported capacity, stale capacity, unavailable
  capacity, and per-request token consumption;
- keep deterministic software as the correctness oracle.

Ask first:

- new public protocol fields, dependencies, transports, or stable SDK promises;
- write-capable children, worktree creation, or any Git mutation;
- automatic model selection or routing;
- an undocumented/private provider capacity endpoint or browser-dashboard
  scraping;
- delegation beyond the configured depth/concurrency limits;
- replacing alternate-screen terminal ownership with an inline/hybrid renderer
  after the workflow study reports its evidence.

Never:

- implement orchestrated children as hidden in-process agent loops;
- merge unbounded child transcripts into the parent model context;
- inherit write authority implicitly;
- pass credentials through prompts, events, or child command arguments;
- estimate subscription capacity from local token accounting or treat an
  unavailable value as zero;
- allow recursive self-dispatch without direct authority and hard bounds;
- use Herdr-specific APIs when ordinary process and protocol contracts suffice;
- mutate Git unless the user explicitly requests the Git operation.

## Deferred Milestones

- Write-capable child instances after the read-only lifecycle and isolated
  workspace contract are proven.
- Remote transports only for a concrete deployment that cannot use local stdio.
- Automatic model routing only after repeated task evidence supports it.
- A sidebar or multi-agent cockpit only if frequent instance switching proves
  command/detail surfaces inadequate.
- A public instance SDK only after a second external harness needs more than ACP
  and JSONL.

## Risks And Open Questions

- A full switch from alternate-screen rendering to inline/native scrollback is
  a large terminal change. The dogfood study must establish whether a hybrid
  committed-transcript plus small live region solves the real problem.
- ACP is the cleanest existing long-lived boundary, but not every harness speaks
  ACP. JSONL handles one-shot dispatch; a second interactive protocol would need
  a demonstrated caller.
- A parent and child pointed at the same writable checkout can race even with
  separate sessions. Read-only children are the safe first slice.
- Child model availability and credentials can change independently. Instance
  startup must report setup/provider failure without misclassifying it as task
  failure.
- ChatGPT and OpenCode can change account-capacity APIs independently of model
  request APIs. Keep capacity optional, typed, and freshness-aware so an outage
  does not block an otherwise usable coding session.
- “Feels right” cannot be closed by adding features indiscriminately. The
  friction ledger and repeated real tasks are the acceptance boundary.
