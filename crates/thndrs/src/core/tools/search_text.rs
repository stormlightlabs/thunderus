use std::path::Path;
use std::process::Command;
use std::time::Duration;

use crate::tools::registry::{ToolContext, ToolError, ToolExecution};
use crate::tools::{MAX_RESULTS, SearchMatch, TIMEOUT_SECS, ToolDefinition, ToolOutput, ToolUseRequest};
use crate::utils;

const NAME: &str = "search_text";

/// Parsed provider input for `search_text`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchTextInput {
    pattern: String,
    glob: Option<String>,
    extensions: Vec<String>,
    context_lines: u32,
    include_hidden: bool,
}

/// Search file contents using `rg --json`.
///
/// Exit code `0` = matches found. Exit code `1` = no matches (not an error).
/// Exit code `2` = error. Respects ignore rules and skips hidden files by
/// default. Enforces containment, result-count, output-byte, and timeout caps.
pub fn exec(
    pattern: &str, root: &Path, glob: Option<&str>, extensions: &[String], max_results: usize, context_lines: u32,
    include_hidden: bool,
) -> ToolOutput {
    if pattern.trim().is_empty() {
        return ToolOutput::failed("search_text", "missing or empty 'pattern' field".to_string());
    }

    if !super::subproc::command_exists("rg") {
        return ToolOutput::failed(
            "search_text",
            "rg is required for search_text; grep fallback is not implemented".to_string(),
        );
    }

    let timeout = Duration::from_secs(TIMEOUT_SECS);

    let mut cmd = Command::new("rg");
    cmd.arg("--json");
    cmd.arg("--line-number");
    if context_lines > 0 {
        cmd.arg("--context").arg(context_lines.to_string());
    }
    if include_hidden {
        cmd.arg("--hidden");
    }
    if let Some(g) = glob {
        cmd.arg("--glob").arg(g);
    }
    for ext in extensions {
        let ext = ext.trim().trim_start_matches('.');
        if !ext.is_empty() {
            cmd.arg("--glob").arg(format!("*.{ext}"));
        }
    }
    cmd.arg(pattern).arg(root);

    let result = super::subproc::run_with_timeout(cmd, timeout);
    match result {
        Ok(output) => {
            if output.exit_code == 1 {
                return ToolOutput::ok("search_text", Vec::new());
            }
            if !output.success && output.exit_code != 0 {
                let err_msg = if output.stderr.is_empty() {
                    format!("rg exited with code {}", output.exit_code)
                } else {
                    output.stderr.lines().take(5).collect::<Vec<_>>().join("\n")
                };
                return ToolOutput::failed("search_text", err_msg);
            }

            let matches = parse_rg_json(&output.stdout);
            let matches = matches.into_iter().take(max_results).collect::<Vec<_>>();
            let lines: Vec<String> = matches
                .iter()
                .map(|m| utils::truncate_line(&format!("{}:{}:{}", m.path, m.line_number, m.text.trim_end())))
                .collect();
            ToolOutput::ok("search_text", lines)
                .with_evidence_content_hash(format!("{:016x}", crate::tools::hash_content(&output.stdout)))
        }
        Err(e) => ToolOutput::failed("search_text", format!("search_text failed: {e}")),
    }
}

/// Provider-visible definition for `search_text`.
pub fn definition() -> ToolDefinition {
    ToolDefinition::new(
        NAME,
        "search_text

Grep file contents by regex under the workspace root.

Returns matching lines as file:line:text. Use this when you need to find where a
symbol, string, or pattern appears in the codebase. Prefer this over listing files
when you need content. Paths are contained to the root; hidden files are off unless
requested. Capped at 100 matches; lines truncate at 512 chars.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "Regex pattern to search for." },
                "glob": { "type": "string" },
                "extensions": { "type": "array", "items": { "type": "string" } },
                "context_lines": { "type": "integer" },
                "include_hidden": { "type": "boolean" }
            },
            "required": ["pattern"]
        }),
    )
}

/// Parse provider JSON arguments for `search_text`.
pub fn parse_arguments(arguments: &str) -> Result<SearchTextInput, ToolError> {
    let args = serde_json::from_str::<serde_json::Value>(arguments).unwrap_or(serde_json::Value::Null);
    Ok(SearchTextInput {
        pattern: args
            .get("pattern")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string(),
        glob: args.get("glob").and_then(|value| value.as_str()).map(str::to_string),
        extensions: args
            .get("extensions")
            .and_then(|value| value.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|value| value.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        context_lines: args.get("context_lines").and_then(|value| value.as_u64()).unwrap_or(0) as u32,
        include_hidden: args
            .get("include_hidden")
            .and_then(|value| value.as_bool())
            .unwrap_or(false),
    })
}

/// Execute a registry request for `search_text`.
pub fn execute_request(request: &ToolUseRequest, ctx: &ToolContext<'_>) -> ToolExecution {
    match parse_arguments(&request.arguments) {
        Ok(input) => ToolExecution::output(exec_input(&input, ctx.root)),
        Err(error) => ToolExecution::output(ToolOutput::failed(NAME, error.to_string())),
    }
}

/// Parse `rg --json` output (NDJSON) into a list of search matches.
///
/// `match` and `context` messages are extracted. `begin`, `end`, and `summary`
/// messages are ignored.
pub fn parse_rg_json(output: &str) -> Vec<SearchMatch> {
    output
        .lines()
        .filter(|l| !l.is_empty())
        .filter_map(|line| {
            let v: serde_json::Value = serde_json::from_str(line).ok()?;
            let record_type = v.get("type").and_then(|t| t.as_str());
            if !matches!(record_type, Some("match" | "context")) {
                return None;
            }
            let data = v.get("data")?;
            let path = data.get("path")?.get("text").and_then(|p| p.as_str())?.to_string();
            let line_number = data
                .get("line_number")
                .and_then(|n| n.as_u64())
                .map(|n| n as u32)
                .unwrap_or(0);
            let text = data
                .get("lines")
                .and_then(|l| l.get("text"))
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();
            Some(SearchMatch { path, line_number, text })
        })
        .collect()
}

fn exec_input(input: &SearchTextInput, root: &Path) -> ToolOutput {
    exec(
        &input.pattern,
        root,
        input.glob.as_deref(),
        &input.extensions,
        MAX_RESULTS,
        input.context_lines,
        input.include_hidden,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app::ToolStatus,
        tools::{self, MAX_RESULTS},
    };

    #[test]
    fn parse_rg_json_extracts_matches() {
        let json = r#"{"type":"begin","data":{"path":{"text":"Cargo.toml"}}}
{"type":"match","data":{"path":{"text":"Cargo.toml"},"lines":{"text":"name = \"thndrs\"\n"},"line_number":2,"absolute_offset":10,"submatches":[{"match":{"text":"thndrs"},"start":8,"end":14}]}}
{"type":"end","data":{"path":{"text":"Cargo.toml"}}}
{"type":"summary","data":{"elapsed_total":{"human":"0.001s","nanos":1000000,"secs":0}}}
"#;
        let matches = parse_rg_json(json);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].path, "Cargo.toml");
        assert_eq!(matches[0].line_number, 2);
        assert!(matches[0].text.contains("thndrs"));
    }

    #[test]
    fn parse_rg_json_no_matches() {
        let json = r#"{"type":"summary","data":{"elapsed_total":{"human":"0.001s","nanos":1000000,"secs":0},"stats":{"matched_lines":0,"matches":0}}}
"#;
        let matches = parse_rg_json(json);
        assert!(matches.is_empty());
    }

    #[test]
    fn parse_rg_json_multiple_matches() {
        let json = r#"{"type":"match","data":{"path":{"text":"a.rs"},"lines":{"text":"let x = 1;\n"},"line_number":5}}
{"type":"match","data":{"path":{"text":"b.rs"},"lines":{"text":"let y = 2;\n"},"line_number":10}}
"#;
        let matches = parse_rg_json(json);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].path, "a.rs");
        assert_eq!(matches[0].line_number, 5);
        assert_eq!(matches[1].path, "b.rs");
        assert_eq!(matches[1].line_number, 10);
    }

    #[test]
    fn parse_rg_json_includes_context_and_ignores_begin() {
        let json = r#"{"type":"begin","data":{"path":{"text":"x.rs"}}}
{"type":"context","data":{"path":{"text":"x.rs"},"lines":{"text":"// comment\n"},"line_number":3}}
{"type":"match","data":{"path":{"text":"x.rs"},"lines":{"text":"let x = 1;\n"},"line_number":4}}
{"type":"end","data":{"path":{"text":"x.rs"}}}
"#;
        let matches = parse_rg_json(json);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].line_number, 3);
        assert_eq!(matches[1].line_number, 4);
    }

    #[test]
    fn parse_rg_json_malformed_line_skipped() {
        let json = r#"not valid json
{"type":"match","data":{"path":{"text":"ok.rs"},"lines":{"text":"ok\n"},"line_number":1}}
"#;
        let matches = parse_rg_json(json);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].path, "ok.rs");
    }

    #[test]
    fn search_text_exit_code_1_is_no_matches() {
        let output = exec(
            "zzz_nonexistent_pattern_zzz",
            Path::new("Cargo.toml"),
            None,
            &[],
            MAX_RESULTS,
            0,
            false,
        );
        assert_eq!(output.status, ToolStatus::Ok);
        assert!(output.display.lines.is_empty());
    }

    #[test]
    fn search_text_finds_matches() {
        let output = exec("thndrs", Path::new("Cargo.toml"), None, &[], MAX_RESULTS, 0, false);
        assert_eq!(output.status, ToolStatus::Ok);
        assert!(!output.display.lines.is_empty());
        assert!(output.display.lines[0].contains("Cargo.toml"));
    }

    #[test]
    fn search_text_rejects_empty_pattern() {
        let output = exec("", Path::new("src"), None, &[], MAX_RESULTS, 0, false);
        assert_eq!(output.status, ToolStatus::Failed);
        assert_eq!(output.error.as_deref(), Some("missing or empty 'pattern' field"));
    }

    #[test]
    fn search_text_extension_filter_accepts_file_extensions() {
        let output = exec(
            "fn search_text_finds_matches",
            Path::new("src"),
            None,
            &["rs".to_string()],
            MAX_RESULTS,
            0,
            false,
        );
        assert_eq!(output.status, ToolStatus::Ok);
        assert!(output.display.lines.iter().any(|line| line.contains("search_text.rs")));
    }

    #[test]
    fn search_text_returns_context_lines() {
        let output = exec(
            "fn search_text_finds_matches",
            Path::new("src/core/tools/search_text.rs"),
            None,
            &[],
            MAX_RESULTS,
            1,
            false,
        );
        assert_eq!(output.status, ToolStatus::Ok);
        assert!(
            output.display.lines.len() >= 2,
            "expected match plus context, got {:?}",
            output.display.lines
        );
    }

    #[test]
    fn parse_arguments_reads_all_fields() {
        let input = parse_arguments(
            r#"{"pattern":"fn main","glob":"src/**/*.rs","extensions":["rs"],"context_lines":2,"include_hidden":true}"#,
        )
        .expect("parse");

        assert_eq!(input.pattern, "fn main");
        assert_eq!(input.glob.as_deref(), Some("src/**/*.rs"));
        assert_eq!(input.extensions, vec!["rs"]);
        assert_eq!(input.context_lines, 2);
        assert!(input.include_hidden);
    }

    #[test]
    fn parse_arguments_malformed_json_uses_safe_defaults() {
        let input = parse_arguments("not valid json").expect("parse");
        assert_eq!(input.pattern, "");
        assert_eq!(input.glob, None);
        assert!(input.extensions.is_empty());
        assert_eq!(input.context_lines, 0);
        assert!(!input.include_hidden);
    }

    #[test]
    fn registry_execute_finds_text() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join("alpha.rs"), "fn alpha() {}\n").expect("write");
        let request = ToolUseRequest::new(
            NAME.to_string(),
            serde_json::json!({"pattern":"alpha"}).to_string(),
            "call_1".to_string(),
        );

        let output = tools::registry::execute(&request, &tools::registry::ToolContext::new(dir.path())).output;

        assert_eq!(output.status, ToolStatus::Ok);
        assert!(output.display.lines.iter().any(|line| line.contains("alpha.rs")));
    }
}
