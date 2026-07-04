# Context Control And Memory Plan

Status: Draft
Captured: 2026-07-03

## Background

Context control is a core part of the product identity `thndrs` should inherit
from Pi: the user should be able to understand what was loaded, why the model
saw it, and how to change it. Hidden prompt/context injection makes model
behavior difficult to predict and difficult to debug.

`thndrs` already has the foundation for a serious context-control system:

- `src/prompt/mod.rs` builds a structured `PromptBundle` instead of concatenated
  prompt text.
- `src/context.rs` loads root `AGENTS.md` with path, scope, hash, truncation,
  and byte-count metadata.
- `src/skills.rs` discovers skill metadata first and loads full instructions
  only after activation.
- `src/internals.rs` renders a compact self-knowledge snapshot with runtime,
  docs, skills, and prompt-context metadata.
- `src/session/mod.rs` persists append-only JSONL session records and prompt
  metadata without copying full project context or prompt text into metadata.
- `src/app.rs` already has command mode, slash commands, file/model/skill
  pickers, queued input, startup context display, and session-writer hooks.

Reference review points to a broader target:

- The original `stormlightlabs/thunderus` memory design contributes durable
  memory kinds, per-workspace/global stores, `memory_store` and
  `memory_recall`, deduplication, access/decay metadata, and sqlite-vec-backed
  semantic recall.
- Pi contributes explicit context, small prompt/tool surfaces, and visible local
  harness state.
- Letta contributes the memory hierarchy: always-visible memory blocks,
  searchable archival memory, file-backed inspectable memory, and later
  background consolidation.
- Polytoken contributes the distinction between durable session history and the
  bounded model context, plus `/compact` and `/clear` style workflows.
- AGENTS.md guidance contributes scoped project instructions where the nearest
  file wins, while explicit user prompts outrank file guidance.
- Memory Sandbox and VISTA both argue that memory/context must be inspectable:
  users need controls, and models need per-block state such as size, recency,
  visibility, and recovery handles.

The durable move is to make context addressable, inspectable, budgeted,
recoverable, and editable. Memory should grow from that substrate instead of
appearing as another invisible input channel.

## Problem

The current implementation is honest but still too coarse:

- only root `AGENTS.md` loads automatically;
- nested `AGENTS.md` scope is not represented;
- the transcript tail is a fixed projection rather than an explainable context
  selection policy;
- the prompt has no context ledger, token budget, item ids, or inclusion
  rationale;
- the model sees some self-knowledge metadata, but not a dashboard of active
  context pressure or omitted recoverable sources;
- users cannot inspect the exact working set before a turn without printing the
  whole prompt;
- users cannot pin task-local context, drop stale context, or recover archived
  context by id;
- there is no durable user or project memory store;
- there is no `/remember` flow for explicit memory writes;
- there is no `/compact` flow that separates full session history from active
  model context;
- context and memory changes do not have first-class audit records.

If this is left as ad hoc prompt assembly, memory will become another hidden
input channel. That would work against the same context-control identity we are
trying to build.

## Milestone Outcome

At the end of this feature, a user should be able to ask `thndrs` what context
is active, see the answer as a compact ledger, pin or drop context items, write
durable memory explicitly, compact old transcript state without losing the
session log, and resume a session with enough metadata to audit what shaped each
turn.

The model should receive a compact context dashboard in addition to selected
context content. It should know which blocks are visible, pinned, summarized,
archived, or omitted, and where recoverable context can be reopened through
normal tools.

Memory is part of this milestone, and the first implementation is deliberately
explicit and file-backed. Autonomous memory suggestions, richer retrieval, and
multi-agent shared memory can build on the same ledger after the visible write
and audit contract exists.

## Goals

1. Introduce a typed context ledger that records candidate and selected context
   items before prompt rendering.
2. Replace the fixed transcript-tail heuristic with an explicit context
   selection policy that can include recent turns, summaries, pins, and memory.
3. Add a compact context dashboard to the model-visible self-knowledge snapshot.
4. Add user-visible commands and UI rows for context inspection, pinning,
   dropping, recovery, compaction, and memory.
5. Support scoped `AGENTS.md` discovery and turn-boundary reload metadata.
6. Add explicit file-backed user and project memory stores.
7. Add `/remember` as the first durable memory write path.
8. Add `/compact` as the first session-history-to-active-context reduction
   path.
9. Persist context ledger, memory write, pin/drop, and compaction metadata in
   append-only session records.
10. Keep the first implementation simple enough to audit while still shipping a
    real sqlite-vec memory path: explicit memory writes, file-backed Markdown
    source, rebuildable SQLite metadata/FTS/vector indexes, visible dashboard
    metadata, and append-only audit records.

## Research-Backed Decisions

| Question                                                                | Decision                                                                                                                                                                                                                                | Basis                                                                                                                                                                                                                                          |
| ----------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| How should token estimates work before exact provider tokenizers exist? | Use `ceil(utf8_bytes / 3) + 16` per item as a conservative approximate budget guard, label it approximate, and keep the type boundary ready for provider-specific tokenizers.                                                           | OpenAI tokenizer guidance says model encodings differ and exact counts come from the model encoding.                                                                                                                                           |
| Should memory items have scopes?                                        | Require explicit scope metadata for every memory item: user, project, path, or session.                                                                                                                                                 | Memory Sandbox and the memory survey emphasize user control, write filtering, privacy, and governance.                                                                                                                                         |
| Should project memory be committed or ignored?                          | Let projects choose, but document the split: shared project memory may be committed; personal project memory belongs in local exclude/user memory.                                                                                      | Git's ignore model distinguishes shared `.gitignore`, local `.git/info/exclude`, and user-global excludes.                                                                                                                                     |
| How should stale or conflicting memory be handled?                      | Show diagnostics with source, scope, timestamp/hash, and the conflicting item; never silently resolve conflict by overwriting.                                                                                                          | AGENTS.md precedence and memory-governance research both favor visible source/scope boundaries.                                                                                                                                                |
| When can autonomous memory writes appear?                               | Only after explicit memory CRUD, source metadata, conflict diagnostics, and audit records exist; first as suggestions requiring confirmation.                                                                                           | The memory survey highlights write-path filtering and trustworthy reflection as hard engineering areas.                                                                                                                                        |
| Should archival memory include semantic recall in this feature?         | Yes. Store Markdown as the source of truth and ship v1 retrieval with rebuildable SQLite metadata, FTS5/BM25, and sqlite-vec indexes under `~/.thndrs/cache/memory/`. Lexical recall remains available when embeddings are unavailable. | The old `thunderus` design shows semantic memory is valuable enough to be part of context control. SQLite FTS5 provides reliable lexical recovery, while sqlite-vec gives a local derived vector index over the same inspectable source files. |
| Should compaction delete old context?                                   | Compaction reduces the active working set but preserves durable session evidence.                                                                                                                                                       | VISTA distinguishes summaries from recoverable exact payloads under context pressure.                                                                                                                                                          |

## Design Principles

### Context Is Data, Not A Prompt String

Every significant context item should have a stable id, kind, source, scope,
byte count, content hash when applicable, estimated token cost, inclusion
status, and recovery path when omitted. The prompt is a rendering of selected
ledger items, not the source of truth.

### Memory Writes Are Explicit First

The first memory system should never silently decide to remember something.
Memory enters through explicit user commands such as `/remember`, manual file
edits, or later confirmed suggestions. Every write is visible and recorded.

### Summaries Do Not Destroy Evidence

Compaction can replace old transcript detail in active context, but the original
session records remain. The summary is a working-set optimization, not the only
surviving copy of evidence.

### User And Model Both Need A Dashboard

The user needs commands and TUI rows that show what is active. The model needs a
small dashboard with context ids, kinds, token estimates, status, recency, and
recovery handles. This gives the model enough state to ask for recovery or avoid
overusing stale context.

### Memory Has Scope And Ownership

User memory and project memory are separate. A memory item records whether it is
global, workspace-local, path-scoped, or session-local. Higher-priority
instructions still win: user prompt and harness policy outrank memory, and
memory cannot grant permissions or suppress errors.

## Context Tiers

### Always-Loaded Harness Context

Stable prompt fragments, environment metadata, tool schemas, and core
self-knowledge remain controlled by the harness. They are listed in the ledger
as system-owned context but are not editable through memory commands.

### Core Memory

Small durable facts that are usually worth including:

- user preferences;
- durable project conventions;
- repo-specific caveats that do not belong in `AGENTS.md`;
- repeated workflow preferences.

Core memory is file-backed and visibly size-limited. If it grows too large, the
context doctor should recommend splitting it into archival notes.

### Scoped Project Instructions

`AGENTS.md` files are read-only guidance. Root `AGENTS.md` remains the default.
Nested `AGENTS.md` files should be discovered as candidates and included when a
turn references paths under their scope, when a pinned file is under their
scope, or when the user explicitly opens them.

Closest applicable guidance wins over broader guidance, but all loaded sources
are still shown in the ledger.

### Pinned Working Context

Pins keep task-local evidence visible across turns until the user drops them or
they expire. Pins can target files, file ranges, tool results, session summary
records, memory notes, or instruction files.

Pins are not memory. They are current-working-set controls.

### Transcript Working Set

Recent user, assistant, reasoning, and settled tool entries remain available,
but selection should be explicit:

- include the current user turn;
- include active pins;
- include recent un-compacted turns within budget;
- include the latest compaction summary when older turns are omitted;
- exclude UI-only and live-only rows.

### Compaction Summaries

`/compact` writes a durable summary record that can stand in for older
transcript entries in future model calls. It records the covered range, source
hashes or sequence numbers, summary text, and whether any tool output was
truncated before summarization.

### Archival Memory

Archival memory is stored as Markdown notes with frontmatter. It is discovered
by metadata and indexed into a rebuildable SQLite cache for v1. The cache
includes ordinary metadata tables, FTS5 full-text search with BM25 ranking, and
a sqlite-vec table for semantic recall over embedding vectors.

Archival memory is not loaded by default. It becomes active when selected by
search, pin, explicit `/memory open`, or a later confirmed retrieval policy.

Derived SQLite indexes live under `~/.thndrs/cache/memory/`, not inside the
project memory tree. Project memory indexes use a workspace-root hash in the
cache filename so `.thndrs/memory/` can remain ordinary source material without
requiring generated cache files to be committed or ignored.

Semantic recall uses sqlite-vec as a derived index, not as the durable source of
truth. If embeddings are unavailable, stale, or incompatible with the configured
embedding model, retrieval degrades to metadata and FTS5/BM25 with a visible
diagnostic. Mixed embedding models are rejected for a single index unless an
explicit rebuild or migration command chooses a new model.

## Memory Kinds

Carry forward the old `thunderus` memory kind model:

- `fact`: durable facts about the codebase or domain;
- `preference`: user or team workflow preferences;
- `procedure`: repeatable steps that worked;
- `context`: conversation-derived context that should survive the current turn.

Every memory item records kind, scope, source, tags, timestamps, content hash,
and optional path scope. Kind and scope are separate: for example, a
path-scoped memory can still be a `procedure`.

## Memory Store

Use Markdown files with YAML frontmatter:

```text
~/.thndrs/memory/core.md
~/.thndrs/memory/notes/*.md
.thndrs/memory/core.md
.thndrs/memory/notes/*.md
```

Initial frontmatter fields:

```yaml
id: mem_...
title: Preferred testing workflow
kind: procedure
scope: user | project | path | session
paths: []
tags: []
created: 2026-07-03T00:00:00Z
updated: 2026-07-03T00:00:00Z
source: explicit-user
embedding_model: text-embedding-3-small
embedding_dimensions: 256
```

The body is plain Markdown. Memory files are normal files: users can inspect,
edit, delete, diff, and back them up. `thndrs` should validate metadata and
surface diagnostics instead of silently ignoring malformed memory.

Project memory under `.thndrs/memory/` should not be assumed committed or
gitignored. Shared team memory can be committed. Personal project memory belongs
in Git's local exclude path or in user memory under `~/.thndrs/memory/`.

## Command Contract

Use existing slash/command-mode plumbing. The first command contract is:

- `/context`: show active context ledger summary.
- `/context all`: show active, omitted, archived, and candidate items.
- `/pin <id-or-path>`: keep a context item visible across turns.
- `/drop <id>`: remove a pin or exclude a selected context item.
- `/recover <id>`: reopen archived or omitted context through the normal read
  path.
- `/remember user <text>`: append an explicit user memory item.
- `/remember project <text>`: append an explicit project memory item.
- `/remember path <path> <text>`: append an explicit path-scoped memory item.
- `/remember session <text>`: append an explicit session-scoped memory item.
- `/memory`: list memory items and diagnostics.
- `/memory open <id>`: load a memory item into the working set.
- `/memory recall <query>`: search memory with metadata, FTS5, and sqlite-vec
  when available.
- `/memory stats`: show memory counts, index status, embedding model, and cache
  health.
- `/memory index rebuild`: rebuild derived metadata, FTS5, and sqlite-vec
  indexes.
- `/memory forget <id>`: delete a memory file after confirmation and append a
  content-free audit record.
- `/compact`: summarize older session context into a durable summary record.
- `/clear-context`: clear active pins and transcript working-set carryover
  without deleting the session log or memory files.
- `/doctor`: audit oversized, stale, conflicting, truncated, or malformed
  context and memory.

Only read-only commands run while the agent is working: `/context`,
`/context all`, `/memory`, and `/doctor`. Mutating or prompt-affecting commands
require idle state: `/pin`, `/drop`, `/recover`, `/remember`, `/memory open`,
`/memory forget`, `/memory index rebuild`, `/compact`, and `/clear-context`.
Rejected running commands produce a clear status row and are not queued as
ordinary prompt text.

## Implementation Shape

### Context Items

Introduce a small context module rather than expanding prompt assembly directly:

```rust
pub enum ContextItemKind {
    Harness,
    ProjectInstruction,
    UserMemory,
    ProjectMemory,
    PinnedFile,
    Skill,
    Transcript,
    Summary,
    ToolArchive,
}

pub enum ContextVisibility {
    Visible,
    Pinned,
    SummaryOnly,
    Archived,
    Candidate,
    Dropped,
    Blocked,
}

pub struct ContextItem {
    pub id: String,
    pub kind: ContextItemKind,
    pub label: String,
    pub source_path: Option<PathBuf>,
    pub scope: String,
    pub content_hash: Option<u64>,
    pub byte_count: usize,
    pub token_estimate: usize,
    pub visibility: ContextVisibility,
    pub reason: String,
}

pub struct ContextLedger {
    pub items: Vec<ContextItem>,
    pub budget: ContextBudget,
    pub diagnostics: Vec<ContextDiagnostic>,
}
```

Keep the first token estimator simple and conservative:
`ceil(utf8_bytes / 3) + 16` per item. Provider-specific tokenizers can replace
it later.

### Prompt Assembly

`PromptBundle` should receive a selected context projection rather than raw
`context_sources` plus fixed `transcript_tail`. Rendering remains structured XML
or XML-shaped Markdown, but the selected item list comes from the ledger.

The model-visible self-knowledge block should include a compact context
dashboard, not the full content of every item.

### Session Records

Add append-only records for:

- `context_ledger`: per-turn selected/omitted metadata;
- `context_pin`: pin/drop actions;
- `memory_write`: explicit remember operations;
- `memory_delete`: forget/delete operations;
- `compaction`: summary text plus covered sequence range;
- `context_recovery`: archived item reopened by id.

These records store metadata and summary text where appropriate. They should not
copy full file contents, raw provider payloads, or secrets.

### UI And Renderer

The transcript should show compact status rows for context actions:

- `context  9 visible · 3 pinned · 2 archived · 18k est. tokens`
- `remembered  project memory: Preferred test command`
- `compacted  seq 12..47 into summary ctx_18`
- `context warning  root AGENTS.md changed since turn 4`

Detailed views can reuse the existing focused surface/picker machinery.

## Security And Privacy Requirements

- Memory files are never created from model inference without an explicit user
  action in this milestone.
- Memory cannot grant permissions, enable tools, change provider/model/search
  settings, suppress errors, or override user/system/developer instructions.
- Secret-shaped memory text should trigger a warning before write.
- Context metadata persisted in sessions must not include full prompt text,
  full file contents, raw provider payloads, or secrets.
- Project memory and user memory must be clearly labeled in UI and docs.
- Deleting memory must affect only memory records, not unrelated session logs or
  project files.
- Context recovery must use normal workspace/path containment rules.

## Files To Touch

- `src/context.rs`: scoped `AGENTS.md`, context source metadata, reload
  diagnostics.
- `src/context_control.rs` or `src/context/mod.rs`: new context ledger,
  selection policy, token estimates, pins, and diagnostics.
- `src/memory.rs`: file-backed memory discovery, validation, write/delete, and
  SQLite metadata, FTS5/BM25, sqlite-vec indexing, embeddings, and search.
- `src/prompt/mod.rs`: render selected context projection and dashboard.
- `src/internals.rs`: include context dashboard metadata.
- `src/session/mod.rs`: add context ledger, pin/drop, memory, and compaction
  records.
- `src/app.rs`: commands, picker/detail surfaces, context actions, memory
  actions, and session writer integration.
- `src/renderer/live.rs` and transcript rendering modules: context/memory rows
  and detail surfaces.
- Public docs under `docs/src/content/docs/usage/` and
  `docs/src/content/docs/reference/`.

## Tests

Testing should focus on pure policy first:

- context item id stability;
- token budget selection;
- root and nested `AGENTS.md` scope selection;
- pins overriding normal recency;
- dropped items staying out of the next projection;
- compaction summary replacing older transcript entries;
- memory read/write/delete metadata;
- SQLite memory index rebuild, BM25 search behavior, sqlite-vec indexing, and
  semantic recall behavior;
- session JSONL round trips;
- prompt snapshots with dashboard metadata;
- TUI snapshots for context and memory rows.

## Validation Commands

- `cargo fmt`
- `cargo clippy --fix --all-targets --allow-dirty`
- `cargo clippy --all-targets`
- `cargo test context`
- `cargo test memory`
- `cargo test prompt`
- `cargo test session`
- `cargo test app`
- `cargo test renderer`
- `cargo test`
- `pnpm --dir docs build`
