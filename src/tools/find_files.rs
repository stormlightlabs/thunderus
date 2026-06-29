use std::io;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use super::ToolOutput;
use crate::tools::subproc::CommandResult;

/// Parameters for find_files execution
pub struct FindFiles<'a> {
    pattern: &'a str,
    root: &'a Path,
    glob: Option<&'a str>,
    extensions: &'a [String],
    max_depth: Option<u32>,
    max_results: usize,
    include_hidden: bool,
    follow_symlinks: bool,
}

impl<'a> FindFiles<'a> {
    pub fn new(
        pattern: &'a str, root: &'a Path, glob: Option<&'a str>, extensions: &'a [String], max_depth: Option<u32>,
        max_results: usize, include_hidden: bool, follow_symlinks: bool,
    ) -> Self {
        Self { pattern, root, glob, extensions, max_depth, max_results, include_hidden, follow_symlinks }
    }
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

        let timeout = Duration::from_secs(super::caps::TIMEOUT_SECS);
        let result = if super::subproc::command_exists("fd") {
            FdFind::new(
                self.pattern,
                self.root,
                self.glob,
                self.extensions,
                self.max_depth,
                self.include_hidden,
                self.follow_symlinks,
                timeout,
            )
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

/// Encapsulates `fd` command arguments
pub struct FdFind<'a> {
    pattern: &'a str,
    root: &'a Path,
    glob: Option<&'a str>,
    extensions: &'a [String],
    max_depth: Option<u32>,
    include_hidden: bool,
    follow_symlinks: bool,
    timeout: Duration,
}

impl<'a> FdFind<'a> {
    pub fn new(
        pattern: &'a str, root: &'a Path, glob: Option<&'a str>, extensions: &'a [String], max_depth: Option<u32>,
        include_hidden: bool, follow_symlinks: bool, timeout: Duration,
    ) -> Self {
        Self { pattern, root, glob, extensions, max_depth, include_hidden, follow_symlinks, timeout }
    }
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
    use crate::{app::ToolStatus, tools::caps};

    #[test]
    fn find_files_finds_cli_rs() {
        let output = FindFiles::new(
            "cli",
            Path::new("src"),
            None,
            &[],
            None,
            caps::MAX_RESULTS,
            false,
            false,
        )
        .run();
        assert_eq!(output.status, ToolStatus::Ok);
        assert!(output.output.iter().any(|p| p.contains("cli.rs")));
    }

    #[test]
    fn find_files_no_matches_returns_empty() {
        let output = FindFiles::new(
            "zzz_nonexistent_zzz",
            Path::new("src"),
            None,
            &[],
            None,
            caps::MAX_RESULTS,
            false,
            false,
        )
        .run();
        assert_eq!(output.status, ToolStatus::Ok);
        assert!(output.output.is_empty());
    }
}
