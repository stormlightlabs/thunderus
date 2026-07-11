---
title: thndrs Baseline
status: Ready
captured: 2026-07-11
---

## Objective

Establish the smallest durable foundation for `thndrs`: one reusable
provider-neutral agent library and one CLI/TUI/ACP application. Context control
is part of the agent primitive; application adapters own filesystem discovery,
session persistence, terminal I/O, and protocol transport.

The workspace contains two crates:

| Crate | Role |
| --- | --- |
| `thndrs-agent` | Provider-neutral agent loop, contracts, and context control |
| `thndrs` | CLI/TUI package, executable, and ACP server mode |

All public APIs remain pre-1.0. Publishing stays human-only.

## Product Principles

1. **Start small and explicit.** A fresh install has a prompt, scoped project
   instructions, local tools, and session evidence.
2. **Keep context close to the loop.** `thndrs-agent::context` owns typed
   ledgers, budgets, deterministic selection, projection inputs, and
   compaction policy without provider or UI knowledge.
3. **Keep effects in application adapters.** `thndrs` owns `AGENTS.md`
   discovery, configuration, persistence, filesystem/tool policy, terminal UI,
   ACP transport, and provider wire payloads.
4. **Earn abstractions and crates.** Extract a boundary only when a real
   consumer needs it; avoid utility, plugin, and symmetry-driven packages.
5. **Preserve behavior through refactors.** The fake provider, local CLI/TUI,
   session, renderer, and ACP boundaries stay green after each move.

## Baseline Contract

`thndrs-agent` exposes provider-neutral turn/message/event, tool, permission,
cancellation, retry, and background-run contracts. Its context module exposes
pure context policy only; host applications supply candidates and render the
result.

`thndrs` supplies scoped `AGENTS.md` discovery, structured prompt construction,
append-only content-free context and compaction audit records, session
inspection/export, local-tool policy, and the terminal/ACP adapters.

```text
thndrs-agent::context ──> thndrs (CLI/TUI and `acp serve`)
```

## Minimum Usability Requirements

- Typed, budgeted context ledgers with reasons and a compact model-visible
  dashboard.
- Scoped `AGENTS.md` discovery and closest-applicable selection.
- Structured prompt projection rather than ad hoc concatenation.
- Append-only context and compaction audit records, with side-effect-free
  inspection and export.
- Session list/show/resume/inspect/export plus bounded, redacted log readers.
- `thndrs acp serve` preserves its stdio lifecycle, session, cancellation,
  permission, and fake-client behavior.

## Deferred Milestones

| Source | Retained work |
| --- | --- |
| `001_context_control` | Context inspection, working-set controls, compaction review, health, rendering, and documentation |
| `009_acp_agent_server` | Real-editor smoke test, registry packaging, and transport expansion |
| `010_setup` | Reasoning readiness |
| `012_iocraft` | Focused-surface hardening and expansion |

An agent-agnostic memory engine is a separate future product. It has no
implementation, package, protocol, or compatibility contract in this workspace.

## Boundaries

Always:

- keep context policy deterministic and side-effect free;
- keep generic MCP and ACP behavior in their existing application boundaries;
- reuse existing dependencies unless a new one materially improves correctness.

Ask first:

- adding a workspace crate, dependency, or default capability;
- committing a stable public API or changing session, permission, provider, or
  MCP behavior.

Never:

- publish a crate, tag a release, or change `thndrs` package metadata without
  direct approval;
- move terminal, ACP, filesystem policy, or provider payloads into
  `thndrs-agent`;
- create an in-process durable-memory capability or compatibility path.

## Verification

```text
cargo fmt
cargo clippy --workspace --fix --allow-dirty --allow-staged
cargo clippy --workspace
cargo test --workspace
cargo package -p thndrs-agent
cargo package -p thndrs
```

Public documentation changes additionally require `pnpm --dir docs build`.
