---
title: "Local Memory Retrieval"
sources:
  - https://www.sqlite.org/fts5.html
  - https://alexgarcia.xyz/sqlite-vec/features/vec0.html
  - https://alexgarcia.xyz/sqlite-vec/api-reference.html
Author: SQLite maintainers; sqlite-vec maintainers
Date: 2026-07-04
Tags: [memory, sqlite, fts5, sqlite-vec, embeddings, retrieval]
---

## Summary

Local agent memory can combine inspectable source files with rebuildable SQLite
indexes: FTS5 for lexical retrieval and sqlite-vec for semantic vector recall.

## Key Ideas

- **FTS5 handles exact language:** It is useful for commands, file paths,
  package names, error messages, and phrases.
- **sqlite-vec handles semantic similarity:** Its `vec0` virtual tables support
  vector search with KNN-style queries.
- **Metadata matters:** Scope, tags, paths, kind, timestamps, hashes, and model
  metadata should be queryable or joinable.
- **Indexes are derived:** Source memory should remain inspectable; SQLite
  tables can be rebuilt when stale or corrupt.
- **Hybrid retrieval is safer:** Lexical and semantic retrieval fail in
  different ways, so both should stay available.

## Claims & Evidence

### FTS5 is suited to local full-text search.

SQLite FTS5 supports full-text indexes, ranking, prefix and phrase queries, and
snippet-like result display. That makes it a good fit for local memory text and
source-derived notes.

Confidence: high.

### sqlite-vec supports vector search inside SQLite.

sqlite-vec's `vec0` table supports vector columns, KNN queries using `MATCH`,
metadata columns, partition keys, and auxiliary columns.

Confidence: high, with the caveat that the documentation labels parts of the
project as work in progress.

### Metadata filters need deliberate schema design.

sqlite-vec metadata columns can be used in KNN `WHERE` constraints, but only for
supported types and operators. More complex filters may need ordinary SQLite
tables and joins.

Confidence: high.

## Important Terms

| Term          | Meaning                                                 |
| ------------- | ------------------------------------------------------- |
| FTS5          | SQLite extension for full-text indexing and search.     |
| BM25          | Ranking function commonly used for full-text relevance. |
| sqlite-vec    | SQLite extension for vector storage and search.         |
| `vec0`        | sqlite-vec virtual table for vector search.             |
| Derived index | Rebuildable cache generated from durable source files.  |

## Questions for Review

- Which memory fields belong in ordinary metadata tables versus `vec0`
  metadata columns?
- Should memory chunks be whole notes, headings, paragraphs, or explicit
  sections?
- How should FTS5 and vector results be merged?
- What diagnostics should appear when embeddings are stale or unavailable?

## Connections

- Related ideas: context ledger, archival memory, `/memory recall`, context
  doctor, session audit.
- Related sources: Letta memory notes, context-control notes, SQLite docs,
  sqlite-vec docs.

## Open Questions

- Which embedding model should be the first supported model?
- Should embeddings be produced locally, by a provider API, or both?
- Should project memory use a separate index per workspace hash?

## Takeaways

- Use FTS5 and sqlite-vec together.
- Keep memory source inspectable and indexes rebuildable.
- Treat semantic recall as a context-control capability, not hidden prompt magic.
