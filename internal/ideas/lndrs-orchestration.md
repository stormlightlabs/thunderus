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

### A separate crate and binary

`crates/lndrs` produces a `lndrs` binary depending on `thndrs` as a library.
A `thndrs orchestrate` subcommand would invert that dependency, putting the
orchestrator inside the thing it orchestrates.

The name carries a prior meaning in this repository, and the idea file should
say so rather than let someone find it later. `archive/x/lndrs` at `f6785b7` is
Landorus, a Bun, TypeScript, Svelte 5, and OpenTUI frontend that talked to Rust
over a versioned `thndrs frontend --stdio` NDJSON boundary. That subcommand is
gone from `edge`; the `Command` enum at `crates/thndrs/src/cli/mod.rs:181` has
no `frontend` variant. Its plan named "subagents, Fleet, Workbench, or dashboard
surfaces" as non-goals, which is close to an inversion of this document. The
branch is prior art for the frontend-neutral protocol and is not a spec for this
work.

### The interface waits for the verification loop

No review pass can currently see a frame. That is the problem
`tui-screenshot-verification` (`01M2RH1Z3FCE0WBY5H6G9VAGS0`) exists to solve and
`../features/tui-verification/plan.md` is still building. A full-screen
multi-pane interface shipped before that lands is an interface nobody reviews.

## Open

### Where the work graph lives

Thunderstorm keeps it in GitHub issues, which
survives a killed session and any harness, and needs the network. A local
durable store works offline and can diverge from the board. Probably both, with
issues authoritative and the local store a cache, but the reconciliation rule is
the decision and it is not made. Settled by writing down what happens when the
two disagree.

### Whether the control protocol extends ACP or is new

ACP supports `_meta`
fields, custom methods prefixed with `_`, and capabilities advertised at
initialization, so orchestration verbs can ride it without a fork. It models
nothing about concurrent sessions, delegation, or sub-agents, so those verbs
have to be designed either way. Settled by attempting the verb set as an ACP
extension and seeing whether session-scoped methods can express a work graph
that outlives any one session.

### Isolation past the worktree

Worktrees are settled by the `worktree` skill
and cover source. They do not cover port allocation, per-worker database state,
or environment variables that still point at shared resources. These are
unsolved across the tools surveyed, not specific to this design, and they arrive
the first time two workers start a dev server. Settled by picking which of the
three lndrs owns and which it documents as the operator's problem.

### Whether lndrs runs unattended

`loop-engineering` argues for a bounded loop
where a human triggers each run and the stop rule is structural, and
thunderstorm adopts it. An engine makes the unattended shape cheap enough to
reach for. Settled by deciding whether a scheduled trigger is a supported entry
point or a thing the protocol declines to offer.

## Sources

- `internal/thunderstorm.md` (`01M2PWX233GKXE5M9SPTNTGN0D`): the statuses, stop
  rules, claim semantics, and review sequence an engine would execute.
- `crates/thndrs-agent/src/instances.rs`: the delegation contracts, bounds, and
  eight-state lifecycle. Read in full; the absent consumer was confirmed by
  grepping `thndrs_agent::instances` across `crates/thndrs/src`.
- `crates/thndrs/src/server/mod.rs` and `crates/thndrs/src/core/acp/`: the two
  existing ACP surfaces.
- `crates/thndrs/src/cli/mod.rs:181`: the `Command` enum, which no longer
  carries `frontend`.
- `archive/x/lndrs` at `f6785b7`, `docs/internal/features/lndrs/plan.md`: the
  Landorus frontend, its NDJSON boundary, and its non-goals.
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
