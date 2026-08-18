use std::path::Path;
use std::process::Command;
use std::time::Duration;

use crate::tools::registry::{ToolContext, ToolError, ToolExecution};
use crate::tools::subproc::CommandResult;
use crate::tools::{MAX_RESULTS, TIMEOUT_SECS, ToolDefinition, ToolOutput, ToolUseRequest};

const NAME: &str = "list_searchable_files";

/// Parsed provider input for `list_searchable_files`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListSearchableFilesInput {
    glob: Option<String>,
    include_hidden: bool,
}

/// List searchable files in a directory tree.
///
/// Backed by `fd --type f` with `rg --files` and an in-process fallback. Respects
/// ignore rules when the selected backend supports them and skips hidden files
/// by default. Enforces containment, result-count, output-byte, and timeout caps.
pub fn exec(root: &Path, glob: Option<&str>, max_results: usize, include_hidden: bool) -> ToolOutput {
    exec_with_programs(
        root,
        glob,
        max_results,
        include_hidden,
        &super::search::backend::SearchPrograms::discover(),
    )
}

fn exec_with_programs(
    root: &Path, glob: Option<&str>, max_results: usize, include_hidden: bool,
    programs: &super::search::backend::SearchPrograms,
) -> ToolOutput {
    let timeout = Duration::from_secs(TIMEOUT_SECS);
    let result = if let Some(fd) = programs.fd() {
        run_fd_files(fd, root, include_hidden, timeout).map(|output| ("fd", command_paths(&output)))
    } else if let Some(rg) = programs.rg() {
        run_rg_files(rg, root, include_hidden, timeout).map(|output| ("rg --files (degraded)", command_paths(&output)))
    } else {
        super::search::backend::fallback_files(root, include_hidden, None).map(|files| {
            let paths = files
                .into_iter()
                .map(|path| super::search::backend::display_path(root, &path))
                .collect();
            ("in-process fallback (degraded)", paths)
        })
    };

    match result {
        Ok((label, mut paths)) => {
            if let Some(g) = glob {
                paths.retain(|path| super::search::backend::matches_glob(path, g));
            }

            let paths = super::subproc::truncate_results(paths, max_results);
            ToolOutput::ok(
                "list_searchable_files",
                super::search::backend::with_implementation_line(label, paths),
            )
        }
        Err(e) => ToolOutput::failed("list_searchable_files", format!("list failed: {e}")),
    }
}

/// Provider-visible definition for `list_searchable_files`.
pub fn definition() -> ToolDefinition {
    ToolDefinition::new(
        NAME,
        r#"list_searchable_files

Enumerate searchable files under the workspace root.

Use this to get an overview of the project structure. Prefer find_files when you know
a file name, or search_text when you need content matches. Respects ignore rules and
skips hidden files by default. Capped at 100 results."#,
        serde_json::json!({
            "type": "object",
            "properties": {
                "glob": { "type": "string" },
                "include_hidden": { "type": "boolean" }
            }
        }),
    )
}

/// Parse provider JSON arguments for `list_searchable_files`.
pub fn parse_arguments(arguments: &str) -> Result<ListSearchableFilesInput, ToolError> {
    let args = serde_json::from_str::<serde_json::Value>(arguments).unwrap_or(serde_json::Value::Null);
    Ok(ListSearchableFilesInput {
        glob: args.get("glob").and_then(|value| value.as_str()).map(str::to_string),
        include_hidden: args
            .get("include_hidden")
            .and_then(|value| value.as_bool())
            .unwrap_or(false),
    })
}

/// Execute a registry request for `list_searchable_files`.
pub fn execute_request(request: &ToolUseRequest, ctx: &ToolContext<'_>) -> ToolExecution {
    match parse_arguments(&request.arguments) {
        Ok(input) => ToolExecution::output(exec(ctx.root, input.glob.as_deref(), MAX_RESULTS, input.include_hidden)),
        Err(error) => ToolExecution::output(ToolOutput::failed(NAME, error.to_string())),
    }
}

fn run_rg_files(
    executable: &Path, root: &Path, include_hidden: bool, timeout: Duration,
) -> std::io::Result<CommandResult> {
    let mut cmd = Command::new(executable);
    cmd.arg("--files");
    if include_hidden {
        cmd.arg("--hidden");
    }
    cmd.arg(root);
    super::subproc::run_with_timeout(cmd, timeout)
}

fn run_fd_files(
    executable: &Path, root: &Path, include_hidden: bool, timeout: Duration,
) -> std::io::Result<CommandResult> {
    let mut cmd = Command::new(executable);
    cmd.arg("--type").arg("f");
    if include_hidden {
        cmd.arg("--hidden");
    }
    cmd.arg(".").arg(root);
    super::subproc::run_with_timeout(cmd, timeout)
}

fn command_paths(output: &CommandResult) -> Vec<String> {
    output
        .stdout
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app::ToolStatus,
        tools::{self, MAX_RESULTS},
    };

    #[test]
    fn list_searchable_files_lists_source_files() {
        let output = exec(Path::new("src"), None, MAX_RESULTS, false);
        assert_eq!(output.status, ToolStatus::Ok);
        assert!(!output.display.lines.is_empty());
        assert!(output.display.lines.iter().any(|p| p.contains(".rs")));
    }

    #[test]
    fn list_searchable_files_with_glob_filter() {
        let output = exec(Path::new("src"), Some("*.rs"), MAX_RESULTS, false);
        assert_eq!(output.status, ToolStatus::Ok);
        assert!(output.display.lines.iter().skip(1).all(|p| p.ends_with(".rs")));
    }

    #[test]
    fn in_process_fallback_lists_files_and_names_degradation() {
        let output = exec_with_programs(Path::new("src"), None, MAX_RESULTS, false, &Default::default());
        assert_eq!(
            output.display.lines[0],
            "[implementation: in-process fallback (degraded)]"
        );
        assert!(output.display.lines.iter().any(|path| path.ends_with(".rs")));
    }

    #[test]
    fn matches_glob_simple() {
        assert!(super::super::search::backend::matches_glob("src/main.rs", "*.rs"));
        assert!(super::super::search::backend::matches_glob("src/cli/mod.rs", "*.rs"));
        assert!(!super::super::search::backend::matches_glob("src/main.ts", "*.rs"));
    }

    #[test]
    fn matches_glob_prefix() {
        assert!(super::super::search::backend::matches_glob("src/main.rs", "src/*"));
    }

    #[test]
    fn parse_arguments_reads_optional_fields() {
        let input = parse_arguments(r#"{"glob":"*.rs","include_hidden":true}"#).expect("parse");
        assert_eq!(input.glob.as_deref(), Some("*.rs"));
        assert!(input.include_hidden);
    }

    #[test]
    fn parse_arguments_malformed_json_uses_safe_defaults() {
        let input = parse_arguments("not valid json").expect("parse");
        assert_eq!(input.glob, None);
        assert!(!input.include_hidden);
    }

    #[test]
    fn registry_execute_lists_files() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join("alpha.rs"), "fn main() {}\n").expect("write");
        let request = ToolUseRequest::new(
            NAME.to_string(),
            serde_json::json!({"glob":"*.rs"}).to_string(),
            "call_1".to_string(),
        );

        let output = tools::registry::execute(&request, &tools::registry::ToolContext::new(dir.path())).output;

        assert_eq!(output.status, ToolStatus::Ok);
        assert!(output.display.lines.iter().any(|path| path.ends_with("alpha.rs")));
    }
}
