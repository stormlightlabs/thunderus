---
title: Tickets - Token Optimization And Context Observability
status: In Progress
captured: 2026-07-16
---

## Milestone 1: Observe Without Changing Requests

**Exit criterion:** For any request, thndrs can explain what was considered,
what was sent, what the provider reported, what shadow reducers proposed, and
what bounded evidence is recoverable. Existing provider-request fixtures remain
behavior-equivalent.

### Ticket 1: Separate Evidence, Display, And Model Projections

Expanded the provider-neutral tool result contract so one tool
execution can carry bounded durable-evidence metadata, a user-facing display,
and a model-facing projection while every current provider, ACP, CLI, TUI, MCP,
and session path continues to send and display the same content as before.

### Ticket 2: Account For Every Final Provider Request

Recorded exact serialized bytes, conservative estimated tokens,
and provider-reported usage components for each final provider request through
one provider-neutral accounting model with explicit provenance and unknown
values.

### Ticket 3: Preserve Bounded Redacted Artifacts And Recovery

Stores bounded redacted tool evidence locally, refer to it by
stable handles from tool results and context items, and recover it through the
existing context-control boundary without making raw output durable by default.

### Ticket 4: Establish The Replay And Benchmark Framework

Adds frozen context fixtures, a typed baseline/candidate
evaluator, and Divan benchmarks so maintainers can measure projection size,
preservation, recovery, and runtime before reducers change production requests.

### Ticket 5: Inspect And Export Context Accounting

Completed the observability-first user slice: extend context
inspection, add request token detail, render the quiet workbench indicator, and
export the selected request through versioned JSON or Markdown from one typed
model.

## Milestone 2: Apply Proven Deterministic Reductions

**Exit criterion:** Individually enabled deterministic reducers reduce frozen
fixtures without losing protected facts, diagnostics, provider validity,
recovery, or workspace behavior. No behavioral preset is introduced.

### Ticket 6: Add Explicit Lifecycle, Protection, And Verification Relations

Gives context items auditable lifecycle and protection state,
then let the agent propose and the user approve/reject/release verification
links without inferring task completion or command meaning.

### Ticket 7: Apply Lossless Terminal And Repetition Reduction

Graduated terminal-control cleanup, progress-redraw cleanup, blank-run
normalization, and exact repeated-line collapse from shadow receipts to
independently configurable model projections after their preservation gates
pass.

### Ticket 8: Deduplicate Only State-Identical Evidence

Suppressed exact duplicate and superseded read/search/result
projections only when a tool-specific state fingerprint proves equivalence,
while preserving relations, causally important placeholders, and recovery.

### Ticket 9: Reduce Command Results Without Hiding Failure Evidence

Added narrow command-family projections for shell, compiler,
test, and MCP output that preserve operational evidence and always retain a
bounded recovery path.

## Milestone 3: Add Review-Gated Semantic Compression

Failed or rejected summaries leave context unchanged;
approved range summaries preserve protected facts and provenance, remain
recoverable, and report request/cache effects where providers expose them.

### Ticket 10: Compress Closed Ranges With Review And Provenance

Replaced the current whole-active-transcript compaction shape
with provider-neutral, addressable range summaries that preserve source
relationships, protected facts, review, and recovery without adding inferred
task states.
