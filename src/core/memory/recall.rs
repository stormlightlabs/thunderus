//! Lexical memory recall: metadata + FTS5/BM25 over user and project memory.
//!
//! Recall is a read-only retrieval policy layered over [`MemoryIndex`]. It
//! searches both memory roots, orders results with core memory before
//! archival memory, applies count and byte caps, and returns a useful
//! diagnostic when nothing matches.
//!
//! Semantic vector recall is deferred (P5); the API here is shaped so an
//! embedding provider can be added without changing the recall contract.

use std::path::Path;

use crate::memory::{
    MemoryIndex, MemoryIndexError, MemoryRootKind, MemoryRoots, MemoryScope, MemorySearchFilter, MemorySearchResult,
    discover_memory,
};

/// Default maximum number of recall results.
pub const DEFAULT_RECALL_MAX_COUNT: usize = 20;

/// Default maximum total bytes of recall result snippets.
pub const DEFAULT_RECALL_MAX_BYTES: usize = 8_192;

/// A recall request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecallRequest {
    /// Free-text FTS5/BM25 query. Empty means metadata-only matches.
    pub query: String,
    /// Metadata filters (scope, kind, tag, path-prefix).
    pub filter: MemorySearchFilter,
    /// Maximum number of results to return.
    pub max_count: usize,
    /// Maximum total bytes of result snippets to return.
    pub max_bytes: usize,
}

impl RecallRequest {
    /// Build a recall request with default caps and no metadata filter.
    pub fn new(query: impl Into<String>) -> Self {
        RecallRequest {
            query: query.into(),
            filter: MemorySearchFilter::default(),
            max_count: DEFAULT_RECALL_MAX_COUNT,
            max_bytes: DEFAULT_RECALL_MAX_BYTES,
        }
    }

    /// Restrict recall to a single scope (e.g. `MemoryScope::User`).
    pub fn with_scope(mut self, scope: MemoryScope) -> Self {
        self.filter.scope = Some(scope);
        self
    }
}

/// The outcome of a recall: ordered results plus an optional diagnostic.
///
/// `diagnostic` is `Some` when no results matched, carrying a useful message
/// for the caller to surface. When results are present, `diagnostic` is
/// `None`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecallOutcome {
    pub results: Vec<RecallResult>,
    pub diagnostic: Option<String>,
}

/// A recall result: the underlying search result plus whether it is core
/// memory, used to order core memory before archival memory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecallResult {
    pub search: MemorySearchResult,
    /// Whether this result is core memory (`core.md`).
    pub is_core: bool,
}

impl RecallResult {
    /// Render a compact one-line summary for `/memory recall` output.
    pub fn summary(&self) -> String {
        let tier = if self.is_core { "core" } else { "archival" };
        format!("{}  {}  {}", tier, self.search.summary(), self.search.path.display())
    }
}

/// Recall memory across user and project roots.
///
/// `cache_dir` is where derived SQLite indexes live (see
/// [`crate::memory::cache_dir`]); pass a temp dir in tests to isolate index
/// files from the real `~/.thndrs/cache/memory/`.
///
/// Searches both indexes (when present), merges results, and orders them with
/// core memory before archival memory. Within each tier, higher BM25 score
/// wins, with a stable tie-break on id so ordering is deterministic.
///
/// Applies [`RecallRequest::max_count`] and [`RecallRequest::max_bytes`] caps
/// by total snippet bytes.
///
/// Returns a [`RecallOutcome`] whose `diagnostic` is `Some` when no results
/// matched, so the caller can surface a useful message instead of an empty
/// list.
pub fn recall(
    roots: &MemoryRoots, workspace_root: Option<&Path>, cache_dir: Option<&Path>, request: &RecallRequest,
) -> RecallOutcome {
    let inventory = discover_memory(roots);
    let user_items = items_for_root(&inventory, MemoryRootKind::User);
    let project_items = items_for_root(&inventory, MemoryRootKind::Project);

    let mut results = Vec::new();

    if let Some(conn) = open_index(MemoryRootKind::User, workspace_root, cache_dir, &user_items) {
        results.extend(search_root(&conn, &request.query, &request.filter));
    }
    if let Some(conn) = open_index(MemoryRootKind::Project, workspace_root, cache_dir, &project_items) {
        results.extend(search_root(&conn, &request.query, &request.filter));
    }

    for result in &mut results {
        result.is_core = is_core_path(&result.search.path);
    }

    order_results(&mut results);
    let capped = apply_caps(results, request.max_count, request.max_bytes);

    if capped.is_empty() {
        let scope_hint = request
            .filter
            .scope
            .map(|s| format!(" within {} scope", s.label()))
            .unwrap_or_default();
        return RecallOutcome {
            results: Vec::new(),
            diagnostic: Some(format!(
                "no memory matched {:?}{}; memory is written via /remember and indexed into a rebuildable SQLite cache",
                request.query, scope_hint
            )),
        };
    }

    RecallOutcome { results: capped, diagnostic: None }
}

/// The items belonging to a single root kind, for indexing that root.
fn items_for_root(inventory: &crate::memory::MemoryInventory, kind: MemoryRootKind) -> Vec<crate::memory::MemoryItem> {
    inventory
        .all()
        .into_iter()
        .filter(|item| item.root == kind)
        .cloned()
        .collect()
}

/// Open a root's index within `cache_dir`, returning `None` when the cache
/// dir is unavailable or the index cannot be opened.
///
/// Errors are swallowed here so one corrupt or missing root does not hide
/// matches in the other; the caller still gets whatever the other root found.
fn open_index(
    kind: MemoryRootKind, workspace_root: Option<&Path>, cache_dir: Option<&Path>, items: &[crate::memory::MemoryItem],
) -> Option<rusqlite::Connection> {
    let cache_dir = cache_dir?;
    let db_path = MemoryIndex::index_path_in(cache_dir, kind, workspace_root)?;
    MemoryIndex::ensure_indexed(&db_path, items).ok()
}

/// Search one index connection, converting errors into an empty result list
/// rather than aborting the whole recall (one corrupt root should not hide
/// matches in the other).
fn search_root(conn: &rusqlite::Connection, query: &str, filter: &MemorySearchFilter) -> Vec<RecallResult> {
    match MemoryIndex::search(conn, query, filter) {
        Ok(rows) => rows
            .into_iter()
            .map(|search| RecallResult { search, is_core: false })
            .collect(),
        Err(MemoryIndexError::Query { .. }) => Vec::new(),
        Err(_) => Vec::new(),
    }
}

/// Whether a memory source path is a `core.md` file.
fn is_core_path(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == crate::memory::CORE_FILE)
}

/// Order results
///
/// core memory before archival, then by score (higher first), then by id for a stable tie-break.
fn order_results(results: &mut [RecallResult]) {
    results.sort_by(|a, b| {
        b.is_core
            .cmp(&a.is_core)
            .then_with(|| b.search.score.cmp(&a.search.score))
            .then_with(|| b.search.from_fts.cmp(&a.search.from_fts))
            .then_with(|| a.search.id.cmp(&b.search.id))
    });
}

/// Apply count and total-byte caps.
///
/// Accumulates results until either the count cap is reached or adding the
/// next result's snippet would exceed the byte cap. The first result is
/// always included when present so a single long match is not dropped entirely.
fn apply_caps(results: Vec<RecallResult>, max_count: usize, max_bytes: usize) -> Vec<RecallResult> {
    let mut capped = Vec::with_capacity(max_count.min(results.len()));
    let mut bytes = 0usize;
    for result in results {
        if capped.len() >= max_count {
            break;
        }
        bytes += result.search.snippet.len();
        if bytes > max_bytes && !capped.is_empty() {
            break;
        }
        capped.push(result);
    }
    capped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{MemoryRoots, MemoryScope, write_memory};
    use std::io::Write;
    use std::path::PathBuf;

    fn temp_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("create temp dir")
    }

    fn write_file(root: &Path, rel: &str, content: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).expect("create parent");
        let mut f = std::fs::File::create(&path).expect("create file");
        f.write_all(content.as_bytes()).expect("write file");
    }

    fn frontmatter(id: &str, title: &str, kind: &str, scope: &str, body: &str) -> String {
        format!(
            "---\nid: {id}\ntitle: {title}\nkind: {kind}\nscope: {scope}\ncreated: 2026-07-03T00:00:00Z\nupdated: 2026-07-03T00:00:00Z\nsource: explicit-user\n---\n\n{body}\n"
        )
    }

    fn roots_for(workspace: &Path) -> MemoryRoots {
        MemoryRoots { user: Some(workspace.join("user-memory")), project: workspace.join(".thndrs").join("memory") }
    }

    /// Isolated cache dir under the temp workspace so tests do not touch the
    /// real `~/.thndrs/cache/memory/`.
    fn cache_for(workspace: &Path) -> PathBuf {
        workspace.join("cache")
    }

    #[test]
    fn recall_finds_archival_matches() {
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
                "run cargo test for unit tests",
            ),
        );
        let roots = roots_for(workspace);

        let outcome = recall(
            &roots,
            Some(workspace),
            Some(&cache_for(workspace)),
            &RecallRequest::new("cargo test"),
        );
        assert!(outcome.diagnostic.is_none());
        assert_eq!(outcome.results.len(), 1);
        assert_eq!(outcome.results[0].search.id, "mem_build");
        assert!(!outcome.results[0].is_core);
        assert!(outcome.results[0].search.from_fts);
    }

    #[test]
    fn recall_includes_core_before_archival() {
        let dir = temp_dir();
        let workspace = dir.path();
        write_file(
            &workspace.join("user-memory"),
            "core.md",
            &frontmatter("mem_core", "Core", "fact", "user", "cargo test is the test command"),
        );
        write_file(
            &workspace.join("user-memory"),
            "notes/extra.md",
            &frontmatter("mem_extra", "Extra", "fact", "user", "cargo test details here"),
        );
        let roots = roots_for(workspace);

        let outcome = recall(
            &roots,
            Some(workspace),
            Some(&cache_for(workspace)),
            &RecallRequest::new("cargo test"),
        );
        assert!(outcome.diagnostic.is_none());
        assert!(!outcome.results.is_empty());
        assert!(outcome.results[0].is_core, "core memory must precede archival");
        assert_eq!(outcome.results[0].search.id, "mem_core");
    }

    #[test]
    fn recall_returns_diagnostic_when_empty() {
        let dir = temp_dir();
        let workspace = dir.path();
        write_file(
            &workspace.join("user-memory"),
            "notes/a.md",
            &frontmatter("mem_a", "A", "fact", "user", "nothing relevant here"),
        );
        let roots = roots_for(workspace);

        let outcome = recall(
            &roots,
            Some(workspace),
            Some(&cache_for(workspace)),
            &RecallRequest::new("zzznomatch"),
        );
        assert!(outcome.results.is_empty());
        let diagnostic = outcome.diagnostic.expect("diagnostic when empty");
        assert!(diagnostic.contains("no memory matched"));
        assert!(diagnostic.contains("zzznomatch"));
    }

    #[test]
    fn recall_orders_by_score_then_tie_break() {
        let dir = temp_dir();
        let workspace = dir.path();
        write_file(
            &workspace.join("user-memory"),
            "notes/b.md",
            &frontmatter("mem_b", "B", "fact", "user", "cargo cargo cargo build"),
        );
        write_file(
            &workspace.join("user-memory"),
            "notes/a.md",
            &frontmatter("mem_a", "A", "fact", "user", "cargo build"),
        );
        let roots = roots_for(workspace);

        let outcome = recall(
            &roots,
            Some(workspace),
            Some(&cache_for(workspace)),
            &RecallRequest::new("cargo"),
        );
        assert!(outcome.diagnostic.is_none());
        assert_eq!(outcome.results.len(), 2);
        assert!(outcome.results[0].search.score >= outcome.results[1].search.score);
    }

    #[test]
    fn recall_metadata_only_matches_when_query_empty() {
        let dir = temp_dir();
        let workspace = dir.path();
        write_file(
            &workspace.join("user-memory"),
            "notes/a.md",
            &frontmatter("mem_a", "A note", "fact", "user", "body text"),
        );
        let roots = roots_for(workspace);

        let outcome = recall(
            &roots,
            Some(workspace),
            Some(&cache_for(workspace)),
            &RecallRequest::new(""),
        );
        assert!(outcome.diagnostic.is_none());
        assert_eq!(outcome.results.len(), 1);
        assert!(!outcome.results[0].search.from_fts);
    }

    #[test]
    fn recall_caps_by_count() {
        let dir = temp_dir();
        let workspace = dir.path();
        for i in 0..10 {
            write_file(
                &workspace.join("user-memory"),
                &format!("notes/n{i}.md"),
                &frontmatter(&format!("mem_{i}"), &format!("Note {i}"), "fact", "user", "cargo build"),
            );
        }
        let roots = roots_for(workspace);

        let mut request = RecallRequest::new("cargo");
        request.max_count = 3;
        let outcome = recall(&roots, Some(workspace), Some(&cache_for(workspace)), &request);
        assert!(outcome.diagnostic.is_none());
        assert!(outcome.results.len() <= 3, "count cap must limit results");
    }

    #[test]
    fn recall_caps_by_total_bytes() {
        let dir = temp_dir();
        let workspace = dir.path();
        for i in 0..6 {
            write_file(
                &workspace.join("user-memory"),
                &format!("notes/n{i}.md"),
                &frontmatter(&format!("mem_{i}"), &format!("Note {i}"), "fact", "user", "cargo build"),
            );
        }
        let roots = roots_for(workspace);

        let mut request = RecallRequest::new("cargo");
        request.max_count = 10;
        request.max_bytes = 1;
        let outcome = recall(&roots, Some(workspace), Some(&cache_for(workspace)), &request);
        assert!(outcome.diagnostic.is_none());
        assert_eq!(outcome.results.len(), 1, "byte cap must keep only the first result");
    }

    #[test]
    fn recall_searches_both_user_and_project_roots() {
        let dir = temp_dir();
        let workspace = dir.path();
        write_file(
            &workspace.join("user-memory"),
            "notes/u.md",
            &frontmatter("mem_user", "User", "fact", "user", "cargo user build"),
        );
        write_file(
            &workspace.join(".thndrs").join("memory"),
            "notes/p.md",
            &frontmatter("mem_proj", "Project", "fact", "project", "cargo project build"),
        );
        let roots = roots_for(workspace);

        let outcome = recall(
            &roots,
            Some(workspace),
            Some(&cache_for(workspace)),
            &RecallRequest::new("cargo"),
        );
        assert!(outcome.diagnostic.is_none());
        let ids: Vec<&str> = outcome.results.iter().map(|r| r.search.id.as_str()).collect();
        assert!(ids.contains(&"mem_user"));
        assert!(ids.contains(&"mem_proj"));
    }

    #[test]
    fn recall_scope_filter_restricts_results() {
        let dir = temp_dir();
        let workspace = dir.path();
        write_file(
            &workspace.join("user-memory"),
            "notes/u.md",
            &frontmatter("mem_user", "User", "fact", "user", "cargo user build"),
        );
        write_file(
            &workspace.join(".thndrs").join("memory"),
            "notes/p.md",
            &frontmatter("mem_proj", "Project", "fact", "project", "cargo project build"),
        );
        let roots = roots_for(workspace);

        let request = RecallRequest::new("cargo").with_scope(MemoryScope::Project);
        let outcome = recall(&roots, Some(workspace), Some(&cache_for(workspace)), &request);
        assert!(outcome.diagnostic.is_none());
        assert!(outcome.results.iter().all(|r| r.search.scope == MemoryScope::Project));
    }

    #[test]
    fn recall_result_summary_marks_tier() {
        let result = RecallResult {
            search: MemorySearchResult {
                id: "mem_x".to_string(),
                title: "Title".to_string(),
                root: MemoryRootKind::User,
                path: Path::new("/repo/.thndrs/memory/core.md").to_path_buf(),
                scope: MemoryScope::User,
                kind: crate::memory::MemoryKind::Fact,
                score: 5,
                matched_field: crate::memory::MemoryMatchField::Title,
                snippet: "snip".to_string(),
                from_fts: true,
            },
            is_core: true,
        };
        let s = result.summary();
        assert!(s.starts_with("core"));
        assert!(s.contains("mem_x"));
    }

    #[test]
    fn recall_with_no_memory_returns_diagnostic() {
        let dir = temp_dir();
        let workspace = dir.path();
        let roots = roots_for(workspace);

        let outcome = recall(
            &roots,
            Some(workspace),
            Some(&cache_for(workspace)),
            &RecallRequest::new("anything"),
        );
        assert!(outcome.results.is_empty());
        assert!(outcome.diagnostic.is_some());
    }

    #[test]
    fn written_memory_is_recallable() {
        let dir = temp_dir();
        let workspace = dir.path();
        let roots = roots_for(workspace);
        write_memory(
            &roots,
            MemoryScope::User,
            "Cargo workflow",
            "use cargo build --release",
            &[],
        )
        .expect("write memory");

        let outcome = recall(
            &roots,
            Some(workspace),
            Some(&cache_for(workspace)),
            &RecallRequest::new("cargo build"),
        );
        assert!(outcome.diagnostic.is_none());
        assert_eq!(outcome.results.len(), 1);
        assert_eq!(outcome.results[0].search.title, "Cargo workflow");
    }
}
