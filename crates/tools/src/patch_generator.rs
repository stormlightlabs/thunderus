//! Patch generation utilities for creating patches from edits
//!
//! This module provides utilities for generating unified diffs from file edits,
//! which can then be applied through the patch queue system.
use imara_diff::{Algorithm, BasicLineDiffPrinter, Diff, InternedInput, UnifiedDiffConfig};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Generate a unified diff for a single file edit
///
/// This creates a git-style unified diff from the old and new content of a file using
/// the Histogram diff algorithm for better semantic diffing of moved/refactored code.
pub fn generate_unified_diff(
    file_path: &Path, old_content: &str, new_content: &str, base_snapshot: &str,
) -> Result<String, String> {
    if old_content == new_content {
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

    let input = InternedInput::new(old_content, new_content);
    let mut diff = Diff::compute(Algorithm::Histogram, &input);
    diff.postprocess_lines(&input);
    let mut result = String::new();
    result.push_str(&format!("diff --git a/{} b/{}\n", path_str, path_str));

    let snapshot_len = base_snapshot.len().min(7);
    result.push_str(&format!(
        "index {}..{} 100644\n",
        &base_snapshot[..snapshot_len],
        &base_snapshot[..snapshot_len]
    ));

    let printer = BasicLineDiffPrinter(&input.interner);
    let unified = diff.unified_diff(&printer, UnifiedDiffConfig::default(), &input);

    result.push_str(&unified.to_string());

    Ok(result)
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

    #[test]
    fn test_histogram_diff_for_moved_blocks() {
        let old = r#"fn function_one() {
    println!("one");
}

fn function_two() {
    println!("two");
}

fn function_three() {
    println!("three");
}"#;

        let new = r#"fn function_two() {
    println!("two");
}

fn function_one() {
    println!("one");
}

fn function_three() {
    println!("three");
}"#;

        let result = generate_unified_diff(Path::new("test.rs"), old, new, "abc123");
        assert!(result.is_ok());
        let diff = result.unwrap();
        assert!(diff.contains("diff --git"));
        assert!(diff.contains("@@"));
    }

    #[test]
    fn test_histogram_diff_for_refactored_code() {
        let old = r#"struct Data {
    name: String,
    value: i32,
}

impl Data {
    fn new(name: String, value: i32) -> Self {
        Self { name, value }
    }

    fn get_value(&self) -> i32 {
        self.value
    }
}"#;

        let new = r#"struct Data {
    name: String,
    value: i32,
    description: Option<String>,
}

impl Data {
    fn new(name: String, value: i32) -> Self {
        Self { name, value, description: None }
    }

    fn with_description(mut self, desc: String) -> Self {
        self.description = Some(desc);
        self
    }

    fn get_value(&self) -> i32 {
        self.value
    }
}"#;

        let result = generate_unified_diff(Path::new("test.rs"), old, new, "abc123");
        assert!(result.is_ok());
        let diff = result.unwrap();
        assert!(diff.contains("description"));
        assert!(diff.contains("with_description"));
        assert!(diff.contains("Option<String>"));
    }

    #[test]
    fn test_histogram_diff_with_indentation_changes() {
        let old = r#"if condition {
    do_something();
    do_another();
}"#;

        let new = r#"
if condition {
    do_something();
    do_another();
    do_extra();
}"#;

        let result = generate_unified_diff(Path::new("test.rs"), old, new, "abc123");
        assert!(result.is_ok());
        let diff = result.unwrap();
        assert!(diff.contains("do_extra"));
    }
}
