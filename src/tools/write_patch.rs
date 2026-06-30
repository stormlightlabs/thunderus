//! Unified patch apply: apply a structured patch to a file.
//!
//! A patch specifies an operation (`create`, `replace`, or `edit`) plus the
//! fields needed for that operation. This provides a single tool entry point
//! for all write operations, delegating to the appropriate primitive.
//!
//! - `create`: calls [`create_file::exec`] — fails if the file already exists.
//! - `replace`: writes full new content, overwriting any existing file.
//! - `edit`: calls [`replace_range::exec`] — replaces a unique exact string.
//!
//! All operations enforce workspace-root containment. Failed patches never
//! modify the file.

use std::path::Path;

use super::{ToolOutput, WriteOp, WriteResult, create_file, path, replace_range};

/// A structured patch describing a single file write operation.
///
/// Each variant carries the minimum fields needed for that operation. The
/// `op` field is derived from the variant, not passed separately.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Patch {
    /// Create a new file. Fails if it already exists.
    Create {
        /// Path relative to the workspace root.
        path: String,
        /// Full file content to write.
        content: String,
    },
    /// Replace the entire contents of an existing (or new) file.
    Replace {
        /// Path relative to the workspace root.
        path: String,
        /// Full file content to write.
        content: String,
    },
    /// Edit a file by replacing a unique exact string occurrence.
    Edit {
        /// Path relative to the workspace root.
        path: String,
        /// The exact string to find. Must appear exactly once.
        old_string: String,
        /// The replacement string.
        new_string: String,
    },
}

impl Patch {
    /// The operation kind this patch represents.
    ///
    /// Accessor retained for callers that need the op without pattern-matching.
    #[allow(dead_code)]
    pub fn op(&self) -> WriteOp {
        match self {
            Patch::Create { .. } => WriteOp::Create,
            Patch::Replace { .. } => WriteOp::Replace,
            Patch::Edit { .. } => WriteOp::Edit,
        }
    }

    /// The target file path (relative to root).
    ///
    /// Accessor retained for callers that need the path without pattern-matching.
    #[allow(dead_code)]
    pub fn path(&self) -> &str {
        match self {
            Patch::Create { path, .. } | Patch::Replace { path, .. } | Patch::Edit { path, .. } => path,
        }
    }

    /// Parse a patch from a JSON arguments string.
    ///
    /// The JSON must have an `op` field (`"create"`, `"replace"`, or `"edit"`)
    /// and the fields required by that operation.
    ///
    /// Unknown `op` values or missing required fields produce an error.
    pub fn from_json(args: &str) -> Result<Self, String> {
        let v: serde_json::Value = serde_json::from_str(args).map_err(|e| format!("invalid arguments: {e}"))?;

        let op = v
            .get("op")
            .and_then(|o| o.as_str())
            .ok_or_else(|| "missing or non-string 'op' field".to_string())?;

        let path = v
            .get("path")
            .and_then(|p| p.as_str())
            .ok_or_else(|| "missing or non-string 'path' field".to_string())?
            .to_string();

        match op {
            "create" => {
                let content = v
                    .get("content")
                    .and_then(|c| c.as_str())
                    .ok_or_else(|| "create requires a 'content' field".to_string())?
                    .to_string();
                Ok(Patch::Create { path, content })
            }
            "replace" => {
                let content = v
                    .get("content")
                    .and_then(|c| c.as_str())
                    .ok_or_else(|| "replace requires a 'content' field".to_string())?
                    .to_string();
                Ok(Patch::Replace { path, content })
            }
            "edit" => {
                let old_string = v
                    .get("old_string")
                    .and_then(|s| s.as_str())
                    .ok_or_else(|| "edit requires an 'old_string' field".to_string())?
                    .to_string();
                let new_string = v
                    .get("new_string")
                    .and_then(|s| s.as_str())
                    .ok_or_else(|| "edit requires a 'new_string' field".to_string())?
                    .to_string();
                Ok(Patch::Edit { path, old_string, new_string })
            }
            other => Err(format!(
                "unknown patch op: '{other}' (expected create, replace, or edit)"
            )),
        }
    }
}

/// Apply a structured patch to a file.
///
/// Dispatches to the appropriate write primitive based on the patch operation.
/// All operations enforce workspace-root containment. On failure, the file is
/// left unchanged and no [`WriteResult`] is returned.
pub fn exec(patch: &Patch, root: &Path) -> (ToolOutput, Option<WriteResult>) {
    match patch {
        Patch::Create { path, content } => create_file::exec(path, root, content),
        Patch::Replace { path, content } => exec_replace(path, root, content),
        Patch::Edit { path, old_string, new_string } => replace_range::exec(path, root, old_string, new_string),
    }
}

/// Replace the entire contents of a file.
///
/// Unlike `create_file`, this overwrites an existing file. Records before/after
/// metadata for audit. On failure, the file is left unchanged.
fn exec_replace(path_str: &str, root: &Path, content: &str) -> (ToolOutput, Option<WriteResult>) {
    let resolved = match path::resolve_within_root(root, path_str) {
        Ok(p) => p,
        Err(e) => {
            return (ToolOutput::failed("write_patch", e.to_string()), None);
        }
    };

    let (before_hash, before_bytes) = match std::fs::read_to_string(&resolved) {
        Ok(existing) => (Some(super::hash_content(&existing)), Some(existing.len())),
        Err(_) => (None, None),
    };

    let after_hash = super::hash_content(content);
    let after_bytes = content.len();

    if let Some(parent) = resolved.parent()
        && !parent.exists()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        return (
            ToolOutput::failed("write_patch", format!("failed to create directories: {e}")),
            None,
        );
    }

    match std::fs::write(&resolved, content) {
        Err(e) => (ToolOutput::failed("write_patch", format!("write failed: {e}")), None),
        Ok(_) => {
            let op = WriteOp::Replace;
            let result = WriteResult { op, path: resolved, before_hash, before_bytes, after_hash, after_bytes };
            (ToolOutput::ok("write_patch", vec![result.summary()]), Some(result))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ToolStatus;

    #[test]
    fn patch_from_json_create() {
        let args = r#"{"op":"create","path":"a.txt","content":"hello"}"#;
        let patch = Patch::from_json(args).expect("parse");
        assert_eq!(
            patch,
            Patch::Create { path: "a.txt".to_string(), content: "hello".to_string() }
        );
        assert_eq!(patch.op(), WriteOp::Create);
    }

    #[test]
    fn patch_from_json_replace() {
        let args = r#"{"op":"replace","path":"a.txt","content":"world"}"#;
        let patch = Patch::from_json(args).expect("parse");
        assert_eq!(
            patch,
            Patch::Replace { path: "a.txt".to_string(), content: "world".to_string() }
        );
        assert_eq!(patch.op(), WriteOp::Replace);
    }

    #[test]
    fn patch_from_json_edit() {
        let args = r#"{"op":"edit","path":"a.txt","old_string":"foo","new_string":"bar"}"#;
        let patch = Patch::from_json(args).expect("parse");
        assert_eq!(
            patch,
            Patch::Edit { path: "a.txt".to_string(), old_string: "foo".to_string(), new_string: "bar".to_string() }
        );
        assert_eq!(patch.op(), WriteOp::Edit);
    }

    #[test]
    fn patch_from_json_unknown_op_rejected() {
        let args = r#"{"op":"delete","path":"a.txt"}"#;
        let result = Patch::from_json(args);
        assert!(result.is_err());
        assert!(
            result.as_ref().unwrap_err().contains("unknown patch op"),
            "error should mention unknown op"
        );
    }

    #[test]
    fn patch_from_json_missing_op_rejected() {
        let args = r#"{"path":"a.txt","content":"x"}"#;
        assert!(Patch::from_json(args).is_err());
    }

    #[test]
    fn patch_from_json_missing_path_rejected() {
        let args = r#"{"op":"create","content":"x"}"#;
        assert!(Patch::from_json(args).is_err());
    }

    #[test]
    fn patch_from_json_edit_missing_old_string_rejected() {
        let args = r#"{"op":"edit","path":"a.txt","new_string":"bar"}"#;
        assert!(Patch::from_json(args).is_err());
    }

    #[test]
    fn patch_from_json_malformed_json_rejected() {
        assert!(Patch::from_json("not json").is_err());
    }

    #[test]
    fn patch_apply_create_success() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        let patch = Patch::Create { path: "new.txt".to_string(), content: "hello\n".to_string() };
        let (output, result) = exec(&patch, root);
        assert_eq!(output.status, ToolStatus::Ok);
        assert!(result.is_some());

        let written = std::fs::read_to_string(root.join("new.txt")).expect("read");
        assert_eq!(written, "hello\n");

        let r = result.unwrap();
        assert_eq!(r.op, WriteOp::Create);
        assert!(r.before_hash.is_none());
    }

    #[test]
    fn patch_apply_create_already_exists_fails() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        std::fs::write(root.join("exists.txt"), "old").expect("write");

        let patch = Patch::Create { path: "exists.txt".to_string(), content: "new".to_string() };
        let (output, result) = exec(&patch, root);
        assert_eq!(output.status, ToolStatus::Failed);
        assert!(result.is_none());

        let content = std::fs::read_to_string(root.join("exists.txt")).expect("read");
        assert_eq!(content, "old");
    }

    #[test]
    fn patch_apply_replace_existing_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        std::fs::write(root.join("file.txt"), "old content\n").expect("write");

        let patch = Patch::Replace { path: "file.txt".to_string(), content: "new content\n".to_string() };
        let (output, result) = exec(&patch, root);
        assert_eq!(output.status, ToolStatus::Ok);
        assert!(result.is_some());

        let written = std::fs::read_to_string(root.join("file.txt")).expect("read");
        assert_eq!(written, "new content\n");

        let r = result.unwrap();
        assert_eq!(r.op, WriteOp::Replace);
        assert!(r.before_hash.is_some());
        assert_eq!(r.before_bytes, Some(12));
        assert_eq!(r.after_bytes, 12);
    }

    #[test]
    fn patch_apply_replace_creates_new_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        let patch = Patch::Replace { path: "brand_new.txt".to_string(), content: "content\n".to_string() };
        let (output, result) = exec(&patch, root);
        assert_eq!(output.status, ToolStatus::Ok);
        assert!(result.is_some());

        let written = std::fs::read_to_string(root.join("brand_new.txt")).expect("read");
        assert_eq!(written, "content\n");

        let r = result.unwrap();
        assert_eq!(r.op, WriteOp::Replace);
        assert!(r.before_hash.is_none());
    }

    #[test]
    fn patch_apply_edit_success() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        std::fs::write(root.join("file.txt"), "hello world\n").expect("write");

        let patch = Patch::Edit {
            path: "file.txt".to_string(),
            old_string: "world".to_string(),
            new_string: "there".to_string(),
        };

        let (output, result) = exec(&patch, root);
        assert_eq!(output.status, ToolStatus::Ok);
        assert!(result.is_some());

        let written = std::fs::read_to_string(root.join("file.txt")).expect("read");
        assert_eq!(written, "hello there\n");

        let r = result.unwrap();
        assert_eq!(r.op, WriteOp::Edit);
        assert!(r.before_hash.is_some());
    }

    #[test]
    fn patch_apply_edit_not_found_fails() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        std::fs::write(root.join("file.txt"), "hello\n").expect("write");

        let patch = Patch::Edit {
            path: "file.txt".to_string(),
            old_string: "nonexistent".to_string(),
            new_string: "x".to_string(),
        };

        let (output, result) = exec(&patch, root);
        assert_eq!(output.status, ToolStatus::Failed);
        assert!(result.is_none());

        let content = std::fs::read_to_string(root.join("file.txt")).expect("read");
        assert_eq!(content, "hello\n");
    }

    #[test]
    fn patch_apply_edit_multiple_occurrences_fails() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        std::fs::write(root.join("file.txt"), "foo foo foo\n").expect("write");

        let patch =
            Patch::Edit { path: "file.txt".to_string(), old_string: "foo".to_string(), new_string: "bar".to_string() };

        let (output, result) = exec(&patch, root);
        assert_eq!(output.status, ToolStatus::Failed);
        assert!(result.is_none());

        let content = std::fs::read_to_string(root.join("file.txt")).expect("read");
        assert_eq!(content, "foo foo foo\n");
    }

    #[test]
    fn patch_apply_outside_root_fails() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        let parent = root.parent().unwrap();
        let escape = parent.join("escape.txt");
        let escape_str = escape.to_string_lossy().to_string();
        let patch = Patch::Create { path: escape_str, content: "x".to_string() };
        let (output, result) = exec(&patch, root);
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
    fn patch_apply_failed_edit_leaves_file_unchanged() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        std::fs::write(root.join("file.txt"), "original content\n").expect("write");

        let patch = Patch::Edit {
            path: "file.txt".to_string(),
            old_string: "nonexistent string".to_string(),
            new_string: "replacement".to_string(),
        };

        let (output, result) = exec(&patch, root);
        assert_eq!(output.status, ToolStatus::Failed);
        assert!(result.is_none());

        let content = std::fs::read_to_string(root.join("file.txt")).expect("read");
        assert_eq!(content, "original content\n");
    }
}
