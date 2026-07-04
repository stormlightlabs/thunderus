# Context Control And Memory Tasks

Status: Draft
Captured: 2026-07-03

## P0: Lock The Contract

- [x] Context control includes file-backed memory in this feature.
- [x] Memory writes are explicit-only for the first implementation.
- [x] Autonomous memory suggestions may write durable files only after the user
      explicitly confirms the suggested diff/write.
- [x] V1 retrieval uses rebuildable SQLite metadata, FTS5/BM25, and sqlite-vec
      indexes over Markdown memory files.
- [x] SQLite indexes are derived caches under `~/.thndrs/cache/memory/`, not
      project-tracked source files.
- [x] Embeddings are part of v1 retrieval as a derived sqlite-vec index over
      Markdown memory files, not the memory source of truth.
- [x] Lexical FTS5/BM25 recall remains available when embeddings are missing,
      stale, or disabled.
- [x] A single memory index rejects mixed embedding models unless the user
      explicitly rebuilds or migrates the index.
- [x] The durable source is append-only session JSONL plus memory Markdown
      files.
- [x] Memory storage roots are `~/.thndrs/memory/` and `.thndrs/memory/`.
- [x] The first memory file format is a Markdown body with YAML frontmatter.
- [x] Initial context item kinds are `Harness`, `ProjectInstruction`,
      `UserMemory`, `ProjectMemory`, `PinnedFile`, `Skill`, `Transcript`,
      `Summary`, and `ToolArchive`.
- [x] Memory item kinds are `fact`, `preference`, `procedure`, and `context`.
- [x] Initial context visibility states are `Visible`, `Pinned`,
      `SummaryOnly`, `Archived`, `Candidate`, `Dropped`, and `Blocked`.
- [x] Read-only commands allowed while a turn is running: `/context`,
      `/context all`, `/memory`, and `/doctor`.
- [x] Mutating or prompt-affecting commands require idle state:
      `/pin`, `/drop`, `/recover`, `/remember`, `/memory open`,
      `/memory forget`, `/compact`, and `/clear-context`.
- [x] `/remember` requires an explicit scope:
      `/remember user <text>`, `/remember project <text>`,
      `/remember path <path> <text>`, or `/remember session <text>`.
- [x] `/memory forget` deletes the memory file after confirmation and appends a
      content-free `memory_delete` audit record; it does not write a tombstone
      file containing forgotten content.
- [x] Project memory docs explain that shared team memory may be committed,
      while personal project memory belongs in Git's local exclude path or user
      memory.
- [x] Token estimates use `ceil(utf8_bytes / 3) + 16` per item as a
      conservative approximate budget guard until provider-specific tokenizers
      are available.

## P1: Ledger Foundation

- [ ] Add a context-control module with `ContextItemKind`.
- [ ] Add `ContextVisibility`.
- [ ] Add `ContextItem`.
- [ ] Add `ContextBudget`.
- [ ] Add `ContextLedger`.
- [ ] Add `ContextDiagnostic`.
- [ ] Add stable context item id generation for file-backed sources.
- [ ] Add stable context item id generation for transcript/session sources.
- [ ] Add a conservative token estimator.
- [ ] Add formatting helpers for user-visible ledger summaries.
- [ ] Add model-visible dashboard rendering that excludes full content.
- [ ] Keep ledger types free of crossterm, renderer, and provider-specific
      types.
- [ ] Test context item id stability for paths.
- [ ] Test context item id stability for session ranges.
- [ ] Test token-estimation behavior on ASCII, Unicode, and code blocks.

## P2: Scoped Project Instructions

- [ ] Discover root `AGENTS.md` as the default project instruction source.
- [ ] Discover nested `AGENTS.md` files below the workspace root.
- [ ] Assign every nested `AGENTS.md` a subtree scope.
- [ ] Load nested instruction content only when applicable or explicitly pinned.
- [ ] Select closest applicable instruction sources for mentioned or pinned
      paths.
- [ ] Keep broader instruction sources visible as metadata when overridden.
- [ ] Reload instruction metadata at turn boundaries.
- [ ] Detect changed instruction file hashes between turns.
- [ ] Add diagnostics for unreadable instruction files.
- [ ] Add diagnostics for oversized or truncated instruction files.
- [ ] Preserve existing root-only behavior when no nested files exist.
- [ ] Test root `AGENTS.md` selection.
- [ ] Test nested `AGENTS.md` selection by mentioned path.
- [ ] Test closest instruction precedence.
- [ ] Test changed instruction hash diagnostics.
- [ ] Update project-context docs with scoped `AGENTS.md` behavior.

## P3: Memory Source Files

- [ ] Add `src/memory.rs`.
- [ ] Implement the user and project memory roots selected in P0.
- [ ] Implement memory item metadata with id, title, kind, scope, paths, tags,
      created, updated, and source.
- [ ] Read `core.md` from user memory.
- [ ] Read `core.md` from project memory.
- [ ] Discover archival notes under `notes/*.md`.
- [ ] Validate memory frontmatter.
- [ ] Surface diagnostics for malformed memory files.
- [ ] Keep memory body loading size-capped.
- [ ] Add explicit scoped memory write helper for `/remember`.
- [ ] Add memory deletion helper for `/memory forget`.
- [ ] Add path-scoped memory selection for project notes.
- [ ] Add secret-shaped content warning before memory writes.
- [ ] Keep memory files ordinary inspectable Markdown.
- [ ] Test memory discovery in user root.
- [ ] Test memory discovery in project root.
- [ ] Test malformed memory diagnostics.
- [ ] Test explicit scoped `/remember` writes valid Markdown/frontmatter.
- [ ] Test `/remember` without a scope is rejected.
- [ ] Test `/memory forget` deletes the memory file after confirmation.

## P4: SQLite Metadata And FTS Index

- [ ] Add SQLite memory index schema versioning.
- [ ] Add a memory metadata table keyed by memory id and file path.
- [ ] Add an FTS5 table for title, headings, tags, paths, and body text.
- [ ] Use BM25 ranking for lexical memory retrieval.
- [ ] Store content hash, mtime, and byte size for stale-index detection.
- [ ] Store derived SQLite indexes under `~/.thndrs/cache/memory/`.
- [ ] Use a workspace-root hash for project memory index cache names.
- [ ] Rebuild the SQLite index from Markdown when the cache is missing, stale,
      or corrupt.
- [ ] Add metadata-filtered FTS5 search over memory notes.
- [ ] Return FTS results with match reason, snippet, score, source, scope, and
      recovery handle.
- [ ] Test SQLite index creation and schema versioning.
- [ ] Test stale SQLite index rebuild after memory file edits.
- [ ] Test corrupt SQLite index rebuild.
- [ ] Test FTS5/BM25 search finds exact command, path, package, and error text.
- [ ] Test metadata-filtered FTS search by scope, tag, and path.
- [ ] Test FTS retrieval result snippets and recovery handles.

## P5: sqlite-vec Embedding Index

- [ ] Add sqlite-vec as the vector index for memory embeddings.
- [ ] Add an embeddings table with memory id, vector blob, model, dimensions,
      content hash, and updated time.
- [ ] Add a vec table keyed by memory id for approximate semantic recall.
- [ ] Define the initial embedding model and dimensions.
- [ ] Add embedding config validation.
- [ ] Reject mixed embedding models in one derived index unless an explicit
      rebuild is requested.
- [ ] Normalize vectors before storage.
- [ ] Store packed little-endian `f32` vectors in the embeddings table.
- [ ] Keep sqlite-vec rows derived from the embeddings table.
- [ ] Add fallback behavior when sqlite-vec is unavailable.
- [ ] Add fallback behavior when the embedding provider is unavailable.
- [ ] Surface semantic-recall degradation diagnostics in `/memory stats` and
      `/doctor`.
- [ ] Test sqlite-vec table creation.
- [ ] Test embedding vector byte round trips.
- [ ] Test normalized vector cosine similarity behavior.
- [ ] Test semantic recall finds related memory text.
- [ ] Test semantic recall degrades to FTS5 when embeddings are unavailable.
- [ ] Test mixed embedding model rejection.
- [ ] Test explicit vector-index rebuild.

## P6: Hybrid Memory Recall

- [ ] Add metadata-filtered sqlite-vec search over memory notes.
- [ ] Combine lexical and semantic recall results with stable scoring and
      tie-breaking.
- [ ] Return memory retrieval results with match reason, snippet, score, source,
      scope, and recovery handle.
- [ ] Include whether a result came from metadata, FTS5, sqlite-vec, or a
      blended score.
- [ ] Include core memory before archival memory in recall projections.
- [ ] Add recall result caps by count and total bytes.
- [ ] Test blended lexical and semantic recall ordering.
- [ ] Test recall caps.
- [ ] Test recall degrades to lexical results with visible diagnostics.

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
- [ ] Omit UI-only and live-only transcript entries.
- [ ] Mark oversized items as blocked or summary-only instead of truncating
      silently.
- [ ] Record a reason for every visible, omitted, archived, dropped, blocked,
      or summary-only item.
- [ ] Test short, normal, and overloaded context budgets.
- [ ] Test pins survive across turns.
- [ ] Test drops remove pins from future turns.
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
- [ ] Include compaction summary text and covered sequence range.
- [ ] Include hashes for summarized source ranges where practical.
- [ ] Add JSON round-trip tests for every new record.
- [ ] Add replay/projection tests with context and compaction records.
- [ ] Add corruption-tolerant reader tests for malformed optional records.
- [ ] Test `memory_delete` records omit forgotten content.
- [ ] Test session JSONL records for context, memory, and compaction.
- [ ] Test secrets are not serialized into session records.

## P10: Read-Only Commands

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

## P11: Mutating Commands

- [ ] Add `/pin <id-or-path>`.
- [ ] Add `/drop <id>`.
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
- [ ] Test app command routing for mutating commands.

## P12: UI And Renderer

- [ ] Add transcript rows for context summary actions.
- [ ] Add transcript rows for memory writes.
- [ ] Add transcript rows for memory deletion.
- [ ] Add transcript rows for compaction.
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

## P13: Context Doctor

- [ ] Detect oversized core memory.
- [ ] Detect oversized `AGENTS.md`.
- [ ] Detect stale instruction hashes since the previous turn.
- [ ] Detect memory files with malformed frontmatter.
- [ ] Detect duplicate memory ids.
- [ ] Detect stale or missing memory indexes.
- [ ] Detect sqlite-vec unavailable or unhealthy.
- [ ] Detect mixed embedding model metadata.
- [ ] Detect memory items that look secret-shaped.
- [ ] Detect conflicting memory items with the same title/path scope where
      simple string rules can catch them.
- [ ] Detect pins that point to missing files.
- [ ] Detect pins that dominate the available context budget.
- [ ] Show actionable remediation text without rewriting files automatically.

## P14: Public Docs And Notebook Links

- [ ] Add public context-control usage docs.
- [ ] Add public memory usage docs.
- [ ] Document context dashboard fields.
- [ ] Document user memory and project memory paths.
- [ ] Document memory file frontmatter.
- [ ] Document memory kinds.
- [ ] Document metadata, FTS5, and sqlite-vec as rebuildable derived indexes.
- [ ] Document semantic recall degradation behavior.
- [ ] Document memory precedence below user/system/harness instructions.
- [ ] Document that memory cannot grant permissions or enable tools.
- [ ] Document how to inspect and delete memory files manually.
- [ ] Document `/remember`, `/memory`, `/pin`, `/drop`, `/recover`,
      `/compact`, `/clear-context`, and `/doctor`.
- [ ] Document compaction versus deletion.
- [ ] Document that summaries do not remove session history.
- [ ] Update session-format docs with new records.
- [ ] Cross-link notebook research:
      `context-control.md`, `letta.md`, `pi.md`, `polytoken.md`,
      `agents-md.md`, `sessions.md`, `skills.md`, and
      `local-memory-retrieval.md`.

## P15: Validation Commands

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
