# Memory Specification

Persistent agent memory backed by SQLite. No background processes, no external servers - a single `.db` file per workspace.

## Design Principles

1. **Single file.** One SQLite database. Portable, backupable, inspectable.
2. **No server.** Aligns with the project principle of no background processes.
3. **Two-phase approach.** Start with brute-force BLOB search (simple, metadata-filterable). Migrate to `sqlite-vec` when row count warrants it.
4. **Embeddings are pluggable.** Support local models (offline) and API models (higher quality). Default to local.

## Storage Layout

```text
~/.thunderus/memory/
├── global.db          # Cross-workspace memories (user preferences, general knowledge)
└── workspaces/
    └── {workspace_hash}.db   # Per-workspace memories (codebase-specific)
```

Each `.db` file has the same schema.

## Schema

```sql
-- Memory entries
CREATE TABLE memories (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  content     TEXT NOT NULL,                      -- the memorized text
  kind        TEXT NOT NULL DEFAULT 'fact',        -- 'fact', 'preference', 'procedure', 'context'
  source      TEXT,                                -- where it came from: 'user', 'agent', 'tool_result'
  tags        TEXT,                                -- comma-separated tags for coarse filtering
  created_at  TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at  TEXT NOT NULL DEFAULT (datetime('now')),
  accessed_at TEXT NOT NULL DEFAULT (datetime('now')),  -- for LRU-style decay
  access_count INTEGER NOT NULL DEFAULT 0,
  archived    INTEGER NOT NULL DEFAULT 0           -- soft delete
);

-- Embedding vectors stored as BLOBs
CREATE TABLE embeddings (
  memory_id   INTEGER PRIMARY KEY REFERENCES memories(id) ON DELETE CASCADE,
  vector      BLOB NOT NULL,                       -- packed float32, little-endian
  model       TEXT NOT NULL,                        -- embedding model used
  dimensions  INTEGER NOT NULL                      -- vector dimensionality
);

-- Indexes
CREATE INDEX idx_memories_kind ON memories(kind) WHERE archived = 0;
CREATE INDEX idx_memories_tags ON memories(tags) WHERE archived = 0;
CREATE INDEX idx_memories_accessed ON memories(accessed_at) WHERE archived = 0;
```

### Memory Kinds

| Kind         | Description                                 | Example                                                  |
| ------------ | ------------------------------------------- | -------------------------------------------------------- |
| `fact`       | A learned fact about the codebase or domain | "The project uses Axum for HTTP routing"                 |
| `preference` | User preference or workflow convention      | "User prefers tabs over spaces"                          |
| `procedure`  | A multi-step process that worked            | "To deploy: run cargo build --release, then scp to prod" |
| `context`    | Conversation-derived context                | "User is building a CLI tool for database migrations"    |

## Embedding Strategy

### Default: Local Model

| Model                    | Dimensions | Size  | Latency              |
| ------------------------ | ---------- | ----- | -------------------- |
| `all-MiniLM-L6-v2`       | 384        | ~23MB | 1–5ms/sentence (CPU) |
| `BAAI/bge-small-en-v1.5` | 384        | ~24MB | 1–5ms/sentence (CPU) |

Use 384-dimensional embeddings. At this size, 50K memories consume ~75MB of vector storage - trivial.

### Optional: API Model

| Model                             | Dimensions                     | Cost             | Notes                            |
| --------------------------------- | ------------------------------ | ---------------- | -------------------------------- |
| `text-embedding-3-small` (OpenAI) | 256 (truncated via Matryoshka) | ~$0.02/1M tokens | Higher quality, requires network |

The embedding model is recorded per-vector in `embeddings.model`. Mixed models are allowed but queries must use the same model as the stored vectors. In practice, pick one model per database and stick with it.

### Vector Format

Vectors are stored as raw IEEE 754 float32, little-endian, packed contiguously:

```text
[f32][f32][f32]...[f32]
 0    1    2       N-1
```

Total bytes = `dimensions * 4`. For 384 dimensions: 1,536 bytes per vector.

## Operations

### 1. Store

Save a new memory with its embedding.

```text
store(content: str, kind: str, source: str, tags: list[str]) -> int
```

**Steps:**

1. Compute embedding: `embed(content) → float32[384]`
2. Insert into `memories` table.
3. Insert packed vector into `embeddings` table.
4. Return the memory ID.

**Deduplication:** Before inserting, search for existing memories with cosine similarity > 0.95. If found, update the existing memory's `content` and `updated_at` instead of creating a duplicate.

### 2. Recall

Retrieve relevant memories given a query.

```text
recall(query: str, k: int = 5, kind: str? = None, tags: list[str]? = None) -> list[Memory]
```

**Steps:**

1. Compute query embedding: `embed(query) → float32[384]`
2. Load candidate vectors from `embeddings`, pre-filtered by metadata if `kind` or `tags` are specified:

   ```sql
   SELECT e.memory_id, e.vector
   FROM embeddings e
   JOIN memories m ON m.id = e.memory_id
   WHERE m.archived = 0
     AND (m.kind = ?1 OR ?1 IS NULL)
     AND (?2 IS NULL OR m.tags LIKE '%' || ?2 || '%')
   ```

3. Compute cosine similarity in application code (pre-normalize all vectors at insert time so dot product = cosine similarity).
4. Return top-k results sorted by similarity, with a minimum threshold of 0.3 (discard irrelevant noise).
5. Update `accessed_at` and `access_count` on returned memories.

**Why brute-force:** At 50K memories × 384 dimensions, a vectorized dot product over the full set takes <10ms with SIMD. No index needed.

### 3. Forget

Soft-delete a memory.

```text
forget(memory_id: int) -> bool
```

```sql
UPDATE memories SET archived = 1, updated_at = datetime('now') WHERE id = ?1;
```

Hard delete with `DELETE FROM memories WHERE id = ?1` only when explicitly requested.

### 4. Decay

Periodic maintenance to archive stale memories.

```text
decay(max_age_days: int = 90, min_access_count: int = 1) -> int
```

```sql
UPDATE memories
SET archived = 1, updated_at = datetime('now')
WHERE archived = 0
  AND accessed_at < datetime('now', '-' || ?1 || ' days')
  AND access_count < ?2;
```

Returns count of archived memories. Run this on agent startup, not as a background job.

## LLM-Facing Tool Definitions

Two tools exposed to the model: one to store, one to recall. Forget and decay are runtime-managed - the model doesn't invoke them directly.

### `memory_store`

```json
{
  "name": "memory_store",
  "description": "Save information to persistent memory for future conversations. Use this to remember facts about the codebase, user preferences, procedures that worked, or important context.",
  "parameters": {
    "type": "object",
    "properties": {
      "content": {
        "type": "string",
        "description": "The information to remember. Be specific and self-contained."
      },
      "kind": {
        "type": "string",
        "enum": ["fact", "preference", "procedure", "context"],
        "description": "The type of memory."
      },
      "tags": {
        "type": "array",
        "items": { "type": "string" },
        "description": "Tags for categorization (e.g., ['rust', 'testing', 'deployment'])."
      }
    },
    "required": ["content", "kind"]
  }
}
```

### `memory_recall`

```json
{
  "name": "memory_recall",
  "description": "Search persistent memory for relevant information from past conversations. Use this when you need context about the codebase, user preferences, or previously learned procedures.",
  "parameters": {
    "type": "object",
    "properties": {
      "query": {
        "type": "string",
        "description": "What to search for. Describe the information you need."
      },
      "kind": {
        "type": "string",
        "enum": ["fact", "preference", "procedure", "context"],
        "description": "Filter by memory type."
      },
      "count": {
        "type": "integer",
        "description": "Number of results to return (1-20).",
        "default": 5
      }
    },
    "required": ["query"]
  }
}
```

## Automatic Recall

On every conversation turn, the runtime performs an implicit recall before the model generates a response:

1. Take the latest user message.
2. `recall(user_message, k=3)` - fetch top 3 relevant memories.
3. If any result has similarity > 0.5, inject it into the system prompt as a `<memory>` block:

```xml
<memory>
- [preference] User prefers concise responses without emojis.
- [fact] The project uses SQLite via rusqlite with bundled feature.
- [procedure] Run `cargo test -- --nocapture` to see test output.
</memory>
```

This gives the model relevant context without it needing to explicitly call `memory_recall`. The tool is still available for deliberate, targeted searches.

## Migration Path: BLOB → sqlite-vec

When memory count exceeds ~50K rows and query latency becomes noticeable:

1. Add `sqlite-vec` as a dependency.
2. Create the virtual table:

   ```sql
   CREATE VIRTUAL TABLE vec_memories USING vec0(
     memory_id INTEGER PRIMARY KEY,
     embedding float[384] distance_metric=cosine
   );
   ```

3. Backfill from `embeddings` table:

   ```sql
   INSERT INTO vec_memories(memory_id, embedding)
   SELECT memory_id, vector FROM embeddings;
   ```

4. Update recall queries to use `MATCH` syntax:

   ```sql
   SELECT memory_id, distance
   FROM vec_memories
   WHERE embedding MATCH ?1
   ORDER BY distance
   LIMIT ?2;
   ```

5. Metadata filtering becomes a post-filter JOIN (overfetch by 3x, then filter).
6. Keep the `embeddings` table as the source of truth; `vec_memories` is a derived index.

The BLOB approach and sqlite-vec approach share the same vector format (packed float32 LE), so migration is a bulk insert - no re-embedding needed.

## Implementation Notes

1. **Pre-normalize all vectors** to unit length at insert time. This makes dot product = cosine similarity and avoids per-query normalization.
2. **Embedding model must be consistent** within a database. Store the model name in a `meta` table and reject inserts from a different model.
3. **Content size limit:** 4KB per memory. Force the model to write concise, self-contained memories. Long content gets poor embeddings.
4. **Transaction batching:** Wrap bulk inserts in a transaction. SQLite is fast for single writes but 100x faster in a transaction batch.
5. **WAL mode:** Enable `PRAGMA journal_mode=WAL` on database open. Required for concurrent reads during writes.
6. **No background processes.** Decay runs synchronously at startup. Embedding computation is synchronous at store time. No worker threads or job queues.
