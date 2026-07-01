use std::io;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use crate::tools::subproc::CommandResult;
use crate::tools::{TIMEOUT_SECS, ToolOutput};

/// List searchable files in a directory tree.
///
/// Backed by `fd --type f` with `rg --files` and `find` fallbacks. Respects
/// ignore rules when the selected backend supports them and skips hidden files
/// by default. Enforces containment, result-count, output-byte, and timeout caps.
pub fn exec(root: &Path, glob: Option<&str>, max_results: usize, include_hidden: bool) -> ToolOutput {
    let timeout = Duration::from_secs(TIMEOUT_SECS);
    let result = if super::subproc::command_exists("fd") {
        run_fd_files(root, include_hidden, timeout)
    } else if super::subproc::command_exists("rg") {
        run_rg_files(root, include_hidden, timeout)
    } else if super::subproc::command_exists("find") {
        run_find_files(root, include_hidden, timeout)
    } else {
        return ToolOutput::failed(
            "list_searchable_files",
            "none of fd, rg, or find is available".to_string(),
        );
    };

    match result {
        Ok(output) => {
            let mut paths: Vec<String> = output
                .stdout
                .lines()
                .filter(|l| !l.is_empty())
                .map(|s| s.to_string())
                .collect();

            if let Some(g) = glob {
                paths.retain(|p| matches_glob(p, g));
            }

            let paths = super::subproc::truncate_results(paths, max_results);
            ToolOutput::ok("list_searchable_files", paths)
        }
        Err(e) => ToolOutput::failed("list_searchable_files", format!("list failed: {e}")),
    }
}

fn run_rg_files(root: &Path, include_hidden: bool, timeout: Duration) -> io::Result<CommandResult> {
    let mut cmd = Command::new("rg");
    cmd.arg("--files");
    if include_hidden {
        cmd.arg("--hidden");
    }
    cmd.arg(root);
    super::subproc::run_with_timeout(cmd, timeout)
}

fn run_fd_files(root: &Path, include_hidden: bool, timeout: Duration) -> io::Result<CommandResult> {
    let mut cmd = Command::new("fd");
    cmd.arg("--type").arg("f");
    if include_hidden {
        cmd.arg("--hidden");
    }
    cmd.arg(".").arg(root);
    super::subproc::run_with_timeout(cmd, timeout)
}

fn run_find_files(root: &Path, include_hidden: bool, timeout: Duration) -> io::Result<CommandResult> {
    let mut cmd = Command::new("find");
    cmd.arg(root).arg("-type").arg("f");
    if !include_hidden {
        cmd.arg("-not").arg("-path").arg("*/.*");
    }
    super::subproc::run_with_timeout(cmd, timeout)
}

/// Simple glob match using `*` as wildcard. Not a full glob implementation,
/// but sufficient for file filtering.
fn matches_glob(path: &str, glob: &str) -> bool {
    if glob.contains('*') {
        let parts: Vec<&str> = glob.split('*').collect();
        let mut pos = 0;
        for (i, part) in parts.iter().enumerate() {
            if part.is_empty() {
                continue;
            }
            if i == 0 && !path.starts_with(part) {
                return false;
            }
            match path[pos..].find(part) {
                Some(idx) => pos += idx + part.len(),
                None => return false,
            }
        }
        if let Some(last) = parts.last()
            && !last.is_empty()
            && !path.ends_with(last)
        {
            return false;
        }
        true
    } else {
        path.contains(glob)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app::ToolStatus,
        tools::{MAX_RESULTS, TIMEOUT_SECS},
    };

    #[test]
    fn list_searchable_files_lists_source_files() {
        let output = exec(Path::new("src"), None, MAX_RESULTS, false);
        assert_eq!(output.status, ToolStatus::Ok);
        assert!(!output.output.is_empty());
        assert!(output.output.iter().any(|p| p.contains(".rs")));
    }

    #[test]
    fn list_searchable_files_with_glob_filter() {
        let output = exec(Path::new("src"), Some("*.rs"), MAX_RESULTS, false);
        assert_eq!(output.status, ToolStatus::Ok);
        assert!(output.output.iter().all(|p| p.ends_with(".rs")));
    }

    #[test]
    fn find_fallback_lists_files() {
        let output = run_find_files(Path::new("src"), false, Duration::from_secs(TIMEOUT_SECS))
            .expect("find fallback should run");
        assert!(output.stdout.lines().any(|p| p.ends_with(".rs")));
    }

    #[test]
    fn matches_glob_simple() {
        assert!(matches_glob("src/main.rs", "*.rs"));
        assert!(matches_glob("src/cli.rs", "*.rs"));
        assert!(!matches_glob("src/main.ts", "*.rs"));
    }

    #[test]
    fn matches_glob_prefix() {
        assert!(matches_glob("src/main.rs", "src/*"));
    }
}
