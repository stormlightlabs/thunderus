/// Patch generation utilities for creating patches from edits
///
/// This module provides utilities for generating unified diffs from file edits,
/// which can then be applied through the patch queue system.
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Generate a unified diff for a single file edit
///
/// This creates a git-style unified diff from the old and new content of a file.
pub fn generate_unified_diff(
    file_path: &Path, old_content: &str, new_content: &str, base_snapshot: &str,
) -> Result<String, String> {
    let hunks = compute_hunks(old_content, new_content)?;

    if hunks.is_empty() {
        return Err("No changes detected".to_string());
    }

    let relative_path = if file_path.is_absolute() {
        file_path
            .strip_prefix(std::env::current_dir().unwrap_or(PathBuf::from(".")))
            .unwrap_or(file_path)
    } else {
        file_path
    };

    let path_str = relative_path.to_string_lossy();

    let mut diff = String::new();
    diff.push_str(&format!("diff --git a/{} b/{}\n", path_str, path_str));

    let snapshot_len = base_snapshot.len().min(7);
    diff.push_str(&format!(
        "index {}..{} 100644\n",
        &base_snapshot[..snapshot_len],
        &base_snapshot[..snapshot_len]
    ));

    for hunk in hunks {
        diff.push_str(&hunk);
    }

    Ok(diff)
}

/// Compute hunks between old and new content
fn compute_hunks(old_content: &str, new_content: &str) -> Result<Vec<String>, String> {
    let old_lines: Vec<&str> = old_content.lines().collect();
    let new_lines: Vec<&str> = new_content.lines().collect();

    let mut hunks = Vec::new();
    let mut context_lines = Vec::new();
    let old_context_start = 1;
    let new_context_start = 1;
    let mut old_deleted = 0;
    let mut new_added = 0;

    let max_lines = old_lines.len().max(new_lines.len());

    for i in 0..max_lines {
        let old_line = old_lines.get(i);
        let new_line = new_lines.get(i);

        match (old_line, new_line) {
            (Some(o), Some(n)) if o == n => {
                if old_deleted > 0 || new_added > 0 {
                    context_lines.push(format!(" {}", o));
                }

                if (old_deleted > 0 || new_added > 0) && context_lines.len() >= 3 {
                    hunks.push(format_hunk(
                        old_context_start,
                        new_context_start,
                        old_deleted,
                        new_added,
                        &context_lines,
                    ));
                    context_lines.clear();
                    old_deleted = 0;
                    new_added = 0;
                }
            }
            (Some(o), None) => {
                old_deleted += 1;
                context_lines.push(format!("-{}", o));
            }
            (None, Some(n)) => {
                new_added += 1;
                context_lines.push(format!("+{}", n));
            }
            (Some(o), Some(n)) => {
                old_deleted += 1;
                new_added += 1;
                context_lines.push(format!("-{}", o));
                context_lines.push(format!("+{}", n));
            }
            (None, None) => (),
        }
    }

    if old_deleted > 0 || new_added > 0 {
        hunks.push(format_hunk(
            old_context_start,
            new_context_start,
            old_deleted,
            new_added,
            &context_lines,
        ));
    }

    Ok(hunks)
}

/// Format a hunk in unified diff format
fn format_hunk(old_start: usize, new_start: usize, old_deleted: usize, new_added: usize, lines: &[String]) -> String {
    let old_count = if old_deleted > 0 { old_deleted + 3 } else { 1 }; // Include context
    let new_count = if new_added > 0 { new_added + 3 } else { 1 };

    let mut hunk = format!("@@ -{},{} +{},{} @@\n", old_start, old_count, new_start, new_count);

    for line in lines {
        hunk.push_str(line);
        hunk.push('\n');
    }

    hunk
}

/// Create a patch from file edits
pub struct PatchGenerator {
    /// Base snapshot (git commit)
    base_snapshot: String,
    /// File contents before edits
    original_contents: HashMap<PathBuf, String>,
}

impl PatchGenerator {
    /// Create a new patch generator
    pub fn new(base_snapshot: String) -> Self {
        Self { base_snapshot, original_contents: HashMap::new() }
    }

    /// Register the original content of a file before editing
    pub fn register_file(&mut self, file_path: &Path, content: String) {
        self.original_contents.insert(file_path.to_path_buf(), content);
    }

    /// Generate a unified diff for a file edit
    pub fn generate_diff(&self, file_path: &Path, new_content: &str) -> Result<String, String> {
        let old_content = self
            .original_contents
            .get(file_path)
            .ok_or_else(|| format!("Original content not found for file: {}", file_path.display()))?;

        generate_unified_diff(file_path, old_content, new_content, &self.base_snapshot)
    }

    /// Get the original content of a file
    pub fn get_original(&self, file_path: &Path) -> Option<&String> {
        self.original_contents.get(file_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_unified_diff_no_changes() {
        let old = "line1\nline2\nline3";
        let new = "line1\nline2\nline3";

        let result = generate_unified_diff(Path::new("test.txt"), old, new, "abc123");

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "No changes detected");
    }

    #[test]
    fn test_generate_unified_diff_single_change() {
        let old = "line1\nold line\nline3";
        let new = "line1\nnew line\nline3";

        let result = generate_unified_diff(Path::new("test.txt"), old, new, "abc123");

        assert!(result.is_ok());
        let diff = result.unwrap();

        assert!(diff.contains("diff --git"));
        assert!(diff.contains("-old line"));
        assert!(diff.contains("+new line"));
        assert!(diff.contains("@@"));
    }

    #[test]
    fn test_patch_generator() {
        let mut generator = PatchGenerator::new("abc123".to_string());

        let file_path = Path::new("test.txt");
        let old_content = "line1\nline2\nline3";

        generator.register_file(file_path, old_content.to_string());

        assert_eq!(generator.get_original(file_path), Some(&old_content.to_string()));
    }

    #[test]
    fn test_patch_generator_generate_diff() {
        let mut generator = PatchGenerator::new("abc123".to_string());

        let file_path = Path::new("test.txt");
        let old_content = "line1\nold line\nline3";
        let new_content = "line1\nnew line\nline3";

        generator.register_file(file_path, old_content.to_string());

        let diff = generator.generate_diff(file_path, new_content).unwrap();

        assert!(diff.contains("-old line"));
        assert!(diff.contains("+new line"));
    }
}
