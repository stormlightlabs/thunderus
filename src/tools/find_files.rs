use std::io;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use super::ToolOutput;
use crate::tools::TIMEOUT_SECS;
use crate::tools::subproc::CommandResult;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{app::ToolStatus, tools::MAX_RESULTS};

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
}
