//! Core memory loading and hierarchical merging
//!
//! Core memory is the always-loaded context that grounds the agent in project knowledge.
//! It loads from multiple sources with hierarchical precedence.

use crate::error::{self, Result};
use crate::memory::document::MemoryDoc;
use crate::memory::paths::MemoryPaths;

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// Soft token limit for core memory (4,000 tokens ~ 16KB)
pub const CORE_MEMORY_SOFT_LIMIT: usize = 4000;

/// Hard token limit for core memory (8,000 tokens ~ 32KB)
pub const CORE_MEMORY_HARD_LIMIT: usize = 8000;

/// Source of core memory content with priority
#[derive(Debug, Clone)]
pub struct CoreMemorySource {
    /// Path to the source file
    pub path: PathBuf,
    /// Priority level (lower = higher priority)
    pub priority: u8,
    /// Content hash for deduplication
    pub content_hash: String,
    /// Whether the source was successfully loaded
    pub loaded: bool,
}

impl CoreMemorySource {
    /// Create a new core memory source
    pub fn new(path: PathBuf, priority: u8) -> Self {
        Self { path, priority, content_hash: String::new(), loaded: false }
    }

    /// Compute hash of the content
    pub fn compute_hash(content: &str) -> String {
        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }
}

/// Loaded and merged core memory
///
/// Core memory is loaded from multiple sources with hierarchical precedence:
/// 1. Repository root: `.thunderus/memory/core/CORE.md`
/// 2. Current working directory: `./CORE.md` (if different from repo root)
/// 3. Local override: `.thunderus/memory/core/CORE.local.md`
#[derive(Debug, Clone, Default)]
pub struct CoreMemory {
    /// Merged content from all sources
    pub content: String,
    /// Sources used (for provenance tracking)
    pub sources: Vec<CoreMemorySource>,
    /// Approximate token count
    pub token_count: usize,
}

impl CoreMemory {
    /// Load and merge core memory from all sources
    ///
    /// Searches for core memory in the following order (lowest to highest priority):
    /// 1. Repository root: `.thunderus/memory/core/CORE.md`
    /// 2. Current working directory: `./CORE.md` (if different from repo root)
    /// 3. Local override: `.thunderus/memory/core/CORE.local.md`
    ///
    /// Later sources override or append to earlier sources.
    pub fn load(paths: &MemoryPaths, cwd: &Path) -> Result<Self> {
        let mut sources = Vec::new();
        let mut merged_content = String::new();

        let git_root = Self::find_git_root(cwd);

        if let Some(ref root) = git_root {
            let repo_core_path = root.join(".thunderus").join("memory").join("core").join("CORE.md");
            if let Ok(content) = Self::load_source(&repo_core_path) {
                let hash = CoreMemorySource::compute_hash(&content);
                sources.push(CoreMemorySource {
                    path: repo_core_path.clone(),
                    priority: 1,
                    content_hash: hash.clone(),
                    loaded: true,
                });
                merged_content.push_str(&Self::wrap_source(&repo_core_path, &content));
            }
        }

        let cwd_core_path = cwd.join("CORE.md");
        if git_root.as_ref().is_none_or(|r| r != cwd)
            && let Ok(content) = Self::load_source(&cwd_core_path)
        {
            let hash = CoreMemorySource::compute_hash(&content);
            if !sources.iter().any(|s| s.content_hash == hash) {
                sources.push(CoreMemorySource {
                    path: cwd_core_path.clone(),
                    priority: 2,
                    content_hash: hash,
                    loaded: true,
                });
                merged_content.push_str(&Self::wrap_source(&cwd_core_path, &content));
            }
        }

        let local_core_path = paths.core_local_memory_file();
        if let Ok(content) = Self::load_source(&local_core_path) {
            let hash = CoreMemorySource::compute_hash(&content);
            sources.push(CoreMemorySource {
                path: local_core_path.clone(),
                priority: 3,
                content_hash: hash,
                loaded: true,
            });
            merged_content.push_str(&Self::wrap_source(&local_core_path, &content));
        }

        sources.sort_by_key(|s| s.priority);

        let token_count = Self::estimate_tokens(&merged_content);

        Ok(Self { content: merged_content, sources, token_count })
    }

    /// Load content from a file path
    fn load_source(path: &Path) -> Result<String> {
        fs::read_to_string(path).map_err(error::Error::Io)
    }

    /// Wrap source content with metadata comment
    fn wrap_source(path: &Path, content: &str) -> String {
        let source_name = path.file_name().unwrap_or_default().to_string_lossy();
        format!(
            "<!-- {} from {} -->\n\n{}\n\n",
            source_name,
            path.display(),
            content.trim()
        )
    }

    /// Estimate token count (rough approximation: ~4 chars per token)
    fn estimate_tokens(content: &str) -> usize {
        content.len() / 4
    }

    /// Find git repository root by traversing upward
    fn find_git_root(start: &Path) -> Option<PathBuf> {
        let mut current = start.canonicalize().ok()?;

        loop {
            let git_dir = current.join(".git");
            if git_dir.exists() {
                return Some(current);
            }

            if !current.pop() {
                return None;
            }
        }
    }

    /// Check if content exceeds soft limit
    pub fn is_over_soft_limit(&self) -> bool {
        self.token_count > CORE_MEMORY_SOFT_LIMIT
    }

    /// Check if content exceeds hard limit
    pub fn is_over_hard_limit(&self) -> bool {
        self.token_count > CORE_MEMORY_HARD_LIMIT
    }

    /// Get soft limit ratio (0.0 to 1.0+)
    pub fn soft_limit_ratio(&self) -> f64 {
        self.token_count as f64 / CORE_MEMORY_SOFT_LIMIT as f64
    }

    /// Get hard limit ratio (0.0 to 1.0+)
    pub fn hard_limit_ratio(&self) -> f64 {
        self.token_count as f64 / CORE_MEMORY_HARD_LIMIT as f64
    }

    /// Parse the merged content as a memory document
    ///
    /// This attempts to parse the merged content as if it were a single document.
    /// Note that merged content may not be a valid document due to multiple sources.
    pub fn parse_as_document(&self) -> Result<MemoryDoc> {
        MemoryDoc::parse(&self.content)
    }

    /// Get just the body content (without source wrappers)
    pub fn body_content(&self) -> String {
        let mut body = self.content.clone();

        while let Some(start) = body.find("<!-- ") {
            if let Some(end) = body[start..].find("-->") {
                let end = start + end + 4;
                body.replace_range(start..end, "");
            } else {
                break;
            }
        }

        body.trim().to_string()
    }

    /// Check if a specific source was loaded
    pub fn has_source(&self, path: &Path) -> bool {
        self.sources.iter().any(|s| s.path == path)
    }

    /// Get sources by priority level
    pub fn sources_by_priority(&self, priority: u8) -> Vec<&CoreMemorySource> {
        self.sources.iter().filter(|s| s.priority == priority).collect()
    }
}

/// Lint warning for core memory
#[derive(Debug, Clone)]
pub struct CoreMemoryLint {
    /// Lint rule ID
    pub rule: String,
    /// Severity level
    pub severity: LintSeverity,
    /// Warning message
    pub message: String,
}

/// Severity level for lint warnings
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintSeverity {
    /// Informational only
    Info,
    /// Warning (should fix)
    Warning,
    /// Error (must fix)
    Error,
}

impl CoreMemory {
    /// Validate core memory and return lint warnings
    pub fn validate(&self) -> Vec<CoreMemoryLint> {
        let mut lints = Vec::new();

        if self.is_over_hard_limit() {
            lints.push(CoreMemoryLint {
                rule: "mem004".to_string(),
                severity: LintSeverity::Error,
                message: format!(
                    "Core memory exceeds hard limit: {} tokens (limit: {})",
                    self.token_count, CORE_MEMORY_HARD_LIMIT
                ),
            });
        }

        if self.is_over_soft_limit() {
            lints.push(CoreMemoryLint {
                rule: "mem003".to_string(),
                severity: LintSeverity::Warning,
                message: format!(
                    "Core memory exceeds soft limit: {} tokens (limit: {})",
                    self.token_count, CORE_MEMORY_SOFT_LIMIT
                ),
            });
        }

        if self.content.trim().is_empty() {
            lints.push(CoreMemoryLint {
                rule: "mem007".to_string(),
                severity: LintSeverity::Warning,
                message: "Core memory is empty".to_string(),
            });
        }

        if let Ok(doc) = self.parse_as_document() {
            for error in doc.validate() {
                lints.push(CoreMemoryLint {
                    rule: "mem001".to_string(),
                    severity: LintSeverity::Error,
                    message: format!("{}: {}", error.field, error.message),
                });
            }
        }

        lints
    }

    /// Get errors only (excluding warnings and info)
    pub fn errors(&self) -> Vec<CoreMemoryLint> {
        self.validate()
            .into_iter()
            .filter(|l| l.severity == LintSeverity::Error)
            .collect()
    }

    /// Get warnings only (excluding errors and info)
    pub fn warnings(&self) -> Vec<CoreMemoryLint> {
        self.validate()
            .into_iter()
            .filter(|l| l.severity == LintSeverity::Warning)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_core_memory(content: &str) -> TempDir {
        let temp = TempDir::new().unwrap();
        fs::create_dir_all(temp.path().join(".git")).unwrap();

        let core_dir = temp.path().join(".thunderus").join("memory").join("core");
        fs::create_dir_all(&core_dir).unwrap();

        let core_file = core_dir.join("CORE.md");
        fs::write(&core_file, content).unwrap();

        temp
    }

    #[test]
    fn test_core_memory_load_empty() {
        let temp = TempDir::new().unwrap();
        let paths = MemoryPaths::from_thunderus_root(temp.path());
        let core = CoreMemory::load(&paths, temp.path()).unwrap();
        assert!(core.content.is_empty());
        assert!(core.sources.is_empty());
        assert_eq!(core.token_count, 0);
    }

    #[test]
    fn test_core_memory_load_from_repo() {
        let content = r#"---
id: core.project
title: Test Project
kind: core
tags: [core]
created: 2026-01-21T00:00:00Z
updated: 2026-01-21T00:00:00Z
---

# Test Project

## Identity
Test content"#;

        let temp = create_test_core_memory(content);
        let paths = MemoryPaths::from_thunderus_root(temp.path());

        let core = CoreMemory::load(&paths, temp.path()).unwrap();

        assert!(!core.content.is_empty());
        assert!(core.content.contains("Test Project"));
        assert!(core.content.contains("## Identity"));
        assert_eq!(core.sources.len(), 1);
    }

    #[test]
    fn test_core_memory_load_with_local_override() {
        let repo_content = "# Repository\n\nRepo content";
        let local_content = "# Local\n\nLocal override";

        let temp = TempDir::new().unwrap();
        fs::create_dir_all(temp.path().join(".git")).unwrap();

        let core_dir = temp.path().join(".thunderus").join("memory").join("core");
        fs::create_dir_all(&core_dir).unwrap();

        fs::write(core_dir.join("CORE.md"), repo_content).unwrap();
        fs::write(core_dir.join("CORE.local.md"), local_content).unwrap();

        let paths = MemoryPaths::from_thunderus_root(temp.path());
        let core = CoreMemory::load(&paths, temp.path()).unwrap();

        assert!(core.content.contains("Repository"));
        assert!(core.content.contains("Local override"));
        assert_eq!(core.sources.len(), 2);
    }

    #[test]
    fn test_core_memory_token_estimation() {
        let content = "a".repeat(4000);
        let temp = create_test_core_memory(&content);
        let paths = MemoryPaths::from_thunderus_root(temp.path());

        let core = CoreMemory::load(&paths, temp.path()).unwrap();

        assert!(core.token_count > 0);
        assert!(core.token_count < 2000);
    }

    #[test]
    fn test_core_memory_soft_limit() {
        let content = "a".repeat(20000);
        let temp = create_test_core_memory(&content);
        let paths = MemoryPaths::from_thunderus_root(temp.path());

        let core = CoreMemory::load(&paths, temp.path()).unwrap();

        assert!(core.is_over_soft_limit());
        assert!(!core.is_over_hard_limit());
    }

    #[test]
    fn test_core_memory_hard_limit() {
        let content = "a".repeat(40000);
        let temp = create_test_core_memory(&content);
        let paths = MemoryPaths::from_thunderus_root(temp.path());

        let core = CoreMemory::load(&paths, temp.path()).unwrap();

        assert!(core.is_over_soft_limit());
        assert!(core.is_over_hard_limit());
    }

    #[test]
    fn test_core_memory_soft_limit_ratio() {
        let temp = create_test_core_memory(&"a".repeat(16000));
        let paths = MemoryPaths::from_thunderus_root(temp.path());

        let core = CoreMemory::load(&paths, temp.path()).unwrap();

        let ratio = core.soft_limit_ratio();
        assert!(ratio > 0.0);
        assert!(ratio >= 1.0);
    }

    #[test]
    fn test_core_memory_hard_limit_ratio() {
        let temp = create_test_core_memory(&"a".repeat(32000));
        let paths = MemoryPaths::from_thunderus_root(temp.path());
        let core = CoreMemory::load(&paths, temp.path()).unwrap();
        let ratio = core.hard_limit_ratio();
        assert!(ratio > 0.0);
        assert!(ratio >= 1.0);
    }

    #[test]
    fn test_core_memory_body_content() {
        let content = "# Test\n\nContent here";
        let temp = create_test_core_memory(content);
        let paths = MemoryPaths::from_thunderus_root(temp.path());

        let core = CoreMemory::load(&paths, temp.path()).unwrap();

        let body = core.body_content();
        assert!(body.contains("Test"));
        assert!(body.contains("Content here"));
        assert!(!body.contains("<!--"));
    }

    #[test]
    fn test_core_memory_has_source() {
        let temp = create_test_core_memory("# Test");
        let paths = MemoryPaths::from_thunderus_root(temp.path());
        let core = CoreMemory::load(&paths, temp.path()).unwrap();
        assert!(!core.sources.is_empty());
        assert!(core.sources[0].path.to_string_lossy().ends_with("CORE.md"));
        assert!(!core.has_source(Path::new("/nonexistent/path")));
    }

    #[test]
    fn test_core_memory_sources_by_priority() {
        let repo_content = "# Repo";
        let local_content = "# Local";
        let temp = TempDir::new().unwrap();
        fs::create_dir_all(temp.path().join(".git")).unwrap();

        let core_dir = temp.path().join(".thunderus").join("memory").join("core");
        fs::create_dir_all(&core_dir).unwrap();

        fs::write(core_dir.join("CORE.md"), repo_content).unwrap();
        fs::write(core_dir.join("CORE.local.md"), local_content).unwrap();

        let paths = MemoryPaths::from_thunderus_root(temp.path());
        let core = CoreMemory::load(&paths, temp.path()).unwrap();

        let priority_1 = core.sources_by_priority(1);
        let priority_3 = core.sources_by_priority(3);

        assert_eq!(priority_1.len(), 1);
        assert_eq!(priority_3.len(), 1);
    }

    #[test]
    fn test_core_memory_validate_empty() {
        let core = CoreMemory::default();
        let lints = core.validate();

        assert!(!lints.is_empty());
        assert!(lints.iter().any(|l| l.rule == "mem007"));
    }

    #[test]
    fn test_core_memory_validate_over_limits() {
        let temp = create_test_core_memory(&"a".repeat(40000));
        let paths = MemoryPaths::from_thunderus_root(temp.path());

        let core = CoreMemory::load(&paths, temp.path()).unwrap();
        let lints = core.validate();

        assert!(!lints.is_empty());
        assert!(lints.iter().any(|l| l.rule == "mem003"));
        assert!(lints.iter().any(|l| l.rule == "mem004"));
    }

    #[test]
    fn test_core_memory_errors_only() {
        let temp = create_test_core_memory(&"a".repeat(40000));
        let paths = MemoryPaths::from_thunderus_root(temp.path());

        let core = CoreMemory::load(&paths, temp.path()).unwrap();
        let errors = core.errors();

        assert!(!errors.is_empty());
        assert!(errors.iter().all(|e| e.severity == LintSeverity::Error));
    }

    #[test]
    fn test_core_memory_warnings_only() {
        let temp = create_test_core_memory(&"a".repeat(20000));
        let paths = MemoryPaths::from_thunderus_root(temp.path());

        let core = CoreMemory::load(&paths, temp.path()).unwrap();
        let warnings = core.warnings();

        assert!(!warnings.is_empty());
        assert!(warnings.iter().all(|w| w.severity == LintSeverity::Warning));
    }

    #[test]
    fn test_constants() {
        assert_eq!(CORE_MEMORY_SOFT_LIMIT, 4000);
        assert_eq!(CORE_MEMORY_HARD_LIMIT, 8000);
    }

    #[test]
    fn test_core_memory_default() {
        let core = CoreMemory::default();

        assert!(core.content.is_empty());
        assert!(core.sources.is_empty());
        assert_eq!(core.token_count, 0);
    }

    #[test]
    fn test_core_memory_source_compute_hash() {
        let content = "test content";
        let hash1 = CoreMemorySource::compute_hash(content);
        let hash2 = CoreMemorySource::compute_hash(content);

        assert_eq!(hash1, hash2);
        assert!(!hash1.is_empty());
    }

    #[test]
    fn test_core_memory_source_different_hashes() {
        let hash1 = CoreMemorySource::compute_hash("content 1");
        let hash2 = CoreMemorySource::compute_hash("content 2");

        assert_ne!(hash1, hash2);
    }
}
