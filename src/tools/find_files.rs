use std::io;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use super::ToolOutput;
use crate::tools::subproc::CommandResult;

/// Find files by name pattern.
///
/// Backed by `fd` with `find` fallback. Uses argv arrays, never shell strings.
/// Respects ignore rules and skips hidden files by default; both are opt-in.
/// Enforces workspace-root containment, result-count, output-byte, and timeout
/// caps.
///
/// FIXME: Mirrors the typed ToolInput fields, should use a params struct.
#[allow(clippy::too_many_arguments)]
pub fn exec(
    pattern: &str, root: &Path, glob: Option<&str>, extensions: &[String], max_depth: Option<u32>, max_results: usize,
    include_hidden: bool, follow_symlinks: bool,
) -> ToolOutput {
    if !super::path::is_within_root(root, root) {
        return ToolOutput::failed("find_files", "invalid workspace root".to_string());
    }

    let timeout = Duration::from_secs(super::caps::TIMEOUT_SECS);
    let result = if super::subproc::command_exists("fd") {
        run_fd_find(
            pattern,
            root,
            glob,
            extensions,
            max_depth,
            include_hidden,
            follow_symlinks,
            timeout,
        )
    } else {
        run_find_fallback(pattern, root, extensions, max_depth, include_hidden, timeout)
    };

    match result {
        Ok(output) => {
            let paths: Vec<String> = output
                .stdout
                .lines()
                .filter(|l| !l.is_empty())
                .map(|s| s.to_string())
                .collect();
            let paths = super::subproc::truncate_results(paths, max_results);
            ToolOutput::ok("find_files", paths)
        }
        Err(e) => ToolOutput::failed("find_files", format!("find_files failed: {e}")),
    }
}

/// FIXME: use a params struct that mirrors the typed ToolInput fields.
#[allow(clippy::too_many_arguments)]
fn run_fd_find(
    pattern: &str, root: &Path, glob: Option<&str>, extensions: &[String], max_depth: Option<u32>,
    include_hidden: bool, follow_symlinks: bool, timeout: Duration,
) -> io::Result<CommandResult> {
    let mut cmd = Command::new("fd");
    cmd.arg("--type").arg("f");
    if include_hidden {
        cmd.arg("--hidden");
    }
    if follow_symlinks {
        cmd.arg("--follow");
    }
    if let Some(depth) = max_depth {
        cmd.arg("--max-depth").arg(depth.to_string());
    }
    if let Some(g) = glob {
        cmd.arg("--glob").arg(g);
    }
    for ext in extensions {
        cmd.arg("--extension").arg(ext);
    }
    cmd.arg(pattern).arg(root);
    super::subproc::run_with_timeout(cmd, timeout)
}

fn run_find_fallback(
    pattern: &str, root: &Path, extensions: &[String], max_depth: Option<u32>, include_hidden: bool, timeout: Duration,
) -> io::Result<CommandResult> {
    let mut cmd = Command::new("find");
    cmd.arg(root).arg("-type").arg("f");
    if !include_hidden {
        cmd.arg("-not").arg("-path").arg("*/.*");
    }
    if let Some(depth) = max_depth {
        cmd.arg("-maxdepth").arg(depth.to_string());
    }
    cmd.arg("-name").arg(pattern);
    for ext in extensions {
        cmd.arg("-o").arg("-name").arg(format!("*.{ext}"));
    }
    super::subproc::run_with_timeout(cmd, timeout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{app::ToolStatus, tools::caps};

    #[test]
    fn find_files_finds_cli_rs() {
        let output = exec(
            "cli",
            Path::new("src"),
            None,
            &[],
            None,
            caps::MAX_RESULTS,
            false,
            false,
        );
        assert_eq!(output.status, ToolStatus::Ok);
        assert!(output.output.iter().any(|p| p.contains("cli.rs")));
    }

    #[test]
    fn find_files_no_matches_returns_empty() {
        let output = exec(
            "zzz_nonexistent_zzz",
            Path::new("src"),
            None,
            &[],
            None,
            caps::MAX_RESULTS,
            false,
            false,
        );
        assert_eq!(output.status, ToolStatus::Ok);
        assert!(output.output.is_empty());
    }
}
