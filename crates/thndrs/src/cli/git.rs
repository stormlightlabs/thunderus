//! Reads `git status --porcelain=v1` for the workspace subtree and reduces it
//! to the branch name plus added, modified, and deleted file counts.
//!
//! The collector uses read-only git commands with optional locks disabled.

use std::{path::Path, process::Command};

/// A changed-file category reported by `git status`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GitStatusKind {
    Added,
    Deleted,
    Modified,
}

impl GitStatusKind {
    fn parse(c: &str) -> GitStatusKind {
        match c {
            "??" => GitStatusKind::Added,
            code => {
                if code.contains('U') {
                    return GitStatusKind::Modified;
                }
                if code.contains('A') && !code.contains('D') {
                    return GitStatusKind::Added;
                }
                if code.contains('D') && !code.contains('A') {
                    return GitStatusKind::Deleted;
                }
                GitStatusKind::Modified
            }
        }
    }
}

/// One parsed porcelain status item.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitStatusItem {
    pub file: String,
    pub code: String,
    pub status: GitStatusKind,
}

/// Bounded semantic summary of the workspace git state.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct GitStatusSummary {
    pub branch: Option<String>,
    pub added: usize,
    pub modified: usize,
    pub deleted: usize,
}

impl GitStatusSummary {
    pub fn from_items(branch: Option<String>, items: &[GitStatusItem]) -> Self {
        let mut summary = Self { branch, ..Default::default() };
        for item in items {
            match item.status {
                GitStatusKind::Added => summary.added += 1,
                GitStatusKind::Deleted => summary.deleted += 1,
                GitStatusKind::Modified => summary.modified += 1,
            }
        }
        summary
    }

    /// Format the summary for the compact direct-renderer status line.
    pub fn display(&self) -> String {
        let branch = self.branch.as_deref().unwrap_or("detached");
        if self.added == 0 && self.modified == 0 && self.deleted == 0 {
            return format!("git: {branch} clean");
        }
        format!("git: {branch} +{} ~{} -{}", self.added, self.modified, self.deleted)
    }
}

/// Collect a semantic status summary without changing the workspace.
pub fn collect(cwd: &Path) -> Option<GitStatusSummary> {
    if !git_success(cwd, &["rev-parse", "--is-inside-work-tree"]) {
        return None;
    }
    let branch = git_output(cwd, &["symbolic-ref", "--quiet", "--short", "HEAD"])
        .and_then(|branch| (!branch.is_empty()).then_some(branch));
    let output = git_output(
        cwd,
        &[
            "status",
            "--porcelain=v1",
            "--untracked-files=all",
            "--no-renames",
            "-z",
            "--",
            ".",
        ],
    )?;

    Some(GitStatusSummary::from_items(branch, &parse_status_output(&output)))
}

fn git_success(cwd: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .args(git_config_args())
        .args(args)
        .current_dir(cwd)
        .output()
        .is_ok_and(|output| output.status.success())
}

fn git_output(cwd: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(git_config_args())
        .args(args)
        .current_dir(cwd)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn git_config_args() -> [&'static str; 9] {
    [
        "--no-optional-locks",
        "-c",
        "core.autocrlf=false",
        "-c",
        "core.fsmonitor=false",
        "-c",
        "core.quotepath=false",
        "-c",
        "status.relativePaths=true",
    ]
}

fn parse_status_output(output: &str) -> Vec<GitStatusItem> {
    output
        .split('\0')
        .filter_map(|item| {
            if item.len() < 4 {
                return None;
            }
            let code = item[..2].to_string();
            let file = item[3..].to_string();
            if file.is_empty() {
                return None;
            }
            Some(GitStatusItem { status: GitStatusKind::parse(&code), code, file })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn parse_porcelain_status_items() {
        let items = parse_status_output(" M src/main.rs\0?? new.txt\0D  old.txt\0");
        assert_eq!(
            items,
            vec![
                GitStatusItem {
                    file: "src/main.rs".to_string(),
                    code: " M".to_string(),
                    status: GitStatusKind::Modified,
                },
                GitStatusItem { file: "new.txt".to_string(), code: "??".to_string(), status: GitStatusKind::Added },
                GitStatusItem { file: "old.txt".to_string(), code: "D ".to_string(), status: GitStatusKind::Deleted },
            ]
        );
    }

    #[test]
    fn summary_display_shows_clean_or_counts() {
        assert_eq!(
            GitStatusSummary::from_items(Some("main".to_string()), &[]).display(),
            "git: main clean"
        );
        let items = parse_status_output(" M src/main.rs\0?? new.txt\0D  old.txt\0");
        assert_eq!(
            GitStatusSummary::from_items(Some("main".to_string()), &items).display(),
            "git: main +1 ~1 -1"
        );
    }

    #[test]
    fn collect_returns_none_outside_git_repo() {
        let dir = tempfile::tempdir().expect("temp dir");
        assert_eq!(collect(dir.path()), None);
    }

    #[test]
    fn collect_reports_clean_temp_repo() {
        let dir = temp_git_repo();
        let summary = collect(dir.path()).expect("git summary");
        assert_eq!(summary.added, 0);
        assert_eq!(summary.modified, 0);
        assert_eq!(summary.deleted, 0);
        assert!(summary.branch.is_some(), "clean repo should report current branch");
    }

    #[test]
    fn collect_reports_untracked_modified_and_deleted_files() {
        let dir = temp_git_repo();
        std::fs::write(dir.path().join("tracked.txt"), "dirty\n").expect("modify tracked file");
        std::fs::write(dir.path().join("new.txt"), "new\n").expect("write untracked file");
        std::fs::remove_file(dir.path().join("deleted.txt")).expect("delete tracked file");

        let summary = collect(dir.path()).expect("git summary");
        assert_eq!(summary.added, 1);
        assert_eq!(summary.modified, 1);
        assert_eq!(summary.deleted, 1);
    }

    fn temp_git_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("temp git dir");
        git(dir.path(), &["init"]);
        git(dir.path(), &["config", "user.email", "test@example.com"]);
        git(dir.path(), &["config", "user.name", "Test User"]);
        std::fs::write(dir.path().join("tracked.txt"), "clean\n").expect("write tracked file");
        std::fs::write(dir.path().join("deleted.txt"), "delete me\n").expect("write deleted file");
        git(dir.path(), &["add", "."]);
        git(dir.path(), &["commit", "-m", "initial"]);
        dir
    }

    fn git(cwd: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .unwrap_or_else(|err| panic!("git {args:?} failed to start: {err}"));
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
