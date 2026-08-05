use std::path::Path;
use std::path::PathBuf;

use crate::tools::registry::{ToolContext, ToolError, ToolExecution};
use crate::{tools::ToolDefinition, tools::ToolOutput, tools::ToolUseRequest, utils};

const NAME: &str = "read_file_range";

/// Parsed provider input for `read_file_range`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadFileRangeInput {
    path: String,
    start_line: u32,
    end_line: Option<u32>,
}

/// Read a range of lines from a file, implemented in Rust.
///
/// Lines are 1-indexed. `start_line` is inclusive, `end_line` is inclusive
/// (defaults to `start_line + 20`). Enforces workspace-root containment and
/// line-length caps.
pub fn exec(path: &Path, root: &Path, start_line: u32, end_line: Option<u32>) -> ToolOutput {
    exec_with_extra_read_roots(path, root, &[], start_line, end_line)
}

fn exec_with_extra_read_roots(
    path: &Path, root: &Path, extra_read_roots: &[PathBuf], start_line: u32, end_line: Option<u32>,
) -> ToolOutput {
    if !super::path::is_within_root(path, root)
        && !extra_read_roots
            .iter()
            .any(|extra_root| super::path::is_within_root(path, extra_root))
    {
        return ToolOutput::failed(
            "read_file_range",
            format!("path escapes workspace root: {}", path.display()),
        );
    }

    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            return ToolOutput::failed("read_file_range", format!("read failed: {e}"));
        }
    };

    let start = start_line.max(1) as usize;
    let end = end_line.map(|e| e.max(start as u32) as usize).unwrap_or(start + 20);

    let selected = content
        .lines()
        .enumerate()
        .skip(start.saturating_sub(1))
        .take(end.saturating_sub(start) + 1)
        .collect::<Vec<_>>();
    let lines: Vec<String> = selected
        .iter()
        .map(|(i, line)| format!("{}: {}", i + 1, utils::truncate_line(line)))
        .collect();

    if lines.is_empty() {
        return ToolOutput::failed("read_file_range", format!("no lines in range {start}-{end}"));
    }

    ToolOutput::ok("read_file_range", lines).with_evidence_content_hash(format!(
        "{:016x}",
        crate::tools::hash_content(
            &selected
                .into_iter()
                .map(|(_, line)| line)
                .collect::<Vec<_>>()
                .join("\n")
        )
    ))
}

/// Provider-visible definition for `read_file_range`.
pub fn definition() -> ToolDefinition {
    ToolDefinition::new(
        NAME,
        r#"read_file_range

Read a 1-indexed line range from a file under the workspace root.

Use this to inspect file contents after finding a path with find_files or search_text.
Prefer targeted ranges over reading entire large files. Paths are contained to the root;
escapes are rejected. Output is capped at 65536 bytes; long lines truncate at 512 chars."#,
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path relative to the workspace root." },
                "start_line": { "type": "integer" },
                "end_line": { "type": "integer" }
            },
            "required": ["path", "start_line"]
        }),
    )
}

/// Parse provider JSON arguments for `read_file_range`.
pub fn parse_arguments(arguments: &str) -> Result<ReadFileRangeInput, ToolError> {
    let args = serde_json::from_str::<serde_json::Value>(arguments).unwrap_or(serde_json::Value::Null);
    Ok(ReadFileRangeInput {
        path: args
            .get("path")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string(),
        start_line: args.get("start_line").and_then(|value| value.as_u64()).unwrap_or(1) as u32,
        end_line: args
            .get("end_line")
            .and_then(|value| value.as_u64())
            .map(|line| line as u32),
    })
}

/// Execute a registry request for `read_file_range`.
pub fn execute_request(request: &ToolUseRequest, ctx: &ToolContext<'_>) -> ToolExecution {
    match parse_arguments(&request.arguments) {
        Ok(input) => ToolExecution::output(exec_input(&input, ctx)),
        Err(error) => ToolExecution::output(ToolOutput::failed(NAME, error.to_string())),
    }
}

fn exec_input(input: &ReadFileRangeInput, ctx: &ToolContext<'_>) -> ToolOutput {
    let path = super::path::resolve_within_root(ctx.root, &input.path).unwrap_or_else(|_| PathBuf::from(&input.path));
    if ctx.extra_read_roots.is_empty() {
        return exec(&path, ctx.root, input.start_line, input.end_line);
    }
    exec_with_extra_read_roots(&path, ctx.root, &ctx.extra_read_roots, input.start_line, input.end_line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{app::ToolStatus, tools};

    #[test]
    fn read_file_range_reads_specific_lines() {
        let root = std::env::current_dir().unwrap();
        let path = root.join("Cargo.toml");
        let output = exec(&path, &root, 1, Some(3));
        assert_eq!(output.status, ToolStatus::Ok);
        assert_eq!(output.display.lines.len(), 3);
        assert!(output.display.lines[0].starts_with("1:"));
    }

    #[test]
    fn read_file_range_outside_root_fails() {
        let root = std::env::current_dir().unwrap();
        let outside = root.parent().unwrap().join("some_file.txt");
        let output = exec(&outside, &root, 1, Some(10));
        assert_eq!(output.status, ToolStatus::Failed);
        assert!(
            output
                .error
                .as_ref()
                .is_some_and(|e| e.contains("escapes workspace root"))
        );
    }

    #[test]
    fn read_file_range_nonexistent_file_fails() {
        let root = std::env::current_dir().unwrap();
        let path = root.join("nonexistent_file_zzz.rs");
        let output = exec(&path, &root, 1, Some(10));
        assert_eq!(output.status, ToolStatus::Failed);
    }

    #[test]
    fn parse_arguments_reads_required_and_optional_fields() {
        let input = parse_arguments(r#"{"path":"Cargo.toml","start_line":2,"end_line":4}"#).expect("parse");
        assert_eq!(input.path, "Cargo.toml");
        assert_eq!(input.start_line, 2);
        assert_eq!(input.end_line, Some(4));
    }

    #[test]
    fn parse_arguments_malformed_json_uses_safe_defaults() {
        let input = parse_arguments("not valid json").expect("parse");
        assert_eq!(input.path, "");
        assert_eq!(input.start_line, 1);
        assert_eq!(input.end_line, None);
    }

    #[test]
    fn registry_execute_reads_from_extra_root() {
        let workspace = tempfile::tempdir().expect("workspace");
        let skill = tempfile::tempdir().expect("skill root");
        let skill_path = skill.path().join("SKILL.md");
        std::fs::write(&skill_path, "skill instructions\n").expect("write skill");
        let request = ToolUseRequest::new(
            NAME.to_string(),
            serde_json::json!({
                "path": skill_path,
                "start_line": 1,
                "end_line": 1,
            })
            .to_string(),
            "call_1".to_string(),
        );
        let context =
            tools::registry::ToolContext::new(workspace.path()).with_extra_read_roots(&[skill.path().to_path_buf()]);

        let output = tools::registry::execute(&request, &context).output;

        assert_eq!(output.status, ToolStatus::Ok);
        assert_eq!(output.display.lines, vec!["1: skill instructions"]);
    }

    #[test]
    fn registry_execute_reads_range() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join("alpha.txt"), "a\nb\nc\n").expect("write");
        let request = ToolUseRequest::new(
            NAME.to_string(),
            serde_json::json!({"path":"alpha.txt","start_line":2,"end_line":3}).to_string(),
            "call_1".to_string(),
        );

        let output = tools::registry::execute(&request, &tools::registry::ToolContext::new(dir.path())).output;

        assert_eq!(output.status, ToolStatus::Ok);
        assert_eq!(output.display.lines, vec!["2: b", "3: c"]);
    }

    #[test]
    fn evidence_hash_covers_untruncated_range_content() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path().canonicalize().expect("canonical temp dir");
        let path = root.join("long.txt");
        let prefix = "x".repeat(crate::tools::MAX_LINE_LEN + 32);
        std::fs::write(&path, format!("{prefix}one\n")).expect("write first content");
        let first = exec(&path, &root, 1, Some(1));
        std::fs::write(&path, format!("{prefix}two\n")).expect("write changed content");
        let second = exec(&path, &root, 1, Some(1));

        assert_eq!(
            first.display.lines, second.display.lines,
            "both rendered lines truncate identically"
        );
        assert_ne!(first.evidence.content_hash, second.evidence.content_hash);
    }
}
