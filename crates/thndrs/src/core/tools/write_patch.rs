//! Structured file write operations.
//!
//! A call contains a `patches` array. Each patch specifies one file operation
//! (`create`, `replace`, or `edit`) plus the fields needed for that operation.
//! Multiple edits may be batched for one file and are all matched against the
//! original content.
//!
//! Relative paths resolve from the workspace root. Failed patches leave the
//! target file unchanged.

use std::path::Path;

use super::{
    ToolDefinition, ToolOutput, ToolUseRequest, WriteOp, WriteResult, atomic_write, create_file, path, replace_range,
};
use crate::tools::registry::{ToolContext, ToolExecution};
use replace_range::Replacement;

const NAME: &str = "write_patch";

/// A structured patch describing a single file write operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Patch {
    /// Create a new file. Fails if it already exists.
    Create {
        /// Absolute path or path relative to the workspace root.
        path: String,
        /// Full file content to write.
        content: String,
    },
    /// Replace the entire contents of an existing or new file.
    Replace {
        /// Absolute path or path relative to the workspace root.
        path: String,
        /// Full file content to write.
        content: String,
        /// Optional current-content hash guard to reject stale rewrites.
        expected_before_hash: Option<u64>,
    },
    /// Edit a file by replacing one or more unique exact string occurrences.
    Edit {
        /// Absolute path or path relative to the workspace root.
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
    /// A multi-item batch must contain edits for one file so it can be applied
    /// as one validated write.
    pub fn from_json(args: &str) -> Result<Self, String> {
        let v = serde_json::from_str::<serde_json::Value>(args).map_err(|e| format!("invalid arguments: {e}"))?;
        let patches = v.get("patches").ok_or_else(|| "missing 'patches' field".to_string())?;
        Self::parse_patch_batch(patches)
    }

    fn parse_patch(v: &serde_json::Value) -> Result<Patch, String> {
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
                let content = string_field(v, "content", "create requires a 'content' field")?;
                Ok(Patch::Create { path, content })
            }
            "replace" => {
                let content = string_field(v, "content", "replace requires a 'content' field")?;
                Ok(Patch::Replace { path, content, expected_before_hash })
            }
            "edit" => {
                let old_string = string_field(v, "old_string", "edit requires an 'old_string' field")?;
                let new_string = string_field(v, "new_string", "edit requires a 'new_string' field")?;
                Ok(Patch::Edit { path, edits: vec![Replacement { old_string, new_string }], expected_before_hash })
            }
            other => Err(format!(
                "unknown patch op: '{other}' (expected create, replace, or edit)"
            )),
        }
    }

    fn parse_patch_batch(value: &serde_json::Value) -> Result<Patch, String> {
        let patches = value
            .as_array()
            .ok_or_else(|| "'patches' must be an array".to_string())?;
        if patches.is_empty() {
            return Err("'patches' must contain at least one patch".to_string());
        }

        let mut patches = patches.iter().map(Self::parse_patch);
        let first = patches
            .next()
            .ok_or_else(|| "'patches' must contain at least one patch".to_string())?
            .map_err(|error| format!("patches[0]: {error}"))?;
        if patches.len() == 0 {
            return Ok(first);
        }

        let Patch::Edit { path, mut edits, expected_before_hash } = first else {
            return Err("multi-patch calls only support edit operations for one file".to_string());
        };

        for (index, patch) in patches.enumerate() {
            let patch = patch.map_err(|error| format!("patches[{}]: {error}", index + 1))?;
            let Patch::Edit { path: patch_path, edits: patch_edits, expected_before_hash: patch_hash } = patch else {
                return Err("multi-patch calls only support edit operations for one file".to_string());
            };
            if patch_path != path {
                return Err("multi-patch calls must target one file".to_string());
            }
            if patch_hash != expected_before_hash {
                return Err("multi-patch calls must use the same expected_before_hash".to_string());
            }
            edits.extend(patch_edits);
        }

        Ok(Patch::Edit { path, edits, expected_before_hash })
    }

    /// Apply a structured patch to a file.
    ///
    /// Dispatches to the appropriate write primitive based on the patch operation.
    /// On failure, the file is left unchanged and no [`WriteResult`] is returned.
    pub fn exec(&self, root: &Path) -> (ToolOutput, Option<WriteResult>) {
        match self {
            Patch::Create { path, content } => create_file::exec(path, root, content),
            Patch::Replace { path, content, expected_before_hash } => {
                exec_replace(path, root, content, *expected_before_hash)
            }
            Patch::Edit { path, edits, expected_before_hash } => {
                replace_range::exec_many(path, root, edits, *expected_before_hash)
            }
        }
    }
}

/// Provider-visible definition for `write_patch`.
pub fn definition() -> ToolDefinition {
    ToolDefinition::new(
        NAME,
        r#"write_patch

Apply one or more structured patches to a file.

Use this as the preferred file-write tool. Put operations in patches. A call may
contain one create/replace operation or one or more edits for the same file. All
edits match the original file, not earlier edits in the call. Relative paths resolve
from the workspace; absolute paths and paths outside it are allowed. Failures leave
the file unchanged. Content is synchronized in a same-directory temporary file
before installation."#,
        serde_json::json!({
            "type": "object",
            "properties": {
                "patches": {
                    "type": "array",
                    "minItems": 1,
                    "items": {
                        "type": "object",
                        "properties": {
                            "op": { "type": "string", "enum": ["create", "replace", "edit"], "description": "The patch operation. create/replace must be the only patch in a call." },
                            "path": { "type": "string", "description": "Absolute path or path relative to the workspace root." },
                            "content": { "type": "string", "description": "Full file content, required for create/replace." },
                            "old_string": { "type": "string", "description": "The exact unique string to find in the original file." },
                            "new_string": { "type": "string", "description": "The replacement string." },
                            "expected_before_hash": { "type": "integer", "description": "Optional current-content hash guard." }
                        },
                        "required": ["op", "path"]
                    },
                    "description": "One create/replace patch, or one or more disjoint edits for the same file."
                }
            },
            "required": ["patches"]
        }),
    )
}

/// Execute a registry request for `write_patch`.
pub fn execute_request(request: &ToolUseRequest, ctx: &ToolContext<'_>) -> ToolExecution {
    match Patch::from_json(&request.arguments) {
        Ok(patch) => {
            let (output, write_result) = patch.exec(ctx.root);
            ToolExecution::full(output, write_result, None)
        }
        Err(error) => ToolExecution::output(ToolOutput::failed(NAME, error)),
    }
}

fn string_field(v: &serde_json::Value, field: &str, message: &str) -> Result<String, String> {
    v.get(field)
        .and_then(|s| s.as_str())
        .map(str::to_string)
        .ok_or_else(|| message.to_string())
}

/// Replace the entire contents of a file.
///
/// Unlike `create_file`, this overwrites an existing file. If
/// `expected_before_hash` is supplied, the current file hash must match before
/// any bytes are written.
fn exec_replace(
    path_str: &str, root: &Path, content: &str, expected_before_hash: Option<u64>,
) -> (ToolOutput, Option<WriteResult>) {
    let resolved = path::resolve_from_root(root, path_str);
    replace_range::with_file_lock(&resolved, || {
        exec_replace_locked(path_str, &resolved, content, expected_before_hash)
    })
}

fn exec_replace_locked(
    path_str: &str, resolved: &Path, content: &str, expected_before_hash: Option<u64>,
) -> (ToolOutput, Option<WriteResult>) {
    let before_content = match std::fs::read_to_string(resolved) {
        Ok(existing) => Some(existing),
        Err(e) => match e.kind() {
            std::io::ErrorKind::NotFound => None,
            _ => return (ToolOutput::failed("write_patch", format!("read failed: {e}")), None),
        },
    };
    let before_hash = before_content.as_deref().map(super::hash_content);
    let before_bytes = before_content.as_ref().map(String::len);

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

    match atomic_write::write(resolved, content.as_bytes(), atomic_write::WriteMode::Replace) {
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
            let mut lines = vec![result.summary()];
            lines.extend(replace_range::diff_lines(path_str, before_content.as_deref(), content));
            (ToolOutput::ok("write_patch", lines), Some(result))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ToolStatus;

    #[test]
    fn patch_from_json_create() {
        let args = r#"{"patches":[{"op":"create","path":"a.txt","content":"hello"}]}"#;
        let patch = Patch::from_json(args).expect("parse");
        assert_eq!(
            patch,
            Patch::Create { path: "a.txt".to_string(), content: "hello".to_string() }
        );
    }

    #[test]
    fn patch_from_json_replace() {
        let args = r#"{"patches":[{"op":"replace","path":"a.txt","content":"world","expected_before_hash":7}]}"#;
        let patch = Patch::from_json(args).expect("parse");
        assert_eq!(
            patch,
            Patch::Replace { path: "a.txt".to_string(), content: "world".to_string(), expected_before_hash: Some(7) }
        );
    }

    #[test]
    fn patch_from_json_edit() {
        let args = r#"{"patches":[{"op":"edit","path":"a.txt","old_string":"foo","new_string":"bar"}]}"#;
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
    fn patch_from_json_coalesces_same_file_patch_batch() {
        let args = r#"{"patches":[{"op":"edit","path":"a.txt","old_string":"foo","new_string":"bar"},{"op":"edit","path":"a.txt","old_string":"baz","new_string":"qux"}]}"#;
        let patch = Patch::from_json(args).expect("parse patch batch");
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
    fn patch_from_json_rejects_multi_file_patch_batch() {
        let args = r#"{"patches":[{"op":"edit","path":"a.txt","old_string":"foo","new_string":"bar"},{"op":"edit","path":"b.txt","old_string":"baz","new_string":"qux"}]}"#;
        let error = Patch::from_json(args).expect_err("multi-file batch should fail");
        assert!(error.contains("must target one file"));
    }

    #[test]
    fn patch_from_json_unknown_op_rejected() {
        let args = r#"{"patches":[{"op":"delete","path":"a.txt"}]}"#;
        let result = Patch::from_json(args);
        assert!(result.is_err());
        assert!(result.as_ref().unwrap_err().contains("unknown patch op"));
    }

    #[test]
    fn patch_from_json_missing_patches_rejected() {
        let error = Patch::from_json(r#"{"op":"edit","path":"a.txt","old_string":"a","new_string":"b"}"#)
            .expect_err("top-level patch shape should fail");
        assert!(error.contains("missing 'patches' field"));
    }

    #[test]
    fn patch_from_json_edit_missing_old_string_rejected() {
        let args = r#"{"patches":[{"op":"edit","path":"a.txt","new_string":"bar"}]}"#;
        assert!(Patch::from_json(args).is_err());
    }

    #[test]
    fn patch_apply_create_success() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        let patch = Patch::Create { path: "new.txt".to_string(), content: "hello\n".to_string() };
        let (output, result) = patch.exec(root);
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
        let (output, result) = patch.exec(root);
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
        let (output, result) = patch.exec(root);
        assert_eq!(output.status, ToolStatus::Ok);
        assert!(result.is_some());
        assert_eq!(
            std::fs::read_to_string(root.join("file.txt")).expect("read"),
            "new content\n"
        );
        assert!(output.display.lines.iter().any(|line| line == "--- a/file.txt"));
        assert!(output.display.lines.iter().any(|line| line == "+++ b/file.txt"));
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
        let (output, result) = patch.exec(root);
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

        let (output, result) = patch.exec(root);
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

        let (output, result) = patch.exec(root);
        assert_eq!(output.status, ToolStatus::Failed);
        assert!(result.is_none());
        assert_eq!(std::fs::read_to_string(root.join("file.txt")).expect("read"), "hello\n");
    }

    #[test]
    fn patch_apply_replaces_absolute_path_outside_root() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path().join("workspace");
        let outside = dir.path().join("outside.txt");
        std::fs::create_dir(&root).expect("workspace");
        std::fs::write(&outside, "old").expect("write");
        let patch = Patch::Replace {
            path: outside.to_string_lossy().to_string(),
            content: "new".to_string(),
            expected_before_hash: None,
        };

        let (output, result) = patch.exec(&root);
        assert_eq!(output.status, ToolStatus::Ok);
        assert!(result.is_some());
        assert_eq!(std::fs::read_to_string(outside).expect("read"), "new");
    }

    #[test]
    fn registry_execute_create_returns_write_result() {
        let dir = tempfile::tempdir().expect("temp dir");
        let request = crate::tools::ToolUseRequest::new(
            "write_patch".to_string(),
            r#"{"patches":[{"op":"create","path":"file.txt","content":"hello\n"}]}"#.to_string(),
            "call_1".to_string(),
        );

        let execution =
            crate::tools::registry::execute(&request, &crate::tools::registry::ToolContext::new(dir.path()));

        assert_eq!(execution.output.status, ToolStatus::Ok);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("file.txt")).expect("read"),
            "hello\n"
        );
        assert_eq!(
            execution.write_result.as_ref().map(|result| result.op),
            Some(WriteOp::Create)
        );
    }

    #[test]
    fn registry_execute_patch_batch_applies_one_atomic_multi_edit() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join("file.txt"), "alpha beta gamma\n").expect("write fixture");
        let request = crate::tools::ToolUseRequest::new(
            "write_patch".to_string(),
            r#"{"patches":[{"op":"edit","path":"file.txt","old_string":"alpha","new_string":"one"},{"op":"edit","path":"file.txt","old_string":"gamma","new_string":"three"}]}"#.to_string(),
            "call_1".to_string(),
        );

        let execution =
            crate::tools::registry::execute(&request, &crate::tools::registry::ToolContext::new(dir.path()));

        assert_eq!(execution.output.status, ToolStatus::Ok);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("file.txt")).expect("read"),
            "one beta three\n"
        );
        assert_eq!(
            execution.write_result.as_ref().map(|result| result.op),
            Some(WriteOp::Edit)
        );
    }

    #[test]
    fn registry_execute_replace_rejects_stale_hash() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join("file.txt"), "old\n").expect("write");
        let request = crate::tools::ToolUseRequest::new(
            "write_patch".to_string(),
            r#"{"patches":[{"op":"replace","path":"file.txt","content":"new\n","expected_before_hash":7}]}"#
                .to_string(),
            "call_1".to_string(),
        );

        let execution =
            crate::tools::registry::execute(&request, &crate::tools::registry::ToolContext::new(dir.path()));

        assert_eq!(execution.output.status, ToolStatus::Failed);
        assert!(execution.write_result.is_none());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("file.txt")).expect("read"),
            "old\n"
        );
    }

    #[test]
    fn registry_execute_writes_absolute_path_outside_workspace() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path().join("workspace");
        let outside = dir.path().join("outside.txt");
        std::fs::create_dir(&root).expect("workspace");
        let request = crate::tools::ToolUseRequest::new(
            "write_patch".to_string(),
            format!(
                r#"{{"patches":[{{"op":"replace","path":"{}","content":"nope"}}]}}"#,
                outside.display()
            ),
            "call_1".to_string(),
        );

        let execution = crate::tools::registry::execute(&request, &crate::tools::registry::ToolContext::new(&root));

        assert_eq!(execution.output.status, ToolStatus::Ok);
        assert!(execution.write_result.is_some());
        assert_eq!(std::fs::read_to_string(outside).expect("read"), "nope");
    }

    #[test]
    fn patch_apply_replace_failed_write_preserves_previous_bytes_and_cleans_temporary_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        let file = root.join("file.txt");
        std::fs::write(&file, "old content\n").expect("write");
        atomic_write::fail_next_for_test(atomic_write::FailurePoint::BeforeInstall);

        let patch = Patch::Replace {
            path: "file.txt".to_string(),
            content: "new content\n".to_string(),
            expected_before_hash: None,
        };
        let (output, result) = patch.exec(root);

        assert_eq!(output.status, ToolStatus::Failed);
        assert!(result.is_none());
        assert_eq!(std::fs::read_to_string(&file).expect("read"), "old content\n");
        assert_no_temporary_files(root);
    }

    fn assert_no_temporary_files(root: &Path) {
        let temporary_files = std::fs::read_dir(root)
            .expect("read workspace")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().starts_with(".thndrs-write-"))
            .collect::<Vec<_>>();
        assert!(
            temporary_files.is_empty(),
            "temporary files remain: {temporary_files:?}"
        );
    }
}
