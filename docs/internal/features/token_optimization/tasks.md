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

**What to build:** Graduate terminal-control cleanup, progress-redraw cleanup,
blank-run normalization, and exact repeated-line collapse from shadow receipts
to independently configurable model projections after their preservation gates
pass.

**Status:** Complete

**Blocked by:**

- Ticket 4: Establish The Replay And Benchmark Framework
- Ticket 5: Inspect And Export Context Accounting
- Ticket 6: Add Explicit Lifecycle, Protection, And Verification Relations

**Acceptance criteria:**

- [x] Each reducer is independently named, versioned, inspectable, and
      explicitly configurable; there is no bundled behavior preset.
- [x] Reducers operate on bounded projections and never mutate durable evidence.
- [x] Exact repetitions include a count and preserve line order around the
      collapsed run.
- [x] ANSI/progress cleanup retains semantic text, status, and meaningful value
      changes.
- [x] Shadow and applied receipts use the same measurement method.
- [x] A reducer that fails a preservation invariant leaves the baseline
      projection active and emits a diagnostic.
- [x] The model dashboard aggregates routine omissions without placeholder
      spam.

**Verification:**

- [x] Focused reducer unit/property tests and adversarial fixtures.
- [x] Baseline/candidate evaluator reports.
- [x] Divan reducer benchmarks.
- [x] Provider-request structure tests.
- [x] `cargo test --workspace`

### Ticket 8: Deduplicate Only State-Identical Evidence

**What to build:** Suppress exact duplicate and superseded read/search/result
projections only when a tool-specific state fingerprint proves equivalence,
while preserving relations, causally important placeholders, and recovery.

**Blocked by:**

- Ticket 6: Add Explicit Lifecycle, Protection, And Verification Relations
- Ticket 7: Apply Lossless Terminal And Repetition Reduction

**Acceptance criteria:**

- [x] File reads use path, range, and content hash identity.
- [x] Searches and stateful commands include the relevant repository,
      environment, or freshness fingerprint defined by their tool adapter.
- [x] Same command/arguments before and after a relevant state change are never
      treated as duplicates.
- [x] Duplicate and superseded items link to their canonical/newer item and
      remain recoverable.
- [x] A short individual placeholder is used only when removing the projection
      would break causal understanding; routine relations stay in the dashboard.
- [x] Protected evidence is not silently deduplicated away.
- [x] Every applied decision has a receipt and appears in context inspection
      and export.

**Verification:**

- [x] Tool-specific fingerprint tests, including false-positive adversarial cases.
- [x] Frozen repeated-read/search/command evaluator fixtures.
- [x] Provider-valid sequence tests.
- [x] `cargo test --workspace`

### Ticket 9: Reduce Command Results Without Hiding Failure Evidence

**What to build:** Add narrow command-family projections for shell, compiler,
test, and MCP output that preserve operational evidence and always retain a
bounded recovery path.

**Blocked by:**

- Ticket 7: Apply Lossless Terminal And Repetition Reduction
- Ticket 8: Deduplicate Only State-Identical Evidence

**Acceptance criteria:**

- [ ] Universal command projection retains command identity, working context,
      status, exit information, duration, warnings/errors, paths/locations,
      failed test names, final summary, truncation, and recovery.
- [ ] Structured compiler/test formats are parsed only when already available
      from the command path; the reducer does not silently rewrite user command
      arguments.
- [ ] Successful output can become a bounded receipt without implying that it
      verifies a write or releases protection.
- [ ] Failed-tool large inputs can leave the active projection only after their
      failure, bounded artifact, and audit metadata are preserved.
- [ ] MCP results use conservative generic reduction unless a tool-specific
      contract supplies stronger invariants.
- [ ] Middle-error, near-duplicate diagnostic, multi-directory filename, and
      repeated-stack-frame fixtures pass.

**Verification:**

- Frozen command-family evaluator fixtures.
- Focused parser/reducer and fallback tests.
- Artifact recovery and provider-request tests.
- Divan command-projection benchmarks.
- `cargo test --workspace`

## Milestone 3: Add Review-Gated Semantic Compression

**Exit criterion:** Failed or rejected summaries leave context unchanged;
approved range summaries preserve protected facts and provenance, remain
recoverable, and report request/cache effects where providers expose them.

### Ticket 10: Compress Closed Ranges With Review And Provenance

**What to build:** Replace the current whole-active-transcript compaction shape
with provider-neutral, addressable range summaries that preserve source
relationships, protected facts, review, and recovery without adding inferred
task states.

**Blocked by:**

- Ticket 6: Add Explicit Lifecycle, Protection, And Verification Relations
- Ticket 9: Reduce Command Results Without Hiding Failure Evidence

**Acceptance criteria:**

- [ ] A compression request identifies a contiguous context range, focus, source
      ids/hashes, protected facts, and recovery handles.
- [ ] The configured model returns a versioned typed summary containing the
      required objective/findings/decisions/paths/failures/verification/blockers
      fields that apply to the range.
- [ ] Summary validation fails closed when protected facts or source metadata
      are missing.
- [ ] User review can approve or reject without losing the original active
      projection; failure/rejection is atomic.
- [ ] Approved summaries append lifecycle/session records and replace only their
      covered request projection.
- [ ] Higher summaries retain references to source summaries and original
      context ids rather than repeatedly rewriting untraceable prose.
- [ ] Compression remains manual or review-gated and exposes no optimization
      preset.
- [ ] Receipts show local before/after estimates plus provider/cache components
      when the adapter reports them.
- [ ] Provider-native context editing is used only behind the same decision and
      audit contract, with explicit capability diagnostics.

**Verification:**

- Pure request/summary validation and summary-relationship tests.
- Failed, rejected, approved, overlapping, protected-fact, and resume cases.
- Provider capability and applied-edit metadata fixtures.
- Frozen long-session evaluator reports.
- `cargo test --workspace`
- `pnpm --dir docs build`

## Frontier

Tickets that can start immediately:

- Ticket 1: Separate Evidence, Display, And Model Projections
