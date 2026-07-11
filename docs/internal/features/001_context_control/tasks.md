---
title: Context Control: Agent Primitive and Memory Removal Tickets
status: Ready
captured: 2026-07-11
---

Implementation tickets for [the context-control specification](plan.md). Work
the frontier: any ticket whose blockers are complete can start.

## Move pure context policy into `thndrs-agent`

**What to build:** Make `thndrs-agent::context` the reusable home for the
typed ledger, budget, deterministic selection, prompt-projection inputs, and
compaction policy. Keep filesystem discovery and session persistence in the
application, then retire `thndrs-context`.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [x] `thndrs-agent` exposes pure context-control types and functions with no
      provider, terminal, filesystem, ACP, or memory dependency.
- [x] `thndrs` uses the new module for every remaining context-policy call;
      application-owned `AGENTS.md` discovery and session metadata remain in
      `thndrs`.
- [x] `thndrs-context` is absent from the workspace, manifests, lockfile, and
      source imports.
- [x] Existing non-memory ledger, instruction, selection, prompt, and
      compaction behavior has deterministic regression coverage.

**Verification:**

- `cargo test -p thndrs-agent`
- `cargo test -p thndrs`
- `cargo test --workspace`

## Remove the built-in memory surface

**What to build:** Delete every built-in durable-memory behavior from the CLI,
configuration, prompt/session flow, dependencies, and tests. The resulting
application has no memory mode, command, storage, retrieval, or compatibility
path.

**Blocked by:** Move pure context policy into `thndrs-agent`

**Acceptance criteria:**

- [x] No source or supported command accepts memory configuration, performs a
      memory mutation or recall, or creates/reads a memory root or index.
- [x] Context selection and session records contain no memory candidate, item
      kind, mutation, audit, or compatibility variant.
- [x] Memory-only dependencies and files are deleted, while generic MCP and
      ordinary prompt-history behavior remain unchanged.
- [x] Focused search distinguishes removed durable memory from unrelated uses
      of the word “memory,” such as in-memory UI state.

**Verification:**

- `cargo fmt --check`
- `cargo clippy --workspace`
- `cargo test --workspace`

## Finish the context-only controls and documentation

**What to build:** Complete the planned context inspection, working-set,
compaction, health, rendering, and documentation work without reintroducing a
memory concern.

**Blocked by:** Remove the built-in memory surface

**Acceptance criteria:**

- [ ] Context inspection and working-set controls are bounded, redacted,
      preserve prompt input on failure, and explain pins, drops, recovery,
      budgets, and compaction without memory terminology.
- [ ] Compaction review and `/doctor` report only context source, pin, budget,
      and review-state health.
- [ ] Context-only surfaces have normal, narrow, and small-height regression
      coverage and retain native transcript scrollback.
- [ ] Public and internal product documentation no longer presents memory as a
      `thndrs` capability; the project-agnostic notebook is unchanged.

**Verification:**

- `cargo test -p thndrs`
- `cargo test --workspace`
- `pnpm --dir docs build`
