/// Git apply engine for applying patches with conflict detection
///
/// This module implements a patch application engine that uses `git apply`
/// to apply unified diffs, with comprehensive conflict detection and
/// pedagogical error messages.
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use thunderus_core::{Error, Result};

/// Result of a patch application attempt
#[derive(Debug, Clone)]
pub enum ApplyResult {
    /// Patch applied successfully
    Success { files_modified: Vec<String> },
    /// Patch failed with conflicts
    Conflict { conflicts: Vec<ConflictInfo> },
    /// Patch failed for another reason
    Error { message: String },
}

/// Information about a conflict in a patch
#[derive(Debug, Clone)]
pub struct ConflictInfo {
    /// File where the conflict occurred
    pub file: PathBuf,
    /// Line number where the conflict starts
    pub line: usize,
    /// Type of conflict
    pub conflict_type: ConflictType,
    /// Human-readable explanation of the conflict
    pub explanation: String,
    /// Suggested resolution strategies
    pub suggestions: Vec<String>,
}

/// Types of conflicts that can occur
#[derive(Debug, Clone, PartialEq)]
pub enum ConflictType {
    /// Changes overlap with existing uncommitted changes
    OverlappingChanges,
    /// The hunk doesn't match the target file
    HunkMismatch,
    /// The file has been modified since the base snapshot
    StaleBase,
    /// The file doesn't exist
    MissingFile,
    /// Binary file conflict
    BinaryFile,
    /// Unknown conflict type
    Unknown,
}

/// Engine for applying patches with git
pub struct ApplyEngine {
    /// Path to the git repository
    repo_path: PathBuf,
}

impl ApplyEngine {
    /// Create a new apply engine for the given repository
    pub fn new(repo_path: &Path) -> Result<Self> {
        let repo_path = if repo_path.is_absolute() {
            repo_path.to_path_buf()
        } else {
            std::env::current_dir()?.join(repo_path)
        };

        if !repo_path.join(".git").exists() {
            return Err(Error::Validation(format!(
                "Not a git repository: {}",
                repo_path.display()
            )));
        }

        Ok(ApplyEngine { repo_path })
    }

    /// Apply a unified diff patch
    ///
    /// Returns the result of the application attempt
    pub fn apply_patch(&self, diff: &str, base_snapshot: &str) -> ApplyResult {
        if let Err(conflict) = self.check_base_snapshot(base_snapshot) {
            return ApplyResult::Conflict { conflicts: vec![conflict] };
        }

        let check_result = self.git_apply_check(diff);

        match check_result {
            Ok(_) => match self.git_apply(diff) {
                Ok(files) => ApplyResult::Success { files_modified: files },
                Err(e) => ApplyResult::Error { message: format!("Patch apply failed: {}", e) },
            },
            Err(conflicts) => {
                if conflicts.iter().any(|c| c.conflict_type != ConflictType::Unknown) {
                    ApplyResult::Conflict { conflicts }
                } else {
                    ApplyResult::Error {
                        message: conflicts
                            .first()
                            .map(|c| c.explanation.clone())
                            .unwrap_or_else(|| "Unknown patch apply error".to_string()),
                    }
                }
            }
        }
    }

    /// Apply only approved hunks from a patch
    ///
    /// This filters the diff to only include approved hunks before applying
    pub fn apply_approved_hunks(&self, patch: &thunderus_core::Patch) -> ApplyResult {
        let filtered_diff = match self.filter_approved_hunks(patch) {
            Ok(diff) => diff,
            Err(e) => return ApplyResult::Error { message: format!("Failed to filter hunks: {}", e) },
        };

        if filtered_diff.is_empty() {
            return ApplyResult::Error { message: "No approved hunks to apply".to_string() };
        }

        self.apply_patch(&filtered_diff, &patch.base_snapshot)
    }

    /// Check if the current working directory matches the expected base snapshot
    fn check_base_snapshot(&self, expected: &str) -> std::result::Result<(), ConflictInfo> {
        let output = match Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&self.repo_path)
            .output()
        {
            Ok(output) => output,
            Err(e) => {
                return Err(ConflictInfo {
                    file: PathBuf::from("."),
                    line: 0,
                    conflict_type: ConflictType::Unknown,
                    explanation: format!("Failed to get current commit: {}", e),
                    suggestions: vec![
                        "Ensure you're in a valid git repository".to_string(),
                        "Check that git is installed and accessible".to_string(),
                    ],
                });
            }
        };

        if !output.status.success() {
            return Err(ConflictInfo {
                file: PathBuf::from("."),
                line: 0,
                conflict_type: ConflictType::Unknown,
                explanation: "Failed to determine current commit".to_string(),
                suggestions: vec![
                    "Ensure you're in a valid git repository".to_string(),
                    "Check that git is installed and accessible".to_string(),
                ],
            });
        }

        let current = String::from_utf8_lossy(&output.stdout).trim().to_string();

        if !current.starts_with(expected) {
            return Err(ConflictInfo {
                file: PathBuf::from("."),
                line: 0,
                conflict_type: ConflictType::StaleBase,
                explanation: format!(
                    "Repository state has changed since patch was created.\nExpected base: {}\nCurrent HEAD: {}",
                    expected, current
                ),
                suggestions: vec![
                    "Commit or stash your current changes".to_string(),
                    format!("Reset to the base commit: git reset {}", expected),
                    "Re-create the patch from the current state".to_string(),
                ],
            });
        }

        Ok(())
    }

    /// Run `git apply --check` to validate a patch
    fn git_apply_check(&self, _diff: &str) -> std::result::Result<Vec<ConflictInfo>, Vec<ConflictInfo>> {
        let output = Command::new("git")
            .args(["apply", "--check", "-"])
            .current_dir(&self.repo_path)
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| {
                vec![ConflictInfo {
                    file: PathBuf::from("."),
                    line: 0,
                    conflict_type: ConflictType::Unknown,
                    explanation: format!("Failed to run git apply --check: {}", e),
                    suggestions: vec![
                        "Ensure git is installed and accessible".to_string(),
                        "Check that you're in a valid git repository".to_string(),
                    ],
                }]
            })?;

        if output.status.success() {
            return Ok(Vec::new());
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        let conflicts = self.parse_git_apply_errors(&stderr);

        if conflicts.is_empty() {
            return Err(vec![ConflictInfo {
                file: PathBuf::from("."),
                line: 0,
                conflict_type: ConflictType::Unknown,
                explanation: stderr.to_string(),
                suggestions: vec![
                    "Review the patch file for errors".to_string(),
                    "Ensure the patch was generated with `git diff`".to_string(),
                ],
            }]);
        }

        Err(conflicts)
    }

    /// Actually apply a patch with git
    fn git_apply(&self, _diff: &str) -> Result<Vec<String>> {
        let files_output = Command::new("git")
            .args(["apply", "--numstat", "-"])
            .current_dir(&self.repo_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| Error::Tool(format!("Failed to run git apply --numstat: {}", e)))?;

        let files: Vec<String> = if files_output.status.success() {
            String::from_utf8_lossy(&files_output.stdout)
                .lines()
                .filter_map(|line| {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 3 { Some(parts[2].to_string()) } else { None }
                })
                .collect()
        } else {
            Vec::new()
        };

        let output = Command::new("git")
            .args(["apply", "-"])
            .current_dir(&self.repo_path)
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| Error::Tool(format!("Failed to run git apply: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Tool(format!("Patch application failed: {}", stderr)));
        }

        Ok(files)
    }

    /// Parse git apply error output into ConflictInfo structs
    fn parse_git_apply_errors(&self, stderr: &str) -> Vec<ConflictInfo> {
        let mut conflicts = Vec::new();

        for line in stderr.lines() {
            if let Some(info) = self.parse_git_apply_error_line(line) {
                conflicts.push(info);
            }
        }

        if conflicts.is_empty() && !stderr.is_empty() {
            conflicts.push(ConflictInfo {
                file: PathBuf::from("."),
                line: 0,
                conflict_type: ConflictType::Unknown,
                explanation: stderr.to_string(),
                suggestions: vec![
                    "Review the patch file for formatting errors".to_string(),
                    "Ensure the patch was generated with unified diff format".to_string(),
                    "Check that the target files exist".to_string(),
                ],
            });
        }

        conflicts
    }

    /// Parse a single line of git apply error output
    fn parse_git_apply_error_line(&self, line: &str) -> Option<ConflictInfo> {
        let line = line.trim();

        if line.contains("does not match index") {
            let file = line.split(':').nth(1).map(|s| s.trim().to_string())?;
            return Some(ConflictInfo {
                file: PathBuf::from(file.clone()),
                line: 0,
                conflict_type: ConflictType::HunkMismatch,
                explanation: format!(
                    "The patch doesn't match the current state of '{}'.\n\nThis usually means the file has been modified since the patch was created.",
                    file
                ),
                suggestions: vec![
                    "Refresh the patch by regenerating it from the current state".to_string(),
                    "Review the file and manually apply the changes".to_string(),
                    "Reset the file to match the patch's expected state".to_string(),
                ],
            });
        }

        if line.contains("patch does not apply") {
            let file = line.split(':').nth(1).map(|s| s.trim().to_string())?;
            return Some(ConflictInfo {
                file: PathBuf::from(file.clone()),
                line: 0,
                conflict_type: ConflictType::HunkMismatch,
                explanation: format!(
                    "The patch cannot be applied to '{}'.\n\nThe changes in the patch conflict with the current file contents.",
                    file
                ),
                suggestions: vec![
                    "View the current file contents and the patch to understand the conflict".to_string(),
                    "Apply the patch manually by editing the file".to_string(),
                    "Use a 3-way merge tool: git apply --3way < patchfile".to_string(),
                ],
            });
        }

        if line.contains("No such file") || line.contains("cannot stat") {
            let file = line.split('\'').nth(1).or_else(|| line.split('"').nth(1));
            if let Some(file) = file {
                return Some(ConflictInfo {
                    file: PathBuf::from(file),
                    line: 0,
                    conflict_type: ConflictType::MissingFile,
                    explanation: format!(
                        "The file '{}' doesn't exist in the working directory.\n\nThe patch expects this file to be present.",
                        file
                    ),
                    suggestions: vec![
                        "Create the file if it's a new file".to_string(),
                        "Check if the file path in the patch is correct".to_string(),
                        "Verify you're in the correct directory".to_string(),
                    ],
                });
            }
        }

        if line.contains("patch failed") {
            let rest = line.split("patch failed: ").nth(1)?;
            let parts: Vec<&str> = rest.split(':').collect();
            if parts.len() >= 2 {
                let file = PathBuf::from(parts[0].trim());
                let line = parts[1].trim().parse::<usize>().ok()?;
                return Some(ConflictInfo {
                    file: file.clone(),
                    line,
                    conflict_type: ConflictType::OverlappingChanges,
                    explanation: format!(
                        "Failed to apply patch at {}:{}.\n\nThe hunk at this location doesn't match the file contents.",
                        file.display(),
                        line
                    ),
                    suggestions: vec![
                        "Review the file around the indicated line".to_string(),
                        "Check for uncommitted changes that might conflict".to_string(),
                        "Apply the patch manually with a text editor".to_string(),
                    ],
                });
            }
        }

        if line.contains("Binary") {
            return Some(ConflictInfo {
                file: PathBuf::from("<binary>"),
                line: 0,
                conflict_type: ConflictType::BinaryFile,
                explanation:
                    "This patch contains binary file changes, which are not supported by the diff-first workflow."
                        .to_string(),
                suggestions: vec![
                    "Use git checkout or git apply to handle binary files".to_string(),
                    "Consider committing binary file changes separately".to_string(),
                ],
            });
        }

        None
    }

    /// Filter a patch to only include approved hunks
    fn filter_approved_hunks(&self, patch: &thunderus_core::Patch) -> Result<String> {
        let mut result = String::new();

        for file in &patch.files {
            if let Some(hunks) = patch.hunks.get(file) {
                let approved_hunks: Vec<&thunderus_core::Hunk> = hunks.iter().filter(|h| h.approved).collect();

                if approved_hunks.is_empty() {
                    continue;
                }

                result.push_str(&format!("diff --git a/{} b/{}\n", file.display(), file.display()));

                for hunk in approved_hunks {
                    result.push_str(&format!("{}\n{}\n", hunk.header(), hunk.content));
                }
            }
        }

        Ok(result)
    }

    /// Roll back the last applied patch
    ///
    /// This uses `git reset --hard` to revert to the previous state
    pub fn rollback(&self) -> Result<()> {
        let output = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| Error::Tool(format!("Failed to check git status: {}", e)))?;

        let has_changes = !String::from_utf8_lossy(&output.stdout).trim().is_empty();

        if !has_changes {
            return Err(Error::Tool("No changes to rollback".to_string()));
        }

        let output = Command::new("git")
            .args(["reset", "--hard", "HEAD"])
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| Error::Tool(format!("Failed to rollback changes: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Tool(format!("Rollback failed: {}", stderr)));
        }

        Ok(())
    }

    /// Create a git note linking a commit to a session
    pub fn add_session_note(&self, commit: &str, session_id: &str, patch_id: &str) -> Result<()> {
        let note_content = format!("Session: {}\nPatch: {}", session_id, patch_id);

        let output = Command::new("git")
            .args(["notes", "add", "-m", &note_content, commit])
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| Error::Tool(format!("Failed to add git note: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Tool(format!("Failed to add git note: {}", stderr)));
        }

        Ok(())
    }

    /// Get the session associated with a commit via git notes
    pub fn get_session_note(&self, commit: &str) -> Result<Option<(String, String)>> {
        let output = Command::new("git")
            .args(["notes", "show", commit])
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| Error::Tool(format!("Failed to get git note: {}", e)))?;

        if !output.status.success() {
            return Ok(None);
        }

        let note = String::from_utf8_lossy(&output.stdout);

        let mut session_id = None;
        let mut patch_id = None;

        for line in note.lines() {
            if line.starts_with("Session: ")
                && let Some(id) = line.strip_prefix("Session: ")
            {
                session_id = Some(id.to_string());
            } else if line.starts_with("Patch: ")
                && let Some(id) = line.strip_prefix("Patch: ")
            {
                patch_id = Some(id.to_string());
            }
        }

        match (session_id, patch_id) {
            (Some(s), Some(p)) => Ok(Some((s, p))),
            _ => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conflict_type() {
        assert_eq!(ConflictType::OverlappingChanges, ConflictType::OverlappingChanges);
        assert_eq!(ConflictType::HunkMismatch, ConflictType::HunkMismatch);
        assert_eq!(ConflictType::StaleBase, ConflictType::StaleBase);
        assert_eq!(ConflictType::MissingFile, ConflictType::MissingFile);
        assert_eq!(ConflictType::BinaryFile, ConflictType::BinaryFile);
        assert_eq!(ConflictType::Unknown, ConflictType::Unknown);
    }

    #[test]
    fn test_parse_conflict_line_hunk_mismatch() {
        let engine = ApplyEngine::new(Path::new(".")).ok();
        if engine.is_none() {
            return;
        }

        let engine = engine.unwrap();

        let line = "error: src/main.rs: does not match index";
        let info = engine.parse_git_apply_error_line(line);

        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.file, PathBuf::from("src/main.rs"));
        assert_eq!(info.conflict_type, ConflictType::HunkMismatch);
    }

    #[test]
    fn test_parse_conflict_line_patch_failed() {
        let engine = ApplyEngine::new(Path::new(".")).ok();
        if engine.is_none() {
            return;
        }

        let engine = engine.unwrap();

        let line = "patch failed: src/lib.rs:123";
        let info = engine.parse_git_apply_error_line(line);

        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.file, PathBuf::from("src/lib.rs"));
        assert_eq!(info.line, 123);
        assert_eq!(info.conflict_type, ConflictType::OverlappingChanges);
    }
}
