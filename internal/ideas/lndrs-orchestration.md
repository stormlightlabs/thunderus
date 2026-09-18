---
name: lndrs-orchestration
last_updated: 2026-09-18
id: 01M2RY8QAPD62KMAMTPPKT9HZZ
---

# Lndrs, an orchestration engine

## Problem

`thunderstorm` is a working orchestration protocol that no program executes.
`internal/thunderstorm.md` defines statuses, claim semantics, stop rules, a
review sequence, budgets, and escalation paths. All of it runs as Claude Code
skills reading markdown and calling `gh` or the GitHub MCP tools. Two costs
follow. The loop runs only where that harness runs, and its correctness depends
on a model following prose, so a missed rule shows up as a wrong label rather
than an error.

The contracts an engine would need already exist and have no caller.
`crates/thndrs-agent/src/instances.rs` defines `InstanceSpecification`,
`InstanceBounds`, `DelegationBudget`, `InstanceAuthority`, an eight-state
`InstanceLifecycle` with a checked `transition`, `AccountCapacitySnapshot`, and
`SettledInstanceResult` carrying `SemanticEvidence` and `ChangedPath`. Grepping
`thndrs_agent::instances` across `crates/thndrs/src` returns nothing. The
vocabulary for supervised delegation was designed and left without an engine.

Running several agents today means a terminal multiplexer and attention.
`docs/src/content/docs/docs/development/workflow.md` documents that workflow
against Herdr, tmux, and Zellij, and its operating rules are instructions to a
human: do not fill every pane, stop background commands, do not treat `unknown`
as complete. Nothing enforces them.

## Decisions

### What lndrs is

Lndrs is an orchestration engine with a published control protocol. The TUI is
one client of that protocol, and the first one, but it is not the product.

This follows from the comparison that motivates the work. Neovim's departure
from Vim was architectural rather than featural: a MessagePack-RPC API where
external programs "Call any API function, Listen for events, Receive remote
calls from Nvim", `--listen` for sockets and named pipes, `--embed` and
`--headless` so the editor runs with no interface attached, and API clients in
other languages. The plugin ecosystem, the external GUIs, and `nvr` are all
downstream of publishing that seam. A harness that wants the same shape
publishes the seam first.

The order of work follows: engine and protocol, then a headless entry point,
then the interface. Building the interface first makes the protocol whatever
the interface happened to need.

### Lndrs owns work, not panes

Herdr manages processes. Its unit is a pane holding a PTY, and agent state
arrives through `pane.report_agent`, so an integration reports the state of a
terminal. It has no unit of work, no dependency between units, no stop rule, no
budget, and no verifier, because it is not trying to have them. Reimplementing
it yields a second-best multiplexer and no capability the repository lacks.

Lndrs owns the work graph: units, their dependencies, their claims, their
budgets, and the rule that ends a run. It renders processes because a person
needs to see them, and it delegates that rendering where it can.

The two state models show the difference. Herdr tracks `idle`, `working`,
`blocked`, `done`, and `unknown`, which is what a sidebar needs.
`InstanceLifecycle` tracks `starting`, `ready`, `running`, `waiting_permission`,
`stopping`, `completed`, `failed`, and `cancelled`, separating a permission
request from a generic block and a cancellation from a failure. A scheduler
needs the second set.

### Agents are driven over ACP where possible

ACP is a JSON-RPC 2.0 protocol over stdio, published by Zed and developed with
JetBrains. An orchestrator speaking it is told a session's updates, permission
requests, and tool calls rather than inferring them from rendered output. That
is a different class of signal from terminal detection, not a cleaner version of
it.

`thndrs` already sits on both sides. `crates/thndrs/src/server/mod.rs` exposes
it as an ACP agent over stdio, and `crates/thndrs/src/core/acp/` holds the
client with `registry.rs`, `runner.rs`, `permissions.rs`, and `terminal.rs`.
The work is to drive several sessions at once, not to build the transport.

PTY panes stay as the fallback for agents with no ACP server. They are the
degraded path, and the interface should say which path a given worker is on.

### Herdr is a backend, not a competitor

The process backend is a trait with at least two implementations: an embedded
PTY, and Herdr driven through its socket API. Herdr already carries
multi-machine SSH, workflow plugins, session resurrection, and graphics
overlays. Matching that is a year of work returning no new capability, and its
socket API exists for exactly this use.

What would change this: if driving Herdr from outside proves lossy for focus,
resize, or input timing, the embedded backend becomes primary and Herdr becomes
the fallback. That is measurable before it is decided. Run `pane.split`,
`pane.read`, and `agent.wait` against a real session and look at what arrives.

### What to take from Zellij

The client/server split, where the server owns session state and PTYs while
clients attach and detach. Layouts as committed data rather than imperative
setup. Session resurrection by serializing live state on an interval. And the
fact that Zellij's own tab bar, status bar, and session manager are plugins over
its published API, which is the Neovim argument reached from another direction.

Not the WASM plugin runtime. That exists to sandbox third-party code. The
extension points here are the control protocol, skills, and ACP agents, all of
which already exist in some form.

### The work graph is Arc Lightning

Lndrs does not define a work-graph store. `stormlightlabs/arclightning` is one,
built for this, and its `SPEC.md` domain model maps onto thunderstorm almost
term for term: a spec "replaces the current Epic concept", tasks carry status,
dependency edges, blockers, a Markdown handoff, and completion evidence, and
readiness is a deterministic calculation over them. `arcl ready` is the queue a
scheduler dispatches from. `arcl context` is the focused packet a worker
receives, holding the task, its ancestors, spec and plan context, direct
blockers, the latest handoff, and relevant evidence. Read commands emit stable
JSON. Its `ROADMAP.md` records the CLI, the SQLite model, dependencies,
lifecycle state, ready-work queries, context, handoffs, and evidence as already
working under the older vocabulary; MCP and the new product model are not.

That answers the store-format question by having already answered it, and the
answer is SQLite rather than JSONL. The two formats are not competing for one
job. A work graph is queried for current state across edges, which is what
"which tasks are ready" means, and an append-only log answers that only by
replaying itself. A run record is an ordered history of what happened, which a
log holds and a table of current state loses. Lndrs needs both and already has
both: Arc Lightning for the graph, and the append-only JSONL session files
`thndrs` writes through `core/session/writer.rs` for the run.

GitHub then stops being the store and becomes a projection. Arc Lightning
already defines repository-native mode as a projection with three-state
reconciliation that "never resolves a conflict by silently choosing a side",
which is the rule thunderstorm needs and does not currently state.

What is left to decide is the seam, not the store. Arc Lightning has no MCP
crate yet, so the integration today is the CLI with `--json`, and shelling out
per dispatch is a cost worth measuring before it is accepted.

### A separate crate and binary

`crates/lndrs` produces a `lndrs` binary depending on `thndrs` as a library.
A `thndrs orchestrate` subcommand would invert that dependency, putting the
orchestrator inside the thing it orchestrates.

### The interface waits for the verification loop

No review pass can currently see a frame. That is the problem
`tui-screenshot-verification` (`01M2RH1Z3FCE0WBY5H6G9VAGS0`) exists to solve and
`../features/tui-verification/plan.md` is still building. A full-screen
multi-pane interface shipped before that lands is an interface nobody reviews.

## Open

### How lndrs reaches Arc Lightning

The store is settled and the seam is not. Arc Lightning's `crates/` holds
`arcl-cli`, `arcl-core`, `arcl-repo`, and `arcl-store`, with no `arcl-mcp`, so
the options today are the CLI with `--json` per call, an MCP server once that
crate exists, or depending on `arcl-core` directly. The last is fastest and
couples two repositories at the library level. Settled by measuring dispatch
cost against the CLI and deciding whether the coupling is worth removing it.

### Which orchestration verbs ACP carries

Zed answers the general question and
leaves the specific one open. It transmits subagent information through ACP
message metadata under `SUBAGENT_SESSION_INFO_META_KEY`, passing each child the
parent's `SessionId` and a depth counter, while keeping depth policy internal:
`MAX_SUBAGENT_DEPTH` defaults to 1 and is configurable to 4. So the precedent is
to ride `_meta` for identity and keep the graph in the client's own model, which
is what Arc Lightning now holds here.

The unresolved part is whether a session-scoped protocol can express work that
outlives every session in it. Settled by attempting the verb set as an ACP
extension and finding the first thing it cannot say.

### What isolation lndrs owns

Worktrees are settled by the `worktree` skill and
cover source. Port allocation, per-worker database state, and environment
variables pointing at shared resources are not covered, and are unsolved across
the tools surveyed rather than specific to this design.

Lndrs should not decide these for an operator. Ports and databases are
project-specific, and a policy guessed here becomes a thing to work around. The
open question is narrower: which hook lndrs offers so an operator can provision
per-worker resources, and whether that hook runs before a worker starts, after
its worktree exists, or both. Settled by writing the hook contract against two
real projects with different needs.

### What triggers a run

A scheduled or event-driven trigger is a supported
entry point, not the only one and not the default. `loop-engineering` argues for
a bounded loop where a human starts each run and the stop rule is structural,
and thunderstorm adopts it; an engine makes the unattended shape cheap enough
that it will get used either way, so it should be designed rather than
discovered.

What is open is what the unattended path requires that the attended one does
not. Candidates are a mandatory budget, an escalation target that is a person,
and a refusal to start when the previous run left anything unresolved. Settled
by writing the trigger contract with those three as requirements and seeing
which survive contact with a real run.

## Sources

- `internal/thunderstorm.md` (`01M2PWX233GKXE5M9SPTNTGN0D`): the statuses, stop
  rules, claim semantics, and review sequence an engine would execute.
- `crates/thndrs-agent/src/instances.rs`: the delegation contracts, bounds, and
  eight-state lifecycle. Read in full; the absent consumer was confirmed by
  grepping `thndrs_agent::instances` across `crates/thndrs/src`.
- `crates/thndrs/src/server/mod.rs` and `crates/thndrs/src/core/acp/`: the two
  existing ACP surfaces.
- [`stormlightlabs/arclightning`](https://github.com/stormlightlabs/arclightning),
  `SPEC.md` and `ROADMAP.md` at the tip on 2026-09-18: the domain model, the
  readiness calculation, `arcl ready`, `arcl context`, stable JSON on reads,
  repository-native projection with three-state reconciliation, and the roadmap
  statement of what already works. The crate list showing no `arcl-mcp` was read
  from the tree, not from the documents.
- [Zed subagent and thread hierarchy](https://deepwiki.com/zed-industries/zed/8.6-subagent-and-thread-hierarchy):
  `SUBAGENT_SESSION_INFO_META_KEY`, the parent `SessionId` and depth counter
  passed to each child, and depth policy held internally. Medium confidence.
  This is a generated wiki over the Zed source rather than Zed documentation,
  and the default of 1 with a configurable ceiling of 4 comes from secondary
  discussion of the same code.
- [Herdr socket API](https://herdr.dev/docs/socket-api/): NDJSON over a Unix
  socket or named pipe, dotted method namespaces, lifecycle subscriptions that
  do not replay retained events, the five agent states, and `pane.report_agent`
  as the path state arrives by. Read 2026-09-18 and current; the repository's
  own [notebook entry](../../docs/src/content/docs/docs/notebook/herdr.md) was
  captured 2026-06-28.
- [Nvim API documentation](https://neovim.io/doc/user/api.html): MessagePack-RPC,
  `--listen`, embedding through `jobstart()`, and capability discovery through
  `nvim_get_api_info()`. Supports the architectural reading of the Neovim
  comparison.
- [ACP protocol overview](https://agentclientprotocol.com/protocol/overview):
  the session methods, the `_meta` and underscore-prefixed extension points, and
  the absence of any orchestration model. High confidence, read from the spec.
  The adoption figures repeated elsewhere, 25-plus agents and the JetBrains
  registry, come from secondary coverage and are not verified here.
- [Zellij documentation](https://zellij.dev/documentation/creating-a-layout.html)
  and [FAQ](https://zellij.dev/faq/): the client/server split, KDL layouts, and
  the plugin-built interface.
- [The parallel agent session infrastructure gap](https://alexlavaee.me/blog/parallel-agent-sessions-infrastructure-gap/):
  port allocation, per-worktree database state, and shared environment
  variables as the unsolved isolation problems. A practitioner argument, not a
  measurement.
