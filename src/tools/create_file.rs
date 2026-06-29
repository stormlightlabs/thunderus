//! Create a new file with given content.
//!
//! Enforces workspace-root containment. Fails if the file already exists,
//! preventing accidental overwrites. Parent directories are created if needed.

use std::path::Path;

use super::{ToolOutput, WriteOp, WriteResult, hash_content, path};

/// Create a new file at `path` (relative to `root`) with the given `content`.
///
/// - Resolves and validates the path against `root`.
/// - Fails if the file already exists.
/// - Creates parent directories as needed.
/// - Returns a [`WriteResult`] with before/after metadata for audit.
///
/// On failure, the file is left unchanged.
pub fn exec(path_str: &str, root: &Path, content: &str) -> (ToolOutput, Option<WriteResult>) {
    let resolved = match path::resolve_within_root(root, path_str) {
        Ok(p) => p,
        Err(e) => {
            return (ToolOutput::failed("create_file", e.to_string()), None);
        }
    };

    if resolved.exists() {
        return (
            ToolOutput::failed("create_file", format!("file already exists: {}", resolved.display())),
            None,
        );
    }

    if let Some(parent) = resolved.parent()
        && !parent.exists()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        return (
            ToolOutput::failed("create_file", format!("failed to create directories: {e}")),
            None,
        );
    }

    let after_hash = hash_content(content);
    let after_bytes = content.len();

    if let Err(e) = std::fs::write(&resolved, content) {
        return (ToolOutput::failed("create_file", format!("write failed: {e}")), None);
    }

    let result = WriteResult {
        op: WriteOp::Create,
        path: resolved,
        before_hash: None,
        before_bytes: None,
        after_hash,
        after_bytes,
    };

    (ToolOutput::ok("create_file", vec![result.summary()]), Some(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ToolStatus;

    #[test]
    fn create_file_success() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        let (output, result) = exec("new_file.txt", root, "hello world\n");
        assert_eq!(output.status, ToolStatus::Ok);
        assert!(result.is_some());

        let written = std::fs::read_to_string(root.join("new_file.txt")).expect("read file");
        assert_eq!(written, "hello world\n");

        let r = result.unwrap();
        assert_eq!(r.op, WriteOp::Create);
        assert!(r.before_hash.is_none());
        assert!(r.before_bytes.is_none());
        assert_eq!(r.after_bytes, 12);
        assert_eq!(r.after_hash, hash_content("hello world\n"));
    }

    #[test]
    fn create_file_creates_parent_directories() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        let (output, result) = exec("nested/dir/file.txt", root, "content");
        assert_eq!(output.status, ToolStatus::Ok);
        assert!(result.is_some());

        let written = std::fs::read_to_string(root.join("nested/dir/file.txt")).expect("read file");
        assert_eq!(written, "content");
    }

    #[test]
    fn create_file_already_exists_fails() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();

        std::fs::write(root.join("exists.txt"), "old content").expect("write");

        let (output, result) = exec("exists.txt", root, "new content");
        assert_eq!(output.status, ToolStatus::Failed);
        assert!(result.is_none());
        assert!(
            output.error.as_ref().is_some_and(|e| e.contains("already exists")),
            "error should mention already exists, got: {output:?}"
        );

        let content = std::fs::read_to_string(root.join("exists.txt")).expect("read file");
        assert_eq!(content, "old content");
    }

    #[test]
    fn create_file_outside_root_fails() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        let parent = root.parent().unwrap();
        let escape_path = parent.join("escape.txt");
        let escape_str = escape_path.to_string_lossy().to_string();
        let (output, result) = exec(&escape_str, root, "content");

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
    fn create_file_empty_content() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        let (output, result) = exec("empty.txt", root, "");
        assert_eq!(output.status, ToolStatus::Ok);
        assert!(result.is_some());

        let r = result.unwrap();
        assert_eq!(r.after_bytes, 0);

        let written = std::fs::read_to_string(root.join("empty.txt")).expect("read file");
        assert!(written.is_empty());
    }
}
