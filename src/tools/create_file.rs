//! Create a new file with given content.
//!
//! Enforces workspace-root containment. Fails if the file already exists,
//! preventing accidental overwrites. Parent directories are created if needed.

use std::path::Path;

use super::{ToolDefinition, ToolOutput, ToolUseRequest, WriteOp, WriteResult, hash_content, path};
use crate::tools::registry::{ToolContext, ToolError, ToolExecution};

const NAME: &str = "create_file";

/// Parsed provider input for `create_file`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateFileInput {
    path: String,
    content: String,
}

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

/// Provider-visible definition for `create_file`.
pub fn definition() -> ToolDefinition {
    ToolDefinition::new(
        NAME,
        r#"create_file

Create a new file with the given content.

Use this for direct new-file writes. Prefer write_patch op=create when doing a
mixed edit. Fails if the file exists. Paths are contained to the workspace root;
escapes are rejected. Parent directories are created if needed."#,
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path relative to the workspace root." },
                "content": { "type": "string", "description": "The full file content to write." }
            },
            "required": ["path", "content"]
        }),
    )
}

/// Parse provider JSON arguments for `create_file`.
pub fn parse_arguments(arguments: &str) -> Result<CreateFileInput, ToolError> {
    let args = serde_json::from_str::<serde_json::Value>(arguments).unwrap_or(serde_json::Value::Null);
    Ok(CreateFileInput {
        path: args
            .get("path")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string(),
        content: args
            .get("content")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string(),
    })
}

/// Execute a registry request for `create_file`.
pub fn execute_request(request: &ToolUseRequest, ctx: ToolContext<'_>) -> ToolExecution {
    match parse_arguments(&request.arguments) {
        Ok(input) => {
            let (output, write_result) = exec(&input.path, ctx.root, &input.content);
            ToolExecution::full(output, write_result, None)
        }
        Err(error) => ToolExecution::output(ToolOutput::failed(NAME, error.to_string())),
    }
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

    #[test]
    fn parse_arguments_reads_path_and_content() {
        let input = parse_arguments(r#"{"path":"a.txt","content":"hello"}"#).expect("parse");
        assert_eq!(
            input,
            CreateFileInput { path: "a.txt".to_string(), content: "hello".to_string() }
        );
    }

    #[test]
    fn registry_execute_creates_file_with_write_result() {
        let dir = tempfile::tempdir().expect("temp dir");
        let request = crate::tools::ToolUseRequest::new(
            "create_file".to_string(),
            r#"{"path":"created.txt","content":"hello\n"}"#.to_string(),
            "call_1".to_string(),
        );

        let execution = crate::tools::registry::execute(&request, crate::tools::registry::ToolContext::new(dir.path()));

        assert_eq!(execution.output.status, ToolStatus::Ok);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("created.txt")).expect("read"),
            "hello\n"
        );
        assert_eq!(
            execution.write_result.as_ref().map(|result| result.op),
            Some(WriteOp::Create)
        );
    }

    #[test]
    fn registry_execute_rejects_path_escape() {
        let dir = tempfile::tempdir().expect("temp dir");
        let outside = dir.path().parent().unwrap().join("escape.txt");
        let request = crate::tools::ToolUseRequest::new(
            "create_file".to_string(),
            format!(r#"{{"path":"{}","content":"nope"}}"#, outside.display()),
            "call_1".to_string(),
        );

        let execution = crate::tools::registry::execute(&request, crate::tools::registry::ToolContext::new(dir.path()));

        assert_eq!(execution.output.status, ToolStatus::Failed);
        assert!(execution.write_result.is_none());
        assert!(!outside.exists());
    }
}
