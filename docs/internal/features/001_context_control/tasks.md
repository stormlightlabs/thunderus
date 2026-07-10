---
title: Context Control And Memory Tasks
Status: Ready
Captured: 2026-07-03
---

## P1: Ledger Foundation

- [x] Add a context-control module with `ContextItemKind`.
- [x] Add `ContextVisibility`.
- [x] Add `ContextItem`.
- [x] Add `ModelContextLimits` with provider, model id, context window, max
      completion tokens, recommended completion tokens, source, and confidence.
- [x] Add provider-neutral model capability projection from live provider
      metadata when available.
- [x] Add advanced config overrides under
      `[model_limits."provider/model-id"]` with `context_window`,
      `max_completion_tokens`, and `recommended_completion_tokens`.
- [x] Apply model limit precedence: user override, then live metadata, then
      static provider metadata, then conservative fallback.
- [x] Add conservative static model capability fallbacks for providers without
      live context-window metadata.
- [x] Validate model limit overrides as positive integers.
- [x] Reject or diagnose model limit overrides where recommended completion
      tokens exceed max completion tokens or context window.
- [x] Derive available input budget from context window minus reserved
      completion budget and provider overhead.
- [x] Add context budget ratios: target selection at 80% and auto-compaction
      trigger at 92%.
- [x] Add `ContextBudget`.
- [x] Add `ContextLedger`.
- [x] Add `ContextDiagnostic`.
- [x] Add stable context item id generation for file-backed sources.
- [x] Add stable context item id generation for transcript/session sources.
- [x] Add a conservative token estimator.
- [x] Add formatting helpers for user-visible ledger summaries.
- [x] Add model-visible dashboard rendering that excludes full content.
- [x] Keep ledger types free of crossterm, renderer, and provider-specific
      types.
- [x] Test context item id stability for paths.
- [x] Test context item id stability for session ranges.
- [x] Test token-estimation behavior on ASCII, Unicode, and code blocks.
- [x] Test live model metadata context windows feed context budgets.
- [x] Test user model limit overrides take precedence over live metadata.
- [x] Test invalid model limit overrides produce diagnostics.
- [x] Test missing model metadata falls back to conservative limits with a
      diagnostic.
- [x] Test target and auto-compaction ratios are calculated from available
      input budget, not raw context window.

## P2: Scoped Project Instructions Extension

- [x] Reuse existing root `AGENTS.md` loading as the default project
      instruction source.
- [x] Discover nested `AGENTS.md` files below the workspace root.
- [x] Assign every nested `AGENTS.md` a subtree scope.
- [x] Load nested instruction content only when applicable or explicitly pinned.
- [x] Select closest applicable instruction sources for mentioned or pinned
      paths.
- [x] Keep broader instruction sources visible as metadata when overridden.
- [x] Reload instruction metadata at turn boundaries.
- [x] Detect changed instruction file hashes between turns.
- [x] Add diagnostics for unreadable instruction files.
- [x] Add diagnostics for oversized or truncated instruction files.
- [x] Preserve existing root-only behavior when no nested files exist.
- [x] Test root `AGENTS.md` selection.
- [x] Test nested `AGENTS.md` selection by mentioned path.
- [x] Test closest instruction precedence.
- [x] Test changed instruction hash diagnostics.
- [x] Update project-context docs with scoped `AGENTS.md` behavior.

## P3: Memory Source Files

- [x] Add `src/memory.rs`.
- [x] Implement the user and project memory roots selected in P0.
- [x] Implement memory item metadata with id, title, kind, scope, paths, tags,
      created, updated, and source.
- [x] Read `core.md` from user memory.
- [x] Read `core.md` from project memory.
- [x] Discover archival notes under `notes/*.md`.
- [x] Validate memory frontmatter.
- [x] Surface diagnostics for malformed memory files.
- [x] Keep memory body loading size-capped.
- [x] Add explicit scoped memory write helper for `/remember`.
- [x] Add memory deletion helper for `/memory forget`.
- [x] Require confirmation before `/memory forget` deletes a memory file.
- [x] Make `/memory forget` append a content-free `memory_delete` audit record
      with memory id, path, scope, timestamp, and content hash when available.
- [x] Ensure `/memory forget` never writes a tombstone file containing
      forgotten content.
- [x] Ensure `/memory forget` does not delete unrelated memory, project files,
      or session history.
- [x] Add path-scoped memory selection for project notes.
- [x] Persist session-scoped memory in the session log so it survives session
      resume.
- [x] Keep session-scoped memory active after `/compact`, `/clear`, and
      `/clear-context`.
- [x] Add secret-shaped content warning before memory writes.
- [x] Keep memory files ordinary inspectable Markdown.
- [x] Test memory discovery in user root.
- [x] Test memory discovery in project root.
- [x] Test malformed memory diagnostics.
- [x] Test explicit scoped `/remember` writes valid Markdown/frontmatter.
- [x] Test `/remember` without a scope is rejected.
- [x] Test session-scoped memory survives compaction and resume.
- [x] Test `/clear` and `/clear-context` do not remove session-scoped memory.
- [x] Test `/memory forget` deletes the memory file after confirmation.
- [x] Test `/memory forget` appends only content-free delete audit metadata.
- [x] Test `/memory forget` fails safely when the target memory cannot be
      identified.

## P4: SQLite Metadata And FTS Index

- [x] Add SQLite memory index schema versioning.
- [x] Add a memory metadata table keyed by memory id and file path.
- [x] Add an FTS5 table for title, headings, tags, paths, and body text.
- [x] Use BM25 ranking for lexical memory retrieval.
- [x] Store content hash, mtime, and byte size for stale-index detection.
- [x] Store derived SQLite indexes under `~/.thndrs/cache/memory/`.
- [x] Use a workspace-root hash for project memory index cache names.
- [x] Rebuild the SQLite index from Markdown when the cache is missing, stale,
      or corrupt.
- [x] Add metadata-filtered FTS5 search over memory notes.
- [x] Return FTS results with match reason, snippet, score, source, scope, and
      recovery handle.
- [x] Test SQLite index creation and schema versioning.
- [x] Test stale SQLite index rebuild after memory file edits.
- [x] Test corrupt SQLite index rebuild.
- [x] Test FTS5/BM25 search finds exact command, path, package, and error text.
- [x] Test metadata-filtered FTS search by scope, tag, and path.
- [x] Test FTS retrieval result snippets and recovery handles.

## P5: Deferred Embedding Extension Contract

- [ ] Keep v1 memory retrieval code structured so an embedding provider can be
      added without changing memory Markdown files.
- [ ] Document the candidate embedding providers: local in-process
      `fastembed`, local-service Ollama, and remote/API OpenAI embeddings.
- [ ] Document sqlite-vec as the likely first local vector index candidate.
- [ ] Define the future cached vector metadata fields: memory id, provider,
      model, dimensions, content hash, vector hash, and updated time.
- [ ] Define the future mixed-model rule: one vector index rejects mixed
      provider/model/dimension rows unless an explicit rebuild or migration is
      requested.
- [ ] Define the future degradation rule: semantic recall is optional and must
      fall back to metadata plus FTS5/BM25 with visible diagnostics.
- [ ] Do not add sqlite-vec, embedding API calls, model downloads, or vector
      tables in v1.

## P6: Lexical Memory Recall

- [x] Add `/memory recall <query>` over metadata and FTS5/BM25 results.
- [x] Return memory retrieval results with match reason, snippet, score, source,
      scope, and recovery handle.
- [x] Include whether a result came from metadata or FTS5/BM25.
- [x] Include core memory before archival memory in recall projections.
- [x] Add recall result caps by count and total bytes.
- [x] Test lexical recall ordering and stable tie-breaking.
- [x] Test metadata-only matches.
- [x] Test recall caps.
- [x] Test recall returns empty results with a useful diagnostic.

## P7: Context Selection Policy

- [x] Build candidate ledger items from harness context.
- [x] Build candidate ledger items from project instructions.
- [x] Build candidate ledger items from user memory.
- [x] Build candidate ledger items from project memory.
- [x] Build candidate ledger items from active pins.
- [x] Build candidate ledger items from compaction summaries.
- [x] Build candidate ledger items from recent transcript entries.
- [x] Build candidate ledger items from skill metadata and loaded skills.
- [x] Include current user turn outside ordinary budget eviction.
- [x] Include active pins before ordinary recent transcript items.
- [x] Include applicable closest `AGENTS.md` before broader guidance.
- [x] Include the latest compaction summary when older turns are omitted.
- [x] Keep session-scoped memory eligible after compaction and resume.
- [x] Omit UI-only and live-only transcript entries.
- [x] Mark oversized items as blocked or summary-only instead of truncating
      silently.
- [x] Record a reason for every visible, omitted, archived, dropped, blocked,
      or summary-only item.
- [x] Test short, normal, and overloaded context budgets.
- [x] Test pins survive across turns.
- [x] Test drops remove pins from future turns.
- [x] Test explicit dropped-item rules persist until source change or
      `/drop --reset`.
- [x] Test `/clear-context` clears pins, recovered/opened items, and transient
      transcript carryover without deleting memory or compaction summaries.
- [x] Test recover reopens archived/omitted context by id.
- [x] Test compaction summary replaces older transcript entries in prompt
      projection.
- [x] Test secrets are not serialized into context metadata.

## P8: Prompt And Self-Knowledge

- [x] Change `PromptBundle` to accept a selected context projection.
- [x] Preserve existing prompt fragment ordering.
- [x] Render selected project instructions from ledger items.
- [x] Render selected memory items with source/scope labels.
- [x] Render selected compaction summaries with covered sequence metadata.
- [x] Render pinned file excerpts or handles without bypassing file-tool
      containment.
- [x] Replace fixed transcript tail projection with ledger-selected transcript
      context.
- [x] Add context dashboard metadata to `<thndrs_self_knowledge>`.
- [x] Keep dashboard metadata compact enough for every turn.
- [x] Keep full memory and file contents out of the dashboard unless selected as
      normal context content.
- [x] Update `--print-prompt` snapshots.
- [x] Add prompt snapshots for no memory, core memory, archival memory, pins,
      compaction, nested instructions, and overloaded budget.
- [x] Test prompt rendering for context dashboards.

## Remaining Tickets

Work the frontier: tickets with no blockers can begin immediately. Keep one
ticket in one fresh agent context unless its acceptance criteria are already
complete.

### T9.1: Record Context Actions

**What to build:** Persist ledger snapshots and user pin, drop, and recovery
actions in append-only session JSONL so a resumed session can explain why an
item was or was not visible.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [ ] `context_ledger`, `context_pin`, `context_drop`, and `context_recovery`
      records contain metadata and reasons, never full file content.
- [ ] Each record round-trips through JSON and malformed optional fields do not
      prevent a reader from loading the rest of a session.
- [ ] Context metadata serialization excludes secret-shaped content.

**Verification:**

- `cargo test session context`

### T9.2: Record Memory Mutations

**What to build:** Persist explicit memory writes and deletes without copying
the memory body into session history.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [ ] `memory_write` and `memory_delete` records contain memory id, path,
      scope, and audit metadata only.
- [ ] A delete record cannot serialize forgotten content.
- [ ] Records round-trip through JSON and preserve existing session readers.

**Verification:**

- `cargo test session memory`

### T9.3: Record Compaction Audits

**What to build:** Persist the information needed to audit and recover a
manual or automatic compaction.

**Blocked by:** T9.1: Record Context Actions

**Acceptance criteria:**

- [ ] `compaction` records include summary text, covered range, source hashes
      where available, trigger, risk, review result, recovery handles, model,
      and token usage when available.
- [ ] Manual and automatic compactions serialize as distinct triggers.
- [ ] Corrupt optional compaction fields do not break session reading.

**Verification:**

- `cargo test session compaction`

### T10.1: Decide Compaction Policy

**What to build:** Resolve compaction mode, review policy, pressure trigger,
and risk classification as pure config and policy decisions.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [ ] `context.compaction.mode` supports `off`, `manual`, and `auto`, defaulting
      to `manual`.
- [ ] `context.compaction.review` supports `always`, `auto`, and `never`,
      defaulting to `auto`.
- [ ] Auto-compaction is considered only after normal selection remains above
      92% of available input budget.
- [ ] Risk classification identifies tool output, diffs, errors, permission
      prompts, corrections, failed commands, and unresolved work.
- [ ] Policy tests cover all modes, review choices, and the 92% boundary.

**Verification:**

- `cargo test context compaction`

### T10.2: Compact a Turn Manually

**What to build:** Let an idle user request `/compact`, summarize through the
configured model, update the ledger, and write a recoverable audit record.

**Blocked by:** T9.3: Record Compaction Audits; T10.1: Decide Compaction Policy

**Acceptance criteria:**

- [ ] The configured model, never a local fallback, generates the summary.
- [ ] The original session history remains intact and a recovery handle is
      available after success.
- [ ] A model failure leaves active context unchanged and the user turn
      recoverable.

**Verification:**

- `cargo test app compaction session`

### T10.3: Compact Before an Oversized Turn

**What to build:** When auto mode requires compaction, stop before the main
provider request, compact, rebuild context, and restart the same user turn.

**Blocked by:** T10.1: Decide Compaction Policy; T10.2: Compact a Turn Manually

**Acceptance criteria:**

- [ ] The known-oversized request is never sent to the main provider.
- [ ] Pre-restart steering applies to the restarted turn; follow-ups wait until
      it completes.
- [ ] In-flight provider requests are never interrupted for compaction in v1.
- [ ] Failure and review-pending paths preserve the submitted turn.

**Verification:**

- `cargo test app compaction`

### T10.4: Review High-Risk Compactions

**What to build:** Apply the configured review policy to manual and automatic
compactions, including visible no-review status and pending approval state.

**Blocked by:** T10.3: Compact Before an Oversized Turn

**Acceptance criteria:**

- [ ] `always` requests approval for every automatic compaction.
- [ ] `auto` requests approval only for high-risk ranges.
- [ ] `never` and low-risk `auto` paths visibly apply and retain audit metadata.

**Verification:**

- `cargo test app compaction renderer`

### T11.1: Inspect the Current Context

**What to build:** Add `/context` and `/context all` so a user can inspect the
working set and every ledger decision without exposing unselected content.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [ ] `/context` shows selected counts, estimated tokens, and concise reasons.
- [ ] `/context all` includes omitted, archived, dropped, blocked, and
      candidate metadata.
- [ ] Both commands are available while the agent is working and preserve input
      after invalid use.

**Verification:**

- `cargo test app context`

### T11.2: Inspect Memory and Recall Results

**What to build:** Complete `/memory`, `/memory stats`, and `/memory recall`
as read-only views of file-backed memory and its rebuildable index.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [ ] The list and stats distinguish user and project memory without relying
      only on color.
- [ ] Recall reports index failures distinctly from no matching memory.
- [ ] Commands are available while working and preserve input after invalid use.

**Verification:**

- `cargo test app memory`

### T12.1: Manage the Context Working Set

**What to build:** Add `/pin`, `/drop`, `/drop --reset`, `/recover`, and
`/clear-context` with durable action records.

**Blocked by:** T9.1: Record Context Actions; T11.1: Inspect the Current Context

**Acceptance criteria:**

- [ ] Pins, drops, and recovery affect the next selection deterministically.
- [ ] Reset removes only explicit drop rules.
- [ ] Clear-context keeps memory and compaction summaries.
- [ ] Invalid and unsafe requests preserve prompt text and show a clear status.

**Verification:**

- `cargo test app context session`

### T12.2: Manage Durable and Session Memory

**What to build:** Add `/remember`, `/memory open`, and `/memory forget` for
user, project, path, and session memory.

**Blocked by:** T9.2: Record Memory Mutations; T11.2: Inspect Memory and Recall Results

**Acceptance criteria:**

- [ ] Writes surface filesystem errors and secret-shaped warnings.
- [ ] Open and forget operate only on the requested item; forget writes a
      content-free audit record.
- [ ] Session memory survives context clearing and compaction.
- [ ] Unsafe mutations while working are rejected without queuing ordinary text.

**Verification:**

- `cargo test app memory session`

### T12.3: Rebuild the Memory Index Explicitly

**What to build:** Add `/memory index rebuild` to discard and recreate derived
SQLite/FTS metadata without changing Markdown memory source files.

**Blocked by:** T11.2: Inspect Memory and Recall Results

**Acceptance criteria:**

- [ ] Rebuild reports success or a useful cache error.
- [ ] Source Markdown is unchanged after rebuild.
- [ ] The command is rejected safely while an incompatible mutation is active.

**Verification:**

- `cargo test app memory`

### T13.1: Render Context and Memory Actions

**What to build:** Give context, memory, and warning actions compact transcript
rows that remain legible at narrow widths.

**Blocked by:** T12.1: Manage the Context Working Set; T12.2: Manage Durable and Session Memory

**Acceptance criteria:**

- [ ] Context actions, writes, deletes, warnings, and no-review compaction
      outcomes have distinct transcript rows.
- [ ] User and project memory are distinguishable without color alone.
- [ ] Narrow-width snapshots cover every new row type.

**Verification:**

- `cargo test renderer`

### T13.2: Render Focused Inspection Surfaces

**What to build:** Add focused ledger, memory list/detail, and doctor surfaces
without replacing native transcript scrollback.

**Blocked by:** T11.1: Inspect the Current Context; T11.2: Inspect Memory and Recall Results; T14.4: Run Context Doctor

**Acceptance criteria:**

- [ ] Each surface exposes its matching read-only command data.
- [ ] Small-height and narrow-width snapshots remain useful.
- [ ] Transcript history continues to use native scrollback.

**Verification:**

- `cargo test renderer app`

### T13.3: Render Compaction Review State

**What to build:** Show an auto-compaction applied status or a review-pending
surface with the decision and recovery handle.

**Blocked by:** T10.4: Review High-Risk Compactions

**Acceptance criteria:**

- [ ] Applied and pending-review states are visually distinct.
- [ ] Snapshot coverage includes narrow and tiny-height displays.

**Verification:**

- `cargo test renderer compaction`

### T14.1: Diagnose Source Memory Health

**What to build:** Detect unhealthy instruction and memory source material and
provide remediation without rewriting user files.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [ ] Reports oversized or stale `AGENTS.md`, oversized core memory, malformed
      frontmatter, duplicate ids, secret-shaped items, and simple conflicts.
- [ ] Every finding includes actionable remediation text.

**Verification:**

- `cargo test context memory`

### T14.2: Diagnose Index and Pin Health

**What to build:** Detect missing, stale, corrupt, or unhealthy FTS indexes and
pins that are missing or consume too much of the context budget.

**Blocked by:** T12.3: Rebuild the Memory Index Explicitly

**Acceptance criteria:**

- [ ] Reports index health without treating deferred semantic recall as an
      error.
- [ ] Reports missing pins and budget-dominating pins with remedies.

**Verification:**

- `cargo test context memory`

### T14.3: Diagnose Context Pressure and Compaction

**What to build:** Explain model-limit provenance, budget pressure, disabled
auto-compaction, and pending high-risk compaction review.

**Blocked by:** T10.4: Review High-Risk Compactions

**Acceptance criteria:**

- [ ] Reports fallback and overridden limits plus invalid overrides.
- [ ] Reports selection above the 80% target and high pressure with auto mode
      disabled.
- [ ] Reports pending review with a next action.

**Verification:**

- `cargo test context compaction`

### T14.4: Run Context Doctor

**What to build:** Add `/doctor` as the read-only command that combines source,
index, pin, budget, and compaction health findings.

**Blocked by:** T14.1: Diagnose Source Memory Health; T14.2: Diagnose Index and Pin Health; T14.3: Diagnose Context Pressure and Compaction

**Acceptance criteria:**

- [ ] `/doctor` is available while the agent is working.
- [ ] Its output is redacted, actionable, and leaves prompt text intact after
      invalid use.

**Verification:**

- `cargo test app doctor`

### T15.1: Document Everyday Context and Memory Use

**What to build:** Publish usage documentation for dashboard fields, memory
locations/frontmatter/kinds, precedence, safety boundaries, and everyday
commands.

**Blocked by:** T12.2: Manage Durable and Session Memory; T14.4: Run Context Doctor

**Acceptance criteria:**

- [ ] Documents inspection, remember, forget, pin, drop, recover, clear, and
      doctor workflows.
- [ ] Explains that memory cannot grant permissions or override higher-priority
      instructions.

**Verification:**

- `pnpm --dir docs build`

### T15.2: Document Limits, Compaction, and Session Recovery

**What to build:** Publish model-limit, budget, compaction, and session-record
documentation, including v1's deferred semantic recall.

**Blocked by:** T9.3: Record Compaction Audits; T10.4: Review High-Risk Compactions

**Acceptance criteria:**

- [ ] Documents limit sources, overrides, ratios, review policy, and compaction
      versus deletion.
- [ ] Documents session-scoped memory and recovery without implying summaries
      erase history.
- [ ] Links the context-control research notebooks.

**Verification:**

- `pnpm --dir docs build`

## P16: Release Verification

**Exit criterion:** Context control, memory, sessions, commands, renderer, and
public documentation pass their relevant checks together.

- [ ] `cargo fmt`
- [ ] `cargo clippy --fix --all-targets --allow-dirty`
- [ ] `cargo clippy --all-targets`
- [ ] `cargo test context`
- [ ] `cargo test memory`
- [ ] `cargo test prompt`
- [ ] `cargo test session`
- [ ] `cargo test app`
- [ ] `cargo test renderer`
- [ ] `cargo test`
- [ ] `pnpm --dir docs build`
