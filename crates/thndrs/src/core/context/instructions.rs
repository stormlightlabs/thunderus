//! Scoped project instructions: nested `AGENTS.md` discovery, selection, and
//! turn-boundary reload diagnostics.

use std::fs;
use std::path::{Path, PathBuf};

use super::{AGENTS_MD_SIZE_CAP, ContextSource};
use super::{hash_content, scope_depth};

/// Maximum directory depth for nested `AGENTS.md` discovery below the workspace root.
pub const MAX_DISCOVERY_DEPTH: usize = 8;

/// A diagnostic about a discovered instruction file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstructionDiagnostic {
    /// Absolute path to the file, when known.
    pub path: Option<PathBuf>,
    /// Human-readable explanation.
    pub message: String,
    /// Severity.
    pub severity: InstructionSeverity,
}

/// Severity of an [`InstructionDiagnostic`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum InstructionSeverity {
    /// Informational note (e.g. a new file appeared since the previous turn).
    Info,
    /// Recoverable issue (e.g. unreadable nested file skipped).
    Warning,
    /// Blocks correct instruction loading.
    Error,
}

impl InstructionDiagnostic {
    /// A nested file could not be read.
    pub fn unreadable(path: &Path, detail: &str) -> Self {
        InstructionDiagnostic {
            path: Some(path.to_path_buf()),
            message: format!("failed to read AGENTS.md: {detail}"),
            severity: InstructionSeverity::Warning,
        }
    }

    /// A file exceeded the size cap and was truncated.
    pub fn oversized(path: &Path, byte_count: usize) -> Self {
        InstructionDiagnostic {
            path: Some(path.to_path_buf()),
            message: format!("AGENTS.md is {byte_count} bytes, truncated to {AGENTS_MD_SIZE_CAP}"),
            severity: InstructionSeverity::Warning,
        }
    }

    /// A file's content hash changed since the previous turn.
    pub fn changed(path: &Path) -> Self {
        InstructionDiagnostic {
            path: Some(path.to_path_buf()),
            message: "AGENTS.md changed since the previous turn".to_string(),
            severity: InstructionSeverity::Warning,
        }
    }

    /// A file present in the previous turn is now missing.
    pub fn removed(path: &Path) -> Self {
        InstructionDiagnostic {
            path: Some(path.to_path_buf()),
            message: "AGENTS.md removed since the previous turn".to_string(),
            severity: InstructionSeverity::Warning,
        }
    }

    /// A new file appeared since the previous turn.
    pub fn added(path: &Path) -> Self {
        InstructionDiagnostic {
            path: Some(path.to_path_buf()),
            message: "AGENTS.md added since the previous turn".to_string(),
            severity: InstructionSeverity::Info,
        }
    }

    /// Render a compact one-line summary.
    pub fn summary(&self) -> String {
        let location = self.path.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
        format!("{}  {location}  {}", self.severity.label(), self.message)
    }
}

impl InstructionSeverity {
    pub fn label(self) -> &'static str {
        match self {
            InstructionSeverity::Info => "info",
            InstructionSeverity::Warning => "warning",
            InstructionSeverity::Error => "error",
        }
    }
}

/// Result of scoped instruction discovery.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InstructionInventory {
    /// All discovered sources: root first, then nested ordered by scope depth
    /// (shallowest first). Root content is loaded; nested content is loaded
    /// too so the ledger can render it when selected.
    pub sources: Vec<ContextSource>,
    /// Diagnostics for unreadable or oversized files.
    pub diagnostics: Vec<InstructionDiagnostic>,
}

/// Result of pure instruction selection for a turn.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InstructionSelection {
    /// Sources applicable this turn, ordered closest-first.
    ///
    /// The closest applicable source wins for paths under its scope.
    pub applicable: Vec<ContextSource>,
    /// Sources discovered but not applicable, kept as metadata so the
    /// ledger can show them as candidates/overridden.
    pub overridden: Vec<ContextSource>,
}

/// A turn-boundary snapshot of instruction file hashes for change detection.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InstructionSnapshot {
    /// `(path, content_hash)` pairs for every successfully loaded source.
    pub entries: Vec<(PathBuf, u64)>,
}

impl InstructionSnapshot {
    /// Build a snapshot from discovered sources.
    pub fn from_sources(sources: &[ContextSource]) -> Self {
        InstructionSnapshot { entries: sources.iter().map(|s| (s.path.clone(), s.content_hash)).collect() }
    }

    /// Diff against a previous snapshot, producing diagnostics for changed,
    /// removed, and added files.
    pub fn diff(&self, prev: &InstructionSnapshot) -> Vec<InstructionDiagnostic> {
        let mut diagnostics = Vec::new();
        let prev_map: std::collections::HashMap<&PathBuf, u64> = prev.entries.iter().map(|(p, h)| (p, *h)).collect();
        let cur_map: std::collections::HashMap<&PathBuf, u64> = self.entries.iter().map(|(p, h)| (p, *h)).collect();

        for (path, hash) in &self.entries {
            match prev_map.get(path) {
                Some(prev_hash) if prev_hash != hash => diagnostics.push(InstructionDiagnostic::changed(path)),
                None => diagnostics.push(InstructionDiagnostic::added(path)),
                _ => {}
            }
        }
        for (path, _) in &prev.entries {
            if !cur_map.contains_key(path) {
                diagnostics.push(InstructionDiagnostic::removed(path));
            }
        }
        diagnostics
    }
}

/// Discover root and nested `AGENTS.md` files below `workspace_root`.
///
/// Root `AGENTS.md` is always loaded when present. Nested files are discovered
/// below the root, skipping hidden, VCS, and build directories.
///
/// Each nested source is assigned a subtree scope (the relative directory path, or `"."` for root).
///
/// Unreadable nested files produce [`InstructionDiagnostic::unreadable`] warnings and are skipped.
///
/// Oversized files are truncated and produce [`InstructionDiagnostic::oversized`] warnings, matching root behavior.
pub fn discover_instructions(workspace_root: &Path) -> InstructionInventory {
    let mut sources = Vec::new();
    let mut diagnostics = Vec::new();

    if let Some(root_source) = super::load_agents_md(workspace_root) {
        if root_source.truncated {
            diagnostics.push(InstructionDiagnostic::oversized(
                &root_source.path,
                root_source.byte_count,
            ));
        }
        sources.push(root_source);
    }

    discover_nested(workspace_root, workspace_root, 0, &mut sources, &mut diagnostics);

    sources.sort_by(|a, b| {
        let da = scope_depth(&a.scope);
        let db = scope_depth(&b.scope);
        da.cmp(&db).then_with(|| a.path.cmp(&b.path))
    });

    InstructionInventory { sources, diagnostics }
}

/// Recursively discover nested `AGENTS.md` files.
fn discover_nested(
    dir: &Path, workspace_root: &Path, depth: usize, sources: &mut Vec<ContextSource>,
    diagnostics: &mut Vec<InstructionDiagnostic>,
) {
    if depth > MAX_DISCOVERY_DEPTH {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if should_skip_dir(&name) {
            continue;
        }
        let agents_md = path.join("AGENTS.md");
        if agents_md.is_file() {
            match load_nested(&agents_md, workspace_root) {
                Ok(source) => {
                    if source.truncated {
                        diagnostics.push(InstructionDiagnostic::oversized(&source.path, source.byte_count));
                    }
                    sources.push(source);
                }
                Err(detail) => diagnostics.push(InstructionDiagnostic::unreadable(&agents_md, &detail)),
            }
        }
        discover_nested(&path, workspace_root, depth + 1, sources, diagnostics);
    }
}

/// Load a nested `AGENTS.md`, assigning it a subtree scope relative to the workspace root.
fn load_nested(path: &Path, workspace_root: &Path) -> Result<ContextSource, String> {
    let metadata = fs::metadata(path).map_err(|e| e.to_string())?;
    let byte_count = metadata.len() as usize;
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let content_hash = hash_content(&content);

    let scope = path
        .parent()
        .and_then(|p| p.strip_prefix(workspace_root).ok())
        .map(|rel| rel.display().to_string())
        .unwrap_or_else(|| ".".to_string());

    let (content, truncated) = if byte_count > AGENTS_MD_SIZE_CAP {
        let mut capped = content.into_bytes();
        capped.truncate(AGENTS_MD_SIZE_CAP);
        (
            super::trim_to_char_boundary(&String::from_utf8_lossy(&capped), AGENTS_MD_SIZE_CAP),
            true,
        )
    } else {
        (content, false)
    };

    Ok(ContextSource { path: path.to_path_buf(), scope, content, content_hash, truncated, byte_count })
}

/// Select applicable instruction sources for a turn.
pub fn select_instructions(
    sources: &[ContextSource], mentioned_paths: &[PathBuf], pinned_paths: &[PathBuf],
) -> InstructionSelection {
    let mut applicable: Vec<ContextSource> = Vec::new();
    let mut overridden: Vec<ContextSource> = Vec::new();

    let references: Vec<&PathBuf> = mentioned_paths.iter().chain(pinned_paths.iter()).collect();

    for source in sources {
        let is_root = source.scope == ".";
        let applies = is_root || references.iter().any(|p| path_under_scope(p, &source.scope));

        if applies {
            applicable.push(source.clone());
        } else {
            overridden.push(source.clone());
        }
    }

    applicable.sort_by_key(|s| std::cmp::Reverse(scope_depth(&s.scope)));

    InstructionSelection { applicable, overridden }
}

/// Whether `path` falls under the subtree `scope` (a relative directory path
/// from the workspace root, or `"."` for root).
fn path_under_scope(path: &Path, scope: &str) -> bool {
    if scope == "." {
        return true;
    }

    let scope_path = Path::new(scope);
    let components: Vec<_> = path.components().collect();
    let scope_components: Vec<_> = scope_path.components().collect();
    if components.len() < scope_components.len() {
        return false;
    }
    components[..scope_components.len()] == scope_components[..]
}

/// Whether a directory entry should be skipped during nested discovery.
///
/// Skips hidden directories (leading `.`), VCS metadata, and common build
/// output directories so discovery stays fast and avoids noise.
///
/// TODO: use ignore crate?
fn should_skip_dir(name: &str) -> bool {
    name.starts_with('.') || matches!(name, "node_modules" | "target" | "dist" | "build" | "out")
}

#[cfg(test)]
mod tests {
    use super::*;
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

    #[test]
    fn discover_loads_root_agents_md() {
        let dir = temp_dir();
        write_file(dir.path(), "AGENTS.md", "# Root\n");

        let inventory = discover_instructions(dir.path());
        assert_eq!(inventory.sources.len(), 1);
        assert_eq!(inventory.sources[0].scope, ".");
        assert!(inventory.sources[0].content.contains("# Root"));
        assert!(inventory.diagnostics.is_empty());
    }

    #[test]
    fn discover_no_root_returns_empty() {
        let dir = temp_dir();
        let inventory = discover_instructions(dir.path());
        assert!(inventory.sources.is_empty());
        assert!(inventory.diagnostics.is_empty());
    }

    #[test]
    fn discover_finds_nested_agents_md_with_subtree_scope() {
        let dir = temp_dir();
        write_file(dir.path(), "AGENTS.md", "# Root\n");
        write_file(dir.path(), "src/AGENTS.md", "# Src\n");
        write_file(dir.path(), "src/core/AGENTS.md", "# Core\n");

        let inventory = discover_instructions(dir.path());
        let scopes: Vec<&str> = inventory.sources.iter().map(|s| s.scope.as_str()).collect();
        assert!(scopes.contains(&"."));
        assert!(scopes.contains(&"src"));
        assert!(scopes.contains(&"src/core"));
        assert_eq!(inventory.sources[0].scope, ".");
    }

    #[test]
    fn discover_skips_hidden_and_build_dirs() {
        let dir = temp_dir();
        write_file(dir.path(), "AGENTS.md", "# Root\n");
        write_file(dir.path(), ".hidden/AGENTS.md", "# Hidden\n");
        write_file(dir.path(), "target/AGENTS.md", "# Build\n");
        write_file(dir.path(), "node_modules/AGENTS.md", "# Deps\n");

        let inventory = discover_instructions(dir.path());
        let scopes: Vec<&str> = inventory.sources.iter().map(|s| s.scope.as_str()).collect();
        assert_eq!(scopes, vec!["."]);
    }

    #[test]
    fn discover_diagnoses_oversized_root() {
        let dir = temp_dir();
        let big = "x".repeat(AGENTS_MD_SIZE_CAP + 10);
        write_file(dir.path(), "AGENTS.md", &big);

        let inventory = discover_instructions(dir.path());
        assert_eq!(inventory.diagnostics.len(), 1);
        assert!(inventory.diagnostics[0].message.contains("truncated"));
        assert!(inventory.sources[0].truncated);
    }

    #[test]
    fn discover_diagnoses_oversized_nested() {
        let dir = temp_dir();
        write_file(dir.path(), "AGENTS.md", "# Root\n");
        let big = "x".repeat(AGENTS_MD_SIZE_CAP + 10);
        write_file(dir.path(), "src/AGENTS.md", &big);

        let inventory = discover_instructions(dir.path());
        let oversized = inventory
            .diagnostics
            .iter()
            .filter(|d| d.message.contains("truncated"))
            .count();
        assert_eq!(oversized, 1);
        let nested = inventory.sources.iter().find(|s| s.scope == "src").expect("nested src");
        assert!(nested.truncated);
    }

    #[test]
    fn discover_diagnoses_unreadable_nested() {
        let dir = temp_dir();
        write_file(dir.path(), "AGENTS.md", "# Root\n");

        let path = dir.path().join("src").join("AGENTS.md");
        fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
        let mut f = fs::File::create(&path).expect("create file");
        f.write_all(&[0xFF, 0xFE, 0x00]).expect("write invalid utf8");

        let inventory = discover_instructions(dir.path());
        let unreadable = inventory
            .diagnostics
            .iter()
            .filter(|d| d.message.contains("failed to read"))
            .count();
        assert_eq!(unreadable, 1);
    }

    #[test]
    fn select_root_always_applicable() {
        let dir = temp_dir();
        write_file(dir.path(), "AGENTS.md", "# Root\n");
        let inventory = discover_instructions(dir.path());

        let selection = select_instructions(&inventory.sources, &[], &[]);
        assert_eq!(selection.applicable.len(), 1);
        assert_eq!(selection.applicable[0].scope, ".");
        assert!(selection.overridden.is_empty());
    }

    #[test]
    fn select_nested_applicable_when_mentioned_path_under_scope() {
        let dir = temp_dir();
        write_file(dir.path(), "AGENTS.md", "# Root\n");
        write_file(dir.path(), "src/AGENTS.md", "# Src\n");
        let inventory = discover_instructions(dir.path());

        let mentioned = vec![PathBuf::from("src/main.rs")];
        let selection = select_instructions(&inventory.sources, &mentioned, &[]);
        let applicable_scopes: Vec<&str> = selection.applicable.iter().map(|s| s.scope.as_str()).collect();
        assert!(applicable_scopes.contains(&"."));
        assert!(applicable_scopes.contains(&"src"));
    }

    #[test]
    fn select_nested_not_applicable_when_path_outside_scope() {
        let dir = temp_dir();
        write_file(dir.path(), "AGENTS.md", "# Root\n");
        write_file(dir.path(), "src/AGENTS.md", "# Src\n");
        let inventory = discover_instructions(dir.path());

        let mentioned = vec![PathBuf::from("docs/readme.md")];
        let selection = select_instructions(&inventory.sources, &mentioned, &[]);
        assert!(selection.applicable.iter().any(|s| s.scope == "."));
        assert!(!selection.applicable.iter().any(|s| s.scope == "src"));
        assert!(selection.overridden.iter().any(|s| s.scope == "src"));
    }

    #[test]
    fn select_closest_wins_first_in_applicable() {
        let dir = temp_dir();
        write_file(dir.path(), "AGENTS.md", "# Root\n");
        write_file(dir.path(), "src/AGENTS.md", "# Src\n");
        write_file(dir.path(), "src/core/AGENTS.md", "# Core\n");
        let inventory = discover_instructions(dir.path());

        let mentioned = vec![PathBuf::from("src/core/context.rs")];
        let selection = select_instructions(&inventory.sources, &mentioned, &[]);
        assert_eq!(selection.applicable[0].scope, "src/core");
        assert_eq!(selection.applicable[1].scope, "src");
        assert_eq!(selection.applicable[2].scope, ".");
    }

    #[test]
    fn select_pinned_path_triggers_nested_applicability() {
        let dir = temp_dir();
        write_file(dir.path(), "AGENTS.md", "# Root\n");
        write_file(dir.path(), "src/AGENTS.md", "# Src\n");
        let inventory = discover_instructions(dir.path());

        let pinned = vec![PathBuf::from("src/lib.rs")];
        let selection = select_instructions(&inventory.sources, &[], &pinned);
        assert!(selection.applicable.iter().any(|s| s.scope == "src"));
    }

    #[test]
    fn select_preserves_root_only_behavior_when_no_nested() {
        let dir = temp_dir();
        write_file(dir.path(), "AGENTS.md", "# Root\n");
        let inventory = discover_instructions(dir.path());
        let selection = select_instructions(&inventory.sources, &[PathBuf::from("anything.rs")], &[]);
        assert_eq!(selection.applicable.len(), 1);
        assert_eq!(selection.overridden.len(), 0);
    }

    #[test]
    fn select_scope_does_not_match_prefix_only_directory() {
        let dir = temp_dir();
        write_file(dir.path(), "AGENTS.md", "# Root\n");
        write_file(dir.path(), "src/AGENTS.md", "# Src\n");
        write_file(dir.path(), "src-other/AGENTS.md", "# SrcOther\n");

        let inventory = discover_instructions(dir.path());
        let mentioned = vec![PathBuf::from("src/main.rs")];
        let selection = select_instructions(&inventory.sources, &mentioned, &[]);
        let applicable_scopes: Vec<&str> = selection.applicable.iter().map(|s| s.scope.as_str()).collect();
        assert!(applicable_scopes.contains(&"src"));
        assert!(!applicable_scopes.contains(&"src-other"));
    }

    #[test]
    fn snapshot_diff_detects_changed_hash() {
        let dir = temp_dir();
        write_file(dir.path(), "AGENTS.md", "# v1\n");
        let inv1 = discover_instructions(dir.path());
        let snap1 = InstructionSnapshot::from_sources(&inv1.sources);

        write_file(dir.path(), "AGENTS.md", "# v2\n");
        let inv2 = discover_instructions(dir.path());
        let snap2 = InstructionSnapshot::from_sources(&inv2.sources);

        let diagnostics = snap2.diff(&snap1);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("changed"));
    }

    #[test]
    fn snapshot_diff_detects_removed_file() {
        let dir = temp_dir();
        write_file(dir.path(), "AGENTS.md", "# Root\n");
        write_file(dir.path(), "src/AGENTS.md", "# Src\n");
        let inv1 = discover_instructions(dir.path());
        let snap1 = InstructionSnapshot::from_sources(&inv1.sources);

        fs::remove_file(dir.path().join("src/AGENTS.md")).expect("remove");
        let inv2 = discover_instructions(dir.path());
        let snap2 = InstructionSnapshot::from_sources(&inv2.sources);

        let diagnostics = snap2.diff(&snap1);
        let removed = diagnostics.iter().filter(|d| d.message.contains("removed")).count();
        assert_eq!(removed, 1);
    }

    #[test]
    fn snapshot_diff_detects_added_file() {
        let dir = temp_dir();
        write_file(dir.path(), "AGENTS.md", "# Root\n");
        let inv1 = discover_instructions(dir.path());
        let snap1 = InstructionSnapshot::from_sources(&inv1.sources);

        write_file(dir.path(), "src/AGENTS.md", "# Src\n");
        let inv2 = discover_instructions(dir.path());
        let snap2 = InstructionSnapshot::from_sources(&inv2.sources);

        let diagnostics = snap2.diff(&snap1);
        let added = diagnostics.iter().filter(|d| d.message.contains("added")).count();
        assert_eq!(added, 1);
    }

    #[test]
    fn snapshot_diff_empty_when_unchanged() {
        let dir = temp_dir();
        write_file(dir.path(), "AGENTS.md", "# Root\n");
        let inv = discover_instructions(dir.path());
        let snap = InstructionSnapshot::from_sources(&inv.sources);
        assert!(snap.diff(&snap).is_empty());
    }

    #[test]
    fn path_under_scope_matches_subdirectory() {
        assert!(path_under_scope(Path::new("src/main.rs"), "src"));
        assert!(path_under_scope(Path::new("src/core/context.rs"), "src"));
        assert!(!path_under_scope(Path::new("docs/readme.md"), "src"));
        assert!(path_under_scope(Path::new("anything"), "."));
    }

    #[test]
    fn diagnostic_summary_includes_path_and_message() {
        let d = InstructionDiagnostic::changed(Path::new("/repo/AGENTS.md"));
        let s = d.summary();
        assert!(s.contains("/repo/AGENTS.md"));
        assert!(s.contains("changed"));
    }
}
