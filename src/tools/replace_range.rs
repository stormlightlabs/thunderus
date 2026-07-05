//! Exact text replacement for one or more disjoint regions in a file.
//!
//! The caller supplies one or more old/new text replacements. Every old string
//! is matched against the same original file content, must identify a unique
//! non-overlapping region, and is applied only after the entire edit set is
//! validated. Failed edits leave the file unchanged.
//!
//! This module preserves UTF-8 BOMs, preserves the file's dominant line ending,
//! serializes cooperating writes to the same target, and returns a compact diff
//! for model/user review.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use super::{ToolDefinition, ToolOutput, ToolUseRequest, WriteOp, WriteResult, hash_content, path};
use crate::tools::registry::{ToolContext, ToolError, ToolExecution};
use crate::utils;

static FILE_LOCKS: OnceLock<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> = OnceLock::new();

const DIFF_CONTEXT_LINES: usize = 3;
const MAX_DIFF_LINES: usize = 120;
const NO_MATCH_PREVIEW_LINES: usize = 20;
const NAME: &str = "replace_range";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LineEnding {
    Lf,
    Crlf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MatchError {
    NotFound,
    Duplicate(usize),
}

/// One exact text replacement requested for a file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Replacement {
    /// Exact text to find in the original file content.
    pub old_string: String,
    /// Replacement text to write at the matched location.
    pub new_string: String,
}

/// Parsed provider input for `replace_range`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplaceRangeInput {
    path: String,
    old_string: String,
    new_string: String,
    expected_before_hash: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MatchedReplacement {
    edit_index: usize,
    start: usize,
    end: usize,
    new_string: String,
}

pub fn with_file_lock<T>(path: &Path, f: impl FnOnce() -> T) -> T {
    let lock = {
        let locks = FILE_LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
        let mut guard = locks.lock().expect("file lock map poisoned");
        guard
            .entry(path.to_path_buf())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    };
    let _guard = lock.lock().expect("file lock poisoned");
    f()
}

/// Replace a unique exact string occurrence in a file.
///
/// This is the backward-compatible single-edit entry point. New callers that
/// need multiple disjoint edits in the same file should use [`exec_many`] so all
/// replacements are validated against one original snapshot.
pub fn exec(path_str: &str, root: &Path, old_string: &str, new_string: &str) -> (ToolOutput, Option<WriteResult>) {
    exec_many(
        path_str,
        root,
        &[Replacement { old_string: old_string.to_string(), new_string: new_string.to_string() }],
        None,
    )
}

/// Apply one or more exact text replacements to one file.
///
/// All replacements are matched against the original file content rather than
/// sequentially. If any replacement is missing, duplicated, overlapping, empty,
/// stale, or produces no final change, no bytes are written.
pub fn exec_many(
    path_str: &str, root: &Path, replacements: &[Replacement], expected_before_hash: Option<u64>,
) -> (ToolOutput, Option<WriteResult>) {
    let resolved = match path::resolve_within_root(root, path_str) {
        Ok(p) => p,
        Err(e) => return (ToolOutput::failed("replace_range", e.to_string()), None),
    };

    with_file_lock(&resolved, || {
        exec_many_locked(path_str, &resolved, replacements, expected_before_hash)
    })
}

/// Provider-visible definition for `replace_range`.
pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: NAME,
        description: r#"replace_range

Replace a unique exact string occurrence in an existing file.

Use this for direct small edits. Prefer write_patch op=edit when doing a mixed
edit. old_string must match exactly and once; include surrounding context for
uniqueness. Paths are contained to the root; failed edits leave files unchanged."#,
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path relative to the workspace root." },
                "old_string": { "type": "string", "description": "The exact string to find. Must appear exactly once." },
                "new_string": { "type": "string", "description": "The replacement string." },
                "expected_before_hash": { "type": "integer", "description": "Optional current-content hash guard." }
            },
            "required": ["path", "old_string", "new_string"]
        }),
    }
}

/// Parse provider JSON arguments for `replace_range`.
pub fn parse_arguments(arguments: &str) -> Result<ReplaceRangeInput, ToolError> {
    let args = serde_json::from_str::<serde_json::Value>(arguments).unwrap_or(serde_json::Value::Null);
    Ok(ReplaceRangeInput {
        path: args
            .get("path")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string(),
        old_string: args
            .get("old_string")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string(),
        new_string: args
            .get("new_string")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string(),
        expected_before_hash: args.get("expected_before_hash").and_then(|value| value.as_u64()),
    })
}

/// Execute a registry request for `replace_range`.
pub fn execute_request(request: &ToolUseRequest, ctx: ToolContext<'_>) -> ToolExecution {
    match parse_arguments(&request.arguments) {
        Ok(input) => {
            let replacements = [Replacement { old_string: input.old_string, new_string: input.new_string }];
            let (output, write_result) = exec_many(&input.path, ctx.root, &replacements, input.expected_before_hash);
            ToolExecution::full(output, write_result, None)
        }
        Err(error) => ToolExecution::output(ToolOutput::failed(NAME, error.to_string())),
    }
}

fn exec_many_locked(
    path_str: &str, resolved: &Path, replacements: &[Replacement], expected_before_hash: Option<u64>,
) -> (ToolOutput, Option<WriteResult>) {
    if replacements.is_empty() {
        return (
            ToolOutput::failed(
                "replace_range",
                "edits must contain at least one replacement".to_string(),
            ),
            None,
        );
    }

    let raw_content = match std::fs::read_to_string(resolved) {
        Ok(c) => c,
        Err(e) => return (ToolOutput::failed("replace_range", format!("read failed: {e}")), None),
    };

    let before_hash = hash_content(&raw_content);
    if let Some(expected) = expected_before_hash
        && expected != before_hash
    {
        return (
            ToolOutput::failed(
                "replace_range",
                format!("stale file: current hash {before_hash} does not match expected_before_hash {expected}"),
            ),
            None,
        );
    }

    let before_bytes = raw_content.len();
    let (bom, content) = split_bom(&raw_content);
    let ending = detect_line_ending(content);
    let normalized_content = normalize_to_lf(content);
    let normalized_replacements = replacements
        .iter()
        .map(|edit| Replacement {
            old_string: normalize_to_lf(&edit.old_string),
            new_string: normalize_to_lf(&edit.new_string),
        })
        .collect::<Vec<_>>();

    let new_normalized_content = match apply_replacements(&normalized_content, &normalized_replacements, path_str) {
        Ok(c) => c,
        Err(e) => return (ToolOutput::failed("replace_range", e), None),
    };

    let final_content = format!("{bom}{}", restore_line_endings(&new_normalized_content, ending));
    let after_hash = hash_content(&final_content);
    let after_bytes = final_content.len();

    match std::fs::write(resolved, &final_content) {
        Err(e) => (ToolOutput::failed("replace_range", format!("write failed: {e}")), None),
        Ok(_) => {
            let result = WriteResult {
                op: WriteOp::Edit,
                path: resolved.to_path_buf(),
                before_hash: Some(before_hash),
                before_bytes: Some(before_bytes),
                after_hash,
                after_bytes,
            };
            let mut lines = vec![result.summary()];
            lines.extend(diff_lines(&normalized_content, &new_normalized_content));
            (ToolOutput::ok("replace_range", lines), Some(result))
        }
    }
}

/// Validate and apply replacements to LF-normalized content.
///
/// The returned content is still LF-normalized. Callers restore the original
/// file's line ending style after validation succeeds.
fn apply_replacements(content: &str, replacements: &[Replacement], path_str: &str) -> Result<String, String> {
    for (i, edit) in replacements.iter().enumerate() {
        if edit.old_string.is_empty() {
            return Err(format!("edits[{i}].old_string must not be empty in {path_str}"));
        }
    }

    let mut matched = Vec::with_capacity(replacements.len());
    for (i, edit) in replacements.iter().enumerate() {
        let matches = find_unique_match(content, &edit.old_string).map_err(|e| enrich_match_error(content, edit, e))?;
        matched.push(MatchedReplacement {
            edit_index: i,
            start: matches.0,
            end: matches.1,
            new_string: edit.new_string.clone(),
        });
    }

    matched.sort_by_key(|m| m.start);
    for pair in matched.windows(2) {
        let previous = &pair[0];
        let current = &pair[1];
        if previous.end > current.start {
            return Err(format!(
                "edits[{}] and edits[{}] overlap in {path_str}; merge them into one edit",
                previous.edit_index, current.edit_index
            ));
        }
    }

    let mut result = content.to_string();
    for edit in matched.iter().rev() {
        result.replace_range(edit.start..edit.end, &edit.new_string);
    }

    if result == content {
        return Err(format!(
            "no changes made to {path_str}; replacement produced identical content"
        ));
    }

    Ok(result)
}

fn find_unique_match(content: &str, old_string: &str) -> Result<(usize, usize), MatchError> {
    let exact = match_offsets(content, old_string);
    match exact.len() {
        1 => return Ok((exact[0], exact[0] + old_string.len())),
        n if n > 1 => return Err(MatchError::Duplicate(n)),
        _ => {}
    }

    find_block_boundary_fuzzy_match(content, old_string).ok_or(MatchError::NotFound)
}

/// Try a conservative fuzzy match for whole-line blocks.
///
/// This accepts common model drift in copied blocks (trimmed trailing
/// whitespace, smart quotes, Unicode dashes, and non-breaking spaces) only when
/// the requested old string starts and ends on line boundaries. That keeps byte
/// mapping safe: the matched range can be converted back to original line spans
/// without guessing intra-line offsets.
fn find_block_boundary_fuzzy_match(content: &str, old_string: &str) -> Option<(usize, usize)> {
    let old_lines = split_inclusive_lines(old_string);
    if old_lines.is_empty() || old_lines.iter().any(|line| !line.ends_with('\n')) {
        return None;
    }

    let content_lines = split_inclusive_lines(content);

    let normalized_old = old_lines
        .iter()
        .map(|line| normalize_fuzzy_line(line))
        .collect::<Vec<_>>();

    let mut found = None;
    for start_line in 0..=content_lines.len().saturating_sub(old_lines.len()) {
        let candidate = content_lines[start_line..start_line + old_lines.len()]
            .iter()
            .map(|line| normalize_fuzzy_line(line))
            .collect::<Vec<_>>();
        if candidate == normalized_old {
            if found.is_some() {
                return None;
            }
            let start = content_lines[..start_line].iter().map(|line| line.len()).sum::<usize>();
            let len = content_lines[start_line..start_line + old_lines.len()]
                .iter()
                .map(|line| line.len())
                .sum::<usize>();
            found = Some((start, start + len));
        }
    }
    found
}

fn enrich_match_error(content: &str, edit: &Replacement, error: MatchError) -> String {
    match error {
        MatchError::Duplicate(count) => {
            format!("old_string appears {count} times; include more surrounding context to make it unique")
        }
        MatchError::NotFound => {
            let mut msg = "old_string not found; it must match exactly, including whitespace and newlines".to_string();
            if !edit.new_string.is_empty() && content.contains(&edit.new_string) {
                msg.push_str(
                    "\nreplacement text already appears in the file; check whether the edit was already applied",
                );
            }
            if let Some(hint) = similar_context(content, &edit.old_string) {
                msg.push_str(&format!("\n\nDid you mean:\n```\n{hint}\n```"));
            }
            msg.push_str(&format!("\n\nFile preview:\n```\n{}\n```", file_preview(content)));
            msg
        }
    }
}

fn match_offsets(haystack: &str, needle: &str) -> Vec<usize> {
    if needle.is_empty() {
        return Vec::new();
    }
    let mut offsets = Vec::new();
    let mut start = 0;
    while let Some(pos) = haystack[start..].find(needle) {
        offsets.push(start + pos);
        start += pos + needle.len();
    }
    offsets
}

fn split_bom(content: &str) -> (&str, &str) {
    content
        .strip_prefix('\u{FEFF}')
        .map_or(("", content), |stripped| ("\u{FEFF}", stripped))
}

fn detect_line_ending(content: &str) -> LineEnding {
    if content.contains("\r\n") { LineEnding::Crlf } else { LineEnding::Lf }
}

fn normalize_to_lf(content: &str) -> String {
    content.replace("\r\n", "\n").replace('\r', "\n")
}

fn restore_line_endings(content: &str, ending: LineEnding) -> String {
    match ending {
        LineEnding::Lf => content.to_string(),
        LineEnding::Crlf => content.replace('\n', "\r\n"),
    }
}

fn normalize_fuzzy_line(line: &str) -> String {
    line.trim_end_matches([' ', '\t', '\r', '\n'])
        .chars()
        .map(normalize_fuzzy_char)
        .collect::<String>()
}

fn normalize_fuzzy_char(ch: char) -> char {
    match ch {
        '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' => '\'',
        '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' => '"',
        '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2015}' | '\u{2212}' => '-',
        '\u{00A0}' | '\u{2002}'..='\u{200A}' | '\u{202F}' | '\u{205F}' | '\u{3000}' => ' ',
        other => other,
    }
}

fn split_inclusive_lines(content: &str) -> Vec<&str> {
    if content.is_empty() {
        return Vec::new();
    }
    content.split_inclusive('\n').collect()
}

fn similar_context(content: &str, old_string: &str) -> Option<String> {
    let first_non_empty = old_string.lines().find(|line| !line.trim().is_empty())?.trim();
    let lines = content.lines().collect::<Vec<_>>();
    let (index, _) = lines
        .iter()
        .enumerate()
        .find(|(_, line)| line.contains(first_non_empty) || first_non_empty.contains(line.trim()))?;
    let start = index.saturating_sub(2);
    let end = (index + 3).min(lines.len());
    Some(lines[start..end].join("\n"))
}

fn file_preview(content: &str) -> String {
    if content.is_empty() {
        return "(file is empty)".to_string();
    }
    let lines = content.lines().collect::<Vec<_>>();
    let mut preview = lines
        .iter()
        .take(NO_MATCH_PREVIEW_LINES)
        .enumerate()
        .map(|(i, line)| format!("{:>4}: {}", i + 1, utils::truncate_line(line)))
        .collect::<Vec<_>>()
        .join("\n");
    if lines.len() > NO_MATCH_PREVIEW_LINES {
        preview.push_str(&format!("\n... ({} more lines)", lines.len() - NO_MATCH_PREVIEW_LINES));
    }
    preview
}

/// Render a compact unified-style diff with a bounded number of lines.
fn diff_lines(before: &str, after: &str) -> Vec<String> {
    if before == after {
        return Vec::new();
    }

    let before_lines = before.lines().collect::<Vec<_>>();
    let after_lines = after.lines().collect::<Vec<_>>();
    let mut prefix = 0;
    while prefix < before_lines.len() && prefix < after_lines.len() && before_lines[prefix] == after_lines[prefix] {
        prefix += 1;
    }

    let mut suffix = 0;
    while suffix < before_lines.len().saturating_sub(prefix)
        && suffix < after_lines.len().saturating_sub(prefix)
        && before_lines[before_lines.len() - 1 - suffix] == after_lines[after_lines.len() - 1 - suffix]
    {
        suffix += 1;
    }

    let old_start = prefix.saturating_sub(DIFF_CONTEXT_LINES);
    let new_start = old_start;
    let old_end = (before_lines.len() - suffix + DIFF_CONTEXT_LINES).min(before_lines.len());
    let new_end = (after_lines.len() - suffix + DIFF_CONTEXT_LINES).min(after_lines.len());

    let mut out = vec!["--- before".to_string(), "+++ after".to_string()];
    out.push(format!(
        "@@ -{},{} +{},{} @@",
        old_start + 1,
        old_end.saturating_sub(old_start),
        new_start + 1,
        new_end.saturating_sub(new_start)
    ));

    for line in &before_lines[old_start..prefix] {
        out.push(format!(" {}", utils::truncate_line(line)));
    }
    for line in &before_lines[prefix..before_lines.len().saturating_sub(suffix)] {
        out.push(format!("-{}", utils::truncate_line(line)));
    }
    for line in &after_lines[prefix..after_lines.len().saturating_sub(suffix)] {
        out.push(format!("+{}", utils::truncate_line(line)));
    }
    for line in &after_lines[after_lines.len().saturating_sub(suffix)..new_end] {
        out.push(format!(" {}", utils::truncate_line(line)));
    }

    if out.len() > MAX_DIFF_LINES {
        out.truncate(MAX_DIFF_LINES);
        out.push("... diff truncated".to_string());
    }
    out
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
        assert!(output.output.iter().any(|line| line == "-hello world"));
        assert!(output.output.iter().any(|line| line == "+hello there"));
    }

    #[test]
    fn replace_range_multiple_edits_match_original() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        let file = root.join("file.txt");
        std::fs::write(&file, "alpha\nbeta\ngamma\n").expect("write");

        let edits = vec![
            Replacement { old_string: "alpha".to_string(), new_string: "one".to_string() },
            Replacement { old_string: "gamma".to_string(), new_string: "three".to_string() },
        ];
        let (output, result) = exec_many("file.txt", root, &edits, None);

        assert_eq!(output.status, ToolStatus::Ok);
        assert!(result.is_some());
        assert_eq!(std::fs::read_to_string(&file).expect("read"), "one\nbeta\nthree\n");
    }

    #[test]
    fn replace_range_rejects_overlapping_edits() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        let file = root.join("file.txt");
        std::fs::write(&file, "abcdef\n").expect("write");

        let edits = vec![
            Replacement { old_string: "abc".to_string(), new_string: "x".to_string() },
            Replacement { old_string: "bcd".to_string(), new_string: "y".to_string() },
        ];
        let (output, result) = exec_many("file.txt", root, &edits, None);

        assert_eq!(output.status, ToolStatus::Failed);
        assert!(result.is_none());
        assert_eq!(std::fs::read_to_string(&file).expect("read"), "abcdef\n");
    }

    #[test]
    fn replace_range_not_found_shows_context() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        let file = root.join("file.txt");
        std::fs::write(&file, "hello world\n").expect("write");

        let (output, result) = exec("file.txt", root, "hello wurld", "x");
        assert_eq!(output.status, ToolStatus::Failed);
        assert!(result.is_none());
        let error = output.error.unwrap();
        assert!(error.contains("File preview"));
        assert!(error.contains("hello world"));
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
        assert!(output.error.as_ref().is_some_and(|e| e.contains("3 times")));
        assert_eq!(std::fs::read_to_string(&file).expect("read"), "foo foo foo\n");
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
        assert!(output.error.as_ref().is_some_and(|e| e.contains("must not be empty")));
        assert_eq!(std::fs::read_to_string(&file).expect("read"), "hello\n");
    }

    #[test]
    fn replace_range_preserves_crlf_and_bom() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        let file = root.join("file.txt");
        std::fs::write(&file, "\u{FEFF}a\r\nb\r\n").expect("write");

        let (output, result) = exec("file.txt", root, "a\nb\n", "x\ny\n");

        assert_eq!(output.status, ToolStatus::Ok);
        assert!(result.is_some());
        assert_eq!(std::fs::read_to_string(&file).expect("read"), "\u{FEFF}x\r\ny\r\n");
    }

    #[test]
    fn replace_range_expected_hash_rejects_stale_content() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        let file = root.join("file.txt");
        std::fs::write(&file, "hello\n").expect("write");

        let edits = vec![Replacement { old_string: "hello".to_string(), new_string: "hi".to_string() }];
        let (output, result) = exec_many("file.txt", root, &edits, Some(123));

        assert_eq!(output.status, ToolStatus::Failed);
        assert!(result.is_none());
        assert!(output.error.as_ref().is_some_and(|e| e.contains("stale file")));
        assert_eq!(std::fs::read_to_string(&file).expect("read"), "hello\n");
    }

    #[test]
    fn parse_arguments_reads_hash_guard() {
        let input =
            parse_arguments(r#"{"path":"file.txt","old_string":"hello","new_string":"hi","expected_before_hash":7}"#)
                .expect("parse");
        assert_eq!(
            input,
            ReplaceRangeInput {
                path: "file.txt".to_string(),
                old_string: "hello".to_string(),
                new_string: "hi".to_string(),
                expected_before_hash: Some(7),
            }
        );
    }

    #[test]
    fn registry_execute_replaces_text_with_write_result() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join("file.txt"), "hello\n").expect("write");
        let request = crate::tools::ToolUseRequest::new(
            "replace_range".to_string(),
            r#"{"path":"file.txt","old_string":"hello","new_string":"hi"}"#.to_string(),
            "call_1".to_string(),
        );

        let execution = crate::tools::registry::execute(&request, crate::tools::registry::ToolContext::new(dir.path()));

        assert_eq!(execution.output.status, ToolStatus::Ok);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("file.txt")).expect("read"),
            "hi\n"
        );
        assert_eq!(
            execution.write_result.as_ref().map(|result| result.op),
            Some(WriteOp::Edit)
        );
    }

    #[test]
    fn registry_execute_rejects_stale_hash() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join("file.txt"), "hello\n").expect("write");
        let request = crate::tools::ToolUseRequest::new(
            "replace_range".to_string(),
            r#"{"path":"file.txt","old_string":"hello","new_string":"hi","expected_before_hash":7}"#.to_string(),
            "call_1".to_string(),
        );

        let execution = crate::tools::registry::execute(&request, crate::tools::registry::ToolContext::new(dir.path()));

        assert_eq!(execution.output.status, ToolStatus::Failed);
        assert!(execution.write_result.is_none());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("file.txt")).expect("read"),
            "hello\n"
        );
    }

    #[test]
    fn replace_range_fuzzy_matches_trailing_whitespace_for_line_blocks() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        let file = root.join("file.txt");
        std::fs::write(&file, "alpha   \nbeta\n").expect("write");

        let (output, result) = exec("file.txt", root, "alpha\n", "one\n");

        assert_eq!(output.status, ToolStatus::Ok);
        assert!(result.is_some());
        assert_eq!(std::fs::read_to_string(&file).expect("read"), "one\nbeta\n");
    }

    #[test]
    fn count_occurrences_non_overlapping() {
        assert_eq!(match_offsets("aaa", "aa"), vec![0]);
    }
}
