-- Memory Store Schema
-- Version: 1
-- FTS5-based full-text search for memory documents

-- Schema version tracking
CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Content table (stores the actual documents)
CREATE TABLE IF NOT EXISTS memory_docs (
    id TEXT PRIMARY KEY,
    namespace TEXT NOT NULL,
    key TEXT NOT NULL,
    content TEXT NOT NULL,
    meta_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(namespace, key)
);

-- Namespace index for fast lookups
CREATE INDEX IF NOT EXISTS idx_memory_docs_namespace_key
ON memory_docs(namespace, key);

-- FTS5 virtual table for full-text search
-- Note: Not using external content to avoid column reference issues
CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(
    id UNINDEXED,
    title,
    headings,
    tags,
    body,
    path UNINDEXED,
    kind UNINDEXED,
    tokenize='porter unicode61 remove_diacritics 2'
);

-- Triggers to keep FTS index in sync with content table
CREATE TRIGGER IF NOT EXISTS memory_docs_ai AFTER INSERT ON memory_docs BEGIN
    INSERT INTO memory_fts(id, title, headings, tags, body, path, kind)
    SELECT
        NEW.id,
        json_extract(NEW.meta_json, '$.title'),
        json_extract(NEW.meta_json, '$.headings'),
        json_extract(NEW.meta_json, '$.tags'),
        NEW.content,
        json_extract(NEW.meta_json, '$.path'),
        json_extract(NEW.meta_json, '$.kind');
END;

CREATE TRIGGER IF NOT EXISTS memory_docs_ad AFTER DELETE ON memory_docs BEGIN
    DELETE FROM memory_fts WHERE id = OLD.id;
END;

CREATE TRIGGER IF NOT EXISTS memory_docs_au AFTER UPDATE ON memory_docs BEGIN
    DELETE FROM memory_fts WHERE id = OLD.id;
    INSERT INTO memory_fts(id, title, headings, tags, body, path, kind)
    SELECT
        NEW.id,
        json_extract(NEW.meta_json, '$.title'),
        json_extract(NEW.meta_json, '$.headings'),
        json_extract(NEW.meta_json, '$.tags'),
        NEW.content,
        json_extract(NEW.meta_json, '$.path'),
        json_extract(NEW.meta_json, '$.kind');
END;

-- Initialize schema version
INSERT OR IGNORE INTO schema_version (version) VALUES (1);

-- Vector embeddings table (optional, for semantic search)
CREATE TABLE IF NOT EXISTS memory_embeddings (
    doc_id TEXT NOT NULL PRIMARY KEY,
    embedding BLOB NOT NULL,
    dims INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_memory_embeddings_doc_id
ON memory_embeddings(doc_id);
