//! Drift detection for memory staleness
//!
//! Detects when repository has changed since memory was last verified.

use crate::error::{Error, Result};
use crate::memory::document::MemoryDoc;
use crate::memory::manifest::MemoryManifest;
use crate::{MemoryPatch, MemoryPatchParams, PatchId, VerificationStatus};

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Staleness severity
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StalenessSeverity {
    /// Referenced files changed slightly (cosmetic)
    Minor,
    /// Referenced files significantly changed
    Major,
    /// Referenced files deleted or renamed
    Critical,
}

/// Information about a stale document
#[derive(Debug, Clone)]
pub struct StalenessInfo {
    /// Document ID
    pub doc_id: String,
    /// Path to document
    pub path: std::path::PathBuf,
    /// Last verified commit (if any)
    pub last_verified: Option<String>,
    /// Files referenced by doc that changed
    pub changed_files: Vec<std::path::PathBuf>,
    /// Staleness severity
    pub severity: StalenessSeverity,
}

/// Result of drift detection
#[derive(Debug, Clone)]
pub struct DriftResult {
    /// Documents that are stale
    pub stale_docs: Vec<StalenessInfo>,
    /// Current HEAD commit
    pub current_commit: String,
}

/// Detects memory-repo drift
pub struct DriftDetector {
    repo_path: PathBuf,
}

impl DriftDetector {
    /// Create a new drift detector
    pub fn new(repo: &git2::Repository) -> Result<Self> {
        let path = repo
            .workdir()
            .ok_or_else(|| Error::Other("Repository has no workdir".to_string()))?
            .to_path_buf();
        Ok(Self { repo_path: path })
    }

    /// Check all memory documents for drift
    pub fn check_all(&self, manifest: &MemoryManifest) -> Result<DriftResult> {
        let repo = git2::Repository::discover(&self.repo_path)
            .map_err(|e| Error::Other(format!("Failed to open repository: {}", e)))?;

        let head_commit = self.get_head_commit(&repo)?;
        let mut stale_docs = Vec::new();

        for entry in &manifest.docs {
            if let Ok(Some(info)) = self.check_entry(&repo, entry, &head_commit) {
                stale_docs.push(info);
            }
        }

        Ok(DriftResult { stale_docs, current_commit: head_commit })
    }

    /// Check a single document entry for drift
    fn check_entry(
        &self, repo: &git2::Repository, entry: &crate::memory::manifest::ManifestEntry, head_commit: &str,
    ) -> Result<Option<StalenessInfo>> {
        let last_verified = entry.verification.last_verified_commit.clone();

        let last_verified = match last_verified {
            Some(v) => v,
            None => return Ok(None),
        };

        // TODO: Check if document references any files
        let doc_path = std::path::Path::new(&entry.path);

        let changed_files = self.get_changed_files_since(repo, &last_verified, head_commit)?;

        let referenced_files = self.extract_referenced_paths(doc_path);

        let mut doc_changed_files: Vec<std::path::PathBuf> = Vec::new();
        let mut severity = StalenessSeverity::Minor;

        for changed_file in changed_files {
            if referenced_files.contains(&changed_file) {
                doc_changed_files.push(changed_file.clone());
                if !changed_file.exists() {
                    severity = StalenessSeverity::Critical;
                } else if severity != StalenessSeverity::Critical {
                    severity = StalenessSeverity::Major;
                }
            }
        }

        if !doc_changed_files.is_empty() {
            Ok(Some(StalenessInfo {
                doc_id: entry.id.clone(),
                path: doc_path.to_path_buf(),
                last_verified: Some(last_verified),
                changed_files: doc_changed_files,
                severity,
            }))
        } else {
            Ok(None)
        }
    }

    /// Get the current HEAD commit hash
    fn get_head_commit(&self, repo: &git2::Repository) -> Result<String> {
        let head = repo
            .head()
            .map_err(|e| Error::Other(format!("Failed to get HEAD: {}", e)))?;
        let commit = head
            .peel_to_commit()
            .map_err(|e| Error::Other(format!("Failed to peel to commit: {}", e)))?;
        Ok(commit.id().to_string())
    }

    /// Get files that changed since a given commit
    fn get_changed_files_since(
        &self, repo: &git2::Repository, since: &str, until: &str,
    ) -> Result<Vec<std::path::PathBuf>> {
        let since_id = git2::Oid::from_str(since).map_err(|e| Error::Other(format!("Invalid commit ID: {}", e)))?;
        let until_id = git2::Oid::from_str(until).map_err(|e| Error::Other(format!("Invalid commit ID: {}", e)))?;

        let _since_commit = repo
            .find_commit(since_id)
            .map_err(|e| Error::Other(format!("Commit not found: {}", e)))?;
        let _until_commit = repo
            .find_commit(until_id)
            .map_err(|e| Error::Other(format!("Commit not found: {}", e)))?;

        let mut revwalk = repo
            .revwalk()
            .map_err(|e| Error::Other(format!("Failed to create revwalk: {}", e)))?;
        revwalk
            .push(until_id)
            .map_err(|e| Error::Other(format!("Failed to push commit: {}", e)))?;
        revwalk
            .hide(since_id)
            .map_err(|e| Error::Other(format!("Failed to hide commit: {}", e)))?;

        let mut changed_files = HashSet::new();

        for oid in revwalk {
            let oid = oid.map_err(|e| Error::Other(format!("Revwalk error: {}", e)))?;
            let commit = repo
                .find_commit(oid)
                .map_err(|e| Error::Other(format!("Commit not found: {}", e)))?;

            let tree = commit
                .tree()
                .map_err(|e| Error::Other(format!("Failed to get tree: {}", e)))?;

            if let Ok(parent) = commit.parent(0) {
                let parent_tree = parent
                    .tree()
                    .map_err(|e| Error::Other(format!("Failed to get parent tree: {}", e)))?;

                let diff = repo
                    .diff_tree_to_tree(Some(&parent_tree), Some(&tree), None)
                    .map_err(|e| Error::Other(format!("Failed to create diff: {}", e)))?;

                diff.foreach(
                    &mut |delta, _| {
                        if let Some(path) = delta.new_file().path() {
                            changed_files.insert(path.to_path_buf());
                        }
                        true
                    },
                    None,
                    None,
                    None,
                )
                .map_err(|e| Error::Other(format!("Failed to iterate diff: {}", e)))?;
            }
        }

        Ok(changed_files.into_iter().collect())
    }

    /// Extract file paths referenced in a document
    ///
    /// This is a simple heuristic - looks for common patterns like:
    /// - `path/to/file.rs`
    /// - `src/lib.rs`
    /// - references in code blocks
    fn extract_referenced_paths(&self, doc_path: &Path) -> HashSet<std::path::PathBuf> {
        let mut paths = HashSet::new();

        let content = match std::fs::read_to_string(doc_path) {
            Ok(c) => c,
            Err(_) => return paths,
        };

        let extensions = [
            ".rs", ".py", ".js", ".ts", ".jsx", ".tsx", ".go", ".java", ".cpp", ".c", ".h", ".hpp", ".cs", ".rb",
            ".php", ".swift", ".kt", ".scala", ".sh", ".bash", ".zsh", ".fish", ".toml", ".yaml", ".yml", ".json",
            ".xml", ".md", ".txt", ".lock", ".mod", ".sum", ".gradle",
        ];

        let ext_pattern = extensions.join("|");
        let pattern_str = format!(r"(?:[\w\-_./]+/)?[\w\-_]+(?:{})", ext_pattern);

        let regex = match regex::Regex::new(&pattern_str) {
            Ok(r) => r,
            Err(_) => return paths,
        };

        for capture in regex.captures_iter(&content) {
            if let Some(matched) = capture.get(0) {
                let path_str = matched.as_str();

                if path_str.contains("://") || path_str.contains("mailto:") {
                    continue;
                }

                let path = std::path::PathBuf::from(path_str);

                if path.extension().is_some() && !path_str.contains(' ') {
                    paths.insert(path);
                }
            }
        }

        let dir_patterns = [
            r"(?:in|from|at)\s+(src/|lib/|tests/|examples/|benches/|bin/)[\w\-_/]*",
            r"(?:src/|lib/|tests/|examples/)[\w\-_/]+",
        ];

        for dir_pattern in dir_patterns {
            if let Ok(regex) = regex::Regex::new(dir_pattern) {
                for capture in regex.captures_iter(&content) {
                    if let Some(matched) = capture.get(1).or_else(|| capture.get(0)) {
                        let path_str = matched
                            .as_str()
                            .trim_end_matches(|c: char| c.is_whitespace() || c == '.' || c == ',');
                        if !path_str.is_empty() {
                            paths.insert(std::path::PathBuf::from(path_str));
                        }
                    }
                }
            }
        }

        paths
    }

    /// Mark a document as verified at current commit
    pub fn mark_verified(&self, doc: &mut MemoryDoc, status: VerificationStatus) -> Result<MemoryPatch> {
        let repo = git2::Repository::discover(&self.repo_path)
            .map_err(|e| Error::Other(format!("Failed to open repository: {}", e)))?;
        let commit = self.get_head_commit(&repo).unwrap_or_else(|_| "unknown".to_string());

        doc.frontmatter.verification.last_verified_commit = Some(commit);
        doc.frontmatter.verification.status = status;

        Ok(MemoryPatch::new(
            PatchId::new("verify_patch"),
            MemoryPatchParams {
                path: std::path::PathBuf::from("placeholder"),
                doc_id: doc.frontmatter.id.clone(),
                kind: crate::memory::kinds::MemoryKind::Fact,
                description: "Mark as verified".to_string(),
                diff: String::new(),
                source_events: vec![],
                session_id: crate::layout::SessionId::new(),
                seq: 0,
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_staleness_severity() {
        assert_eq!(StalenessSeverity::Minor, StalenessSeverity::Minor);
        assert_eq!(StalenessSeverity::Major, StalenessSeverity::Major);
        assert_eq!(StalenessSeverity::Critical, StalenessSeverity::Critical);
    }

    #[test]
    fn test_staleness_info() {
        let info = StalenessInfo {
            doc_id: "fact.test".to_string(),
            path: std::path::PathBuf::from("memory/test.md"),
            last_verified: Some("abc123".to_string()),
            changed_files: vec![std::path::PathBuf::from("src/main.rs")],
            severity: StalenessSeverity::Major,
        };

        assert_eq!(info.doc_id, "fact.test");
        assert_eq!(info.changed_files.len(), 1);
    }

    #[test]
    fn test_extract_referenced_paths() {
        let temp = tempfile::TempDir::new().unwrap();
        git2::Repository::init(temp.path()).unwrap();
        let repo = git2::Repository::discover(temp.path()).unwrap();
        let detector = DriftDetector::new(&repo).unwrap();
        let doc_path = temp.path().join("test_fact.md");
        let content = r#"# Build Commands

This document references src/main.rs and lib/lib.rs.

For testing, use tests/integration_test.rs.

See src/ for the main source code.
"#;
        std::fs::write(&doc_path, content).unwrap();

        let paths = detector.extract_referenced_paths(&doc_path);
        assert!(paths.iter().any(|p| p.ends_with("src/main.rs")));
        assert!(paths.iter().any(|p| p.ends_with("lib/lib.rs")));
        assert!(paths.iter().any(|p| p.ends_with("tests/integration_test.rs")));
    }

    #[test]
    fn test_extract_referenced_paths_empty() {
        let temp = tempfile::TempDir::new().unwrap();
        git2::Repository::init(temp.path()).unwrap();
        let repo = git2::Repository::discover(temp.path()).unwrap();
        let detector = DriftDetector::new(&repo).unwrap();

        let doc_path = temp.path().join("no_paths.md");
        std::fs::write(&doc_path, "Just some text with no file references").unwrap();

        let paths = detector.extract_referenced_paths(&doc_path);
        assert!(paths.is_empty());
    }

    #[test]
    fn test_extract_referenced_paths_missing_file() {
        let temp = tempfile::TempDir::new().unwrap();
        git2::Repository::init(temp.path()).unwrap();

        let repo = git2::Repository::discover(temp.path()).unwrap();
        let detector = DriftDetector::new(&repo).unwrap();
        let nonexistent = temp.path().join("nonexistent.md");
        let paths = detector.extract_referenced_paths(&nonexistent);
        assert!(paths.is_empty());
    }
}
