//! Rebuildable SQLite metadata + FTS5/BM25 index over memory Markdown files.
//!
//! This module derives a rebuildable SQLite cache for metadata-filtered lexical recall.
//!
//! ## Cache layout
//!
//! Derived indexes live under `~/.thndrs/cache/memory/`, never inside a
//! project memory tree:
//!
//! - `user.db3`: global user memory index.
//! - `project-<workspace_hash>.db3`: project memory index, keyed by a
//!   workspace-root hash so different workspaces do not collide and
//!   `.thndrs/memory/` stays ordinary source material.
//!
//! ## Staleness and recovery
//!
//! Each indexed memory row records `content_hash`, `mtime`, and `byte_size`.
//! [`MemoryIndex::ensure_indexed`] rebuilds when the cache is missing, stale
//! (any discovered item's hash/mtime/bytes differ), or corrupt (the DB cannot
//! be opened or queried).
//!
//! Rebuild never deletes the Markdown source.
//!
//! The SQLite file is an ordinary rebuildable cache: deleting it is always
//! safe and the next `ensure_indexed` rebuilds it from Markdown.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use markdown::{Constructs, ParseOptions, mdast::Node};
use rusqlite::Connection;
use sha2::{Digest, Sha256};

use super::{MemoryItem, MemoryKind, MemoryRootKind, MemoryScope};
use crate::utils;

/// Current SQLite index schema version.
///
/// Bump when the schema changes; [`MemoryIndex::open`] rebuilds on mismatch.
pub const INDEX_SCHEMA_VERSION: u32 = 1;

/// Cache directory name under `~/.thndrs/cache/` for derived memory indexes.
pub const CACHE_DIR_NAME: &str = "memory";

/// Filename for the user memory index.
pub const USER_INDEX_FILE: &str = "user.db3";

/// Filename prefix for project memory indexes (followed by `-<hash>.db3`).
pub const PROJECT_INDEX_PREFIX: &str = "project-";

/// File extension for index databases.
pub const INDEX_EXT: &str = "db3";

/// Where a memory search match was found.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum MemoryMatchField {
    /// Matched the memory title.
    Title,
    /// Matched a Markdown heading.
    Heading,
    /// Matched a tag.
    Tag,
    /// Matched a path scope.
    Path,
    /// Matched the body text.
    Body,
}

impl MemoryMatchField {
    pub fn label(self) -> &'static str {
        match self {
            MemoryMatchField::Title => "title",
            MemoryMatchField::Heading => "heading",
            MemoryMatchField::Tag => "tag",
            MemoryMatchField::Path => "path",
            MemoryMatchField::Body => "body",
        }
    }
}

/// A single lexical memory retrieval result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemorySearchResult {
    /// Memory id (recovery handle).
    pub id: String,
    /// Memory title.
    pub title: String,
    /// Which root the file lives in.
    pub root: MemoryRootKind,
    /// Absolute source path (recovery handle).
    pub path: PathBuf,
    pub scope: MemoryScope,
    pub kind: MemoryKind,
    /// Relevance score (higher is better).
    ///
    /// SQLite BM25 returns negative values (more negative = better); this is
    /// negated so "higher = better". `0` for metadata-only matches.
    pub score: i64,
    /// Which field matched.
    pub matched_field: MemoryMatchField,
    /// Snippet around the match.
    pub snippet: String,
    /// Whether the result came from FTS5/BM25 (`true`) or metadata-only (`false`).
    pub from_fts: bool,
}

impl MemorySearchResult {
    /// Render a compact one-line summary.
    pub fn summary(&self) -> String {
        format!(
            "{}  {}  {}  score={}  [{}: {}]",
            self.id,
            self.kind.label(),
            self.scope.label(),
            self.score,
            self.matched_field.label(),
            self.title,
        )
    }
}

/// Filters applied to a memory search.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MemorySearchFilter {
    pub scope: Option<MemoryScope>,
    pub kind: Option<MemoryKind>,
    pub tag: Option<String>,
    pub path_prefix: Option<String>,
}

/// An error from the memory index.
#[derive(Debug, thiserror::Error)]
pub enum MemoryIndexError {
    #[error("failed to read/write memory index {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to open memory index {path}: {source}")]
    Open {
        path: PathBuf,
        #[source]
        source: rusqlite::Error,
    },
    #[error("memory index query failed{location}: {source}")]
    Query {
        location: String,
        #[source]
        source: rusqlite::Error,
    },
    #[error("failed to close memory index {path}: {source}")]
    Close {
        path: PathBuf,
        #[source]
        source: rusqlite::Error,
    },
    #[error("memory index schema creation failed: {0}")]
    Schema(#[source] rusqlite::Error),
    #[error("no home directory; cannot place {kind} memory index cache")]
    NoHome { kind: MemoryRootKind },
}

impl MemoryIndexError {
    fn io(path: &Path, source: std::io::Error) -> Self {
        MemoryIndexError::Io { path: path.to_path_buf(), source }
    }
    fn open(path: &Path, source: rusqlite::Error) -> Self {
        MemoryIndexError::Open { path: path.to_path_buf(), source }
    }
    fn query(path: &Path, source: rusqlite::Error) -> Self {
        let location = if path.as_os_str().is_empty() { String::new() } else { format!(" at {}", path.display()) };
        MemoryIndexError::Query { location, source }
    }
    fn close(path: &Path, source: rusqlite::Error) -> Self {
        MemoryIndexError::Close { path: path.to_path_buf(), source }
    }
    fn no_home(kind: MemoryRootKind) -> Self {
        MemoryIndexError::NoHome { kind }
    }
    fn schema(source: rusqlite::Error) -> Self {
        MemoryIndexError::Schema(source)
    }
}

/// A rebuildable SQLite index over one memory root.
///
/// One index covers either user memory or one project's memory. Project
/// indexes are keyed by a workspace-root hash so they do not collide.
pub struct MemoryIndex;

impl MemoryIndex {
    /// Resolve the index path for a root kind and optional workspace root.
    ///
    /// User memory uses a single `user.db3`. Project memory uses
    /// `project-<workspace_hash>.db3` derived from the canonical workspace
    /// root so different workspaces get separate indexes.
    pub fn index_path(kind: MemoryRootKind, workspace_root: Option<&Path>) -> Option<PathBuf> {
        let cache_dir = cache_dir()?;
        let filename = match kind {
            MemoryRootKind::User => USER_INDEX_FILE.to_string(),
            MemoryRootKind::Project => {
                let workspace = workspace_root?;
                let hash = workspace_hash(workspace);
                format!("{PROJECT_INDEX_PREFIX}{hash}.{INDEX_EXT}")
            }
        };
        Some(cache_dir.join(filename))
    }

    /// Open or create the index at `db_path`, rebuilding it from `items` when
    /// it is missing, stale, or corrupt.
    ///
    /// Returns the connection ready for queries. Staleness is detected by
    /// comparing each item's `content_hash`, `mtime`, and `byte_count` against
    /// the indexed metadata.
    pub fn ensure_indexed(db_path: &Path, items: &[MemoryItem]) -> Result<Connection, MemoryIndexError> {
        fs::create_dir_all(db_path.parent().unwrap_or_else(|| Path::new(".")))
            .map_err(|e| MemoryIndexError::io(db_path, e))?;

        if Self::needs_rebuild(db_path, items)? {
            Self::rebuild_at(db_path, items)?;
        }

        let conn = Connection::open(db_path).map_err(|e| MemoryIndexError::open(db_path, e))?;
        Self::set_pragmas(&conn)?;
        Ok(conn)
    }

    /// Whether the index at `db_path` must be rebuilt: missing, version
    /// mismatch, stale rows, or corrupt.
    pub fn needs_rebuild(db_path: &Path, items: &[MemoryItem]) -> Result<bool, MemoryIndexError> {
        if !db_path.exists() {
            return Ok(true);
        }

        let conn = match Connection::open(db_path) {
            Ok(c) => c,
            Err(_) => return Ok(true),
        };
        if Self::set_pragmas(&conn).is_err() {
            return Ok(true);
        }

        let version = Self::schema_version(&conn).unwrap_or(None);
        if version != Some(INDEX_SCHEMA_VERSION) {
            return Ok(true);
        }

        Ok(Self::is_stale(&conn, items))
    }

    /// Rebuild the index from scratch at `db_path` from `items`.
    ///
    /// Writes to a temporary file and atomically renames so a crash leaves no
    /// half-written index. Deletes any existing corrupt file first.
    pub fn rebuild_at(db_path: &Path, items: &[MemoryItem]) -> Result<(), MemoryIndexError> {
        let parent = db_path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|e| MemoryIndexError::io(db_path, e))?;

        let tmp_path = db_path.with_extension("db3.tmp");
        if tmp_path.exists() {
            let _ = fs::remove_file(&tmp_path);
        }

        let conn = Connection::open(&tmp_path).map_err(|e| MemoryIndexError::open(&tmp_path, e))?;
        Self::set_pragmas(&conn)?;
        Self::create_schema(&conn)?;

        let tx = conn
            .unchecked_transaction()
            .map_err(|e| MemoryIndexError::query(&tmp_path, e))?;
        for item in items {
            Self::index_item(&tx, item)?;
        }
        tx.commit().map_err(|e| MemoryIndexError::query(&tmp_path, e))?;
        conn.close()
            .map_err(|(_conn, e)| MemoryIndexError::close(&tmp_path, e))?;

        // Atomic replace so readers never see a partial index.
        fs::rename(&tmp_path, db_path).map_err(|e| MemoryIndexError::io(db_path, e))?;
        Ok(())
    }

    /// Search the index with an FTS5/BM25 query, optionally filtered by
    /// metadata.
    ///
    /// Returns results ordered by relevance (highest score first), with match
    /// reason, snippet, score, source, scope, and recovery handle.
    ///
    /// When `query` is empty, returns metadata-only matches ordered by
    /// `updated` descending.
    pub fn search(
        conn: &Connection, query: &str, filter: &MemorySearchFilter,
    ) -> Result<Vec<MemorySearchResult>, MemoryIndexError> {
        let fts_query = sanitize_fts_query(query);
        if fts_query.is_empty() {
            return Self::metadata_only_search(conn, filter);
        }

        let mut sql = String::from(
            "SELECT m.id, m.title, m.root, m.path, m.scope, m.kind, m.snippet_body, \
             bm25(memory_fts) AS rank, \
             highlight(memory_fts, 0, '', '') AS h_title, \
             highlight(memory_fts, 1, '', '') AS h_heading, \
             highlight(memory_fts, 2, '', '') AS h_tag, \
             highlight(memory_fts, 3, '', '') AS h_path, \
             highlight(memory_fts, 4, '', '') AS h_body \
             FROM memory_fts \
             JOIN memory_meta m ON m.rowid = memory_fts.rowid \
             WHERE memory_fts MATCH ?",
        );

        // Owned param values outlive the query. The FTS query is first; filter
        // values follow in order.
        let mut values: Vec<String> = vec![fts_query];
        Self::append_filter(&mut sql, &mut values, filter);
        sql.push_str(" ORDER BY rank LIMIT 50");

        let params: Vec<&dyn rusqlite::ToSql> = values.iter().map(|s| s as &dyn rusqlite::ToSql).collect();

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| MemoryIndexError::query(Path::new(""), e))?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(params), |row| {
                let rank: f64 = row.get(7)?;
                // SQLite BM25 returns negative values (more negative = better).
                // Negate and round so "higher = better".
                let score = (-rank).round() as i64;
                let title: String = row.get(1)?;
                let root_label: String = row.get(2)?;
                let path: String = row.get(3)?;
                let scope_label: String = row.get(4)?;
                let kind_label: String = row.get(5)?;
                let h_title: String = row.get(8).unwrap_or_default();
                let h_heading: String = row.get(9).unwrap_or_default();
                let h_tag: String = row.get(10).unwrap_or_default();
                let h_path: String = row.get(11).unwrap_or_default();
                let h_body: String = row.get(12).unwrap_or_default();
                let snippet_body: String = row.get(6).unwrap_or_default();

                let (matched_field, snippet) =
                    pick_match(&h_title, &h_heading, &h_tag, &h_path, &h_body, &snippet_body);

                Ok(MemorySearchResult {
                    id: row.get(0)?,
                    title,
                    root: parse_root(&root_label),
                    path: PathBuf::from(path),
                    scope: parse_scope(&scope_label),
                    kind: parse_kind(&kind_label),
                    score,
                    matched_field,
                    snippet,
                    from_fts: true,
                })
            })
            .map_err(|e| MemoryIndexError::query(Path::new(""), e))?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| MemoryIndexError::query(Path::new(""), e))?);
        }
        Ok(results)
    }

    /// Return all indexed memory metadata matching `filter`, without FTS.
    ///
    /// Used when the query is empty or purely metadata-driven. Results are
    /// ordered by `updated` descending.
    fn metadata_only_search(
        conn: &Connection, filter: &MemorySearchFilter,
    ) -> Result<Vec<MemorySearchResult>, MemoryIndexError> {
        let mut sql =
            String::from("SELECT id, title, root, path, scope, kind, snippet_body FROM memory_meta m WHERE 1=1");
        let mut values: Vec<String> = Vec::new();
        Self::append_filter(&mut sql, &mut values, filter);
        sql.push_str(" ORDER BY updated DESC LIMIT 50");

        let params: Vec<&dyn rusqlite::ToSql> = values.iter().map(|s| s as &dyn rusqlite::ToSql).collect();

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| MemoryIndexError::query(Path::new(""), e))?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(params), |row| {
                let root_label: String = row.get(2)?;
                let scope_label: String = row.get(4)?;
                let kind_label: String = row.get(5)?;
                let snippet_body: String = row.get(6).unwrap_or_default();
                Ok(MemorySearchResult {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    root: parse_root(&root_label),
                    path: PathBuf::from(row.get::<_, String>(3)?),
                    scope: parse_scope(&scope_label),
                    kind: parse_kind(&kind_label),
                    score: 0,
                    matched_field: MemoryMatchField::Title,
                    snippet: first_snippet_line(&snippet_body),
                    from_fts: false,
                })
            })
            .map_err(|e| MemoryIndexError::query(Path::new(""), e))?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| MemoryIndexError::query(Path::new(""), e))?);
        }
        Ok(results)
    }

    /// Resolve the index for a memory root, ensuring it is indexed from the
    /// provided inventory items.
    ///
    /// Convenience wrapper combining [`MemoryIndex::index_path`] and
    /// [`ensure_indexed`](Self::ensure_indexed).
    pub fn open_for_root(
        kind: MemoryRootKind, workspace_root: Option<&Path>, items: &[MemoryItem],
    ) -> Result<Connection, MemoryIndexError> {
        let db_path = Self::index_path(kind, workspace_root).ok_or_else(|| MemoryIndexError::no_home(kind))?;
        Self::ensure_indexed(&db_path, items)
    }

    /// Append a metadata filter clause to `sql`, pushing owned bound values.
    ///
    /// Values are owned `String`s so they outlive the query; the caller builds
    /// `&dyn ToSql` references from `values`.
    fn append_filter(sql: &mut String, values: &mut Vec<String>, filter: &MemorySearchFilter) {
        if let Some(scope) = filter.scope {
            sql.push_str(" AND m.scope = ?");
            values.push(scope.label().to_string());
        }
        if let Some(kind) = filter.kind {
            sql.push_str(" AND m.kind = ?");
            values.push(kind.label().to_string());
        }
        if let Some(tag) = &filter.tag {
            sql.push_str(" AND m.tags LIKE ?");
            values.push(format!("%{tag}%"));
        }
        if let Some(path_prefix) = &filter.path_prefix {
            sql.push_str(" AND m.paths LIKE ?");
            values.push(format!("%{path_prefix}%"));
        }
    }

    fn set_pragmas(conn: &Connection) -> Result<(), MemoryIndexError> {
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
            .map_err(|e| MemoryIndexError::query(Path::new(""), e))?;
        Ok(())
    }

    fn schema_version(conn: &Connection) -> Result<Option<u32>, MemoryIndexError> {
        let exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='schema_meta')",
                [],
                |row| row.get(0),
            )
            .map_err(|e| MemoryIndexError::query(Path::new(""), e))?;
        if !exists {
            return Ok(None);
        }
        let version: Option<u32> = conn
            .query_row("SELECT value FROM schema_meta WHERE key='schema_version'", [], |row| {
                row.get::<_, String>(0)
            })
            .ok()
            .and_then(|v| v.parse().ok());
        Ok(version)
    }

    fn is_stale(conn: &Connection, items: &[MemoryItem]) -> bool {
        let indexed = match Self::load_indexed_meta(conn) {
            Ok(m) => m,
            Err(_) => return true,
        };

        if indexed.len() != items.len() {
            return true;
        }
        for item in items {
            let mtime = file_mtime(&item.path).unwrap_or(0);
            match indexed.get(&item.id) {
                Some((hash, idx_mtime, bytes)) => {
                    if *hash != item.content_hash || *idx_mtime != mtime || *bytes != item.byte_count {
                        return true;
                    }
                }
                None => return true,
            }
        }
        false
    }

    fn load_indexed_meta(conn: &Connection) -> Result<HashMap<String, (u64, i64, usize)>, MemoryIndexError> {
        let mut stmt = conn
            .prepare("SELECT id, content_hash, mtime, byte_size FROM memory_meta")
            .map_err(|e| MemoryIndexError::query(Path::new(""), e))?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .map_err(|e| MemoryIndexError::query(Path::new(""), e))?;

        let mut map = HashMap::new();
        for row in rows {
            let (id, hash, mtime, bytes) = row.map_err(|e| MemoryIndexError::query(Path::new(""), e))?;
            map.insert(id, (hash as u64, mtime, bytes as usize));
        }
        Ok(map)
    }

    fn create_schema(conn: &Connection) -> Result<(), MemoryIndexError> {
        conn.execute_batch(
            r#"
            CREATE TABLE schema_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            INSERT INTO schema_meta(key, value) VALUES ('schema_version', '__SCHEMA_VERSION__');

            CREATE TABLE memory_meta (
              id TEXT PRIMARY KEY,
              title TEXT NOT NULL,
              root TEXT NOT NULL,
              path TEXT NOT NULL,
              scope TEXT NOT NULL,
              kind TEXT NOT NULL,
              tags TEXT NOT NULL DEFAULT '',
              paths TEXT NOT NULL DEFAULT '',
              created TEXT NOT NULL,
              updated TEXT NOT NULL,
              source TEXT NOT NULL,
              content_hash INTEGER NOT NULL,
              mtime INTEGER NOT NULL,
              byte_size INTEGER NOT NULL,
              snippet_body TEXT NOT NULL DEFAULT ''
            );

            CREATE VIRTUAL TABLE memory_fts USING fts5(
              title, headings, tags, paths, body,
              tokenize='unicode61'
            );
            "#
            .replace("__SCHEMA_VERSION__", &INDEX_SCHEMA_VERSION.to_string())
            .as_str(),
        )
        .map_err(MemoryIndexError::schema)?;
        Ok(())
    }

    fn index_item(tx: &Connection, item: &MemoryItem) -> Result<(), MemoryIndexError> {
        let headings = extract_headings(&item.body);
        let snippet_body = &item.body;
        let mtime = file_mtime(&item.path).unwrap_or(0);
        let tags = item.tags.join(",");
        let paths = item.paths.join(",");

        // Replace any existing row so re-indexing an edited memory does not
        // accumulate stale FTS rows.
        tx.execute("DELETE FROM memory_meta WHERE id = ?1", rusqlite::params![item.id])
            .map_err(|e| MemoryIndexError::query(&item.path, e))?;

        tx.execute(
            "INSERT INTO memory_meta \
             (id, title, root, path, scope, kind, tags, paths, created, updated, source, content_hash, mtime, byte_size, snippet_body) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            rusqlite::params![
                item.id,
                item.title,
                item.root.label(),
                item.path.display().to_string(),
                item.scope.label(),
                item.kind.label(),
                tags,
                paths,
                item.created,
                item.updated,
                item.source.label(),
                item.content_hash as i64,
                mtime,
                item.byte_count as i64,
                snippet_body,
            ],
        )
        .map_err(|e| MemoryIndexError::query(&item.path, e))?;

        // The integer rowid assigned to the new memory_meta row is reused as
        // the FTS rowid so the two stay joined on a stable integer key.
        let rowid: i64 = tx
            .query_row(
                "SELECT rowid FROM memory_meta WHERE id = ?1",
                rusqlite::params![item.id],
                |row| row.get(0),
            )
            .map_err(|e| MemoryIndexError::query(&item.path, e))?;

        tx.execute(
            "INSERT INTO memory_fts(rowid, title, headings, tags, paths, body) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![rowid, item.title, headings, tags, paths, snippet_body],
        )
        .map_err(|e| MemoryIndexError::query(&item.path, e))?;

        Ok(())
    }
}

/// Resolve the derived memory cache directory: `~/.thndrs/cache/memory/`.
pub fn cache_dir() -> Option<PathBuf> {
    utils::home_dir().map(|home| home.join(".thndrs").join("cache").join(CACHE_DIR_NAME))
}

/// Compute a short hex hash of the workspace root path for project index names.
pub fn workspace_hash(workspace_root: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(workspace_root.to_string_lossy().as_bytes());
    let result = hasher.finalize();
    hex_short(&result)
}

/// File mtime in seconds since the Unix epoch, or 0 when unavailable.
fn file_mtime(path: &Path) -> Option<i64> {
    let metadata = fs::metadata(path).ok()?;
    let mtime = metadata.modified().ok()?;
    mtime.duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).ok()
}

/// Extract Markdown heading text from `body`, joined with newlines.
///
/// Used as an FTS5 column so command/section names in headings are searchable.
fn extract_headings(body: &str) -> String {
    let Ok(Node::Root(root)) = markdown::to_mdast(
        body,
        &ParseOptions {
            constructs: Constructs { frontmatter: false, ..Constructs::default() },
            ..ParseOptions::default()
        },
    ) else {
        return String::new();
    };
    let mut headings = Vec::new();
    collect_heading_text(&root.children, &mut headings);
    headings.join("\n")
}

fn collect_heading_text(nodes: &[Node], out: &mut Vec<String>) {
    for node in nodes {
        if let Node::Heading(heading) = node {
            let mut text = String::new();
            collect_text(&heading.children, &mut text);
            if !text.trim().is_empty() {
                out.push(text.trim().to_string());
            }
        }
        if let Some(children) = node.children() {
            collect_heading_text(children, out);
        }
    }
}

fn collect_text(nodes: &[Node], out: &mut String) {
    for node in nodes {
        match node {
            Node::Text(text) => out.push_str(&text.value),
            other => {
                if let Some(children) = other.children() {
                    collect_text(children, out);
                }
            }
        }
    }
}

/// First non-empty line of a body, for metadata-only result snippets.
fn first_snippet_line(body: &str) -> String {
    body.lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .to_string()
}

/// Pick which FTS column matched and a snippet for it.
///
/// FTS5 `highlight` returns empty for columns that did not contribute to the
/// match. The first non-empty highlight wins, with a preference order of
/// title, heading, tag, path, body.
fn pick_match(
    h_title: &str, h_heading: &str, h_tag: &str, h_path: &str, h_body: &str, snippet_body: &str,
) -> (MemoryMatchField, String) {
    if !h_title.trim().is_empty() {
        return (MemoryMatchField::Title, h_title.trim().to_string());
    }
    if !h_heading.trim().is_empty() {
        return (MemoryMatchField::Heading, h_heading.trim().to_string());
    }
    if !h_tag.trim().is_empty() {
        return (MemoryMatchField::Tag, h_tag.trim().to_string());
    }
    if !h_path.trim().is_empty() {
        return (MemoryMatchField::Path, h_path.trim().to_string());
    }
    if !h_body.trim().is_empty() {
        return (MemoryMatchField::Body, h_body.trim().to_string());
    }
    (MemoryMatchField::Body, first_snippet_line(snippet_body))
}

/// Sanitize a free-text query into an FTS5 query string.
///
/// Treats each whitespace-separated token as a prefix term (`token*`) and
/// ANDs them, which is the most forgiving behavior for recall. Quotes tokens
/// containing FTS5-special characters so a query like `error:E0425` does not
/// break the MATCH.
pub fn sanitize_fts_query(query: &str) -> String {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let terms: Vec<String> = trimmed
        .split_whitespace()
        .map(|term| {
            if term
                .chars()
                .any(|c| matches!(c, '"' | '*' | '(' | ')' | ':' | '^' | '-' | '+' | '.' | '/' | '\\'))
            {
                format!("\"{}\"*", term.trim_matches('"'))
            } else {
                format!("{}*", term)
            }
        })
        .collect();
    terms.join(" ")
}

fn parse_root(label: &str) -> MemoryRootKind {
    match label {
        "project" => MemoryRootKind::Project,
        _ => MemoryRootKind::User,
    }
}

fn parse_scope(label: &str) -> MemoryScope {
    match label {
        "project" => MemoryScope::Project,
        "path" => MemoryScope::Path,
        "session" => MemoryScope::Session,
        _ => MemoryScope::User,
    }
}

fn parse_kind(label: &str) -> MemoryKind {
    match label {
        "preference" => MemoryKind::Preference,
        "procedure" => MemoryKind::Procedure,
        "context" => MemoryKind::Context,
        _ => MemoryKind::Fact,
    }
}

fn hex_short(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(16);
    for byte in bytes.iter().take(8) {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{MemoryRoots, discover_memory, write_memory};
    use std::io::Write;

    fn temp_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("create temp dir")
    }

    fn write_file(root: &Path, rel: &str, content: &str) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).expect("create parent");
        let mut f = fs::File::create(&path).expect("create file");
        f.write_all(content.as_bytes()).expect("write file");
    }

    fn frontmatter(id: &str, title: &str, kind: &str, scope: &str, body: &str) -> String {
        format!(
            "---\nid: {id}\ntitle: {title}\nkind: {kind}\nscope: {scope}\ncreated: 2026-07-03T00:00:00Z\nupdated: 2026-07-03T00:00:00Z\nsource: explicit-user\n---\n\n{body}\n"
        )
    }

    /// Discover memory rooted at a temp workspace and index it into a temp DB.
    fn index_workspace(workspace: &Path, items: &[MemoryItem]) -> Connection {
        let path = workspace.join("user.db3");
        MemoryIndex::ensure_indexed(&path, items).expect("index")
    }

    fn roots_for(workspace: &Path) -> MemoryRoots {
        MemoryRoots { user: Some(workspace.join("user-memory")), project: workspace.join(".thndrs").join("memory") }
    }

    fn discover(workspace: &Path) -> Vec<MemoryItem> {
        let roots = roots_for(workspace);
        let inv = discover_memory(&roots);
        inv.all().into_iter().cloned().collect()
    }

    #[test]
    fn index_creates_schema_with_version() {
        let dir = temp_dir();
        let workspace = dir.path();
        write_file(
            &workspace.join("user-memory"),
            "core.md",
            &frontmatter("mem_core", "Core", "fact", "user", "core body"),
        );
        let items = discover(workspace);
        let conn = index_workspace(workspace, &items);

        let version: u32 = conn
            .query_row("SELECT value FROM schema_meta WHERE key='schema_version'", [], |row| {
                Ok(row.get::<_, String>(0).unwrap().parse::<u32>().unwrap())
            })
            .unwrap();
        assert_eq!(version, INDEX_SCHEMA_VERSION);

        let count: u32 = conn
            .query_row("SELECT COUNT(*) FROM memory_meta", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn fts_finds_exact_command_text() {
        let dir = temp_dir();
        let workspace = dir.path();
        write_file(
            &workspace.join("user-memory"),
            "notes/build.md",
            &frontmatter(
                "mem_build",
                "Build",
                "procedure",
                "user",
                "Run cargo test --release for unit tests",
            ),
        );
        write_file(
            &workspace.join("user-memory"),
            "notes/deploy.md",
            &frontmatter(
                "mem_deploy",
                "Deploy",
                "procedure",
                "user",
                "kubectl apply -f deployment.yaml",
            ),
        );
        let items = discover(workspace);
        let conn = index_workspace(workspace, &items);

        let results = MemoryIndex::search(&conn, "cargo test", &MemorySearchFilter::default()).expect("search");
        assert!(!results.is_empty());
        assert_eq!(results[0].id, "mem_build");
        assert!(results[0].from_fts);
    }

    #[test]
    fn fts_finds_exact_path_package_and_error_text() {
        let dir = temp_dir();
        let workspace = dir.path();
        write_file(
            &workspace.join("user-memory"),
            "notes/pkg.md",
            &frontmatter(
                "mem_pkg",
                "Pkg",
                "fact",
                "user",
                "use the serde_json crate for json parsing",
            ),
        );
        write_file(
            &workspace.join("user-memory"),
            "notes/err.md",
            &frontmatter(
                "mem_err",
                "Err",
                "fact",
                "user",
                "error: cannot find value `x` in scope E0425",
            ),
        );
        write_file(
            &workspace.join("user-memory"),
            "notes/path.md",
            &frontmatter(
                "mem_path",
                "Path",
                "fact",
                "user",
                "edit src/core/memory.rs to add a note",
            ),
        );
        let items = discover(workspace);
        let conn = index_workspace(workspace, &items);

        let pkg = MemoryIndex::search(&conn, "serde_json", &MemorySearchFilter::default()).expect("search");
        assert_eq!(pkg.len(), 1);
        assert_eq!(pkg[0].id, "mem_pkg");

        let err = MemoryIndex::search(&conn, "E0425", &MemorySearchFilter::default()).expect("search");
        assert_eq!(err.len(), 1);
        assert_eq!(err[0].id, "mem_err");

        let path = MemoryIndex::search(&conn, "memory.rs", &MemorySearchFilter::default()).expect("search");
        assert_eq!(path.len(), 1);
        assert_eq!(path[0].id, "mem_path");
    }

    #[test]
    fn metadata_filter_by_scope() {
        let dir = temp_dir();
        let workspace = dir.path();
        write_file(
            &workspace.join("user-memory"),
            "notes/a.md",
            &frontmatter("mem_a", "A", "fact", "user", "user note body"),
        );
        write_file(
            &workspace.join("user-memory"),
            "core.md",
            &frontmatter("mem_core", "Core", "fact", "user", "core body"),
        );
        let items = discover(workspace);
        let conn = index_workspace(workspace, &items);

        let filter = MemorySearchFilter { scope: Some(MemoryScope::User), ..Default::default() };
        let results = MemoryIndex::search(&conn, "note", &filter).expect("search");
        assert!(results.iter().all(|r| r.scope == MemoryScope::User));
    }

    #[test]
    fn metadata_filter_by_tag() {
        let dir = temp_dir();
        let workspace = dir.path();
        let content = "---\nid: mem_tag\ntitle: Tagged\nkind: fact\nscope: user\ntags: [rust, testing]\ncreated: 2026-07-03T00:00:00Z\nupdated: 2026-07-03T00:00:00Z\nsource: explicit-user\n---\n\ntagged note body\n";
        write_file(&workspace.join("user-memory"), "notes/tag.md", content);
        write_file(
            &workspace.join("user-memory"),
            "notes/untagged.md",
            &frontmatter("mem_untagged", "Untagged", "fact", "user", "plain body"),
        );
        let items = discover(workspace);
        let conn = index_workspace(workspace, &items);

        let filter = MemorySearchFilter { tag: Some("rust".to_string()), ..Default::default() };
        let results = MemoryIndex::search(&conn, "note", &filter).expect("search");
        let ids: Vec<&str> = results.iter().map(|r| r.id.as_str()).collect();
        assert!(ids.contains(&"mem_tag"));
        assert!(!ids.contains(&"mem_untagged"));
    }

    #[test]
    fn metadata_filter_by_kind() {
        let dir = temp_dir();
        let workspace = dir.path();
        write_file(
            &workspace.join("user-memory"),
            "notes/proc.md",
            &frontmatter("mem_proc", "Proc", "procedure", "user", "step one step two"),
        );
        write_file(
            &workspace.join("user-memory"),
            "notes/fact.md",
            &frontmatter("mem_fact", "Fact", "fact", "user", "a fact about steps"),
        );
        let items = discover(workspace);
        let conn = index_workspace(workspace, &items);

        let filter = MemorySearchFilter { kind: Some(MemoryKind::Procedure), ..Default::default() };
        let results = MemoryIndex::search(&conn, "step", &filter).expect("search");
        assert!(results.iter().all(|r| r.kind == MemoryKind::Procedure));
        assert!(results.iter().any(|r| r.id == "mem_proc"));
    }

    #[test]
    fn search_returns_snippet_and_recovery_handle() {
        let dir = temp_dir();
        let workspace = dir.path();
        write_file(
            &workspace.join("user-memory"),
            "notes/handle.md",
            &frontmatter("mem_handle", "Handle", "fact", "user", "recover me please cargo build"),
        );
        let items = discover(workspace);
        let conn = index_workspace(workspace, &items);

        let results = MemoryIndex::search(&conn, "cargo build", &MemorySearchFilter::default()).expect("search");
        assert_eq!(results.len(), 1);
        let r = &results[0];
        assert_eq!(r.id, "mem_handle");
        assert!(!r.snippet.is_empty());
        assert!(r.path.is_file(), "recovery handle path must point at the source file");
        assert!(r.from_fts);
    }

    #[test]
    fn search_returns_empty_with_no_matches() {
        let dir = temp_dir();
        let workspace = dir.path();
        write_file(
            &workspace.join("user-memory"),
            "notes/a.md",
            &frontmatter("mem_a", "A", "fact", "user", "nothing relevant here"),
        );
        let items = discover(workspace);
        let conn = index_workspace(workspace, &items);

        let results = MemoryIndex::search(&conn, "zzznomatch", &MemorySearchFilter::default()).expect("search");
        assert!(results.is_empty());
    }

    #[test]
    fn empty_query_returns_metadata_only_results() {
        let dir = temp_dir();
        let workspace = dir.path();
        write_file(
            &workspace.join("user-memory"),
            "notes/a.md",
            &frontmatter("mem_a", "A", "fact", "user", "first note"),
        );
        write_file(
            &workspace.join("user-memory"),
            "notes/b.md",
            &frontmatter("mem_b", "B", "fact", "user", "second note"),
        );
        let items = discover(workspace);
        let conn = index_workspace(workspace, &items);

        let results = MemoryIndex::search(&conn, "", &MemorySearchFilter::default()).expect("search");
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| !r.from_fts));
    }

    #[test]
    fn empty_query_with_metadata_filter_applies() {
        // Regression: metadata-only search must alias memory_meta so that
        // metadata filter clauses (m.scope = ?) resolve.
        let dir = temp_dir();
        let workspace = dir.path();
        write_file(
            &workspace.join("user-memory"),
            "notes/a.md",
            &frontmatter("mem_a", "A", "fact", "user", "first note"),
        );
        // A second item with a different kind so the kind filter excludes it.
        write_file(
            &workspace.join("user-memory"),
            "notes/b.md",
            &frontmatter("mem_b", "B", "procedure", "user", "second note"),
        );
        let items = discover(workspace);
        let conn = index_workspace(workspace, &items);

        let filter = MemorySearchFilter { kind: Some(MemoryKind::Procedure), ..Default::default() };
        let results = MemoryIndex::search(&conn, "", &filter).expect("search");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "mem_b");
        assert!(results.iter().all(|r| !r.from_fts));
    }

    #[test]
    fn stale_index_rebuilds_after_file_edit() {
        let dir = temp_dir();
        let workspace = dir.path();
        write_file(
            &workspace.join("user-memory"),
            "notes/v1.md",
            &frontmatter("mem_v1", "V1", "fact", "user", "original cargo content"),
        );
        let items = discover(workspace);
        let path = workspace.join("user.db3");

        let conn = MemoryIndex::ensure_indexed(&path, &items).expect("index v1");
        let before = MemoryIndex::search(&conn, "cargo", &MemorySearchFilter::default()).expect("search");
        assert_eq!(before.len(), 1);
        drop(conn);

        // Edit the file (changes content hash and mtime).
        write_file(
            &workspace.join("user-memory"),
            "notes/v1.md",
            &frontmatter("mem_v1", "V1", "fact", "user", "completely rewritten kafka content"),
        );
        // Sleep briefly so mtime is distinct on filesystems with coarse resolution.
        std::thread::sleep(std::time::Duration::from_millis(20));

        let items2 = discover(workspace);
        assert!(MemoryIndex::needs_rebuild(&path, &items2).expect("needs rebuild"));
        let conn2 = MemoryIndex::ensure_indexed(&path, &items2).expect("rebuild");

        let cargo = MemoryIndex::search(&conn2, "cargo", &MemorySearchFilter::default()).expect("search");
        assert!(cargo.is_empty(), "old content should be gone after rebuild");
        let kafka = MemoryIndex::search(&conn2, "kafka", &MemorySearchFilter::default()).expect("search");
        assert_eq!(kafka.len(), 1);
        assert_eq!(kafka[0].id, "mem_v1");
    }

    #[test]
    fn corrupt_index_rebuilds() {
        let dir = temp_dir();
        let workspace = dir.path();
        write_file(
            &workspace.join("user-memory"),
            "notes/a.md",
            &frontmatter("mem_a", "A", "fact", "user", "cargo build content"),
        );
        let items = discover(workspace);
        let path = workspace.join("user.db3");

        let conn = MemoryIndex::ensure_indexed(&path, &items).expect("index");
        assert!(MemoryIndex::search(&conn, "cargo", &MemorySearchFilter::default()).is_ok());
        drop(conn);

        // Corrupt the index by overwriting it with garbage.
        fs::write(&path, b"not a sqlite database").expect("corrupt");

        // Rebuilding from the same items must succeed and recover searchability.
        let conn = MemoryIndex::ensure_indexed(&path, &items).expect("rebuild after corrupt");
        let results = MemoryIndex::search(&conn, "cargo", &MemorySearchFilter::default()).expect("search");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "mem_a");
    }

    #[test]
    fn rebuild_does_not_touch_source_markdown() {
        let dir = temp_dir();
        let workspace = dir.path();
        let note_path = workspace.join("user-memory").join("notes").join("keep.md");
        write_file(
            &workspace.join("user-memory"),
            "notes/keep.md",
            &frontmatter("mem_keep", "Keep", "fact", "user", "preserve me cargo"),
        );
        let original = fs::read_to_string(&note_path).expect("read source");
        let items = discover(workspace);
        let path = workspace.join("user.db3");

        MemoryIndex::ensure_indexed(&path, &items).expect("index");
        MemoryIndex::rebuild_at(&path, &items).expect("rebuild");

        let after = fs::read_to_string(&note_path).expect("read source after");
        assert_eq!(original, after, "source Markdown must be unchanged by index rebuild");
    }

    #[test]
    fn schema_version_mismatch_triggers_rebuild() {
        let dir = temp_dir();
        let workspace = dir.path();
        write_file(
            &workspace.join("user-memory"),
            "notes/a.md",
            &frontmatter("mem_a", "A", "fact", "user", "cargo content"),
        );
        let items = discover(workspace);
        let path = workspace.join("user.db3");

        // Build, then bump the recorded schema version down to force a mismatch.
        let conn = MemoryIndex::ensure_indexed(&path, &items).expect("index");
        conn.execute("UPDATE schema_meta SET value='999' WHERE key='schema_version'", [])
            .expect("bump version");
        drop(conn);

        assert!(MemoryIndex::needs_rebuild(&path, &items).expect("needs rebuild"));

        let conn = MemoryIndex::ensure_indexed(&path, &items).expect("rebuild");
        let version: u32 = conn
            .query_row("SELECT value FROM schema_meta WHERE key='schema_version'", [], |row| {
                Ok(row.get::<_, String>(0).unwrap().parse::<u32>().unwrap())
            })
            .unwrap();
        assert_eq!(version, INDEX_SCHEMA_VERSION);
    }

    #[test]
    fn project_index_path_uses_workspace_hash() {
        let ws1 = Path::new("/repo/one");
        let ws2 = Path::new("/repo/two");
        let p1 = MemoryIndex::index_path(MemoryRootKind::Project, Some(ws1)).expect("path1");
        let p2 = MemoryIndex::index_path(MemoryRootKind::Project, Some(ws2)).expect("path2");
        assert_ne!(p1, p2);
        assert!(
            p1.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(PROJECT_INDEX_PREFIX)
        );
        assert!(p1.file_name().unwrap().to_string_lossy().ends_with(".db3"));
    }

    #[test]
    fn user_index_path_is_stable() {
        let p1 = MemoryIndex::index_path(MemoryRootKind::User, None).expect("path1");
        let p2 = MemoryIndex::index_path(MemoryRootKind::User, None).expect("path2");
        assert_eq!(p1, p2);
        assert_eq!(p1.file_name().unwrap(), USER_INDEX_FILE);
    }

    #[test]
    fn workspace_hash_is_stable_and_differs() {
        let a = workspace_hash(Path::new("/repo/one"));
        let b = workspace_hash(Path::new("/repo/one"));
        let c = workspace_hash(Path::new("/repo/two"));
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn sanitize_fts_query_handles_special_chars() {
        assert_eq!(sanitize_fts_query(""), "");
        assert_eq!(sanitize_fts_query("   "), "");
        let q = sanitize_fts_query("cargo test");
        assert!(q.contains("cargo*"));
        assert!(q.contains("test*"));
        // Special-char tokens are quoted and prefix-matched.
        let q = sanitize_fts_query("error:E0425");
        assert!(q.contains("\""));
    }

    #[test]
    fn pick_match_prefers_title_then_body() {
        let (field, _) = pick_match("Title hit", "", "", "", "", "body");
        assert_eq!(field, MemoryMatchField::Title);
        let (field, _) = pick_match("", "Heading hit", "", "", "", "body");
        assert_eq!(field, MemoryMatchField::Heading);
        let (field, _) = pick_match("", "", "tag-hit", "", "", "body");
        assert_eq!(field, MemoryMatchField::Tag);
        let (field, _) = pick_match("", "", "", "path-hit", "", "body");
        assert_eq!(field, MemoryMatchField::Path);
        let (field, _) = pick_match("", "", "", "", "body-hit", "body");
        assert_eq!(field, MemoryMatchField::Body);
    }

    #[test]
    fn extract_headings_collects_markdown_headings() {
        let body = "# Top\n\nSome text\n\n## Sub\n\nmore\n\n### Deep\n";
        let h = extract_headings(body);
        assert!(h.contains("Top"));
        assert!(h.contains("Sub"));
        assert!(h.contains("Deep"));
    }

    #[test]
    fn written_memory_is_indexable_and_searchable() {
        let dir = temp_dir();
        let workspace = dir.path();
        let roots = roots_for(workspace);
        let write = write_memory(
            &roots,
            MemoryScope::User,
            "Cargo build command",
            "use cargo build --release",
            &[],
        );
        let items = discover(workspace);

        let path = workspace.join("user.db3");
        let conn = MemoryIndex::ensure_indexed(&path, &items).expect("index");

        let results = MemoryIndex::search(&conn, "cargo build", &MemorySearchFilter::default()).expect("search");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, write.item.id);
        assert_eq!(results[0].title, "Cargo build command");
    }
}
