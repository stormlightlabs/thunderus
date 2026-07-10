---
title: Context Control And Memory: Post-Baseline Plan
status: Draft
captured: 2026-07-10
---

## Objective

Build the user-facing controls and advanced safety work on top of the baseline
`thndrs-context` library. The baseline already makes context inspectable data
and memory an optional capability; this feature makes those capabilities easy
to understand and deliberately use.

## Baseline Capability

The baseline owns and must preserve:

- typed context ledgers, budgets, reasons, and compact model-visible dashboards;
- scoped `AGENTS.md` discovery and closest-applicable selection;
- structured prompt projection, append-only context/memory/compaction audit
  records, and side-effect-free replay;
- bounded file-backed Markdown memory with FTS5/BM25 recall, source metadata,
  redaction, recovery, and explicit deletion;
- memory/retrieval disabled by default for fresh `thndrs` configuration.

## Feature Outcomes

### Visible Interaction

Users can inspect context and optional memory with bounded, redacted commands;
pin, drop, recover, clear, remember, open, forget, and explicitly rebuild an
enabled memory index. Commands preserve prompt input on failure and clearly
explain when memory is disabled.

### Compaction And Health

High-risk compactions visibly distinguish approval-required from no-review
outcomes. A redacted `/doctor` explains source, index, pin, budget, and
compaction health with actionable remediation.

### Rendering And Documentation

Context/memory actions and focused views remain legible on small terminals and
do not replace native transcript scrollback. Public documentation explains
enablement, safety, scope, precedence, recovery, and the semantic-retrieval
boundary.

### Future Semantic Retrieval

Document, but do not implement, semantic retrieval. Markdown remains the source
of truth and FTS/BM25 remains the mandatory fallback. No embeddings, vector
tables, model downloads, or remote retrieval calls are introduced here.

## Success Criteria

- Interaction commands obey existing containment, audit, confirmation,
  redaction, and no-side-effect-on-replay rules.
- Optional memory cannot become active without explicit user enablement.
- Compaction review and `/doctor` make important risk and recovery state
  visible without exposing secret-shaped content.
- Narrow and small-height snapshots cover every new visible state.
- Public docs make the optional-memory default and safety hierarchy explicit.

## Boundaries

Always:

- build on `thndrs-context` rather than reintroducing context policy in the
  TUI;
- keep memory content source-controlled by the user and append-only audit data
  content-free where required;
- preserve the precedence of direct user input and project instructions over
  memory.

Ask first:

- adding semantic/vector dependencies, a provider, or background memory work;
- changing memory defaults, permission policy, or session-record format.

Never:

- write memory from model inference without user confirmation;
- let memory grant permissions or override higher-priority instructions;
- silently replace lexical recall with semantic retrieval.

## Verification

```text
cargo fmt
cargo clippy --workspace --fix --allow-dirty --allow-staged
cargo clippy --workspace
cargo test context
cargo test memory
cargo test app
cargo test renderer
pnpm --dir docs build
```

The detailed implementation frontier is in [tasks.md](tasks.md).
