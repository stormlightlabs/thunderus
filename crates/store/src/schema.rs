//! SQLite schema for the memory store
//!
//! Defines the FTS5 full-text search schema with triggers for automatic indexing.

/// Current schema version for migrations
#[allow(dead_code)]
pub const SCHEMA_VERSION: i32 = 1;

/// SQL to create the complete schema
///
/// Includes:
/// - Schema version tracking table
/// - Content table (stores actual documents)
/// - FTS5 virtual table for full-text search
/// - Triggers to keep FTS index in sync
pub const SCHEMA_SQL: &str = include_str!("schema.sql");

/// SQL to create the schema version table
#[allow(dead_code)]
pub const SCHEMA_VERSION_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL DEFAULT (datetime('now'))
);
"#;

/// SQL to create the memory documents content table
#[allow(dead_code)]
pub const MEMORY_DOCS_SQL: &str = r#"
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

CREATE INDEX IF NOT EXISTS idx_memory_docs_namespace_key
ON memory_docs(namespace, key);
"#;

/// SQL to create the FTS5 virtual table for full-text search
///
/// Uses external content pointing to memory_docs table.
/// The tokenize option uses Porter stemming + Unicode normalization.
pub const MEMORY_FTS_SQL: &str = r#"
CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(
    id,
    title,
    headings,
    tags,
    body,
    path,
    kind,
    content=memory_docs,
    content_rowid=rowid,
    tokenize='porter unicode61 remove_diacritics 2'
);
"#;

/// SQL for triggers to keep FTS index in sync with content table
///
/// Three triggers handle INSERT, DELETE, and UPDATE operations.
pub const FTS_TRIGGERS_SQL: &str = r#"
CREATE TRIGGER IF NOT EXISTS memory_docs_ai AFTER INSERT ON memory_docs BEGIN
    INSERT INTO memory_fts(rowid, id, title, headings, tags, body, path, kind)
    SELECT
        NEW.rowid,
        NEW.id,
        json_extract(NEW.meta_json, '$.title'),
        json_extract(NEW.meta_json, '$.headings'),
        json_extract(NEW.meta_json, '$.tags'),
        NEW.content,
        json_extract(NEW.meta_json, '$.path'),
        json_extract(NEW.meta_json, '$.kind');
END;

CREATE TRIGGER IF NOT EXISTS memory_docs_ad AFTER DELETE ON memory_docs BEGIN
    INSERT INTO memory_fts(memory_fts, rowid, id, title, headings, tags, body, path, kind)
    SELECT
        'delete',
        OLD.rowid,
        OLD.id,
        json_extract(OLD.meta_json, '$.title'),
        json_extract(OLD.meta_json, '$.headings'),
        json_extract(OLD.meta_json, '$.tags'),
        OLD.content,
        json_extract(OLD.meta_json, '$.path'),
        json_extract(OLD.meta_json, '$.kind');
END;

CREATE TRIGGER IF NOT EXISTS memory_docs_au AFTER UPDATE ON memory_docs BEGIN
    INSERT INTO memory_fts(memory_fts, rowid, id, title, headings, tags, body, path, kind)
    SELECT
        'delete',
        OLD.rowid,
        OLD.id,
        json_extract(OLD.meta_json, '$.title'),
        json_extract(OLD.meta_json, '$.headings'),
        json_extract(OLD.meta_json, '$.tags'),
        OLD.content,
        json_extract(OLD.meta_json, '$.path'),
        json_extract(OLD.meta_json, '$.kind');
    INSERT INTO memory_fts(rowid, id, title, headings, tags, body, path, kind)
    SELECT
        NEW.rowid,
        NEW.id,
        json_extract(NEW.meta_json, '$.title'),
        json_extract(NEW.meta_json, '$.headings'),
        json_extract(NEW.meta_json, '$.tags'),
        NEW.content,
        json_extract(NEW.meta_json, '$.path'),
        json_extract(NEW.meta_json, '$.kind');
END;
"#;

/// FTS5 column weights for BM25 ranking
///
/// These weights prioritize structured fields:
/// - Title gets highest weight (10x)
/// - Headings get medium-high weight (5x)
/// - Tags get medium weight (3x)
/// - Body and path get base weight (1x)
#[allow(dead_code)]
pub const BM25_COLUMN_WEIGHTS: &str = "10.0, 5.0, 3.0, 1.0, 1.0, 1.0";

/// Query to get FTS5 column weights
#[allow(dead_code)]
pub const BM25_FUNCTION: &str = "bm25(memory_fts, 10.0, 5.0, 3.0, 1.0, 1.0, 1.0)";
