---
title: thndrs Baseline
status: Ready
captured: 2026-07-10
---

## Objective

Establish the smallest durable foundation for `thndrs` to evolve as a coding
agent: a workspace with two independently useful libraries, a simple default
agent, explicit capability boundaries, and project directives that let an
assigned agent complete one scoped ticket safely.

The target workspace contains four crates over time:

| Crate | Role | Release order |
| --- | --- | --- |
| `thndrs-agent` | Reusable agent loop | First |
| `thndrs-context` | Reusable context and memory system | First |
| `thndrs` | Existing CLI/TUI package and executable | After the libraries |
| `thndrs-acp` | ACP server package and executable | Baseline, after the libraries |

`thndrs` retains its existing package metadata. The package identities are
stable, but all public APIs remain pre-1.0 and may change as use in the two
applications teaches us what to stabilize. Publishing remains a human-only
action.

## Current State

- The repository is one Cargo package with a library plus the `thndrs` and
  `thndrs-acp-server` binaries.
- `src/core/agent.rs` already contains a reusable loop and cancellation/tool
  hooks, but imports CLI and TUI types such as `WebSearchMode` and `App`'s
  `AgentEvent`.
- `src/core/context/`, `src/core/memory/`, `src/core/prompt/`, and
  `src/core/session/` already hold typed context, memory, prompt, and
  append-only JSONL/session contracts. They are useful seams, but currently
  live behind the application crate.
- The direct renderer and the ACP server are real consumers of the same agent,
  context, session, and tool behavior. They are the compatibility boundary for
  this work.
- Context-control already has its typed ledger, scoped instructions, lexical
  memory recall, prompt projection, and append-only audit foundation. Session
  navigation/inspection is planned but not yet usable. ACP's stdio core is
  implemented in the root package but is not yet in its final package.

## Product Principles

1. **Start small and explicit.** A fresh install has an inspectable prompt,
   project instructions, local tools, and session evidence. It does not need
   memory, retrieval, plugins, or background work to be useful.
2. **Make capabilities optional, not invisible.** Durable memory and retrieval
   are disabled at runtime by default. Enabling them is deliberate, visible in
   diagnostics/session metadata, and reversible. The context library also has
   no default `memory` Cargo feature for downstream users.
3. **Keep the libraries independent.** `thndrs-agent` and `thndrs-context`
   have no dependency on each other. `thndrs` and `thndrs-acp` compose typed
   data at their boundaries. A third shared crate requires explicit approval.
4. **Keep side effects at application adapters.** The agent library has no
   terminal UI, ACP protocol, direct filesystem policy, or TUI state. The
   context library has no provider, terminal, or ACP dependency. Applications
   own their local tool policy, persistence locations, and rendering.
5. **Earn abstractions and crates.** Extract an existing tested seam only when
   it has a real consumer. Do not create utility, trait, feature-flag, or
   provider crates merely for symmetry.
6. **Preserve working behavior through each move.** Every extraction keeps the
   fake-provider, local CLI/TUI, and current ACP-server test boundaries green.
   Package layout is never an excuse for a product rewrite.
7. **Assigned work is autonomous; scope is not.** An agent may inspect,
   implement, and verify its assigned ticket. It asks before dependencies,
   public API commitments, destructive operations, baseline changes, or work
   outside that ticket. It never self-starts unrelated work or publishes.

## Library Contracts

### `thndrs-agent`

Own provider-neutral turn input, streaming events, messages, tool definitions,
permission decisions, cancellation, retry policy, and the agent run/harness
API. Provider-wire adapters may be optional implementation modules, but the
public loop must not expose Umans/OpenCode/ChatGPT protocol payloads.

Applications supply tool execution and permission policy through typed hooks
or adapters. Filesystem containment, shell execution, ACP permission RPC, and
terminal rendering stay outside this crate.

### `thndrs-context`

Own typed context items and ledgers, scoped instruction discovery/selection,
prompt-context projection, durable session-record contracts, and the
file-backed memory API. It must preserve current bounded loading, metadata,
audit, redaction, and recovery behavior.

The crate's default features exclude memory. An application that enables the
memory build feature must still expose a runtime setting that defaults to off;
when off, it does not load, index, retrieve, or write memory. Session evidence
and ordinary project instructions remain available without memory.

## Workspace And Dependency Shape

```text
thndrs-agent ───┬──> thndrs
                └──> thndrs-acp
thndrs-context ─┬──> thndrs
                └──> thndrs-acp
```

The baseline first introduces the two libraries, then moves the proven ACP
server into `thndrs-acp`. The root `thndrs` package remains the CLI/TUI
application throughout. No placeholder application crate is created merely to
complete the diagram.

## Directive Baseline

`AGENTS.md` becomes the concise, checked-in operating contract for humans and
agents. It must include the product principles above, the package dependency
rule, the one-assigned-ticket work protocol, the required Rust verification
sequence, and explicit approval/publish boundaries. It should point to this
baseline for current foundation work rather than repeat feature specifications.

`.gitignore` explicitly contains `!AGENTS.md` so a checked-in root directive is
not hidden by any broader ignore rule.

## Minimum Usability Requirements Moved Here

The following requirements are now baseline work. They are not repeated as
active tickets in later feature folders.

### Context And Optional Memory

`thndrs-context` must preserve the existing minimum context contract:

- a typed, budgeted ledger with reasons and a compact model-visible dashboard;
- scoped `AGENTS.md` discovery and closest-applicable selection;
- structured prompt projection rather than ad hoc concatenation;
- append-only context, compaction, and memory audit records that exclude
  secret-shaped body content;
- file-backed Markdown memory, rebuildable lexical FTS/BM25 recall, and
  explicit deletion/recovery semantics.

The baseline changes the activation policy: memory/retrieval is disabled for a
fresh `thndrs` configuration and remains untouched until a user enables it.
The later context feature retains advanced inspection, mutation, compaction
review, health, rendering, public documentation, and semantic retrieval.

### Session Use And Inspection

Baseline includes the entire initial sessions feature. A user must be able to:

- list recent sessions, show a compact session summary, resume an unambiguous
  session id, and view current token totals from the TUI;
- use `thndrs sessions list|show|resume|inspect|export` plus bounded `debug`
  log readers without changing default no-subcommand TUI startup;
- inspect a stable, renderer-independent, redacted JSON projection and export
  readable session JSONL in sequence order;
- resume only after an exclusive writer lock, never restore stale live state,
  and preserve the prompt draft after failed read-only commands;
- tolerate corrupt or missing log/session files with actionable diagnostics.

Session source remains append-only JSONL. Indices and summaries are derived and
rebuildable; inspect/export must never replay side effects such as memory
deletion or compaction.

### ACP Minimum Contract

The current stdio ACP implementation is a baseline compatibility requirement:
version negotiation, session creation/prompt/cancellation, streamed semantic
updates, permission-gated writes/shell, session audit, and fake-client coverage
must survive the library extraction and the baseline `thndrs-acp` package
migration. `009_acp_agent_server` then handles real-editor testing, registry
packaging, and any transport expansion.

### Explicitly Retained For Later Features

| Source | Retained work |
| --- | --- |
| `001_context_control` | advanced context/memory UX, compaction review, health, rendering, docs, and semantic retrieval |
| `009_acp_agent_server` | real-editor smoke test, registry packaging, and remote/custom transports |
| `010_setup` | reasoning readiness |
| `012_iocraft` | focused-surface hardening and expansion |
| `backlog` | broader app-state and input refactors |

## Success Criteria

- `cargo test --workspace`, `cargo clippy --workspace`, and package checks pass
  for `thndrs-agent` and `thndrs-context`.
- The existing `thndrs` CLI/TUI consumes both libraries and retains its current
  fake-provider, prompt, context, session, and renderer behavior.
- `thndrs-agent` has no dependency on CLI/TUI/ACP modules or a direct local
  filesystem/terminal policy.
- `thndrs-context` has no dependency on providers, terminal UI, or ACP.
- The libraries do not depend on each other.
- Fresh `thndrs` configuration leaves memory/retrieval disabled; an explicit
  enable path is visible, testable, and recorded without storing memory body
  text in audit metadata.
- The TUI and CLI session commands, safe resume behavior, inspect/export, and
  bounded redacted log readers work through `thndrs-context` session records.
- `thndrs-acp` is a package/executable that consumes the two libraries without
  a CLI/TUI package dependency.
- `AGENTS.md` and `.gitignore` make the operating contract durable.
- The ACP fake-client contract remains green through the extraction and package
  migration.

## Boundaries

Always:

- preserve existing package metadata for `thndrs`;
- make each move compile and test before the next extraction;
- keep APIs typed, documented, and intentionally small;
- reuse existing dependencies unless a new one materially improves correctness;
- keep all user-visible behavior behind an application adapter.

Ask first:

- adding a workspace crate beyond the four named here;
- adding dependencies or default Cargo features;
- committing a stable public API or a new cross-library shared crate;
- changing tool permissions, session formats, or provider behavior;
- changing a later feature's scope while moving its prerequisite.

Never:

- publish any crate, tag a release, or alter `thndrs` package metadata;
- make memory/retrieval silently active by default;
- move TUI, ACP, terminal, direct filesystem policy, or provider payload types
  into the two reusable library APIs;
- use this baseline to justify a whole-app rewrite or a new plugin framework.

## Deferred Milestones

- `001_context_control`: complete advanced context/memory operations,
  compaction review, diagnostics, rendering, documentation, and semantic
  retrieval on `thndrs-context`.
- `009_acp_agent_server`: complete real-editor and registry work after the
  packaged server is stable.
- `010_setup`: add reasoning readiness after the agent boundary is stable.
- `012_iocraft`: harden focused surfaces without changing the renderer's role.

## Verification

For Rust changes, run:

```text
cargo fmt
cargo clippy --workspace --fix --allow-dirty --allow-staged
cargo clippy --workspace
cargo test --workspace
cargo package -p thndrs-agent
cargo package -p thndrs-context
cargo package -p thndrs
cargo package -p thndrs-acp
```

Run focused cross-crate tests for the fake agent run, structured prompt/context
projection, memory-disabled startup, and existing ACP event mapping. Public
documentation changes additionally require `pnpm --dir docs build`.
