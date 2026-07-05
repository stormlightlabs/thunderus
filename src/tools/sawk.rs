//! Safe `sed`/`awk`-style inspection helpers.
//!
//! This module intentionally does not execute `sed`, `awk`, a shell, or a user
//! supplied program string. It exposes the parts coding agents actually use most
//! often such as printing line ranges, previewing regex substitutions, and extracting
//! fields from matching lines.
//!
//! All operations are read-only, path-contained, line-capped, and line-truncated.

use std::path::Path;

use regex_lite::Regex;
use serde_json::Value;

use super::{MAX_RESULTS, ToolDefinition, ToolOutput, ToolUseRequest, path};
use crate::tools::registry::{ToolContext, ToolError, ToolExecution};
use crate::utils;

const DEFAULT_RANGE_LINES: u32 = 40;
const NAME: &str = "sawk";

/// Parsed provider input for `sawk`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SawkInput {
    action: String,
    path: String,
    start_line: Option<u64>,
    end_line: Option<u64>,
    max_lines: Option<u64>,
    pattern: Option<String>,
    replacement: Option<String>,
    global: bool,
    fields: Vec<u64>,
    delimiter: Option<String>,
}

impl SawkInput {
    fn from_value(args: &Value) -> Result<Self, ToolError> {
        Ok(Self {
            action: args
                .get("action")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string(),
            path: args
                .get("path")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string(),
            start_line: args.get("start_line").and_then(|value| value.as_u64()),
            end_line: args.get("end_line").and_then(|value| value.as_u64()),
            max_lines: args.get("max_lines").and_then(|value| value.as_u64()),
            pattern: args.get("pattern").and_then(|value| value.as_str()).map(str::to_string),
            replacement: args
                .get("replacement")
                .and_then(|value| value.as_str())
                .map(str::to_string),
            global: args.get("global").and_then(|value| value.as_bool()).unwrap_or(false),
            fields: args
                .get("fields")
                .and_then(|value| value.as_array())
                .map(|items| items.iter().filter_map(|value| value.as_u64()).collect())
                .unwrap_or_default(),
            delimiter: args
                .get("delimiter")
                .and_then(|value| value.as_str())
                .map(str::to_string),
        })
    }
}

/// Execute a safe sed/awk-style action from provider JSON arguments.
///
/// Supported actions:
/// - `sed_print`: print a 1-indexed line range.
/// - `sed_substitute_preview`: show changed lines from a regex replacement preview.
/// - `awk_fields`: extract 1-indexed fields from optionally filtered lines.
#[allow(dead_code)]
pub fn exec(args: &Value, root: &Path) -> ToolOutput {
    match SawkInput::from_value(args) {
        Ok(input) => exec_input(&input, root),
        Err(error) => ToolOutput::failed(NAME, error.to_string()),
    }
}

/// Provider-visible definition for `sawk`.
pub fn definition() -> ToolDefinition {
    ToolDefinition::new(
        NAME,
        r#"sawk

Run safe read-only sed/awk-style inspection actions.

Use this for line printing, substitution previews, or field extraction when it is clearer
than raw shell. Actions are typed: sed_print, sed_substitute_preview, awk_fields.
Paths are contained; output is capped/truncated; no sed -i or awk system()."#,
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["sed_print", "sed_substitute_preview", "awk_fields"] },
                "path": { "type": "string", "description": "Path relative to the workspace root." },
                "start_line": { "type": "integer" },
                "end_line": { "type": "integer" },
                "max_lines": { "type": "integer" },
                "pattern": { "type": "string", "description": "Regex pattern for preview/filter actions." },
                "replacement": { "type": "string", "description": "Replacement text for sed_substitute_preview." },
                "global": { "type": "boolean", "description": "Replace all matches per line in previews." },
                "fields": { "type": "array", "items": { "type": "integer" }, "description": "1-indexed fields for awk_fields." },
                "delimiter": { "type": "string", "description": "Optional literal field delimiter; defaults to whitespace." }
            },
            "required": ["action", "path"]
        }),
    )
}

/// Parse provider JSON arguments for `sawk`.
pub fn parse_arguments(arguments: &str) -> Result<SawkInput, ToolError> {
    let args = serde_json::from_str::<serde_json::Value>(arguments).unwrap_or(serde_json::Value::Null);
    SawkInput::from_value(&args)
}

/// Execute a registry request for `sawk`.
pub fn execute_request(request: &ToolUseRequest, ctx: ToolContext<'_>) -> ToolExecution {
    match parse_arguments(&request.arguments) {
        Ok(input) => ToolExecution::output(exec_input(&input, ctx.root)),
        Err(error) => ToolExecution::output(ToolOutput::failed(NAME, error.to_string())),
    }
}

fn exec_input(input: &SawkInput, root: &Path) -> ToolOutput {
    let resolved = match path::resolve_within_root(root, &input.path) {
        Ok(path) => path,
        Err(e) => return ToolOutput::failed("sawk", e.to_string()),
    };
    let content = match std::fs::read_to_string(&resolved) {
        Ok(content) => content,
        Err(e) => return ToolOutput::failed("sawk", format!("read failed: {e}")),
    };

    match input.action.as_str() {
        "sed_print" => sed_print(&content, input),
        "sed_substitute_preview" => sed_substitute_preview(&content, input),
        "awk_fields" => awk_fields(&content, input),
        _ => ToolOutput::failed(
            "sawk",
            "missing or invalid action (expected sed_print, sed_substitute_preview, or awk_fields)".to_string(),
        ),
    }
}

/// Print a bounded 1-indexed line range, mirroring `sed -n 'N,Mp'`.
fn sed_print(content: &str, input: &SawkInput) -> ToolOutput {
    let start = input.start_line.unwrap_or(1).max(1) as usize;
    let end = input
        .end_line
        .map(|n| n.max(start as u64) as usize)
        .unwrap_or(start + DEFAULT_RANGE_LINES as usize - 1);
    let max_lines = max_lines(input);

    let lines = content
        .lines()
        .enumerate()
        .skip(start.saturating_sub(1))
        .take(end.saturating_sub(start) + 1)
        .take(max_lines)
        .map(|(i, line)| format!("{}: {}", i + 1, utils::truncate_line(line)))
        .collect::<Vec<_>>();

    if lines.is_empty() {
        return ToolOutput::failed("sawk", format!("no lines in range {start}-{end}"));
    }
    ToolOutput::ok("sawk", lines)
}

/// Preview regex substitutions without modifying the file.
///
/// Output alternates `-line: old` and `+line: new` for changed lines only. This
/// intentionally omits an in-place equivalent of `sed -i`.
fn sed_substitute_preview(content: &str, input: &SawkInput) -> ToolOutput {
    let pattern = input.pattern.as_deref().unwrap_or("");
    if pattern.is_empty() {
        return ToolOutput::failed("sawk", "sed_substitute_preview requires pattern".to_string());
    }
    let replacement = input.replacement.as_deref().unwrap_or("");
    let regex = match Regex::new(pattern) {
        Ok(regex) => regex,
        Err(e) => return ToolOutput::failed("sawk", format!("invalid regex: {e}")),
    };
    let (start, end) = range(input);
    let mut out = Vec::new();

    for (i, line) in content.lines().enumerate() {
        let line_no = i + 1;
        if line_no < start || line_no > end {
            continue;
        }
        let replaced = if input.global {
            regex.replace_all(line, replacement).to_string()
        } else {
            regex.replace(line, replacement).to_string()
        };
        if replaced != line {
            out.push(format!("-{}: {}", line_no, utils::truncate_line(line)));
            out.push(format!("+{}: {}", line_no, utils::truncate_line(&replaced)));
            if out.len() / 2 >= max_lines(input) {
                break;
            }
        }
    }

    if out.is_empty() {
        return ToolOutput::ok("sawk", vec!["no substitution matches".to_string()]);
    }
    ToolOutput::ok("sawk", out)
}

/// Extract selected 1-indexed fields from matching lines, mirroring common `awk` usage.
fn awk_fields(content: &str, input: &SawkInput) -> ToolOutput {
    let fields = input
        .fields
        .iter()
        .filter(|field| **field > 0)
        .map(|field| *field as usize)
        .collect::<Vec<_>>();
    if fields.is_empty() {
        return ToolOutput::failed("sawk", "awk_fields requires a non-empty fields array".to_string());
    }

    let filter = match input.pattern.as_deref() {
        Some(pattern) if !pattern.is_empty() => match Regex::new(pattern) {
            Ok(regex) => Some(regex),
            Err(e) => return ToolOutput::failed("sawk", format!("invalid regex: {e}")),
        },
        _ => None,
    };
    let delimiter = input.delimiter.as_deref();
    let (start, end) = range(input);
    let mut out = Vec::new();

    for (i, line) in content.lines().enumerate() {
        let line_no = i + 1;
        if line_no < start || line_no > end {
            continue;
        }
        if filter.as_ref().is_some_and(|regex| !regex.is_match(line)) {
            continue;
        }
        let parts = split_fields(line, delimiter);
        let selected = fields
            .iter()
            .map(|field| parts.get(field - 1).copied().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\t");
        out.push(format!("{}: {}", line_no, utils::truncate_line(&selected)));
        if out.len() >= max_lines(input) {
            break;
        }
    }

    if out.is_empty() {
        return ToolOutput::ok("sawk", vec!["no matching rows".to_string()]);
    }
    ToolOutput::ok("sawk", out)
}

fn split_fields<'a>(line: &'a str, delimiter: Option<&str>) -> Vec<&'a str> {
    match delimiter {
        Some(delimiter) if !delimiter.is_empty() => line.split(delimiter).collect(),
        _ => line.split_whitespace().collect(),
    }
}

fn range(input: &SawkInput) -> (usize, usize) {
    let start = input.start_line.unwrap_or(1).max(1) as usize;
    let end = input
        .end_line
        .map(|n| n.max(start as u64) as usize)
        .unwrap_or(usize::MAX);
    (start, end)
}

fn max_lines(input: &SawkInput) -> usize {
    input
        .max_lines
        .map(|n| (n as usize).clamp(1, MAX_RESULTS))
        .unwrap_or(MAX_RESULTS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{app::ToolStatus, tools};

    #[test]
    fn sed_print_reads_range() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join("file.txt"), "a\nb\nc\n").expect("write");
        let args = serde_json::json!({"action":"sed_print","path":"file.txt","start_line":2,"end_line":3});

        let output = exec(&args, dir.path());

        assert_eq!(output.status, ToolStatus::Ok);
        assert_eq!(output.output, vec!["2: b", "3: c"]);
    }

    #[test]
    fn sed_substitute_preview_is_read_only() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("file.txt");
        std::fs::write(&file, "foo\nbar foo\n").expect("write");
        let args = serde_json::json!({
            "action":"sed_substitute_preview",
            "path":"file.txt",
            "pattern":"foo",
            "replacement":"baz",
            "global":true
        });

        let output = exec(&args, dir.path());

        assert_eq!(output.status, ToolStatus::Ok);
        assert_eq!(output.output, vec!["-1: foo", "+1: baz", "-2: bar foo", "+2: bar baz"]);
        assert_eq!(std::fs::read_to_string(&file).expect("read"), "foo\nbar foo\n");
    }

    #[test]
    fn awk_fields_extracts_columns() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join("file.txt"), "one two three\nred blue green\n").expect("write");
        let args = serde_json::json!({"action":"awk_fields","path":"file.txt","fields":[2,3],"pattern":"red|one"});

        let output = exec(&args, dir.path());

        assert_eq!(output.status, ToolStatus::Ok);
        assert_eq!(output.output, vec!["1: two\tthree", "2: blue\tgreen"]);
    }

    #[test]
    fn sawk_rejects_outside_root() {
        let dir = tempfile::tempdir().expect("temp dir");
        let outside = dir.path().parent().unwrap().join("escape.txt");
        let args = serde_json::json!({"action":"sed_print","path":outside,"start_line":1});

        let output = exec(&args, dir.path());

        assert_eq!(output.status, ToolStatus::Failed);
        assert!(
            output
                .error
                .as_ref()
                .is_some_and(|e| e.contains("escapes workspace root"))
        );
    }

    #[test]
    fn parse_arguments_reads_common_and_action_fields() {
        let input = parse_arguments(
            r#"{"action":"awk_fields","path":"file.txt","start_line":2,"end_line":5,"max_lines":3,"pattern":"red","replacement":"blue","global":true,"fields":[1,3],"delimiter":","}"#,
        )
        .expect("parse");

        assert_eq!(input.action, "awk_fields");
        assert_eq!(input.path, "file.txt");
        assert_eq!(input.start_line, Some(2));
        assert_eq!(input.end_line, Some(5));
        assert_eq!(input.max_lines, Some(3));
        assert_eq!(input.pattern.as_deref(), Some("red"));
        assert_eq!(input.replacement.as_deref(), Some("blue"));
        assert!(input.global);
        assert_eq!(input.fields, vec![1, 3]);
        assert_eq!(input.delimiter.as_deref(), Some(","));
    }

    #[test]
    fn parse_arguments_malformed_json_uses_safe_defaults() {
        let input = parse_arguments("not valid json").expect("parse");
        assert_eq!(input.action, "");
        assert_eq!(input.path, "");
        assert_eq!(input.start_line, None);
        assert_eq!(input.end_line, None);
        assert_eq!(input.max_lines, None);
        assert_eq!(input.pattern, None);
        assert_eq!(input.replacement, None);
        assert!(!input.global);
        assert!(input.fields.is_empty());
        assert_eq!(input.delimiter, None);
    }

    #[test]
    fn registry_execute_prints_range() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join("file.txt"), "a\nb\nc\n").expect("write");
        let request = ToolUseRequest::new(
            NAME.to_string(),
            serde_json::json!({"action":"sed_print","path":"file.txt","start_line":2,"end_line":3}).to_string(),
            "call_1".to_string(),
        );

        let output = tools::registry::execute(&request, tools::registry::ToolContext::new(dir.path())).output;

        assert_eq!(output.status, ToolStatus::Ok);
        assert_eq!(output.output, vec!["2: b", "3: c"]);
    }
}
