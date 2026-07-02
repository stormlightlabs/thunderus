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

use super::{MAX_RESULTS, ToolOutput, path};
use crate::utils;

const DEFAULT_RANGE_LINES: u32 = 40;

/// Execute a safe sed/awk-style action from provider JSON arguments.
///
/// Supported actions:
/// - `sed_print`: print a 1-indexed line range.
/// - `sed_substitute_preview`: show changed lines from a regex replacement preview.
/// - `awk_fields`: extract 1-indexed fields from optionally filtered lines.
pub fn exec(args: &Value, root: &Path) -> ToolOutput {
    let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");
    let path_str = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
    let resolved = match path::resolve_within_root(root, path_str) {
        Ok(path) => path,
        Err(e) => return ToolOutput::failed("sawk", e.to_string()),
    };
    let content = match std::fs::read_to_string(&resolved) {
        Ok(content) => content,
        Err(e) => return ToolOutput::failed("sawk", format!("read failed: {e}")),
    };

    match action {
        "sed_print" => sed_print(&content, args),
        "sed_substitute_preview" => sed_substitute_preview(&content, args),
        "awk_fields" => awk_fields(&content, args),
        _ => ToolOutput::failed(
            "sawk",
            "missing or invalid action (expected sed_print, sed_substitute_preview, or awk_fields)".to_string(),
        ),
    }
}

/// Print a bounded 1-indexed line range, mirroring `sed -n 'N,Mp'`.
fn sed_print(content: &str, args: &Value) -> ToolOutput {
    let start = args.get("start_line").and_then(|v| v.as_u64()).unwrap_or(1).max(1) as usize;
    let end = args
        .get("end_line")
        .and_then(|v| v.as_u64())
        .map(|n| n.max(start as u64) as usize)
        .unwrap_or(start + DEFAULT_RANGE_LINES as usize - 1);
    let max_lines = max_lines(args);

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
fn sed_substitute_preview(content: &str, args: &Value) -> ToolOutput {
    let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
    if pattern.is_empty() {
        return ToolOutput::failed("sawk", "sed_substitute_preview requires pattern".to_string());
    }
    let replacement = args.get("replacement").and_then(|v| v.as_str()).unwrap_or("");
    let global = args.get("global").and_then(|v| v.as_bool()).unwrap_or(false);
    let regex = match Regex::new(pattern) {
        Ok(regex) => regex,
        Err(e) => return ToolOutput::failed("sawk", format!("invalid regex: {e}")),
    };
    let (start, end) = range(args);
    let mut out = Vec::new();

    for (i, line) in content.lines().enumerate() {
        let line_no = i + 1;
        if line_no < start || line_no > end {
            continue;
        }
        let replaced = if global {
            regex.replace_all(line, replacement).to_string()
        } else {
            regex.replace(line, replacement).to_string()
        };
        if replaced != line {
            out.push(format!("-{}: {}", line_no, utils::truncate_line(line)));
            out.push(format!("+{}: {}", line_no, utils::truncate_line(&replaced)));
            if out.len() / 2 >= max_lines(args) {
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
fn awk_fields(content: &str, args: &Value) -> ToolOutput {
    let fields = args
        .get("fields")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_u64())
                .filter(|n| *n > 0)
                .map(|n| n as usize)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if fields.is_empty() {
        return ToolOutput::failed("sawk", "awk_fields requires a non-empty fields array".to_string());
    }

    let filter = match args.get("pattern").and_then(|v| v.as_str()) {
        Some(pattern) if !pattern.is_empty() => match Regex::new(pattern) {
            Ok(regex) => Some(regex),
            Err(e) => return ToolOutput::failed("sawk", format!("invalid regex: {e}")),
        },
        _ => None,
    };
    let delimiter = args.get("delimiter").and_then(|v| v.as_str());
    let (start, end) = range(args);
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
        if out.len() >= max_lines(args) {
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

fn range(args: &Value) -> (usize, usize) {
    let start = args.get("start_line").and_then(|v| v.as_u64()).unwrap_or(1).max(1) as usize;
    let end = args
        .get("end_line")
        .and_then(|v| v.as_u64())
        .map(|n| n.max(start as u64) as usize)
        .unwrap_or(usize::MAX);
    (start, end)
}

fn max_lines(args: &Value) -> usize {
    args.get("max_lines")
        .and_then(|v| v.as_u64())
        .map(|n| (n as usize).clamp(1, MAX_RESULTS))
        .unwrap_or(MAX_RESULTS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ToolStatus;

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
}
