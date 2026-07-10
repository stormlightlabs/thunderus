---
title: Context Control And Memory: Post-Baseline Tasks
status: Draft
captured: 2026-07-10
---

The ledger, scoped instructions, file-backed lexical memory, prompt projection,
audit records, and initial session contract moved to
[`000_baseline`](../000_baseline/plan.md). This file retains only work that
builds on a usable, optional memory capability.

## Semantic Retrieval Contract

**What to build:** Define the future semantic retrieval extension without
adding embeddings, vector tables, model downloads, or remote calls.

**Blocked by:** Baseline release gate

**Acceptance criteria:**

- [ ] Memory Markdown remains the source of truth.
- [ ] Candidate provider, cache metadata, mixed-model, rebuild, and lexical
      fallback rules are documented.
- [ ] The future extension cannot silently replace FTS/BM25 or make semantic
      retrieval mandatory.

## Context And Memory Interaction

**What to build:** Add visible inspection and explicit mutation commands above
the baseline ledger and optional memory capability.

**Blocked by:** Baseline release gate

**Acceptance criteria:**

- [ ] `/context`, `/context all`, `/memory`, `/memory stats`, and
      `/memory recall` show bounded, redacted read-only data while preserving
      prompt input on failure.
- [ ] `/pin`, `/drop`, `/recover`, `/clear-context`, `/remember`,
      `/memory open`, `/memory forget`, and `/memory index rebuild` obey the
      existing audit, containment, secret-warning, and no-side-effect-on-replay
      rules.
- [ ] Memory commands clearly report that memory is disabled until a user
      enables it.

## Compaction And Health

**What to build:** Complete high-risk compaction review and make source/index,
pin, budget, and compaction health actionable.

**Blocked by:** Context And Memory Interaction

**Acceptance criteria:**

- [ ] `always`, `auto`, and `never` review policies visibly distinguish
      approval-required and no-review compactions.
- [ ] Health reports actionable source, index, pin, budget, and review-state
      findings through a redacted `/doctor` command.
- [ ] Context/memory/compaction rows and focused surfaces remain clear at
      narrow and small terminal sizes without replacing native scrollback.

## Public Documentation And Verification

**What to build:** Document everyday optional-memory and context-control use
after the interactions are complete.

**Blocked by:** Context And Memory Interaction; Compaction And Health

**Acceptance criteria:**

- [ ] Public docs explain enablement, scope, precedence, inspection, mutation,
      compaction, recovery, and the deferred semantic-retrieval boundary.
- [ ] Docs state that memory cannot grant permissions or override higher-priority
      instructions.
- [ ] Rust and public-doc verification passes.

**Verification:**

- `cargo test context`
- `cargo test memory`
- `cargo test app`
- `cargo test renderer`
- `pnpm --dir docs build`
