//! Reads `git status --porcelain=v1` for the workspace subtree and reduces it
//! to the branch name plus added, modified, and deleted file counts.
//!
//! The collector uses read-only git commands with optional locks disabled.

use std::{
    fs,
    io::Read,
    path::Path,
    process::{Command, Stdio},
};

const MAX_REVIEW_LINES: usize = 4_000;
const MAX_REVIEW_BYTES: usize = 1024 * 1024;

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

/// One changed file in the read-only workspace review surface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitChange {
    pub file: String,
    pub status: GitStatusKind,
    pub added: usize,
    pub removed: usize,
    pub diff: Vec<String>,
}

/// Bounded snapshot of the current workspace changes.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct GitChangeReview {
    pub files: Vec<GitChange>,
    pub added: usize,
    pub removed: usize,
    pub truncated: bool,
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
    let output = git_output_preserving_whitespace(
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

/// Collect current workspace changes for an inspection-only review surface.
///
/// Tracked files are compared with `HEAD`; untracked UTF-8 files are rendered
/// as additions. The result is bounded so a large generated diff cannot take
/// over the interactive renderer.
pub fn collect_review(cwd: &Path) -> Option<GitChangeReview> {
    if !git_success(cwd, &["rev-parse", "--is-inside-work-tree"]) {
        return None;
    }
    let (mut status, status_truncated) = git_output_bounded(
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
        MAX_REVIEW_BYTES,
    )?;
    if status_truncated {
        status.truncate(status.iter().rposition(|byte| *byte == 0).map_or(0, |end| end + 1));
    }
    let items = parse_status_output(&String::from_utf8_lossy(&status));
    let mut review = GitChangeReview { truncated: status_truncated, ..GitChangeReview::default() };
    let mut remaining = MAX_REVIEW_LINES;
    let mut remaining_bytes = MAX_REVIEW_BYTES;

    for item in items {
        let (diff, truncated) = if item.code == "??" {
            untracked_diff(cwd, &item.file, remaining, remaining_bytes)
        } else {
            tracked_diff(cwd, &item.file, remaining, remaining_bytes)
        };
        let (added, removed) = count_diff_lines(&diff);
        review.truncated |= truncated;
        remaining = remaining.saturating_sub(diff.len());
        remaining_bytes = remaining_bytes.saturating_sub(diff.iter().map(|line| line.len() + 1).sum::<usize>());
        review.added += added;
        review.removed += removed;
        review
            .files
            .push(GitChange { file: item.file, status: item.status, added, removed, diff });
    }

    Some(review)
}

fn tracked_diff(cwd: &Path, file: &str, max_lines: usize, max_bytes: usize) -> (Vec<String>, bool) {
    if max_lines == 0 || max_bytes == 0 {
        return (Vec::new(), true);
    }
    let Some((output, bytes_truncated)) = git_output_bounded(
        cwd,
        &["diff", "--no-ext-diff", "--no-color", "--unified=3", "HEAD", "--", file],
        max_bytes,
    ) else {
        return (Vec::new(), false);
    };
    let output = String::from_utf8_lossy(&output);
    let mut lines: Vec<_> = output.lines().take(max_lines + 1).map(ToString::to_string).collect();
    let line_truncated = lines.len() > max_lines;
    lines.truncate(max_lines);
    (lines, bytes_truncated || line_truncated)
}

fn untracked_diff(cwd: &Path, file: &str, max_lines: usize, max_bytes: usize) -> (Vec<String>, bool) {
    if max_lines == 0 || max_bytes == 0 {
        return (Vec::new(), true);
    }
    let path = cwd.join(file);
    let Ok(metadata) = fs::symlink_metadata(&path) else {
        return (vec![format!("Unreadable untracked file: {file}")], false);
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return (vec![format!("Unreviewed non-regular file: {file}")], false);
    }
    let Ok(open_file) = fs::File::open(path) else {
        return (vec![format!("Unreadable untracked file: {file}")], false);
    };
    let mut contents = Vec::new();
    let Ok(_) = open_file
        .take(max_bytes.saturating_add(1) as u64)
        .read_to_end(&mut contents)
    else {
        return (vec![format!("Unreadable untracked file: {file}")], false);
    };
    let bytes_truncated = contents.len() > max_bytes;
    contents.truncate(max_bytes);
    let contents = match String::from_utf8(contents) {
        Ok(contents) => contents,
        Err(error) if bytes_truncated && error.utf8_error().error_len().is_none() => {
            let valid = error.utf8_error().valid_up_to();
            String::from_utf8(error.into_bytes()[..valid].to_vec()).expect("validated UTF-8 prefix")
        }
        Err(_) => return (vec![format!("Binary or unreadable untracked file: {file}")], false),
    };
    let mut lines = vec!["--- /dev/null".to_string(), format!("+++ b/{file}")];
    let mut used_bytes = lines.iter().map(|line| line.len() + 1).sum::<usize>();
    let mut line_truncated = lines.len() > max_lines || used_bytes > max_bytes;
    lines.truncate(max_lines);
    if !line_truncated {
        for line in contents.lines() {
            let rendered = format!("+{line}");
            let rendered_bytes = rendered.len() + 1;
            if lines.len() == max_lines || used_bytes.saturating_add(rendered_bytes) > max_bytes {
                line_truncated = true;
                break;
            }
            used_bytes += rendered_bytes;
            lines.push(rendered);
        }
    }
    (lines, bytes_truncated || line_truncated)
}

fn count_diff_lines(lines: &[String]) -> (usize, usize) {
    let added = lines
        .iter()
        .filter(|line| line.starts_with('+') && !line.starts_with("+++"))
        .count();
    let removed = lines
        .iter()
        .filter(|line| line.starts_with('-') && !line.starts_with("---"))
        .count();
    (added, removed)
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

fn git_output_preserving_whitespace(cwd: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(git_config_args())
        .args(args)
        .current_dir(cwd)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).to_string())
}

fn git_output_bounded(cwd: &Path, args: &[&str], max_bytes: usize) -> Option<(Vec<u8>, bool)> {
    let mut child = Command::new("git")
        .args(git_config_args())
        .args(args)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut output = Vec::new();
    child
        .stdout
        .take()?
        .take(max_bytes.saturating_add(1) as u64)
        .read_to_end(&mut output)
        .ok()?;
    let truncated = output.len() > max_bytes;
    if truncated {
        let _ = child.kill();
        output.truncate(max_bytes);
    }
    let status = child.wait().ok()?;
    (truncated || status.success()).then_some((output, truncated))
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

    #[test]
    fn collect_review_reports_file_counts_and_unified_diff() {
        let dir = temp_git_repo();
        std::fs::write(dir.path().join("tracked.txt"), "clean\nchanged\n").expect("modify tracked file");
        std::fs::write(dir.path().join("new.txt"), "one\ntwo\n").expect("write untracked file");

        let review = collect_review(dir.path()).expect("git review");

        assert_eq!(review.files.len(), 2);
        assert_eq!(review.added, 3);
        assert_eq!(review.removed, 0);
        assert!(!review.truncated);
        let tracked = review
            .files
            .iter()
            .find(|change| change.file == "tracked.txt")
            .expect("tracked change");
        assert_eq!((tracked.added, tracked.removed), (1, 0));
        assert!(tracked.diff.iter().any(|line| line == "+changed"));
        let untracked = review
            .files
            .iter()
            .find(|change| change.file == "new.txt")
            .expect("untracked change");
        assert_eq!((untracked.added, untracked.removed), (2, 0));
        assert_eq!(untracked.diff[0], "--- /dev/null");
    }

    #[test]
    fn collect_review_truncates_large_untracked_files() {
        let dir = temp_git_repo();
        std::fs::write(dir.path().join("large.txt"), "line\n".repeat(MAX_REVIEW_LINES + 10))
            .expect("write large untracked file");

        let review = collect_review(dir.path()).expect("git review");

        assert!(review.truncated);
        assert!(review.files.iter().map(|change| change.diff.len()).sum::<usize>() <= MAX_REVIEW_LINES);
        assert!(
            review
                .files
                .iter()
                .flat_map(|change| &change.diff)
                .map(|line| line.len() + 1)
                .sum::<usize>()
                <= MAX_REVIEW_BYTES
        );
    }

    #[cfg(unix)]
    #[test]
    fn collect_review_does_not_follow_untracked_symlinks() {
        use std::os::unix::fs::symlink;

        let dir = temp_git_repo();
        let outside = tempfile::tempdir().expect("outside temp dir");
        let target = outside.path().join("outside-secret.txt");
        std::fs::write(&target, "must not appear in review\n").expect("write symlink target");
        symlink(&target, dir.path().join("linked.txt")).expect("create untracked symlink");

        let review = collect_review(dir.path()).expect("git review");
        let linked = review
            .files
            .iter()
            .find(|change| change.file == "linked.txt")
            .expect("symlink change");

        assert_eq!(linked.diff, vec!["Unreviewed non-regular file: linked.txt"]);
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
