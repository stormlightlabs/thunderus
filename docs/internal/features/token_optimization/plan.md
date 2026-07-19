---
title: Token Optimization And Context Observability
status: Ready
captured: 2026-07-16
---

## Objective

Build an inspectable, user-controlled context optimization system for a
tool-using coding agent. The system separates durable local evidence from the
user display and the model-facing projection, explains every context decision,
and reduces avoidable input only after measurement proves that the reduction
preserves required evidence and task behavior.

The first shippable milestone is behavior-preserving context observability.
The next milestone adds deterministic reduction and review-gated semantic
compression; explicit lifecycle management now ships as its auditable
foundation. The product does not offer behavioral presets that trade control
for an opaque `quality`, `balanced`, or `economy` mode.

## Users And Use Cases

### Coding-agent users

- A user can see what context was considered and sent for a request, why each
  item was included or omitted, and what can be recovered.
- A user can distinguish exact bytes, estimated token counts, and
  provider-reported usage and cache components.
- A user can inspect and export the model-visible projection without exporting
  raw archived evidence or secrets.
- A user can review verification links and lossy compression before those
  actions release protected evidence or replace model-visible history.
- A user can explicitly control mechanisms and thresholds without selecting an
  opaque optimization persona.

### Agent-library authors

- An application can use pure provider-neutral evidence, projection,
  reduction, lifecycle, token-provenance, and selection contracts from
  `thndrs-agent`.
- Applications remain responsible for artifact persistence, redaction,
  provider normalization, provider capabilities, command-specific reducers,
  session records, and UI.

### Maintainers

- A maintainer can replay frozen context fixtures and compare baseline and
  optimized projections for size, preservation, recovery, and runtime.
- A maintainer can add a reducer only after its invariants and adversarial
  fixtures pass.

## Success Criteria

### Evidence And Projection

- Tool execution produces separate typed values for bounded redacted durable
  evidence, user-facing display, and model-facing projection.
- Raw tool output is ephemeral by default. Complete unredacted local retention
  requires explicit opt-in and is never included in an export.
- Model projection can be changed without rewriting append-only session truth.
- Every lossy projection has a replacement context id and a recoverable bounded
  redacted artifact.
- Provider-required tool-call/result structure remains valid after projection.

### Context Accounting

- The final pre-send boundary records every candidate context item exactly once
  with a stable state and reason code.
- Request accounting records exact serialized bytes and conservative token
  estimates with their estimator version.
- Provider adapters retain available raw usage components and normalize a
  documented inclusive input total without double-counting cache components.
- Missing usage remains absent rather than becoming zero.
- The system does not add model-aware tokenizer dependencies or provider/model
  encoding tables.
- Savings comparisons use the same measurement method on both sides and label
  heuristic, derived, and provider-reported values accurately.

### User Control And Observability

- Normal work shows only a compact context indicator and distinguishes
  estimated values from provider-reported values.
- `/context` exposes item state, reason, replacement, protection, verification
  link, and recovery.
- `/tokens` exposes request accounting, measurement provenance, raw and
  normalized provider components, shadow/applied receipts, and estimate error.
- A bounded metadata dashboard informs the model about important omissions and
  recovery without injecting the complete user report.
- Individually pruned items receive placeholders only when chronology or
  causality would otherwise be broken; routine omissions are aggregated.
- Context exports support versioned JSON and Markdown rendered from one typed
  model.
- Default exports include the rendered model projection and accounting metadata
  but exclude archived artifact bodies, secrets, and unselected source content.
- An explicit export option may add bounded redacted artifact bodies. No export
  option emits unredacted artifacts.

### Reduction And Lifecycle

- Milestone 1 records shadow receipts without changing model-visible behavior.
- Lossless deterministic reducers become automatic only after preservation and
  adversarial fixtures pass.
- Exact duplicate suppression uses state-aware identity rather than tool name
  and arguments alone.
- Protection is released only by explicit, auditable state changes. The system
  does not infer `unresolved`, `task_closed`, or verification from prose or
  command names.
- Write and failure evidence stays protected until the user approves an
  explicit verification/release link or an approved summary preserves the
  relevant failure and resolution.
- Semantic compression is manual or review-gated. It operates on closed ranges,
  records provenance and recovery handles, and never silently removes user
  constraints, pending permissions, safety state, unverified writes, or failed
  operations.

### Benchmarking

- Frozen fixtures cover repeated and overlapping reads, noisy output, passing
  and failing test output, errors hidden in the middle, state-changing repeated
  commands, failed writes with large inputs, protected evidence, compression,
  cache-sensitive checkpoints, and artifact recovery.
- A repository-owned fixture evaluator emits versioned JSON and Markdown
  reports for baseline and candidate policies.
- Reports include exact bytes, estimated tokens, reduction receipts, preserved
  required facts, recovery outcomes, and execution time. Provider usage is
  included only for explicitly recorded/replayed provider fixtures.
- Divan benchmarks pure selection, reduction, projection, and export paths on
  stable Rust with byte/item counters.
- No reducer ships merely because it is faster or smaller. Normal tests remain
  the correctness gate; benchmark reports are evidence, not assertions about
  task success by themselves.

## Settled Decisions

- Durable evidence is bounded and redacted by default; raw output is
  ephemeral, and full unredacted retention is explicit opt-in.
- Observability ships before optimization changes requests.
- Safe deterministic reductions and lossy semantic compression have different
  automation policies.
- There are no `quality`, `balanced`, or `economy` presets.
- Ordinary omissions are aggregated; only causally important omissions receive
  individual model placeholders.
- The library owns pure contracts/policy; the application owns effects and
  provider adapters.
- Token estimates remain heuristics; the system does not integrate local model
  tokenizers.
- Context observability remains local by default and has a thndrs-owned JSON
  and Markdown export contract rather than an OpenTelemetry dependency.
- Protection and verification are explicit state transitions, not semantic
  guesses.
- Verification links begin as agent-proposed, user-reviewed context actions;
  successful command names do not release protection automatically.
- Provider-native context editing is an optional adapter capability, never the
  semantic source of truth.
- Cache reuse is measured beside reduction and does not justify behavioral
  presets or hidden provider-specific policy.

## Current State

### Provider-neutral library

`thndrs-agent` already has a pure context-control layer with model limits,
estimated token budgets, ledger items, visibility states, pins, selection, and
compaction policy. Selection currently retains recent transcript entries under
an 80% target and may compact above a 92% threshold.

`ToolOutput` currently has one `output: Vec<String>` value described as safe
for both display and provider feedback. That coupling prevents independent
durable evidence, display, and model projections.

The context ledger has a `ToolArchive` kind, and selection now carries a
provider-neutral lifecycle/protection contract beside request visibility. The
contract records explicit duplicate, supersession, summary, archive, recovery,
and verification relations without treating omission from one request as a
lifecycle transition. Conservative protection reasons cover user context,
constraints, safety state, permissions, pins, recovery metadata, failures, and
unverified writes.

### Application

`thndrs` assembles prompt bundles, projects settled transcript entries, and
joins tool output directly into provider messages. During an active tool loop,
the display output is likewise joined and sent back to the provider.

Append-only session JSONL records context-ledger metadata, aggregate provider
input/output usage, tool starts/finishes, compaction audits, shell execution,
context actions, and content-free lifecycle transitions. Verification review,
explicit release, and recovery write post-transition lifecycle metadata before
the in-memory state changes; resume replays the latest lifecycle record.
Tool-finished output remains redacted and capped, and lifecycle records carry
metadata and relations rather than raw evidence.

The TUI already has a context surface, pin/drop/recovery actions, context health
information, and manual/automatic compaction review. The feature extends these
surfaces instead of adding a persistent dashboard or a second state system.

### Known limitations to preserve explicitly

- `unresolved work` is a conservative compaction-risk heuristic, not a product
  lifecycle state.
- There is no `task closed` state.
- Successful test/lint/shell commands are not semantically classified as
  verification.
- Provider/model tokenizers are intentionally absent.

## Architecture

### Three truths

The feature maintains three distinct views:

1. **Session truth:** append-only records of what happened and bounded redacted
   durable evidence.
2. **Context truth:** addressable candidates, lifecycle/protection state,
   replacements, summaries, and recovery metadata.
3. **Request projection:** the provider-valid messages and tools chosen for one
   request.

No optimization mutates historical session truth. Context state changes append
new records, and each request receives an immutable final ledger snapshot.

### Evidence boundary

The provider-neutral contracts express:

- evidence identity, kind, exact byte size, content hash, and artifact handle;
- bounded display projection;
- bounded model projection;
- reduction method/version, before/after measurements, lossiness, and
  replacement/recovery metadata.

The application creates and stores artifacts, applies redaction and caps,
renders display output, and lowers model projections to provider messages. Raw
unredacted output is not a provider-neutral library value intended for durable
storage.

### Token values

Token and size values carry provenance rather than appearing as bare counts:

- exact bytes from a named serialization boundary;
- estimated tokens from the versioned conservative heuristic;
- provider-reported components from the adapter response;
- derived inclusive totals with the adapter's normalization rule.

The existing byte heuristic remains suitable for budget safety, but naming and
rendering must consistently mark it estimated. A difference between heuristic
and provider values is estimate error, not exact token savings.

### Context lifecycle

Lifecycle and prompt inclusion are separate concepts. A context item can be
active/protected/duplicate/superseded/summarized/archived while a request
selection separately decides whether its projection is visible, pinned,
summary-only, dropped, or blocked.

Relations are explicit:

- `duplicate_of` identifies the canonical identical evidence;
- `superseded_by` identifies a newer source version;
- `summarized_by` identifies an approved summary;
- `verified_by` identifies a user-approved verification result;
- `recovery_handle` identifies bounded redacted durable evidence.

The initial release does not infer lifecycle completion from assistant prose,
command names, or elapsed turns. Recent-turn retention is a fallback safety
input only.

### Protection

Always or conditionally protected material includes the current user turn,
explicit user-constraint items, safety instructions, pending permissions,
explicit pins, recovery metadata, failed operations, and unverified write/edit
evidence.

The user can approve a proposed verification relation or explicitly release an
item. A later milestone may automate a relation only if a structured tool
contract supplies an unambiguous link; command-name heuristics are not an
acceptable release signal.

### Deterministic reduction

Reducers are pure where practical and return a projection plus receipt. Initial
families are:

- terminal control/progress normalization;
- exact repeated-line collapse with counts;
- bounded command output preserving status, errors, diagnostics, paths,
  locations, failure names, and final summaries;
- state-aware exact duplicate results;
- exact unchanged file-range suppression;
- failed-tool input removal after required audit/recovery metadata is present.

Command-aware reducers live in the application because they interpret
application tools and process results. Generic reducer contracts and
preservation validation live in the library.

### Semantic compression

Semantic compression remains an explicit/reviewed context action. Range-level
compression ships before message-level compression. A summary records covered
context ids and hashes, schema version, configured model, protected facts,
review decision, and recovery handles.

Higher summaries reference source summaries and original source ids. The
system does not repeatedly rewrite the text of a prior summary without source
provenance. Rejected or failed compression leaves the active projection
unchanged.

### Provider capabilities and caching

The provider-neutral policy decides what should be omitted or summarized.
Provider adapters may use native context-editing capabilities only when their
behavior implements that decision and returns enough applied-edit metadata to
audit it. Capability differences are visible diagnostics, never silent policy
changes.

Reduction receipts and request accounting preserve cache-read and cache-write
components when providers report them. Checkpoint transformations should be
large and infrequent enough to justify changing exact cached prefixes, but no
universal percentage threshold is hard-coded without benchmark evidence.

## Observability And Export

### Local inspection

`/context` remains the item and lifecycle view. It supports selection detail,
protection state, relations, recovery, proposed verification review, and
export.

`/tokens` is the request accounting view. It shows local bytes/estimates,
provider-reported components, normalization, shadow/applied receipts, and
estimate error. It must name the measurement source beside the value.

The compact workbench indicator shows only the current estimate and the latest
provider result/cache information that fits without becoming a dashboard.

### Export

One versioned typed export model renders to JSON and Markdown. The formats must
contain the same facts and use deterministic ordering.

Default export content:

- schema and policy versions;
- request, turn, provider, and model identifiers;
- local budget/estimate and provider usage accounting;
- item metadata, state, reasons, relations, protection, and recovery
  availability;
- reduction receipts and diagnostics;
- the exact bounded model projection used for the selected request.

Default exclusions:

- raw archived artifact bodies;
- unselected source bodies;
- raw provider payloads;
- secrets and credentials;
- unredacted tool arguments/results.

Export performs redaction and size limiting again. An explicit include-artifacts
option adds only bounded redacted artifact bodies. JSON is the versioned machine
contract; Markdown is a first-class human rendering of the same model.

## Configuration And Commands

Exact spelling should follow existing configuration and slash-command parsing
conventions. The required product controls are:

- enable/disable shadow reduction measurement;
- enable/disable individual deterministic reducers after they graduate;
- configure artifact byte caps and retention/expiry;
- keep semantic compression `off`, `manual`, or review-gated `auto` using the
  existing compaction policy vocabulary rather than a new preset system;
- inspect a request or current context;
- propose, approve, reject, or release a verification relation;
- recover a bounded artifact;
- export context as JSON or Markdown with optional bounded redacted artifacts.

Configuration rejects unknown keys and invalid combinations. No setting enables
unredacted export.

## Project Structure

The likely change areas are:

- `crates/thndrs-agent/src/contracts.rs` for provider-neutral output and usage
  contracts;
- `crates/thndrs-agent/src/context/` for measurement, lifecycle, reduction
  receipts, protection, selection, and projection validation;
- `crates/thndrs/src/core/tools/` and process/MCP adapters for application-owned
  evidence construction and reducers;
- `crates/thndrs/src/core/prompt/` and the active agent loop for the final
  provider-request projection;
- `crates/thndrs/src/core/providers/` for usage normalization and optional
  native-context capabilities;
- `crates/thndrs/src/core/session/` for append-only audit records;
- `crates/thndrs/src/cli/app/context.rs`, command routing, and renderer views for
  local inspection and review;
- a dedicated application export module for the typed JSON/Markdown contract;
- `crates/thndrs-agent/benches/` plus frozen fixtures for Divan policy/reducer
  benchmarks;
- a small workspace benchmark/evaluation entry point that emits the versioned
  domain report without making Divan output the product data contract.

File placement may follow ongoing application-module extraction, but the
library/application ownership boundary is fixed.

## Testing Plan

### Highest stable boundaries

- Pure policy and contract behavior is tested through `thndrs-agent` inputs and
  outputs.
- Projection correctness is tested at the final provider-neutral message/tool
  request immediately before provider-specific serialization.
- Provider usage normalization is tested through provider response fixtures.
- Session durability is tested by JSONL round trips, resume, append-only state
  transitions, and content/redaction assertions.
- User workflows are tested through existing application update/command and
  renderer boundaries rather than a new UI state machine.
- Export is tested from the typed export model to both JSON and Markdown,
  including deterministic ordering and secret fixtures.

### Required cases

- Existing tool outputs produce byte-for-byte equivalent model projections in
  Milestone 1.
- Streaming and retry usage does not double-count components.
- A provider that omits usage leaves fields unknown.
- Provider fixtures with inclusive or decomposed cache fields normalize once.
- Every candidate has one final request state and one stable reason code.
- Lossy receipts always have replacement and recovery metadata.
- Duplicate commands across a workspace change are not deduplicated.
- Repeated file reads with identical hashes are eligible; changed content is
  not.
- Repeated lines with one differing diagnostic retain the difference.
- Errors in the middle of oversized output survive projection.
- Tool call/result ordering remains provider-valid.
- Failed/rejected compression and verification review are atomic.
- Protection does not expire through recency alone.
- JSON and Markdown exports agree on all typed fields and contain no raw secret
  fixture or unselected content.
- Artifact expiry leaves audit metadata and a clear unavailable-recovery
  diagnostic.

### Commands

During implementation run the narrowest relevant crate/test target, then:

```sh
cargo fmt
cargo clippy --workspace --fix --allow-dirty --allow-staged
cargo clippy --workspace
cargo test --workspace
cargo bench -p thndrs-agent --bench context_projection
pnpm --dir docs build
```

The repository-owned evaluator must also have a deterministic command that can
write JSON and Markdown reports to an explicitly supplied path. Its final
binary/name is chosen in the implementation ticket without changing the export
or report contract.

## Milestones

### Milestone 1: Observe Without Changing Requests

Establish evidence/display/model projection contracts, token provenance,
provider usage normalization, final request ledgers, bounded redacted artifacts,
shadow receipts, `/context`, `/tokens`, JSON/Markdown export, replay fixtures,
and benchmark baselines.

Exit when any request can answer what was considered, what was sent, what the
provider reported, what shadow reducers proposed, and what evidence is
recoverable—while provider request fixtures remain behavior-equivalent.

### Milestone 2: Apply Proven Deterministic Reductions

Graduate reducers individually from shadow to applied behavior after focused
and adversarial preservation tests. Add state-aware duplicate and supersession
relations, protection, verification review, placeholders/dashboard aggregation,
and artifact recovery.

Exit when applied reducers reduce frozen fixtures without losing protected
facts, diagnostics, provider validity, recovery, or workspace behavior.

### Milestone 3: Add Review-Gated Semantic Compression

Add typed range summaries, source provenance, protected-fact validation,
review/rejection, summary relationships, recovery, and checkpoint/cache
accounting. Keep message-level compression deferred until range compression is
demonstrably insufficient.

Exit when failed/rejected summaries are atomic, approved summaries preserve
protected facts, sources remain recoverable, and benchmark reports include
cache effects where provider fixtures expose them.

### Milestone 4: Improve Source And Tool Context On Evidence

Use benchmark findings to decide whether source outlines, overlapping-range
suppression, multi-file packs, or lazy tool discovery materially improve the
measured workload. Each mechanism remains explicit and independently
inspectable rather than becoming a preset.

Exit criteria are defined from Milestones 1–3 evidence before tickets for this
milestone are activated.

## Deferred Milestones

- Message-level semantic compression remains later than range compression
  because it has a larger audit and causality surface.
- Provider-native context editing remains an adapter optimization after the
  provider-neutral policy and applied-edit audit contract are stable.
- Source outlines and lazy tool catalogs remain planned product directions but
  follow evidence/projection correctness and lifecycle pruning.
- Automated verification linking requires a structured, unambiguous relation
  from a future tool contract. It is not implemented using shell-command name
  guesses.

## Boundaries

### Always

- Preserve append-only session truth and existing provider behavior while
  building Milestone 1.
- Keep library policy pure and application effects in adapters.
- Redact and cap every persistence/export boundary.
- Label every count by provenance and preserve unknown values.
- Add adversarial preservation fixtures before enabling a reducer.
- Keep commands and UI within the existing context/workbench architecture.

### Ask first

- Add production dependencies or change a public `thndrs-agent` contract after
  the planned expand/migrate/contract sequence is exhausted.
- Enable unredacted artifact retention by default.
- Change compaction defaults, review semantics, or automatic thresholds.
- Add remote telemetry/export destinations or content capture.
- Adopt provider behavior that cannot satisfy the provider-neutral audit
  contract.

### Never

- Rewrite or delete historical session records as an optimization.
- Export unredacted artifacts, secrets, raw provider payloads, or unselected
  source content.
- Treat missing token usage as zero or heuristic estimates as exact provider
  usage.
- Infer `task_closed`, `unresolved`, or verification from free-form prose or
  command names.
- Introduce behavioral optimization presets.
- Allow a lossy reduction without replacement, provenance, and bounded
  recovery.

## Risks And Tradeoffs

- Conservative protection retains more context and requires user review, but
  avoids silently discarding unverified evidence.
- Separate evidence and projections broaden several internal contracts. Use an
  expand/migrate/contract sequence so provider, ACP, TUI, and session paths stay
  green.
- Durable bounded artifacts increase local storage. Retention, expiry, and an
  unavailable-recovery state must be explicit.
- Redaction can miss unknown secret shapes. Default caps, repeated export
  redaction, and no unredacted export reduce but do not eliminate this risk.
- Prompt changes can reduce cache reuse. Receipts must show cache components
  and checkpoints must be benchmarked rather than assumed beneficial.
- Provider usage semantics vary and evolve. Normalization rules stay in
  adapters with fixtures and raw components preserved.
- Benchmark fixtures can overfit reducers. Include adversarial cases and keep
  normal behavior tests as the release gate.
- Divan measures runtime and throughput, not agent correctness. The typed
  fixture evaluator and preservation assertions carry the domain quality
  contract.

## Research

- [Token optimization for tool-using agents](/notebook/token-optimization/)
- [Context observability for tool-using agents](/notebook/context-observability/)
- [Context control and memory system design](/notebook/context-control/)
- Source discussions: `.sandbox/chat_000.md` and `.sandbox/chat_001.md`.
- External references include TokenWarden, OpenCode DCP, Anthropic context
  editing/tool-context/cache diagnostics, OpenTelemetry GenAI conventions, and
  Divan. External benchmark claims are treated as motivation, not expected
  product results.
