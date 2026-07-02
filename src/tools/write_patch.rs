//! Structured file write operations.
//!
//! A patch specifies one file operation (`create`, `replace`, or `edit`) plus
//! the fields needed for that operation. `edit` supports either the legacy
//! single `old_string`/`new_string` pair or an `edits` array of disjoint
//! replacements that are all matched against the original file.
//!
//! All operations enforce workspace-root containment. Failed patches leave the
//! target file unchanged.

use std::path::Path;

use super::{ToolOutput, WriteOp, WriteResult, create_file, path, replace_range};
use replace_range::Replacement;

/// A structured patch describing a single file write operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Patch {
    /// Create a new file. Fails if it already exists.
    Create {
        /// Path relative to the workspace root.
        path: String,
        /// Full file content to write.
        content: String,
    },
    /// Replace the entire contents of an existing or new file.
    Replace {
        /// Path relative to the workspace root.
        path: String,
        /// Full file content to write.
        content: String,
        /// Optional current-content hash guard to reject stale rewrites.
        expected_before_hash: Option<u64>,
    },
    /// Edit a file by replacing one or more unique exact string occurrences.
    Edit {
        /// Path relative to the workspace root.
        path: String,
        /// Disjoint replacements matched against the same original file.
        edits: Vec<Replacement>,
        /// Optional current-content hash guard to reject stale edits.
        expected_before_hash: Option<u64>,
    },
}

impl Patch {
    /// Parse a patch from a JSON arguments string.
    ///
    /// The JSON must have an `op` field (`"create"`, `"replace"`, or `"edit"`)
    /// and the fields required by that operation. For `edit`, callers may use
    /// either legacy `old_string`/`new_string` fields or an `edits` array whose
    /// entries contain `old_string` and `new_string`.
    pub fn from_json(args: &str) -> Result<Self, String> {
        let v = serde_json::from_str::<serde_json::Value>(args).map_err(|e| format!("invalid arguments: {e}"))?;

        let op = v
            .get("op")
            .and_then(|o| o.as_str())
            .ok_or_else(|| "missing or non-string 'op' field".to_string())?;

        let path = v
            .get("path")
            .and_then(|p| p.as_str())
            .ok_or_else(|| "missing or non-string 'path' field".to_string())?
            .to_string();
        let expected_before_hash = v.get("expected_before_hash").and_then(|h| h.as_u64());

        match op {
            "create" => {
                let content = string_field(&v, "content", "create requires a 'content' field")?;
                Ok(Patch::Create { path, content })
            }
            "replace" => {
                let content = string_field(&v, "content", "replace requires a 'content' field")?;
                Ok(Patch::Replace { path, content, expected_before_hash })
            }
            "edit" => Ok(Patch::Edit { path, edits: parse_edits(&v)?, expected_before_hash }),
            other => Err(format!(
                "unknown patch op: '{other}' (expected create, replace, or edit)"
            )),
        }
    }
}

/// Apply a structured patch to a file.
///
/// Dispatches to the appropriate write primitive based on the patch operation.
/// On failure, the file is left unchanged and no [`WriteResult`] is returned.
pub fn exec(patch: &Patch, root: &Path) -> (ToolOutput, Option<WriteResult>) {
    match patch {
        Patch::Create { path, content } => create_file::exec(path, root, content),
        Patch::Replace { path, content, expected_before_hash } => {
            exec_replace(path, root, content, *expected_before_hash)
        }
        Patch::Edit { path, edits, expected_before_hash } => {
            replace_range::exec_many(path, root, edits, *expected_before_hash)
        }
    }
}

fn string_field(v: &serde_json::Value, field: &str, message: &str) -> Result<String, String> {
    v.get(field)
        .and_then(|s| s.as_str())
        .map(str::to_string)
        .ok_or_else(|| message.to_string())
}

fn parse_edits(v: &serde_json::Value) -> Result<Vec<Replacement>, String> {
    if let Some(edits) = v.get("edits") {
        let edits = edits
            .as_array()
            .ok_or_else(|| "edit field 'edits' must be an array".to_string())?;
        if edits.is_empty() {
            return Err("edit field 'edits' must contain at least one replacement".to_string());
        }
        return edits
            .iter()
            .enumerate()
            .map(|(i, edit)| {
                let old_string = edit
                    .get("old_string")
                    .and_then(|s| s.as_str())
                    .ok_or_else(|| format!("edits[{i}] requires an 'old_string' field"))?
                    .to_string();
                let new_string = edit
                    .get("new_string")
                    .and_then(|s| s.as_str())
                    .ok_or_else(|| format!("edits[{i}] requires a 'new_string' field"))?
                    .to_string();
                Ok(Replacement { old_string, new_string })
            })
            .collect();
    }

    let old_string = string_field(v, "old_string", "edit requires an 'old_string' field")?;
    let new_string = string_field(v, "new_string", "edit requires a 'new_string' field")?;
    Ok(vec![Replacement { old_string, new_string }])
}

/// Replace the entire contents of a file.
///
/// Unlike `create_file`, this overwrites an existing file. If
/// `expected_before_hash` is supplied, the current file hash must match before
/// any bytes are written.
fn exec_replace(
    path_str: &str, root: &Path, content: &str, expected_before_hash: Option<u64>,
) -> (ToolOutput, Option<WriteResult>) {
    let resolved = match path::resolve_within_root(root, path_str) {
        Ok(p) => p,
        Err(e) => return (ToolOutput::failed("write_patch", e.to_string()), None),
    };

    replace_range::with_file_lock(&resolved, || {
        exec_replace_locked(&resolved, content, expected_before_hash)
    })
}

fn exec_replace_locked(
    resolved: &Path, content: &str, expected_before_hash: Option<u64>,
) -> (ToolOutput, Option<WriteResult>) {
    let (before_hash, before_bytes) = match std::fs::read_to_string(resolved) {
        Ok(existing) => (Some(super::hash_content(&existing)), Some(existing.len())),
        Err(e) => match e.kind() {
            std::io::ErrorKind::NotFound => (None, None),
            _ => return (ToolOutput::failed("write_patch", format!("read failed: {e}")), None),
        },
    };

    if let Some(expected) = expected_before_hash
        && before_hash != Some(expected)
    {
        let current = before_hash
            .map(|h| h.to_string())
            .unwrap_or_else(|| "missing file".to_string());
        return (
            ToolOutput::failed(
                "write_patch",
                format!("stale file: current hash {current} does not match expected_before_hash {expected}"),
            ),
            None,
        );
    }

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

    match std::fs::write(resolved, content) {
        Err(e) => (ToolOutput::failed("write_patch", format!("write failed: {e}")), None),
        Ok(_) => {
            let result = WriteResult {
                op: WriteOp::Replace,
                path: resolved.to_path_buf(),
                before_hash,
                before_bytes,
                after_hash,
                after_bytes,
            };
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
    }

    #[test]
    fn patch_from_json_replace() {
        let args = r#"{"op":"replace","path":"a.txt","content":"world","expected_before_hash":7}"#;
        let patch = Patch::from_json(args).expect("parse");
        assert_eq!(
            patch,
            Patch::Replace { path: "a.txt".to_string(), content: "world".to_string(), expected_before_hash: Some(7) }
        );
    }

    #[test]
    fn patch_from_json_edit_legacy() {
        let args = r#"{"op":"edit","path":"a.txt","old_string":"foo","new_string":"bar"}"#;
        let patch = Patch::from_json(args).expect("parse");
        assert_eq!(
            patch,
            Patch::Edit {
                path: "a.txt".to_string(),
                edits: vec![Replacement { old_string: "foo".to_string(), new_string: "bar".to_string() }],
                expected_before_hash: None,
            }
        );
    }

    #[test]
    fn patch_from_json_edit_array() {
        let args = r#"{"op":"edit","path":"a.txt","edits":[{"old_string":"foo","new_string":"bar"},{"old_string":"baz","new_string":"qux"}]}"#;
        let patch = Patch::from_json(args).expect("parse");
        assert_eq!(
            patch,
            Patch::Edit {
                path: "a.txt".to_string(),
                edits: vec![
                    Replacement { old_string: "foo".to_string(), new_string: "bar".to_string() },
                    Replacement { old_string: "baz".to_string(), new_string: "qux".to_string() },
                ],
                expected_before_hash: None,
            }
        );
    }

    #[test]
    fn patch_from_json_unknown_op_rejected() {
        let args = r#"{"op":"delete","path":"a.txt"}"#;
        let result = Patch::from_json(args);
        assert!(result.is_err());
        assert!(result.as_ref().unwrap_err().contains("unknown patch op"));
    }

    #[test]
    fn patch_from_json_missing_op_rejected() {
        let args = r#"{"path":"a.txt","content":"x"}"#;
        assert!(Patch::from_json(args).is_err());
    }

    #[test]
    fn patch_from_json_edit_missing_old_string_rejected() {
        let args = r#"{"op":"edit","path":"a.txt","new_string":"bar"}"#;
        assert!(Patch::from_json(args).is_err());
    }

    #[test]
    fn patch_apply_create_success() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        let patch = Patch::Create { path: "new.txt".to_string(), content: "hello\n".to_string() };
        let (output, result) = exec(&patch, root);
        assert_eq!(output.status, ToolStatus::Ok);
        assert!(result.is_some());
        assert_eq!(std::fs::read_to_string(root.join("new.txt")).expect("read"), "hello\n");
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
        assert_eq!(std::fs::read_to_string(root.join("exists.txt")).expect("read"), "old");
    }

    #[test]
    fn patch_apply_replace_existing_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        std::fs::write(root.join("file.txt"), "old content\n").expect("write");

        let patch = Patch::Replace {
            path: "file.txt".to_string(),
            content: "new content\n".to_string(),
            expected_before_hash: None,
        };
        let (output, result) = exec(&patch, root);
        assert_eq!(output.status, ToolStatus::Ok);
        assert!(result.is_some());
        assert_eq!(
            std::fs::read_to_string(root.join("file.txt")).expect("read"),
            "new content\n"
        );
    }

    #[test]
    fn patch_apply_replace_rejects_stale_hash() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        std::fs::write(root.join("file.txt"), "old content\n").expect("write");

        let patch = Patch::Replace {
            path: "file.txt".to_string(),
            content: "new content\n".to_string(),
            expected_before_hash: Some(123),
        };
        let (output, result) = exec(&patch, root);
        assert_eq!(output.status, ToolStatus::Failed);
        assert!(result.is_none());
        assert_eq!(
            std::fs::read_to_string(root.join("file.txt")).expect("read"),
            "old content\n"
        );
    }

    #[test]
    fn patch_apply_edit_multiple_success() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        std::fs::write(root.join("file.txt"), "hello world\nbye moon\n").expect("write");

        let patch = Patch::Edit {
            path: "file.txt".to_string(),
            edits: vec![
                Replacement { old_string: "world".to_string(), new_string: "there".to_string() },
                Replacement { old_string: "moon".to_string(), new_string: "sun".to_string() },
            ],
            expected_before_hash: None,
        };

        let (output, result) = exec(&patch, root);
        assert_eq!(output.status, ToolStatus::Ok);
        assert!(result.is_some());
        assert_eq!(
            std::fs::read_to_string(root.join("file.txt")).expect("read"),
            "hello there\nbye sun\n"
        );
    }

    #[test]
    fn patch_apply_edit_not_found_fails() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        std::fs::write(root.join("file.txt"), "hello\n").expect("write");

        let patch = Patch::Edit {
            path: "file.txt".to_string(),
            edits: vec![Replacement { old_string: "nonexistent".to_string(), new_string: "x".to_string() }],
            expected_before_hash: None,
        };

        let (output, result) = exec(&patch, root);
        assert_eq!(output.status, ToolStatus::Failed);
        assert!(result.is_none());
        assert_eq!(std::fs::read_to_string(root.join("file.txt")).expect("read"), "hello\n");
    }

    #[test]
    fn patch_apply_outside_root_fails() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        let outside = root.parent().unwrap().join("escape.txt");
        let patch = Patch::Replace {
            path: outside.to_string_lossy().to_string(),
            content: "oops".to_string(),
            expected_before_hash: None,
        };

        let (output, result) = exec(&patch, root);
        assert_eq!(output.status, ToolStatus::Failed);
        assert!(result.is_none());
        assert!(
            output
                .error
                .as_ref()
                .is_some_and(|e| e.contains("escapes workspace root"))
        );
    }
}
