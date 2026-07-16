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

**What to build:** Expand the provider-neutral tool result contract so one tool
execution can carry bounded durable-evidence metadata, a user-facing display,
and a model-facing projection while every current provider, ACP, CLI, TUI, MCP,
and session path continues to send and display the same content as before.

Use expand/migrate/contract: introduce the new form beside compatibility
construction, migrate each application path, then remove the coupled output
form only after all callers are green.

**Blocked by:** None - can start immediately

**Status:** Complete

**Acceptance criteria:**

- [x] Provider-neutral types distinguish evidence metadata, display projection,
      and model projection without importing provider or filesystem types.
- [x] Existing tool constructors have a simple compatibility path that creates
      equivalent display and model projections during migration.
- [x] Prompt-tail lowering and active tool-loop feedback consume the model
      projection, while UI/ACP surfaces consume the display projection.
- [x] Existing provider-request and renderer fixtures remain behavior-equivalent.
- [x] Raw unredacted output is not added to a durable public contract.
- [x] Module/API documentation explains ownership and redaction expectations.

**Verification:**

- Focused `thndrs-agent` contract tests.
- Focused tool, agent-loop, prompt, ACP, and session tests.
- `cargo test --workspace`

### Ticket 2: Account For Every Final Provider Request

**What to build:** Record exact serialized bytes, conservative estimated tokens,
and provider-reported usage components for each final provider request through
one provider-neutral accounting model with explicit provenance and unknown
values.

**Blocked by:** Ticket 1: Separate Evidence, Display, And Model Projections

**Status:** Complete

**Acceptance criteria:**

- [x] Byte and token values carry measurement provenance and estimator or
      normalization version.
- [x] The final pre-send boundary snapshots all context candidates exactly once
      with a state and stable reason code.
- [x] Provider adapters retain available input, output, cache-read,
      cache-creation, and reasoning components and derive inclusive input using
      fixture-tested provider rules.
- [x] Providers that omit a component leave it unknown; zero remains a measured
      value.
- [x] Streaming updates and retries cannot double-count a request.
- [x] Session records correlate request accounting with the turn and context
      ledger without persisting raw provider payloads.
- [x] Existing aggregate session summaries remain correct during migration.

**Verification:**

- Provider usage fixtures for every supported first-party provider path.
- Session JSONL round-trip/resume and redaction tests.
- Focused final-request projection tests.
- `cargo test --workspace`

### Ticket 3: Preserve Bounded Redacted Artifacts And Recovery

**What to build:** Store bounded redacted tool evidence locally, refer to it by
stable handles from tool results and context items, and recover it through the
existing context-control boundary without making raw output durable by default.

**Blocked by:** Ticket 1: Separate Evidence, Display, And Model Projections

**Status:** Complete

**Acceptance criteria:**

- [x] Artifact creation applies redaction and byte caps before persistence.
- [x] Artifact metadata includes identity, kind, content hash, original/bounded
      byte counts, truncation, creation, and expiry/retention state.
- [x] Session JSONL stores artifact metadata and handles rather than raw
      unredacted bodies.
- [x] Recovery returns bounded redacted evidence and records the recovery
      action.
- [x] Missing or expired artifacts produce an explicit diagnostic while their
      audit metadata remains usable.
- [x] Full unredacted retention is disabled by default and cannot be reached
      through export.
- [x] Secret-shaped fixtures do not appear in stored records, recovered output,
      logs, or error messages.

**Verification:**

- Artifact store create/read/expire/corruption tests using temporary roots.
- Session resume and recovery-action tests.
- Redaction and size-bound adversarial tests.
- `cargo test --workspace`

### Ticket 4: Establish The Replay And Benchmark Framework

**What to build:** Add frozen context fixtures, a typed baseline/candidate
evaluator, and Divan benchmarks so maintainers can measure projection size,
preservation, recovery, and runtime before reducers change production requests.

**Blocked by:**

- Ticket 2: Account For Every Final Provider Request
- Ticket 3: Preserve Bounded Redacted Artifacts And Recovery

**Acceptance criteria:**

- [ ] The fixture schema is versioned, deterministic, provider-neutral where
      practical, and represents required facts separately from expected prose.
- [ ] Fixtures cover repeated/overlapping reads, passing/failing test output,
      noisy progress, a middle-position error, repeated commands across state
      changes, failed writes with large inputs, protected evidence, cache
      components, and recovery.
- [ ] The evaluator compares baseline and candidate projections and emits one
      typed report as deterministic JSON or Markdown.
- [ ] Reports include exact bytes, estimated tokens, receipts, required-fact
      preservation, recovery outcomes, and elapsed time; provider usage appears
      only when present in a recorded fixture.
- [ ] Divan benchmarks pure selection, projection, receipt generation, and
      export/evaluation with byte/item counters on stable Rust.
- [ ] Timing results do not decide correctness; failed invariants make the
      evaluator fail independently of Divan output.
- [ ] Focused benchmark commands and fixture-authoring guidance are documented.

**Verification:**

- Run the evaluator twice and compare deterministic JSON/Markdown output.
- `cargo bench -p thndrs-agent --bench context_projection`
- Focused evaluator/fixture schema tests.
- `cargo test --workspace`

### Ticket 5: Inspect And Export Context Accounting

**What to build:** Complete the observability-first user slice: extend context
inspection, add request token detail, render the quiet workbench indicator, and
export the selected request through versioned JSON or Markdown from one typed
model.

**Blocked by:**

- Ticket 2: Account For Every Final Provider Request
- Ticket 3: Preserve Bounded Redacted Artifacts And Recovery
- Ticket 4: Establish The Replay And Benchmark Framework

**Acceptance criteria:**

- [ ] `/context` exposes item state, reason, replacement, protection, and
      recovery availability without displaying raw archived bodies by default.
- [ ] `/tokens` exposes request estimates, provenance, provider components,
      normalized totals, shadow receipts, and estimate error.
- [ ] The normal workbench indicator remains compact and labels estimated and
      provider-reported values distinctly.
- [ ] One versioned export model renders deterministically to JSON and Markdown.
- [ ] Default export includes accounting metadata and the bounded rendered model
      projection but excludes artifact bodies and unselected content.
- [ ] An explicit option adds only bounded redacted artifact bodies; no option
      exports unredacted artifacts.
- [ ] Export redacts and caps again and fails safely on unwritable targets.
- [ ] Narrow-terminal, long-label, unknown-usage, and secret fixtures have
      deterministic behavior coverage.

**Verification:**

- Focused command/update and renderer tests/snapshots.
- JSON schema/round-trip and Markdown golden tests from the same typed fixture.
- Export secret and size-cap tests.
- `cargo test --workspace`
- `pnpm --dir docs build`

## Milestone 2: Apply Proven Deterministic Reductions

**Exit criterion:** Individually enabled deterministic reducers reduce frozen
fixtures without losing protected facts, diagnostics, provider validity,
recovery, or workspace behavior. No behavioral preset is introduced.

### Ticket 6: Add Explicit Lifecycle, Protection, And Verification Relations

**What to build:** Give context items auditable lifecycle and protection state,
then let the agent propose and the user approve/reject/release verification
links without inferring task completion or command meaning.

**Blocked by:** Ticket 5: Inspect And Export Context Accounting

**Acceptance criteria:**

- [ ] Lifecycle state is separate from request visibility and supports explicit
      duplicate, supersession, summary, verification, archive, and recovery
      relations.
- [ ] Current user context, explicit constraints, safety state, pending
      permissions, pins, recovery metadata, failures, and unverified write/edit
      evidence receive the specified conservative protection.
- [ ] A proposed verification relation names the protected evidence and the
      candidate result; it changes no protection until approved.
- [ ] Approval, rejection, release, and recovery append session records and are
      atomic across failure/resume.
- [ ] Recency, assistant prose, and command names never release protection.
- [ ] `/context` and exports explain lifecycle, protection, and verification
      relations.

**Verification:**

- Pure lifecycle transition-table tests.
- Application review/approval/rejection/resume tests.
- Adversarial tests for successful but unrelated commands.
- `cargo test --workspace`

### Ticket 7: Apply Lossless Terminal And Repetition Reduction

**What to build:** Graduate terminal-control cleanup, progress-redraw cleanup,
blank-run normalization, and exact repeated-line collapse from shadow receipts
to independently configurable model projections after their preservation gates
pass.

**Blocked by:**

- Ticket 4: Establish The Replay And Benchmark Framework
- Ticket 5: Inspect And Export Context Accounting
- Ticket 6: Add Explicit Lifecycle, Protection, And Verification Relations

**Acceptance criteria:**

- [ ] Each reducer is independently named, versioned, inspectable, and
      explicitly configurable; there is no bundled behavior preset.
- [ ] Reducers operate on bounded projections and never mutate durable evidence.
- [ ] Exact repetitions include a count and preserve line order around the
      collapsed run.
- [ ] ANSI/progress cleanup retains semantic text, status, and meaningful value
      changes.
- [ ] Shadow and applied receipts use the same measurement method.
- [ ] A reducer that fails a preservation invariant leaves the baseline
      projection active and emits a diagnostic.
- [ ] The model dashboard aggregates routine omissions without placeholder
      spam.

**Verification:**

- Focused reducer unit/property tests and adversarial fixtures.
- Baseline/candidate evaluator reports.
- Divan reducer benchmarks.
- Provider-request structure tests.
- `cargo test --workspace`

### Ticket 8: Deduplicate Only State-Identical Evidence

**What to build:** Suppress exact duplicate and superseded read/search/result
projections only when a tool-specific state fingerprint proves equivalence,
while preserving relations, causally important placeholders, and recovery.

**Blocked by:**

- Ticket 6: Add Explicit Lifecycle, Protection, And Verification Relations
- Ticket 7: Apply Lossless Terminal And Repetition Reduction

**Acceptance criteria:**

- [ ] File reads use path, range, and content hash identity.
- [ ] Searches and stateful commands include the relevant repository,
      environment, or freshness fingerprint defined by their tool adapter.
- [ ] Same command/arguments before and after a relevant state change are never
      treated as duplicates.
- [ ] Duplicate and superseded items link to their canonical/newer item and
      remain recoverable.
- [ ] A short individual placeholder is used only when removing the projection
      would break causal understanding; routine relations stay in the dashboard.
- [ ] Protected evidence is not silently deduplicated away.
- [ ] Every applied decision has a receipt and appears in context inspection
      and export.

**Verification:**

- Tool-specific fingerprint tests, including false-positive adversarial cases.
- Frozen repeated-read/search/command evaluator fixtures.
- Provider-valid sequence tests.
- `cargo test --workspace`

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
