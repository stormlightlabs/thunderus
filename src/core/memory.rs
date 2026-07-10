//! File-backed durable memory: discovery, validation, write, and delete.
//!
//! Memory is ordinary inspectable Markdown with YAML frontmatter. The source
//! of truth is the file tree; nothing here is invisible to the user.
//!
//! ## Roots
//!
//! - User memory: `~/.thndrs/memory/`
//!   - `core.md`: always-loaded core memory.
//!   - `notes/*.md`: archival notes discovered on demand.
//! - Project memory: `<workspace>/.thndrs/memory/`
//!   - `core.md`: always-loaded core memory.
//!   - `notes/*.md`: archival notes discovered on demand.
//!
//! Project memory is not assumed committed or gitignored. Shared team memory
//! may be committed; personal project memory belongs in Git's local exclude
//! path or in user memory.
//!
//! ## Frontmatter
//!
//! ```yaml
//! id: mem_...
//! title: Preferred testing workflow
//! kind: procedure
//! scope: user | project | path | session
//! paths: []
//! tags: []
//! created: 2026-07-03T00:00:00Z
//! updated: 2026-07-03T00:00:00Z
//! source: explicit-user
//! ```
//!
//! The body is plain Markdown.
//!
//! ## Precedence
//!
//! Memory is guidance, not permission. It cannot grant permissions, enable
//! tools, change provider/model/search settings, suppress errors, or override
//! user/system/developer instructions. User prompt and harness policy outrank
//! memory.
//!
//! ## Deletion
//!
//! `/memory forget <id>` deletes the selected memory file after confirmation
//! and appends a content-free [`MemoryDeleteRecord`] audit record. It never
//! writes a tombstone containing forgotten content, rewrites unrelated session
//! records, deletes unrelated project files, or removes session history.

#![allow(dead_code)]

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::{fs, io};

use markdown::{Constructs, ParseOptions, mdast::Node};
use serde::{Deserialize, Serialize};

use crate::tools;
use crate::utils;

/// Maximum bytes read from a memory body.
///
/// Content beyond this is truncated and the truncation is marked visibly so a
/// large note cannot exhaust context before the budget policy runs.
pub const MEMORY_BODY_SIZE_CAP: usize = 32_768;

/// Subdirectory holding archival memory notes.
pub const NOTES_DIR: &str = "notes";

/// Core memory filename, always-loaded when present.
pub const CORE_FILE: &str = "core.md";

/// File extension for memory notes.
pub const MEMORY_EXT: &str = "md";

/// A memory kind, carried forward from the old `thunderus` model.
///
/// Kind and scope are separate: a path-scoped memory can still be a
/// `Procedure`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    /// Durable facts about the codebase or domain.
    Fact,
    /// User or team workflow preferences.
    Preference,
    /// Repeatable steps that worked.
    Procedure,
    /// Conversation-derived context that should survive the current turn.
    Context,
}

impl MemoryKind {
    /// Stable lowercase label used in frontmatter, dashboards, and ids.
    pub fn label(self) -> &'static str {
        match self {
            MemoryKind::Fact => "fact",
            MemoryKind::Preference => "preference",
            MemoryKind::Procedure => "procedure",
            MemoryKind::Context => "context",
        }
    }
}

/// Ownership and lifetime scope of a memory item.
///
/// Required for every memory item so privacy, governance, and selection stay
/// explicit.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryScope {
    /// Global user memory under `~/.thndrs/memory/`.
    User,
    /// Workspace-local project memory under `.thndrs/memory/`.
    Project,
    /// Path-scoped project memory, applicable under listed `paths`.
    Path,
    /// Session-local memory durable inside the session log.
    Session,
}

impl MemoryScope {
    /// Stable lowercase label used in frontmatter, dashboards, and ids.
    pub fn label(self) -> &'static str {
        match self {
            MemoryScope::User => "user",
            MemoryScope::Project => "project",
            MemoryScope::Path => "path",
            MemoryScope::Session => "session",
        }
    }
}

/// Provenance of a memory item.
///
/// In this milestone every write is explicit; autonomous suggestions are a
/// later, confirmation-gated path.
#[allow(clippy::enum_variant_names)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MemorySource {
    /// Written by an explicit user action such as `/remember`.
    ExplicitUser,
    /// Written by an explicit user action targeting a path scope.
    ExplicitUserPath,
    /// Written by an explicit user action targeting the session scope.
    ExplicitUserSession,
}

impl MemorySource {
    /// Stable lowercase label used in frontmatter and audit records.
    pub fn label(self) -> &'static str {
        match self {
            MemorySource::ExplicitUser => "explicit-user",
            MemorySource::ExplicitUserPath => "explicit-user-path",
            MemorySource::ExplicitUserSession => "explicit-user-session",
        }
    }
}

/// Severity of a [`MemoryDiagnostic`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemorySeverity {
    /// Informational note (e.g. an empty memory root).
    Info,
    /// Recoverable issue that may degrade memory quality (e.g. malformed file skipped).
    Warning,
    /// Blocks a write or delete until resolved.
    Error,
}

impl MemorySeverity {
    pub fn label(self) -> &'static str {
        match self {
            MemorySeverity::Info => "info",
            MemorySeverity::Warning => "warning",
            MemorySeverity::Error => "error",
        }
    }
}

/// A diagnostic about memory discovery, validation, or mutation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryDiagnostic {
    /// Absolute path to the memory file, when known.
    pub path: Option<PathBuf>,
    pub severity: MemorySeverity,
    /// Short code (e.g. `"malformed_frontmatter"`, `"secret_shaped"`).
    pub code: String,
    /// Human-readable explanation.
    pub message: String,
}

impl MemoryDiagnostic {
    /// A memory file could not be read.
    pub fn unreadable(path: &Path, detail: &str) -> Self {
        Self {
            path: Some(path.to_path_buf()),
            severity: MemorySeverity::Warning,
            code: "unreadable".to_string(),
            message: format!("failed to read memory file: {detail}"),
        }
    }

    /// A memory file's frontmatter is missing or invalid.
    pub fn malformed_frontmatter(path: &Path, detail: &str) -> Self {
        Self {
            path: Some(path.to_path_buf()),
            severity: MemorySeverity::Warning,
            code: "malformed_frontmatter".to_string(),
            message: format!("malformed memory frontmatter: {detail}"),
        }
    }

    /// A memory file's body exceeded the size cap and was truncated.
    pub fn oversized(path: &Path, byte_count: usize) -> Self {
        Self {
            path: Some(path.to_path_buf()),
            severity: MemorySeverity::Warning,
            code: "oversized".to_string(),
            message: format!("memory body is {byte_count} bytes, truncated to {MEMORY_BODY_SIZE_CAP}"),
        }
    }

    /// A memory item looks secret-shaped and should be reviewed before write.
    pub fn secret_shaped(path: Option<&Path>, detail: &str) -> Self {
        Self {
            path: path.map(Path::to_path_buf),
            severity: MemorySeverity::Warning,
            code: "secret_shaped".to_string(),
            message: format!("memory content looks secret-shaped: {detail}"),
        }
    }

    /// A memory file is missing the required `id`.
    pub fn missing_id(path: &Path) -> Self {
        Self {
            path: Some(path.to_path_buf()),
            severity: MemorySeverity::Warning,
            code: "missing_id".to_string(),
            message: "memory frontmatter is missing required `id`".to_string(),
        }
    }

    /// A memory file is missing the required `title`.
    pub fn missing_title(path: &Path) -> Self {
        Self {
            path: Some(path.to_path_buf()),
            severity: MemorySeverity::Warning,
            code: "missing_title".to_string(),
            message: "memory frontmatter is missing required `title`".to_string(),
        }
    }

    /// A memory file is missing the required `kind`.
    pub fn missing_kind(path: &Path) -> Self {
        Self {
            path: Some(path.to_path_buf()),
            severity: MemorySeverity::Warning,
            code: "missing_kind".to_string(),
            message: "memory frontmatter is missing required `kind`".to_string(),
        }
    }

    /// A memory file is missing the required `scope`.
    pub fn missing_scope(path: &Path) -> Self {
        Self {
            path: Some(path.to_path_buf()),
            severity: MemorySeverity::Warning,
            code: "missing_scope".to_string(),
            message: "memory frontmatter is missing required `scope`".to_string(),
        }
    }

    /// A duplicate memory id was discovered.
    pub fn duplicate_id(path: &Path, id: &str) -> Self {
        Self {
            path: Some(path.to_path_buf()),
            severity: MemorySeverity::Warning,
            code: "duplicate_id".to_string(),
            message: format!("duplicate memory id `{id}`"),
        }
    }

    /// Render a compact one-line summary.
    pub fn summary(&self) -> String {
        let location = self.path.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
        format!("{}  {location}  {}  {}", self.severity.label(), self.code, self.message)
    }
}

/// Which memory root a file lives in.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryRootKind {
    /// `~/.thndrs/memory/`.
    User,
    /// `<workspace>/.thndrs/memory/`.
    Project,
}

impl MemoryRootKind {
    pub fn label(self) -> &'static str {
        match self {
            MemoryRootKind::User => "user",
            MemoryRootKind::Project => "project",
        }
    }

    /// The memory scope this root implies for `core.md` and rootless notes.
    pub fn default_scope(self) -> MemoryScope {
        match self {
            MemoryRootKind::User => MemoryScope::User,
            MemoryRootKind::Project => MemoryScope::Project,
        }
    }
}

/// Resolved memory roots for user and project memory.
///
/// A missing root directory is represented as `None`; discovery simply finds
/// nothing there rather than erroring.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MemoryRoots {
    /// `~/.thndrs/memory/` when the user home is known.
    pub user: Option<PathBuf>,
    /// `<workspace>/.thndrs/memory/`.
    pub project: PathBuf,
}

impl MemoryRoots {
    /// Resolve memory roots from the user home and workspace root.
    ///
    /// Roots are not required to exist on disk; discovery handles missing
    /// directories gracefully.
    pub fn resolve(workspace_root: &Path) -> Self {
        let user = utils::home_dir().map(|home| home.join(".thndrs").join("memory"));
        let project = workspace_root.join(".thndrs").join("memory");
        MemoryRoots { user, project }
    }

    /// The directory for a given root kind.
    pub fn root_dir(&self, kind: MemoryRootKind) -> Option<&Path> {
        match kind {
            MemoryRootKind::User => self.user.as_deref(),
            MemoryRootKind::Project => Some(&self.project),
        }
    }

    /// Iterate over the root kinds that exist on disk.
    pub fn existing_kinds(&self) -> Vec<MemoryRootKind> {
        let mut kinds = Vec::new();
        if let Some(user) = &self.user
            && user.is_dir()
        {
            kinds.push(MemoryRootKind::User);
        }
        if self.project.is_dir() {
            kinds.push(MemoryRootKind::Project);
        }
        kinds
    }
}

/// Memory frontmatter parsed from YAML.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct MemoryFrontmatter {
    pub id: String,
    pub title: String,
    pub kind: Option<MemoryKind>,
    pub scope: Option<MemoryScope>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    pub created: String,
    pub updated: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<MemorySource>,
}

/// A single discovered memory item: metadata plus size-capped body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryItem {
    /// Stable id (`mem_<16-hex>`).
    pub id: String,
    pub title: String,
    pub kind: MemoryKind,
    pub scope: MemoryScope,
    /// Path scopes for `MemoryScope::Path`; empty otherwise.
    pub paths: Vec<String>,
    pub tags: Vec<String>,
    pub created: String,
    pub updated: String,
    pub source: MemorySource,
    /// Which root the file lives in.
    pub root: MemoryRootKind,
    /// Absolute source path.
    pub path: PathBuf,
    /// Stable hash of the full original content (before truncation).
    pub content_hash: u64,
    /// Original byte count of the file (before truncation).
    pub byte_count: usize,
    /// Whether the body was truncated to fit [`MEMORY_BODY_SIZE_CAP`].
    pub truncated: bool,
    /// Memory body (plain Markdown, possibly truncated).
    pub body: String,
}

impl MemoryItem {
    /// Whether this item is core memory (`core.md` in a root).
    pub fn is_core(&self) -> bool {
        self.path.file_name().is_some_and(|name| name == CORE_FILE)
    }

    /// Render a compact one-line summary for `/memory` and transcript rows.
    pub fn summary(&self) -> String {
        let scope_label = match self.scope {
            MemoryScope::Path => {
                let paths = self.paths.join(",");
                format!("path[{}]", if paths.is_empty() { "*" } else { &paths })
            }
            other => other.label().to_string(),
        };
        format!(
            "{}  {}  {}  {}  {}",
            self.id,
            self.kind.label(),
            scope_label,
            self.title,
            self.path.display()
        )
    }
}

/// Result of memory discovery across roots.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MemoryInventory {
    /// Core memory items (`core.md`), user then project.
    pub core: Vec<MemoryItem>,
    /// Archival notes under `notes/*.md`, user then project.
    pub notes: Vec<MemoryItem>,
    /// Diagnostics for unreadable, malformed, oversized, or duplicate files.
    pub diagnostics: Vec<MemoryDiagnostic>,
}

impl MemoryInventory {
    /// All items, core first then archival notes.
    pub fn all(&self) -> Vec<&MemoryItem> {
        self.core.iter().chain(self.notes.iter()).collect()
    }

    /// Find an item by id across core and notes.
    pub fn find(&self, id: &str) -> Option<&MemoryItem> {
        self.all().into_iter().find(|item| item.id == id)
    }

    /// Archival notes whose path scope applies to `target`.
    ///
    /// Core memory is always applicable and not filtered here. A note with no
    /// `paths` is considered broadly applicable. Path matching is a simple
    /// prefix check on path components so it stays local and inspectable.
    pub fn notes_applicable_to(&self, target: &Path) -> Vec<&MemoryItem> {
        self.notes
            .iter()
            .filter(|note| note.scope != MemoryScope::Path || paths_cover(note.paths.as_slice(), target))
            .collect()
    }
}

/// A target identified for deletion, with audit metadata.
///
/// `/memory forget <id>` resolves to this before deleting so the caller can
/// confirm with the user and append a content-free [`MemoryDeleteRecord`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MemoryDeletion {
    /// Memory id being deleted.
    pub id: String,
    /// Title for the confirmation prompt (not persisted in audit content).
    #[serde(skip)]
    pub title: String,
    /// Absolute source path.
    pub path: PathBuf,
    /// Scope at deletion time.
    pub scope: MemoryScope,
    /// Which root the file lived in.
    pub root: MemoryRootKind,
    /// Content hash of the file at deletion time, when computed.
    pub content_hash: Option<u64>,
}

/// Content-free audit record appended after a memory deletion.
///
/// Persists memory id, source path, scope, timestamp, and content hash when
/// available. It must never include the forgotten memory body.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MemoryDeleteRecord {
    /// Memory id that was deleted.
    pub id: String,
    /// Absolute source path of the deleted file.
    pub path: String,
    /// Scope at deletion time.
    pub scope: MemoryScope,
    /// Which root the file lived in.
    pub root: MemoryRootKind,
    /// ISO 8601 UTC timestamp of the deletion.
    pub timestamp: String,
    /// Content hash of the deleted file, when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<u64>,
}

impl MemoryDeletion {
    /// Build the content-free audit record for this deletion at `timestamp`.
    ///
    /// The record omits the memory body and title by construction.
    pub fn to_audit_record(&self, timestamp: &str) -> MemoryDeleteRecord {
        MemoryDeleteRecord {
            id: self.id.clone(),
            path: self.path.display().to_string(),
            scope: self.scope,
            root: self.root,
            timestamp: timestamp.to_string(),
            content_hash: self.content_hash,
        }
    }
}

/// Outcome of a memory write for `/remember`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryWrite {
    /// The written memory item.
    pub item: MemoryItem,
    /// Secret-shaped warning, when the content matched secret heuristics.
    pub secret_warning: Option<MemoryDiagnostic>,
}

/// In-memory session-scoped memory store.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SessionMemoryStore {
    items: Vec<MemoryItem>,
}

impl SessionMemoryStore {
    /// Create an empty session memory store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Rebuild the store from session-log-derived items (on resume).
    pub fn from_items(items: Vec<MemoryItem>) -> Self {
        Self { items }
    }

    /// All session-scoped memory items.
    pub fn items(&self) -> &[MemoryItem] {
        &self.items
    }

    /// Add a session-scoped memory item.
    pub fn add(&mut self, item: MemoryItem) {
        self.items.push(item);
    }

    /// Find a session memory item by id.
    pub fn find(&self, id: &str) -> Option<&MemoryItem> {
        self.items.iter().find(|item| item.id == id)
    }

    /// Remove a session memory item by id, returning it for audit.
    ///
    /// This is the only way session memory is removed, matching the rule that
    /// `/clear`, `/clear-context`, and `/compact` must not touch it.
    pub fn forget(&mut self, id: &str) -> Option<MemoryItem> {
        let pos = self.items.iter().position(|item| item.id == id)?;
        Some(self.items.remove(pos))
    }

    /// Whether this store is untouched by working-set resets.
    ///
    /// Always true, as session memory survives `/clear`, `/clear-context`, and
    /// `/compact`.
    ///
    /// Exists as an explicit, testable contract.
    pub fn survives_working_set_reset(&self) -> bool {
        true
    }
}

/// Track an item, recording duplicate-id and oversized diagnostics when needed.
fn track(item: MemoryItem, inventory: &mut MemoryInventory, seen_ids: &mut Vec<String>) {
    if item.truncated {
        inventory
            .diagnostics
            .push(MemoryDiagnostic::oversized(&item.path, item.byte_count));
    }
    if seen_ids.contains(&item.id) {
        inventory
            .diagnostics
            .push(MemoryDiagnostic::duplicate_id(&item.path, &item.id));
    } else {
        seen_ids.push(item.id.clone());
        if item.is_core() {
            inventory.core.push(item);
        } else {
            inventory.notes.push(item);
        }
    }
}

/// Discover archival notes under a `notes/` directory.
fn discover_notes(notes_dir: &Path, kind: MemoryRootKind, inventory: &mut MemoryInventory, seen_ids: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(notes_dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    paths.sort();

    for path in paths {
        if !path.is_file() || path.extension().is_none_or(|ext| !ext.eq_ignore_ascii_case(MEMORY_EXT)) {
            continue;
        }
        match load_memory_file(&path, kind) {
            Ok(item) => track(item, inventory, seen_ids),
            Err(diagnostic) => inventory.diagnostics.push(diagnostic),
        }
    }
}

/// Discover memory across user and project roots.
///
/// Core memory (`core.md`) is loaded from each existing root. Archival notes
/// under `notes/*.md` are discovered but loaded with size-capped bodies.
///
/// Unreadable files produce [`MemoryDiagnostic::unreadable`] warnings and are
/// skipped.
///
/// Malformed frontmatter produces [`MemoryDiagnostic::malformed_frontmatter`]
/// and the file is skipped.
///
/// Oversized bodies are truncated with an [`MemoryDiagnostic::oversized`] warning.
///
/// Duplicate ids produce a [`MemoryDiagnostic::duplicate_id`] warning.
/// The later item is kept.
pub fn discover_memory(roots: &MemoryRoots) -> MemoryInventory {
    let mut inventory = MemoryInventory::default();
    let mut seen_ids: Vec<String> = Vec::new();

    for kind in roots.existing_kinds() {
        let Some(root_dir) = roots.root_dir(kind) else { continue };

        let core_path = root_dir.join(CORE_FILE);
        if core_path.is_file() {
            match load_memory_file(&core_path, kind) {
                Ok(item) => track(item, &mut inventory, &mut seen_ids),
                Err(diagnostic) => inventory.diagnostics.push(diagnostic),
            }
        }

        let notes_dir = root_dir.join(NOTES_DIR);
        if notes_dir.is_dir() {
            discover_notes(&notes_dir, kind, &mut inventory, &mut seen_ids);
        }
    }

    inventory
}

/// Load and validate a single memory file.
///
/// Reads the file, parses frontmatter, validates required fields, and applies
/// the body size cap. Returns a diagnostic on failure rather than panicking.
pub fn load_memory_file(path: &Path, root: MemoryRootKind) -> Result<MemoryItem, MemoryDiagnostic> {
    let raw = fs::read_to_string(path).map_err(|e| MemoryDiagnostic::unreadable(path, &e.to_string()))?;
    let byte_count = raw.len();
    let content_hash = tools::hash_content(&raw);

    let frontmatter = parse_frontmatter(path, &raw)?;
    let body = extract_body(&raw);

    validate_frontmatter(path, &frontmatter)?;

    let kind = frontmatter.kind.ok_or_else(|| MemoryDiagnostic::missing_kind(path))?;
    let scope = frontmatter.scope.unwrap_or_else(|| root.default_scope());
    let (body, truncated) = cap_body(body);

    Ok(MemoryItem {
        id: frontmatter.id,
        title: frontmatter.title,
        kind,
        scope,
        paths: frontmatter.paths,
        tags: frontmatter.tags,
        created: frontmatter.created,
        updated: frontmatter.updated,
        source: frontmatter.source.unwrap_or_else(|| default_source(scope)),
        root,
        path: path.to_path_buf(),
        content_hash,
        byte_count,
        truncated,
        body,
    })
}

/// Default source for a scope when frontmatter omits it.
fn default_source(scope: MemoryScope) -> MemorySource {
    match scope {
        MemoryScope::Path => MemorySource::ExplicitUserPath,
        MemoryScope::Session => MemorySource::ExplicitUserSession,
        MemoryScope::User | MemoryScope::Project => MemorySource::ExplicitUser,
    }
}

/// Validate required frontmatter fields, returning the first error.
fn validate_frontmatter(path: &Path, fm: &MemoryFrontmatter) -> Result<(), MemoryDiagnostic> {
    if fm.id.trim().is_empty() {
        return Err(MemoryDiagnostic::missing_id(path));
    }
    if fm.title.trim().is_empty() {
        return Err(MemoryDiagnostic::missing_title(path));
    }
    match fm.kind {
        Some(_) => match fm.scope {
            Some(_) => Ok(()),
            None => Err(MemoryDiagnostic::missing_scope(path)),
        },
        None => Err(MemoryDiagnostic::missing_scope(path)),
    }
}

/// Apply the body size cap, marking truncation.
fn cap_body(body: String) -> (String, bool) {
    if body.len() <= MEMORY_BODY_SIZE_CAP {
        (body, false)
    } else {
        let mut capped = body.into_bytes();
        capped.truncate(MEMORY_BODY_SIZE_CAP);
        (
            trim_to_char_boundary(&String::from_utf8_lossy(&capped), MEMORY_BODY_SIZE_CAP),
            true,
        )
    }
}

/// Parse memory YAML frontmatter using the same Markdown AST approach as skills.
fn parse_frontmatter(path: &Path, raw: &str) -> Result<MemoryFrontmatter, MemoryDiagnostic> {
    let yaml = match markdown::to_mdast(raw, &frontmatter_parse_options()) {
        Ok(Node::Root(root)) => root.children.into_iter().find_map(|node| match node {
            Node::Yaml(yaml) => Some(yaml.value),
            _ => None,
        }),
        _ => None,
    };

    match yaml {
        Some(yaml) => serde_yaml_ng::from_str::<MemoryFrontmatter>(&yaml)
            .map_err(|err| MemoryDiagnostic::malformed_frontmatter(path, &err.to_string())),
        None => Err(MemoryDiagnostic::malformed_frontmatter(
            path,
            "missing YAML frontmatter",
        )),
    }
}

fn frontmatter_parse_options() -> ParseOptions {
    ParseOptions { constructs: Constructs { frontmatter: true, ..Constructs::default() }, ..ParseOptions::default() }
}

/// Extract the Markdown body (everything after the YAML frontmatter block).
fn extract_body(raw: &str) -> String {
    let trimmed_start = raw.strip_prefix("---\n").or_else(|| raw.strip_prefix("---\r\n"));
    match trimmed_start {
        Some(after_fence) => match after_fence.find("\n---\n").or_else(|| after_fence.find("\n---\r\n")) {
            Some(end) => after_fence[end + "\n---\n".len().saturating_sub(1)..]
                .trim_start_matches(['\n', '\r'])
                .to_string(),
            None => after_fence.to_string(),
        },
        None => raw.to_string(),
    }
}

/// Trim a string to at most `max_bytes` bytes on a UTF-8 char boundary.
fn trim_to_char_boundary(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

/// Write an explicit scoped memory item for `/remember`.
///
/// Generates a stable id, frontmatter, and a Markdown body. Creates the
/// destination directory and file. Returns a secret-shaped warning when the
/// content matches secret heuristics, without blocking the write.
///
/// `scope` must be explicit: `User`, `Project`, `Path`, or `Session`. A
/// session-scoped write returns an item with no on-disk path and does not
/// write a file; the caller persists it through the session log.
pub fn write_memory(roots: &MemoryRoots, scope: MemoryScope, title: &str, body: &str, paths: &[String]) -> MemoryWrite {
    let timestamp = utils::datetime::now_iso8601();
    let id = memory_id(scope, title, body);

    let secret_warning = if looks_secret_shaped(body) {
        Some(MemoryDiagnostic::secret_shaped(
            None,
            "memory body matches secret heuristics",
        ))
    } else {
        None
    };

    let source = default_source(scope);

    match scope {
        MemoryScope::Session => MemoryWrite {
            item: MemoryItem {
                id,
                title: title.to_string(),
                kind: MemoryKind::Context,
                scope,
                paths: Vec::new(),
                tags: Vec::new(),
                created: timestamp.clone(),
                updated: timestamp,
                source,
                root: MemoryRootKind::User,
                path: PathBuf::new(),
                content_hash: tools::hash_content(body),
                byte_count: body.len(),
                truncated: false,
                body: body.to_string(),
            },
            secret_warning,
        },
        MemoryScope::User | MemoryScope::Project | MemoryScope::Path => {
            let (root_kind, dir) = write_destination(roots, scope, paths);
            let filename = format!("{id}.{}", MEMORY_EXT);
            let path = dir.join(&filename);
            let frontmatter = MemoryFrontmatter {
                id: id.clone(),
                title: title.to_string(),
                kind: Some(MemoryKind::Context),
                scope: Some(scope),
                paths: paths.to_vec(),
                tags: Vec::new(),
                created: timestamp.clone(),
                updated: timestamp,
                source: Some(source),
            };
            let content = render_memory_file(&frontmatter, body);
            fs::create_dir_all(&dir).ok();
            fs::write(&path, &content).ok();

            let (capped_body, truncated) = cap_body(body.to_string());
            MemoryWrite {
                item: MemoryItem {
                    id,
                    title: title.to_string(),
                    kind: MemoryKind::Context,
                    scope,
                    paths: paths.to_vec(),
                    tags: Vec::new(),
                    created: frontmatter.created.clone(),
                    updated: frontmatter.updated,
                    source,
                    root: root_kind,
                    path,
                    content_hash: tools::hash_content(&content),
                    byte_count: content.len(),
                    truncated,
                    body: capped_body,
                },
                secret_warning,
            }
        }
    }
}

/// Resolve the destination root kind and directory for a scoped write.
fn write_destination(roots: &MemoryRoots, scope: MemoryScope, _: &[String]) -> (MemoryRootKind, PathBuf) {
    match scope {
        MemoryScope::User => {
            let dir = roots.user.clone().unwrap_or_else(|| {
                utils::home_dir()
                    .map(|h| h.join(".thndrs").join("memory"))
                    .unwrap_or_else(|| PathBuf::from(".thndrs").join("memory"))
            });
            (MemoryRootKind::User, dir.join(NOTES_DIR))
        }
        MemoryScope::Project => (MemoryRootKind::Project, roots.project.join(NOTES_DIR)),
        MemoryScope::Path => (MemoryRootKind::Project, roots.project.join(NOTES_DIR)),
        MemoryScope::Session => (MemoryRootKind::User, PathBuf::new()),
    }
}

/// Render a memory file (frontmatter + body) as Markdown.
pub fn render_memory_file(frontmatter: &MemoryFrontmatter, body: &str) -> String {
    let yaml = serde_yaml_ng::to_string(frontmatter).unwrap_or_default();
    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(yaml.trim_end_matches('\n'));
    out.push_str("\n---\n\n");
    out.push_str(body);
    if !body.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Resolve a memory item by id for `/memory forget`.
///
/// Returns `None` when the id cannot be identified from any discovered item.
/// The caller confirms with the user, then calls [`delete_memory`].
pub fn resolve_for_forget(roots: &MemoryRoots, id: &str) -> Option<MemoryDeletion> {
    let inventory = discover_memory(roots);
    let item = inventory.find(id)?;
    Some(MemoryDeletion {
        id: item.id.clone(),
        title: item.title.clone(),
        path: item.path.clone(),
        scope: item.scope,
        root: item.root,
        content_hash: Some(item.content_hash),
    })
}

/// Delete a memory file after confirmation.
///
/// Deletes only the target file. Never writes a tombstone, never deletes
/// unrelated files, and never touches session history. Returns the deletion
/// audit metadata so the caller can append a [`MemoryDeleteRecord`].
///
/// Returns an error diagnostic when the target path is empty (e.g. a
/// session-scoped item, which is deleted through the session store instead) or
/// when the file is already missing but the id could not be identified.
pub fn delete_memory(target: &MemoryDeletion) -> Result<MemoryDeletion, MemoryDiagnostic> {
    if target.path.as_os_str().is_empty() {
        return Err(MemoryDiagnostic {
            path: None,
            severity: MemorySeverity::Error,
            code: "not_file_backed".to_string(),
            message: "memory item is not file-backed; use the session store to forget session memory".to_string(),
        });
    }

    match fs::remove_file(&target.path) {
        Ok(()) => Ok(target.clone()),
        Err(e) => match e.kind() {
            io::ErrorKind::NotFound => Err(MemoryDiagnostic {
                path: Some(target.path.clone()),
                severity: MemorySeverity::Warning,
                code: "already_missing".to_string(),
                message: "memory file already missing; appending audit record only".to_string(),
            }),
            _ => Err(MemoryDiagnostic::unreadable(&target.path, &e.to_string())),
        },
    }
}

/// Generate a stable memory id: `mem_<16-hex>` derived from scope, title, and body.
///
/// Stable across calls for the same content so re-writing does not fork ids.
pub fn memory_id(scope: MemoryScope, title: &str, body: &str) -> String {
    let mut hasher = DefaultHasher::new();
    scope.label().hash(&mut hasher);
    title.hash(&mut hasher);
    body.hash(&mut hasher);
    format!("mem_{:016x}", hasher.finish())
}

/// Heuristic check for secret-shaped memory content.
///
/// Matches common secret indicators: long base64/hex runs, `api_key`/`token`/
/// `secret`/`password` labels with assignment, and `-----BEGIN ... PRIVATE KEY-----`.
pub fn looks_secret_shaped(content: &str) -> bool {
    let lower = content.to_lowercase();
    if lower.contains("-----begin") && lower.contains("private key-----") {
        return true;
    }
    for marker in ["api_key", "apikey", "token", "secret", "password", "passwd", "bearer"] {
        if let Some(pos) = lower.find(marker) {
            let after = &content[pos + marker.len()..];
            if after.trim_start().starts_with(['=', ':']) {
                return true;
            }
        }
    }

    for token in content.split_whitespace() {
        let stripped = token.trim_matches(|c: char| !c.is_alphanumeric());
        if stripped.len() >= 40 && stripped.chars().all(|c| c.is_ascii_hexdigit()) {
            return true;
        }
        if stripped.len() >= 32
            && stripped
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '=')
            && stripped.chars().any(|c| c.is_ascii_uppercase())
            && stripped.chars().any(|c| c.is_ascii_lowercase())
            && stripped.chars().any(|c| c.is_ascii_digit())
        {
            return true;
        }
    }
    false
}

/// Whether a path scope covers `target`.
///
/// A path scope covers the target when the target's components begin with the
/// scope's components. Empty paths cover everything.
fn paths_cover(paths: &[String], target: &Path) -> bool {
    if paths.is_empty() {
        return true;
    }
    let target_components: Vec<_> = target.components().collect();
    paths.iter().any(|scope| {
        let scope_path = Path::new(scope);
        let scope_components: Vec<_> = scope_path.components().collect();
        if target_components.len() < scope_components.len() {
            return false;
        }
        target_components[..scope_components.len()] == scope_components[..]
    })
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

    fn frontmatter(id: &str, title: &str, kind: &str, scope: &str) -> String {
        format!(
            "---\nid: {id}\ntitle: {title}\nkind: {kind}\nscope: {scope}\ncreated: 2026-07-03T00:00:00Z\nupdated: 2026-07-03T00:00:00Z\nsource: explicit-user\n---\n\nbody text\n"
        )
    }

    fn roots_for(workspace: &Path) -> MemoryRoots {
        MemoryRoots { user: Some(workspace.join("user-memory")), project: workspace.join(".thndrs").join("memory") }
    }

    #[test]
    fn discover_user_core_memory() {
        let dir = temp_dir();
        let roots = roots_for(dir.path());
        write_file(
            roots.user.as_ref().unwrap(),
            CORE_FILE,
            &frontmatter("mem_user_core", "User core", "fact", "user"),
        );

        let inv = discover_memory(&roots);
        assert_eq!(inv.core.len(), 1);
        assert_eq!(inv.core[0].id, "mem_user_core");
        assert_eq!(inv.core[0].scope, MemoryScope::User);
        assert!(inv.core[0].is_core());
        assert!(inv.diagnostics.is_empty());
    }

    #[test]
    fn discover_project_core_memory() {
        let dir = temp_dir();
        let roots = roots_for(dir.path());
        write_file(
            &roots.project,
            CORE_FILE,
            &frontmatter("mem_proj_core", "Project core", "fact", "project"),
        );

        let inv = discover_memory(&roots);
        assert_eq!(inv.core.len(), 1);
        assert_eq!(inv.core[0].scope, MemoryScope::Project);
    }

    #[test]
    fn discover_archival_notes() {
        let dir = temp_dir();
        let roots = roots_for(dir.path());
        write_file(
            &roots.project,
            "notes/build.md",
            &frontmatter("mem_build", "Build steps", "procedure", "project"),
        );
        write_file(
            &roots.project,
            "notes/test.md",
            &frontmatter("mem_test", "Test steps", "procedure", "project"),
        );

        let inv = discover_memory(&roots);
        assert_eq!(inv.core.len(), 0);
        assert_eq!(inv.notes.len(), 2);
        let ids: Vec<&str> = inv.notes.iter().map(|n| n.id.as_str()).collect();
        assert!(ids.contains(&"mem_build"));
        assert!(ids.contains(&"mem_test"));
    }

    #[test]
    fn discover_skips_non_markdown_files() {
        let dir = temp_dir();
        let roots = roots_for(dir.path());
        write_file(&roots.project, "notes/readme.txt", "not memory");
        write_file(
            &roots.project,
            "notes/real.md",
            &frontmatter("mem_real", "Real", "fact", "project"),
        );

        let inv = discover_memory(&roots);
        assert_eq!(inv.notes.len(), 1);
        assert_eq!(inv.notes[0].id, "mem_real");
    }

    #[test]
    fn malformed_frontmatter_diagnostic_skips_file() {
        let dir = temp_dir();
        let roots = roots_for(dir.path());
        write_file(
            &roots.project,
            "notes/bad.md",
            "---\nid: mem_bad\ntitle: Bad\nkind: not-a-kind\nscope: project\n---\n\nbody\n",
        );

        let inv = discover_memory(&roots);
        assert!(inv.notes.is_empty());
        assert_eq!(inv.diagnostics.len(), 1);
        assert_eq!(inv.diagnostics[0].code, "malformed_frontmatter");
    }

    #[test]
    fn missing_required_fields_produce_diagnostics() {
        let dir = temp_dir();
        let roots = roots_for(dir.path());
        write_file(
            &roots.project,
            "notes/empty.md",
            "---\nkind: fact\nscope: project\n---\n\nbody\n",
        );

        let inv = discover_memory(&roots);
        assert!(inv.notes.is_empty());
        assert_eq!(inv.diagnostics.len(), 1);
        assert_eq!(inv.diagnostics[0].code, "missing_id");
    }

    #[test]
    fn unreadable_file_diagnostic() {
        let dir = temp_dir();
        let roots = roots_for(dir.path());
        let path = roots.project.join("notes").join("bad.md");
        fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
        let mut f = fs::File::create(&path).expect("create file");
        f.write_all(&[0xFF, 0xFE, 0x00]).expect("write invalid utf8");

        let inv = discover_memory(&roots);
        assert!(inv.notes.is_empty());
        assert_eq!(inv.diagnostics.len(), 1);
        assert_eq!(inv.diagnostics[0].code, "unreadable");
    }

    #[test]
    fn oversized_body_truncated() {
        let dir = temp_dir();
        let roots = roots_for(dir.path());
        let big_body = "x".repeat(MEMORY_BODY_SIZE_CAP + 1000);
        let content = format!(
            "---\nid: mem_big\ntitle: Big\nkind: fact\nscope: project\ncreated: 2026-07-03T00:00:00Z\nupdated: 2026-07-03T00:00:00Z\n---\n\n{}\n",
            big_body
        );
        write_file(&roots.project, "notes/big.md", &content);

        let inv = discover_memory(&roots);
        assert_eq!(inv.notes.len(), 1);
        assert!(inv.notes[0].truncated);
        assert!(inv.notes[0].body.len() <= MEMORY_BODY_SIZE_CAP);
        let oversized = inv.diagnostics.iter().find(|d| d.code == "oversized");
        assert!(oversized.is_some());
    }

    #[test]
    fn duplicate_id_diagnostic() {
        let dir = temp_dir();
        let roots = roots_for(dir.path());
        write_file(
            &roots.project,
            "notes/a.md",
            &frontmatter("mem_dup", "A", "fact", "project"),
        );
        write_file(
            &roots.project,
            "notes/b.md",
            &frontmatter("mem_dup", "B", "fact", "project"),
        );

        let inv = discover_memory(&roots);
        assert_eq!(inv.notes.len(), 1);
        let dup = inv.diagnostics.iter().find(|d| d.code == "duplicate_id");
        assert!(dup.is_some());
    }

    #[test]
    fn write_user_memory_creates_valid_markdown() {
        let dir = temp_dir();
        let roots = roots_for(dir.path());

        let write = write_memory(&roots, MemoryScope::User, "Preferred test command", "cargo test", &[]);
        assert!(write.item.id.starts_with("mem_"));
        assert_eq!(write.item.scope, MemoryScope::User);
        assert!(write.item.path.is_file());

        let inv = discover_memory(&roots);
        assert_eq!(inv.notes.len(), 1);
        assert_eq!(inv.notes[0].title, "Preferred test command");
        assert!(inv.notes[0].body.contains("cargo test"));
    }

    #[test]
    fn write_project_memory() {
        let dir = temp_dir();
        let roots = roots_for(dir.path());

        let write = write_memory(&roots, MemoryScope::Project, "Build with cargo", "cargo build", &[]);
        assert_eq!(write.item.scope, MemoryScope::Project);
        assert!(write.item.path.starts_with(&roots.project));

        let inv = discover_memory(&roots);
        assert_eq!(inv.notes.len(), 1);
    }

    #[test]
    fn write_path_scoped_memory() {
        let dir = temp_dir();
        let roots = roots_for(dir.path());

        let write = write_memory(
            &roots,
            MemoryScope::Path,
            "src convention",
            "use modules",
            &["src".to_string()],
        );
        assert_eq!(write.item.scope, MemoryScope::Path);
        assert_eq!(write.item.paths, vec!["src".to_string()]);

        let inv = discover_memory(&roots);
        let note = inv.notes.first().expect("note written");
        assert_eq!(note.scope, MemoryScope::Path);
        assert_eq!(note.paths, vec!["src".to_string()]);
    }

    #[test]
    fn write_session_memory_is_not_file_backed() {
        let dir = temp_dir();
        let roots = roots_for(dir.path());

        let write = write_memory(&roots, MemoryScope::Session, "temp note", "remember this", &[]);
        assert_eq!(write.item.scope, MemoryScope::Session);
        assert!(write.item.path.as_os_str().is_empty());
        assert!(!write.item.path.is_file());
        assert_eq!(write.item.source, MemorySource::ExplicitUserSession);
    }

    #[test]
    fn memory_id_is_stable() {
        let a = memory_id(MemoryScope::User, "title", "body");
        let b = memory_id(MemoryScope::User, "title", "body");
        assert_eq!(a, b);
        assert!(a.starts_with("mem_"));
    }

    #[test]
    fn memory_id_differs_by_scope() {
        let user = memory_id(MemoryScope::User, "title", "body");
        let project = memory_id(MemoryScope::Project, "title", "body");
        assert_ne!(user, project);
    }

    #[test]
    fn delete_memory_removes_file() {
        let dir = temp_dir();
        let roots = roots_for(dir.path());
        let write = write_memory(&roots, MemoryScope::Project, "To delete", "bye", &[]);

        let target = resolve_for_forget(&roots, &write.item.id).expect("resolve");
        assert_eq!(target.id, write.item.id);

        let deleted = delete_memory(&target).expect("delete");
        assert!(deleted.id == write.item.id);
        assert!(!write.item.path.is_file());
    }

    #[test]
    fn delete_audit_record_is_content_free() {
        let dir = temp_dir();
        let roots = roots_for(dir.path());
        let write = write_memory(&roots, MemoryScope::Project, "To delete", "secret-ish body", &[]);

        let target = resolve_for_forget(&roots, &write.item.id).expect("resolve");
        delete_memory(&target).expect("delete");

        let record = target.to_audit_record("2026-07-03T00:00:00Z");
        let json = serde_json::to_string(&record).expect("serialize");
        assert!(json.contains("\"id\""));
        assert!(json.contains("\"path\""));
        assert!(json.contains("\"scope\""));
        assert!(json.contains("\"timestamp\""));
        assert!(!json.contains("secret-ish body"));
        assert!(!json.contains("To delete"));
    }

    #[test]
    fn delete_memory_fails_safely_when_missing() {
        let dir = temp_dir();
        let roots = roots_for(dir.path());
        let write = write_memory(&roots, MemoryScope::Project, "Gone", "body", &[]);
        fs::remove_file(&write.item.path).expect("remove first");

        let target = resolve_for_forget(&roots, &write.item.id);
        assert!(target.is_none());
    }

    #[test]
    fn delete_memory_appends_audit_only_when_identifiable_but_missing() {
        let dir = temp_dir();
        let roots = roots_for(dir.path());
        let write = write_memory(&roots, MemoryScope::Project, "Gone", "body", &[]);
        let target = MemoryDeletion {
            id: write.item.id.clone(),
            title: write.item.title.clone(),
            path: write.item.path.clone(),
            scope: write.item.scope,
            root: write.item.root,
            content_hash: Some(write.item.content_hash),
        };
        fs::remove_file(&write.item.path).expect("remove first");

        let result = delete_memory(&target);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert_eq!(err.code, "already_missing");
        assert_eq!(err.severity, MemorySeverity::Warning);
    }

    #[test]
    fn delete_memory_never_writes_tombstone() {
        let dir = temp_dir();
        let roots = roots_for(dir.path());
        let write = write_memory(&roots, MemoryScope::Project, "Tomb", "body", &[]);
        let target = resolve_for_forget(&roots, &write.item.id).expect("resolve");
        delete_memory(&target).expect("delete");

        assert!(!write.item.path.is_file());
    }

    #[test]
    fn delete_memory_does_not_delete_unrelated_files() {
        let dir = temp_dir();
        let roots = roots_for(dir.path());
        let keep = write_memory(&roots, MemoryScope::Project, "Keep", "body", &[]);
        let drop = write_memory(&roots, MemoryScope::Project, "Drop", "body", &[]);

        let target = resolve_for_forget(&roots, &drop.item.id).expect("resolve");
        delete_memory(&target).expect("delete");

        assert!(keep.item.path.is_file(), "unrelated memory must survive");
        assert!(!drop.item.path.is_file());
    }

    #[test]
    fn session_memory_survives_compact_and_clear() {
        let mut store = SessionMemoryStore::new();
        let item = MemoryItem {
            id: "mem_session_1".to_string(),
            title: "session note".to_string(),
            kind: MemoryKind::Context,
            scope: MemoryScope::Session,
            paths: Vec::new(),
            tags: Vec::new(),
            created: "2026-07-03T00:00:00Z".to_string(),
            updated: "2026-07-03T00:00:00Z".to_string(),
            source: MemorySource::ExplicitUserSession,
            root: MemoryRootKind::User,
            path: PathBuf::new(),
            content_hash: 1,
            byte_count: 4,
            truncated: false,
            body: "keep".to_string(),
        };
        store.add(item);

        assert!(store.survives_working_set_reset());
        assert_eq!(store.items().len(), 1);

        let forgotten = store.forget("mem_session_1");
        assert!(forgotten.is_some());
        assert!(store.items().is_empty());
    }

    #[test]
    fn session_memory_survives_resume() {
        let item = MemoryItem {
            id: "mem_session_resume".to_string(),
            title: "resume".to_string(),
            kind: MemoryKind::Context,
            scope: MemoryScope::Session,
            paths: Vec::new(),
            tags: Vec::new(),
            created: "2026-07-03T00:00:00Z".to_string(),
            updated: "2026-07-03T00:00:00Z".to_string(),
            source: MemorySource::ExplicitUserSession,
            root: MemoryRootKind::User,
            path: PathBuf::new(),
            content_hash: 2,
            byte_count: 3,
            truncated: false,
            body: "r".to_string(),
        };
        let store = SessionMemoryStore::from_items(vec![item]);
        assert_eq!(store.items().len(), 1);
        assert!(store.find("mem_session_resume").is_some());
    }

    #[test]
    fn secret_shaped_warning_before_write() {
        let dir = temp_dir();
        let roots = roots_for(dir.path());
        let write = write_memory(&roots, MemoryScope::User, "creds", "api_key=abc123", &[]);
        assert!(write.secret_warning.is_some());
        assert_eq!(write.secret_warning.unwrap().code, "secret_shaped");
    }

    #[test]
    fn secret_shaped_detects_private_key() {
        assert!(looks_secret_shaped("-----BEGIN RSA PRIVATE KEY-----\nMIIE..."));
    }

    #[test]
    fn secret_shaped_detects_long_hex_token() {
        assert!(looks_secret_shaped("token: deadbeefdeadbeefdeadbeefdeadbeefdeadbeef"));
    }

    #[test]
    fn secret_shaped_does_not_flag_normal_text() {
        assert!(!looks_secret_shaped("Prefer cargo test for unit tests."));
        assert!(!looks_secret_shaped("The build uses cargo build --release."));
    }

    #[test]
    fn memory_files_are_ordinary_markdown() {
        let dir = temp_dir();
        let roots = roots_for(dir.path());
        let write = write_memory(&roots, MemoryScope::User, "Ordinary", "Just text", &[]);

        let raw = fs::read_to_string(&write.item.path).expect("read back");
        assert!(raw.starts_with("---\n"));
        assert!(raw.contains("id: "));
        assert!(raw.contains("title: Ordinary"));
        assert!(raw.contains("kind: context"));
        assert!(raw.contains("scope: user"));
        assert!(raw.contains("\n---\n"));
        assert!(raw.contains("Just text"));
    }

    #[test]
    fn path_scoped_notes_selected_by_target() {
        let dir = temp_dir();
        let roots = roots_for(dir.path());
        write_file(
            &roots.project,
            "notes/src.md",
            &frontmatter_with_paths("mem_src", "src note", "fact", "path", &["src"]),
        );
        write_file(
            &roots.project,
            "notes/docs.md",
            &frontmatter_with_paths("mem_docs", "docs note", "fact", "path", &["docs"]),
        );

        let inv = discover_memory(&roots);
        let applicable = inv.notes_applicable_to(Path::new("src/main.rs"));
        let ids: Vec<&str> = applicable.iter().map(|n| n.id.as_str()).collect();
        assert!(ids.contains(&"mem_src"));
        assert!(!ids.contains(&"mem_docs"));
    }

    #[test]
    fn path_scoped_note_with_no_paths_is_broadly_applicable() {
        let dir = temp_dir();
        let roots = roots_for(dir.path());
        write_file(
            &roots.project,
            "notes/broad.md",
            &frontmatter("mem_broad", "broad note", "fact", "path"),
        );

        let inv = discover_memory(&roots);
        let applicable = inv.notes_applicable_to(Path::new("anywhere/x.rs"));
        assert_eq!(applicable.len(), 1);
    }

    #[test]
    fn inventory_find_by_id() {
        let dir = temp_dir();
        let roots = roots_for(dir.path());
        write_file(
            &roots.project,
            "notes/a.md",
            &frontmatter("mem_find", "Find", "fact", "project"),
        );

        let inv = discover_memory(&roots);
        assert!(inv.find("mem_find").is_some());
        assert!(inv.find("missing").is_none());
    }

    #[test]
    fn discover_handles_missing_roots() {
        let dir = temp_dir();
        let roots = MemoryRoots { user: None, project: dir.path().join("nonexistent").join("memory") };
        let inv = discover_memory(&roots);
        assert!(inv.core.is_empty());
        assert!(inv.notes.is_empty());
        assert!(inv.diagnostics.is_empty());
    }

    #[test]
    fn roots_resolve_from_workspace() {
        let workspace = Path::new("/repo");
        let roots = MemoryRoots::resolve(workspace);
        assert_eq!(roots.project, Path::new("/repo/.thndrs/memory"));

        if let Some(user) = &roots.user {
            assert!(user.ends_with(".thndrs/memory"));
        }
    }

    #[test]
    fn kind_and_scope_labels() {
        assert_eq!(MemoryKind::Fact.label(), "fact");
        assert_eq!(MemoryKind::Procedure.label(), "procedure");
        assert_eq!(MemoryScope::User.label(), "user");
        assert_eq!(MemoryScope::Path.label(), "path");
        assert_eq!(MemoryScope::Session.label(), "session");
    }

    #[test]
    fn diagnostic_summary_includes_path_and_code() {
        let d = MemoryDiagnostic::malformed_frontmatter(Path::new("/repo/m.md"), "bad yaml");
        let s = d.summary();
        assert!(s.contains("/repo/m.md"));
        assert!(s.contains("malformed_frontmatter"));
        assert!(s.contains("warning"));
    }

    #[test]
    fn render_memory_file_round_trips() {
        let fm = MemoryFrontmatter {
            id: "mem_rt".to_string(),
            title: "Round trip".to_string(),
            kind: Some(MemoryKind::Procedure),
            scope: Some(MemoryScope::Project),
            paths: Vec::new(),
            tags: Vec::new(),
            created: "2026-07-03T00:00:00Z".to_string(),
            updated: "2026-07-03T00:00:00Z".to_string(),
            source: Some(MemorySource::ExplicitUser),
        };
        let rendered = render_memory_file(&fm, "do the thing");
        let dir = temp_dir();
        let roots = roots_for(dir.path());
        write_file(&roots.project, "notes/rt.md", &rendered);

        let inv = discover_memory(&roots);
        assert_eq!(inv.notes.len(), 1);
        assert_eq!(inv.notes[0].id, "mem_rt");
        assert_eq!(inv.notes[0].kind, MemoryKind::Procedure);
        assert!(inv.notes[0].body.contains("do the thing"));
    }

    fn frontmatter_with_paths(id: &str, title: &str, kind: &str, scope: &str, paths: &[&str]) -> String {
        let paths_yaml = if paths.is_empty() {
            String::from("[]")
        } else {
            let items: Vec<String> = paths.iter().map(|p| format!("\"{p}\"")).collect();
            format!("[{}]", items.join(", "))
        };
        format!(
            "---\nid: {id}\ntitle: {title}\nkind: {kind}\nscope: {scope}\npaths: {paths_yaml}\ncreated: 2026-07-03T00:00:00Z\nupdated: 2026-07-03T00:00:00Z\nsource: explicit-user-path\n---\n\nbody\n"
        )
    }
}
