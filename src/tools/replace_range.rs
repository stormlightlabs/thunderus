//! Exact-range replace: replace a unique string occurrence in a file.
//!
//! This is the simplest safe edit primitive. The caller provides an exact
//! `old_string` that must appear **exactly once** in the file and a
//! `new_string` to replace it with. If `old_string` appears zero or multiple
//! times, the operation fails and the file is left unchanged.
//!
//! Enforces workspace-root containment. Failed edits never modify the file.

use std::path::Path;

use super::{ToolOutput, WriteOp, WriteResult, hash_content, path};

/// Replace a unique exact string occurrence in a file.
///
/// - Resolves and validates the path against `root`.
/// - Reads the current file content.
/// - Counts occurrences of `old_string`; fails if zero (not found) or >1
///   (ambiguous / stale range).
/// - Writes the file with the replacement applied.
/// - Returns a [`WriteResult`] with before/after metadata for audit.
///
/// On failure, the file is left unchanged.
pub fn exec(path_str: &str, root: &Path, old_string: &str, new_string: &str) -> (ToolOutput, Option<WriteResult>) {
    let resolved = match path::resolve_within_root(root, path_str) {
        Ok(p) => p,
        Err(e) => {
            return (ToolOutput::failed("replace_range", e.to_string()), None);
        }
    };

    let content = match std::fs::read_to_string(&resolved) {
        Ok(c) => c,
        Err(e) => {
            return (ToolOutput::failed("replace_range", format!("read failed: {e}")), None);
        }
    };

    let count = count_occurrences(&content, old_string);

    if count == 0 {
        return (
            ToolOutput::failed(
                "replace_range",
                format!("old_string not found in {}", resolved.display()),
            ),
            None,
        );
    }

    if count > 1 {
        return (
            ToolOutput::failed(
                "replace_range",
                format!(
                    "old_string appears {count} times in {}; expected exactly one",
                    resolved.display()
                ),
            ),
            None,
        );
    }

    let new_content = content.replacen(old_string, new_string, 1);

    let before_hash = hash_content(&content);
    let before_bytes = content.len();
    let after_hash = hash_content(&new_content);
    let after_bytes = new_content.len();

    match std::fs::write(&resolved, &new_content) {
        Err(e) => (ToolOutput::failed("replace_range", format!("write failed: {e}")), None),
        Ok(_) => {
            let result = WriteResult {
                op: WriteOp::Edit,
                path: resolved,
                before_hash: Some(before_hash),
                before_bytes: Some(before_bytes),
                after_hash,
                after_bytes,
            };

            (ToolOutput::ok("replace_range", vec![result.summary()]), Some(result))
        }
    }
}

/// Count non-overlapping occurrences of `needle` in `haystack`.
fn count_occurrences(haystack: &str, needle: &str) -> usize {
    if needle.is_empty() {
        return 0;
    }
    let mut count = 0;
    let mut start = 0;
    while let Some(pos) = haystack[start..].find(needle) {
        count += 1;
        start += pos + needle.len();
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ToolStatus;

    #[test]
    fn replace_range_success() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        let file = root.join("file.txt");
        std::fs::write(&file, "hello world\nfoo bar\n").expect("write");

        let (output, result) = exec("file.txt", root, "world", "there");
        assert_eq!(output.status, ToolStatus::Ok);
        assert!(result.is_some());

        let written = std::fs::read_to_string(&file).expect("read");
        assert_eq!(written, "hello there\nfoo bar\n");

        let r = result.unwrap();
        assert_eq!(r.op, WriteOp::Edit);
        assert!(r.before_hash.is_some());
        assert_eq!(r.before_bytes, Some(20));
        assert_eq!(r.after_bytes, 20);
    }

    #[test]
    fn replace_range_not_found_fails() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        let file = root.join("file.txt");
        std::fs::write(&file, "hello world\n").expect("write");

        let (output, result) = exec("file.txt", root, "nonexistent", "x");
        assert_eq!(output.status, ToolStatus::Failed);
        assert!(result.is_none());
        assert!(
            output.error.as_ref().is_some_and(|e| e.contains("not found")),
            "error should mention not found"
        );

        let content = std::fs::read_to_string(&file).expect("read");
        assert_eq!(content, "hello world\n");
    }

    #[test]
    fn replace_range_multiple_occurrences_fails() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        let file = root.join("file.txt");
        std::fs::write(&file, "foo foo foo\n").expect("write");

        let (output, result) = exec("file.txt", root, "foo", "bar");
        assert_eq!(output.status, ToolStatus::Failed);
        assert!(result.is_none());
        assert!(
            output.error.as_ref().is_some_and(|e| e.contains("3 times")),
            "error should mention multiple occurrences, got: {output:?}"
        );

        let content = std::fs::read_to_string(&file).expect("read");
        assert_eq!(content, "foo foo foo\n");
    }

    #[test]
    fn replace_range_stale_range_leaves_file_unchanged() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        let file = root.join("file.txt");
        std::fs::write(&file, "new content here\n").expect("write");

        let (output, result) = exec("file.txt", root, "old content", "replaced");

        assert_eq!(output.status, ToolStatus::Failed);
        assert!(result.is_none());

        let content = std::fs::read_to_string(&file).expect("read");
        assert_eq!(content, "new content here\n");
    }

    #[test]
    fn replace_range_empty_old_string_fails() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        let file = root.join("file.txt");

        std::fs::write(&file, "hello\n").expect("write");

        let (output, result) = exec("file.txt", root, "", "x");
        assert_eq!(output.status, ToolStatus::Failed);
        assert!(result.is_none());
        assert!(
            output.error.as_ref().is_some_and(|e| e.contains("not found")),
            "empty old_string should be treated as not found"
        );

        let content = std::fs::read_to_string(&file).expect("read");
        assert_eq!(content, "hello\n");
    }

    #[test]
    fn replace_range_multiline_old_string() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        let file = root.join("file.txt");
        std::fs::write(&file, "line1\nline2\nline3\n").expect("write");

        let (output, result) = exec("file.txt", root, "line1\nline2", "A\nB");
        assert_eq!(output.status, ToolStatus::Ok);
        assert!(result.is_some());

        let written = std::fs::read_to_string(&file).expect("read");
        assert_eq!(written, "A\nB\nline3\n");
    }

    #[test]
    fn replace_range_nonexistent_file_fails() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        let (output, result) = exec("nope.txt", root, "old", "new");
        assert_eq!(output.status, ToolStatus::Failed);
        assert!(result.is_none());
    }

    #[test]
    fn replace_range_outside_root_fails() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        let parent = root.parent().unwrap();
        let escape = parent.join("escape.txt");
        let escape_str = escape.to_string_lossy().to_string();
        let (output, result) = exec(&escape_str, root, "old", "new");
        assert_eq!(output.status, ToolStatus::Failed);
        assert!(result.is_none());
        assert!(
            output
                .error
                .as_ref()
                .is_some_and(|e| e.contains("escapes workspace root")),
            "error should mention workspace root escape"
        );
    }

    #[test]
    fn count_occurrences_basic() {
        assert_eq!(count_occurrences("hello world", "world"), 1);
        assert_eq!(count_occurrences("foo foo foo", "foo"), 3);
        assert_eq!(count_occurrences("abc", "xyz"), 0);
        assert_eq!(count_occurrences("abc", ""), 0);
    }

    #[test]
    fn count_occurrences_non_overlapping() {
        assert_eq!(count_occurrences("aaa", "aa"), 1);
    }
}
