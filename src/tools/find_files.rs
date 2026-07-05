use std::io;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use super::{MAX_RESULTS, ToolDefinition, ToolOutput, ToolUseRequest};
use crate::tools::TIMEOUT_SECS;
use crate::tools::registry::{ToolContext, ToolError, ToolExecution};
use crate::tools::subproc::CommandResult;

const NAME: &str = "find_files";

/// Parsed provider input for `find_files`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FindFilesInput {
    pattern: String,
    glob: Option<String>,
    extensions: Vec<String>,
    max_depth: Option<u32>,
    include_hidden: bool,
    follow_symlinks: bool,
}

/// Parameters for `find_files` execution.
///
/// Construct with `FindFiles { pattern, root, .. }` or `FindFiles::default()`.
#[derive(Clone, Debug)]
pub struct FindFiles<'a> {
    pub pattern: &'a str,
    pub root: &'a Path,
    pub glob: Option<&'a str>,
    pub extensions: &'a [String],
    pub max_depth: Option<u32>,
    pub max_results: usize,
    pub include_hidden: bool,
    pub follow_symlinks: bool,
}

impl FindFiles<'_> {
    /// Find files by name pattern.
    ///
    /// Backed by `fd` with `find` fallback. Uses argv arrays, never shell strings.
    /// Respects ignore rules and skips hidden files by default; both are opt-in.
    ///
    /// Enforces workspace-root containment, result-count, output-byte, and timeout caps.
    pub fn run(&self) -> ToolOutput {
        if !super::path::is_within_root(self.root, self.root) {
            return ToolOutput::failed("find_files", "invalid workspace root".to_string());
        }

        let timeout = Duration::from_secs(TIMEOUT_SECS);
        let result = if super::subproc::command_exists("fd") {
            FdFind {
                pattern: self.pattern,
                root: self.root,
                glob: self.glob,
                extensions: self.extensions,
                max_depth: self.max_depth,
                include_hidden: self.include_hidden,
                follow_symlinks: self.follow_symlinks,
                timeout,
            }
            .run()
        } else {
            self.fallback(timeout)
        };

        match result {
            Ok(output) => {
                let paths: Vec<String> = output
                    .stdout
                    .lines()
                    .filter(|l| !l.is_empty())
                    .map(|s| s.to_string())
                    .collect();
                let paths = super::subproc::truncate_results(paths, self.max_results);
                ToolOutput::ok("find_files", paths)
            }
            Err(e) => ToolOutput::failed("find_files", format!("find_files failed: {e}")),
        }
    }

    fn fallback(&self, timeout: Duration) -> io::Result<CommandResult> {
        let mut cmd = Command::new("find");
        cmd.arg(self.root).arg("-type").arg("f");
        if !self.include_hidden {
            cmd.arg("-not").arg("-path").arg("*/.*");
        }
        if let Some(depth) = self.max_depth {
            cmd.arg("-maxdepth").arg(depth.to_string());
        }
        cmd.arg("-name").arg(self.pattern);
        for ext in self.extensions {
            cmd.arg("-o").arg("-name").arg(format!("*.{ext}"));
        }
        super::subproc::run_with_timeout(cmd, timeout)
    }
}

/// Encapsulates `fd` command arguments.
#[derive(Clone, Debug)]
pub struct FdFind<'a> {
    pub pattern: &'a str,
    pub root: &'a Path,
    pub glob: Option<&'a str>,
    pub extensions: &'a [String],
    pub max_depth: Option<u32>,
    pub include_hidden: bool,
    pub follow_symlinks: bool,
    pub timeout: Duration,
}

impl FdFind<'_> {
    pub fn run(&self) -> io::Result<CommandResult> {
        let mut cmd = Command::new("fd");
        cmd.arg("--type").arg("f");
        if self.include_hidden {
            cmd.arg("--hidden");
        }
        if self.follow_symlinks {
            cmd.arg("--follow");
        }
        if let Some(depth) = self.max_depth {
            cmd.arg("--max-depth").arg(depth.to_string());
        }
        if let Some(g) = self.glob {
            cmd.arg("--glob").arg(g);
        }
        for ext in self.extensions {
            cmd.arg("--extension").arg(ext);
        }
        cmd.arg(self.pattern).arg(self.root);
        super::subproc::run_with_timeout(cmd, self.timeout)
    }
}

/// Provider-visible definition for `find_files`.
pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: NAME,
        description: r#"find_files

Locate files by name or glob under the workspace root.

Use this when you know (or can guess) a file name and need its path. Prefer this over
listing all files. Paths are contained to the root; hidden files and symlinks are off
unless requested. Capped at 100 results; long lines truncate at 512 chars."#,
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "File name or glob pattern to search for." },
                "glob": { "type": "string", "description": "Optional additional glob filter." },
                "extensions": { "type": "array", "items": { "type": "string" } },
                "max_depth": { "type": "integer" },
                "include_hidden": { "type": "boolean" },
                "follow_symlinks": { "type": "boolean" }
            },
            "required": ["pattern"]
        }),
    }
}

/// Parse provider JSON arguments for `find_files`.
pub fn parse_arguments(arguments: &str) -> Result<FindFilesInput, ToolError> {
    let args = serde_json::from_str::<serde_json::Value>(arguments).unwrap_or(serde_json::Value::Null);
    Ok(FindFilesInput {
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
        max_depth: args
            .get("max_depth")
            .and_then(|value| value.as_u64())
            .map(|depth| depth as u32),
        include_hidden: args
            .get("include_hidden")
            .and_then(|value| value.as_bool())
            .unwrap_or(false),
        follow_symlinks: args
            .get("follow_symlinks")
            .and_then(|value| value.as_bool())
            .unwrap_or(false),
    })
}

/// Execute a registry request for `find_files`.
pub fn execute_request(request: &ToolUseRequest, ctx: ToolContext<'_>) -> ToolExecution {
    match parse_arguments(&request.arguments) {
        Ok(input) => ToolExecution::output(exec_input(&input, ctx.root)),
        Err(error) => ToolExecution::output(ToolOutput::failed(NAME, error.to_string())),
    }
}

fn exec_input(input: &FindFilesInput, root: &Path) -> ToolOutput {
    FindFiles {
        pattern: &input.pattern,
        root,
        glob: input.glob.as_deref(),
        extensions: &input.extensions,
        max_depth: input.max_depth,
        max_results: MAX_RESULTS,
        include_hidden: input.include_hidden,
        follow_symlinks: input.follow_symlinks,
    }
    .run()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app::ToolStatus,
        tools::{self, MAX_RESULTS},
    };

    #[test]
    fn find_files_finds_cli_rs() {
        let output = FindFiles {
            pattern: "cli",
            root: Path::new("src"),
            glob: None,
            extensions: &[],
            max_depth: None,
            max_results: MAX_RESULTS,
            include_hidden: false,
            follow_symlinks: false,
        }
        .run();
        assert_eq!(output.status, ToolStatus::Ok);
        assert!(output.output.iter().any(|p| p.contains("cli.rs")));
    }

    #[test]
    fn find_files_no_matches_returns_empty() {
        let output = FindFiles {
            pattern: "zzz_nonexistent_zzz",
            root: Path::new("src"),
            glob: None,
            extensions: &[],
            max_depth: None,
            max_results: MAX_RESULTS,
            include_hidden: false,
            follow_symlinks: false,
        }
        .run();
        assert_eq!(output.status, ToolStatus::Ok);
        assert!(output.output.is_empty());
    }

    #[test]
    fn parse_arguments_reads_all_fields() {
        let input = parse_arguments(
            r#"{"pattern":"cli","glob":"*.rs","extensions":["rs"],"max_depth":2,"include_hidden":true,"follow_symlinks":true}"#,
        )
        .expect("parse");

        assert_eq!(input.pattern, "cli");
        assert_eq!(input.glob.as_deref(), Some("*.rs"));
        assert_eq!(input.extensions, vec!["rs"]);
        assert_eq!(input.max_depth, Some(2));
        assert!(input.include_hidden);
        assert!(input.follow_symlinks);
    }

    #[test]
    fn parse_arguments_malformed_json_uses_safe_defaults() {
        let input = parse_arguments("not valid json").expect("parse");
        assert_eq!(input.pattern, "");
        assert_eq!(input.glob, None);
        assert!(input.extensions.is_empty());
        assert_eq!(input.max_depth, None);
        assert!(!input.include_hidden);
        assert!(!input.follow_symlinks);
    }

    #[test]
    fn registry_execute_finds_file() {
        let request = ToolUseRequest::new(
            NAME.to_string(),
            serde_json::json!({"pattern":"Cargo.toml"}).to_string(),
            "call_1".to_string(),
        );

        let output = tools::registry::execute(&request, tools::registry::ToolContext::new(Path::new("."))).output;

        assert_eq!(output.status, ToolStatus::Ok);
        assert!(output.output.iter().any(|path| path.contains("Cargo.toml")));
    }
}
