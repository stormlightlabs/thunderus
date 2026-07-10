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

- [ ] Add `/memory recall <query>` over metadata and FTS5/BM25 results.
- [ ] Return memory retrieval results with match reason, snippet, score, source,
      scope, and recovery handle.
- [ ] Include whether a result came from metadata or FTS5/BM25.
- [ ] Include core memory before archival memory in recall projections.
- [ ] Add recall result caps by count and total bytes.
- [ ] Test lexical recall ordering and stable tie-breaking.
- [ ] Test metadata-only matches.
- [ ] Test recall caps.
- [ ] Test recall returns empty results with a useful diagnostic.

## P7: Context Selection Policy

- [ ] Build candidate ledger items from harness context.
- [ ] Build candidate ledger items from project instructions.
- [ ] Build candidate ledger items from user memory.
- [ ] Build candidate ledger items from project memory.
- [ ] Build candidate ledger items from active pins.
- [ ] Build candidate ledger items from compaction summaries.
- [ ] Build candidate ledger items from recent transcript entries.
- [ ] Build candidate ledger items from skill metadata and loaded skills.
- [ ] Include current user turn outside ordinary budget eviction.
- [ ] Include active pins before ordinary recent transcript items.
- [ ] Include applicable closest `AGENTS.md` before broader guidance.
- [ ] Include the latest compaction summary when older turns are omitted.
- [ ] Keep session-scoped memory eligible after compaction and resume.
- [ ] Omit UI-only and live-only transcript entries.
- [ ] Mark oversized items as blocked or summary-only instead of truncating
      silently.
- [ ] Record a reason for every visible, omitted, archived, dropped, blocked,
      or summary-only item.
- [ ] Test short, normal, and overloaded context budgets.
- [ ] Test pins survive across turns.
- [ ] Test drops remove pins from future turns.
- [ ] Test explicit dropped-item rules persist until source change or
      `/drop --reset`.
- [ ] Test `/clear-context` clears pins, recovered/opened items, and transient
      transcript carryover without deleting memory or compaction summaries.
- [ ] Test recover reopens archived/omitted context by id.
- [ ] Test compaction summary replaces older transcript entries in prompt
      projection.
- [ ] Test secrets are not serialized into context metadata.

## P8: Prompt And Self-Knowledge

- [ ] Change `PromptBundle` to accept a selected context projection.
- [ ] Preserve existing prompt fragment ordering.
- [ ] Render selected project instructions from ledger items.
- [ ] Render selected memory items with source/scope labels.
- [ ] Render selected compaction summaries with covered sequence metadata.
- [ ] Render pinned file excerpts or handles without bypassing file-tool
      containment.
- [ ] Replace fixed transcript tail projection with ledger-selected transcript
      context.
- [ ] Add context dashboard metadata to `<thndrs_self_knowledge>`.
- [ ] Keep dashboard metadata compact enough for every turn.
- [ ] Keep full memory and file contents out of the dashboard unless selected as
      normal context content.
- [ ] Update `--print-prompt` snapshots.
- [ ] Add prompt snapshots for no memory, core memory, archival memory, pins,
      compaction, nested instructions, and overloaded budget.
- [ ] Test prompt rendering for context dashboards.

## P9: Session Records

- [ ] Add `context_ledger` session record.
- [ ] Add `context_pin` session record.
- [ ] Add `context_drop` session record.
- [ ] Add `context_recovery` session record.
- [ ] Add `memory_write` session record.
- [ ] Add `memory_delete` session record.
- [ ] Add `compaction` session record.
- [ ] Include context item metadata without full file contents.
- [ ] Include memory ids and paths without duplicating full memory bodies.
- [ ] Include compaction summary text, covered sequence range, trigger reason,
      risk classification, review outcome, and recovery handles.
- [ ] Include hashes for summarized source ranges where practical.
- [ ] Add JSON round-trip tests for every new record.
- [ ] Add corruption-tolerant reader tests for malformed optional records.
- [ ] Test `memory_delete` records omit forgotten content.
- [ ] Test session JSONL records for context, memory, and compaction.
- [ ] Test compaction records distinguish manual and auto trigger reasons.
- [ ] Test secrets are not serialized into session records.

## P10: Compaction Policy

- [ ] Add `context.compaction.mode = "off" | "manual" | "auto"` config.
- [ ] Add `context.compaction.review = "always" | "auto" | "never"` config.
- [ ] Default compaction mode to `"manual"`.
- [ ] Default compaction review to `"auto"`.
- [ ] Implement explicit `/compact` as a manual compaction request.
- [ ] Implement auto-compaction trigger under context pressure only when
      `mode = "auto"`.
- [ ] Trigger auto-compaction only after normal eviction and summary candidates
      still exceed 92% of available input budget.
- [ ] Run auto-compaction as a preflight step before the main provider request.
- [ ] Stop the submitted turn before sending the main provider request when
      auto-compaction is required.
- [ ] Generate every compaction summary through the configured model.
- [ ] Rebuild the context ledger after successful compaction.
- [ ] Restart the same user turn after successful auto-compaction.
- [ ] Apply steering queued before the restarted provider request to the
      restarted turn.
- [ ] Keep queued follow-up prompts queued until after the restarted turn
      completes.
- [ ] Leave active context unchanged when the compaction model call fails.
- [ ] Keep the submitted user turn recoverable when compaction fails or waits
      for review.
- [ ] Do not interrupt in-flight provider requests for compaction in v1.
- [ ] Do not send a main provider request that the context policy already knows
      is oversized.
- [ ] Do not add a deterministic or local-only summarizer fallback in v1.
- [ ] Classify high-risk compaction ranges that include tool outputs, file
      diffs, error logs, permission prompts, user corrections, failed commands,
      or unresolved action items.
- [ ] Require approval for all auto-compactions when `review = "always"`.
- [ ] Require approval for high-risk auto-compactions when `review = "auto"`.
- [ ] Apply low-risk auto-compactions without approval when `review = "auto"`,
      with a visible status row.
- [ ] Apply auto-compactions without approval when `review = "never"`, with a
      visible status row.
- [ ] Preserve original session records for every compaction path.
- [ ] Record compaction model id and token usage when available.
- [ ] Provide recovery handles for compacted ranges.
- [ ] Test defaults keep auto-compaction disabled.
- [ ] Test auto-compaction does not trigger below 92% of available input
      budget.
- [ ] Test auto-compaction can trigger above 92% of available input budget.
- [ ] Test manual compaction uses the configured model.
- [ ] Test auto-compaction uses the configured model.
- [ ] Test auto-compaction stops before the main provider request.
- [ ] Test successful auto-compaction restarts the same user turn.
- [ ] Test pre-restart steering is applied to the restarted turn.
- [ ] Test queued follow-ups are held until after the restarted turn completes.
- [ ] Test failed compaction model calls leave active context unchanged.
- [ ] Test failed compaction keeps the submitted user turn recoverable.
- [ ] Test high-risk auto-compaction requires review under `review = "auto"`.
- [ ] Test low-risk auto-compaction applies under `review = "auto"`.
- [ ] Test `review = "always"` always requires approval.
- [ ] Test `review = "never"` still writes audit and recovery metadata.

## P11: Read-Only Commands

- [ ] Add `/context`.
- [ ] Add `/context all`.
- [ ] Add `/memory`.
- [ ] Add `/memory recall <query>`.
- [ ] Add `/memory stats`.
- [ ] Add `/doctor`.
- [ ] Add command suggestions for read-only context and memory commands.
- [ ] Allow only `/context`, `/context all`, `/memory`, and `/doctor` while
      the agent is working.
- [ ] Preserve prompt text after failed read-only context or memory commands.
- [ ] Test app command routing for read-only commands.

## P12: Mutating Commands

- [ ] Add `/pin <id-or-path>`.
- [ ] Add `/drop <id>`.
- [ ] Add `/drop --reset`.
- [ ] Add `/recover <id>`.
- [ ] Add `/remember user <text>`.
- [ ] Add `/remember project <text>`.
- [ ] Add `/remember path <path> <text>`.
- [ ] Add `/remember session <text>`.
- [ ] Add `/memory open <id>`.
- [ ] Add `/memory index rebuild`.
- [ ] Add `/memory forget <id>`.
- [ ] Add `/compact`.
- [ ] Add `/clear-context`.
- [ ] Add command suggestions for mutating context and memory commands.
- [ ] Reject unsafe memory/context writes while running with clear status rows.
- [ ] Keep unknown slash commands from being queued as ordinary text.
- [ ] Wire context actions into the session writer.
- [ ] Wire memory actions into the session writer.
- [ ] Preserve prompt text after failed mutating context or memory commands.
- [ ] Test `/drop --reset` clears explicit dropped-item rules.
- [ ] Test `/clear-context` does not delete session-scoped memory or
      compaction summaries.
- [ ] Test app command routing for mutating commands.

## P13: UI And Renderer

- [ ] Add transcript rows for context summary actions.
- [ ] Add transcript rows for memory writes.
- [ ] Add transcript rows for memory deletion.
- [ ] Add transcript rows for compaction.
- [ ] Add transcript rows for auto-compaction applied without review.
- [ ] Add transcript rows for auto-compaction waiting for review.
- [ ] Add transcript rows for context warnings.
- [ ] Add a focused context ledger view.
- [ ] Add a focused memory picker/list view.
- [ ] Add a focused memory detail view.
- [ ] Add a focused doctor/audit view.
- [ ] Show context counts and estimated tokens in compact rows.
- [ ] Show user memory versus project memory distinctly without relying only on
      color.
- [ ] Add narrow-width snapshots for context and memory rows.
- [ ] Add tiny-height snapshots for context and memory focused surfaces.
- [ ] Ensure native scrollback remains the transcript history path.
- [ ] Test renderer snapshots for context and memory surfaces.

## P14: Context Doctor

- [ ] Detect oversized core memory.
- [ ] Detect oversized `AGENTS.md`.
- [ ] Detect stale instruction hashes since the previous turn.
- [ ] Detect memory files with malformed frontmatter.
- [ ] Detect duplicate memory ids.
- [ ] Detect stale or missing memory indexes.
- [ ] Detect stale or unhealthy FTS5 indexes.
- [ ] Report that semantic recall is not enabled in v1 without treating it as
      an error.
- [ ] Detect memory items that look secret-shaped.
- [ ] Detect conflicting memory items with the same title/path scope where
      simple string rules can catch them.
- [ ] Detect pins that point to missing files.
- [ ] Detect pins that dominate the available context budget.
- [ ] Detect unknown or fallback model context limits.
- [ ] Detect user-overridden model context limits.
- [ ] Detect invalid or internally inconsistent model limit overrides.
- [ ] Detect selected context above the 80% target budget.
- [ ] Detect auto-compaction disabled while context pressure is high.
- [ ] Detect pending high-risk compaction reviews.
- [ ] Show actionable remediation text without rewriting files automatically.

## P15: Public Docs And Notebook Links

- [ ] Add public context-control usage docs.
- [ ] Add public memory usage docs.
- [ ] Document context dashboard fields.
- [ ] Document model context limit sources and fallback behavior.
- [ ] Document advanced model context limit overrides.
- [ ] Document context budget ratios for selection and auto-compaction.
- [ ] Document user memory and project memory paths.
- [ ] Document memory file frontmatter.
- [ ] Document memory kinds.
- [ ] Document metadata and FTS5/BM25 as rebuildable derived indexes.
- [ ] Document embeddings and sqlite-vec as deferred semantic-recall research,
      not v1 behavior.
- [ ] Document memory precedence below user/system/harness instructions.
- [ ] Document that memory cannot grant permissions or enable tools.
- [ ] Document how to inspect and delete memory files manually.
- [ ] Document `/remember`, `/memory`, `/pin`, `/drop`, `/drop --reset`,
      `/recover`, `/compact`, `/clear-context`, and `/doctor`.
- [ ] Document compaction versus deletion.
- [ ] Document compaction mode and review settings.
- [ ] Document high-risk compaction review behavior.
- [ ] Document session-scoped memory lifetime.
- [ ] Document `/clear-context` behavior.
- [ ] Document that summaries do not remove session history.
- [ ] Update session-format docs with new records.
- [ ] Cross-link notebook research:
      `context-control.md`, `letta.md`, `pi.md`, `polytoken.md`,
      `agents-md.md`, `sessions.md`, `skills.md`, and
      `local-memory-retrieval.md`.

## P16: Validation Commands

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
