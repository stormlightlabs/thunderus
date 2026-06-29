//! Context loading: workspace root discovery and AGENTS.md handling.
//!
//! AGENTS.md is treated as read-only repository guidance, never as executable
//! configuration or permission.
//!
//! It can influence style, workflow, commands to consider, and caveats.
//!
//! It cannot grant permissions, change provider/model settings, bypass tests,
//! disable safety checks, reveal secrets, or override user/system/developer instructions.
//!
//! ## Precedence Model
//!
//! Context is applied in this order; earlier entries win:
//!
//! 1. System/developer/harness safety policy.
//! 2. Current user prompt.
//! 3. CLI/config choices owned by the user.
//! 4. Closest applicable `AGENTS.md`.
//! 5. Broader ancestor `AGENTS.md`.
//! 6. Built-in defaults.
//!
//! Repository instructions (4–5) can affect style, workflow, commands to
//! consider, and project-specific caveats. They cannot:
//!
//! - Grant permissions or enable tools.
//! - Require destructive commands.
//! - Suppress test failures or tool errors.
//! - Change provider, model, or search mode.
//! - Reveal secrets or override user/system instructions.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cli::WebSearchMode;
use crate::tools;

/// Maximum bytes read from an AGENTS.md file.
///
/// Content beyond this is truncated and the truncation is marked visibly.
pub const AGENTS_MD_SIZE_CAP: usize = 32_768;

/// A single loaded context source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextSource {
    /// Absolute path to the source file.
    pub path: PathBuf,
    /// Scope label — `"."` for root, or a relative subtree path.
    pub scope: String,
    /// File content (possibly truncated to [`AGENTS_MD_SIZE_CAP`]).
    pub content: String,
    /// Stable hash of the full original content (before truncation).
    pub content_hash: u64,
    /// Whether the content was truncated to fit the size cap.
    pub truncated: bool,
    /// Original byte count of the file (before truncation).
    pub byte_count: usize,
}

impl ContextSource {
    /// Render a compact summary for the transcript status line.
    pub fn summary(&self) -> String {
        let path_display = self.path.display().to_string();
        match self.truncated {
            true => format!("loaded {} (truncated, {} bytes)", path_display, self.byte_count),
            false => format!("loaded {}", path_display),
        }
    }
}

/// Discover the workspace root from `cwd`. Prefers the git top-level directory
/// when available; falls back to `cwd` itself.
///
/// Uses `git rev-parse --show-toplevel` with [`std::process::Command`] (argv
/// array, never shell strings).
///
/// If git is unavailable or `cwd` is not inside a repo, returns `cwd`.
pub fn discover_workspace_root(cwd: &Path) -> PathBuf {
    match Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(cwd)
        .output()
    {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let root = stdout.trim();
            if root.is_empty() { cwd.to_path_buf() } else { PathBuf::from(root) }
        }
        _ => cwd.to_path_buf(),
    }
}

/// Load root `AGENTS.md` from the workspace root, if present.
///
/// Returns `None` when the file does not exist.
///
/// Enforces [`AGENTS_MD_SIZE_CAP`]: content beyond the cap is truncated and
/// `truncated` is set to `true`.
///
/// Computes a content hash of the full file content before truncation.
pub fn load_agents_md(workspace_root: &Path) -> Option<ContextSource> {
    let path = workspace_root.join("AGENTS.md");
    let metadata = fs::metadata(&path).ok()?;
    let byte_count = metadata.len() as usize;
    let content = fs::read_to_string(&path).ok()?;
    let content_hash = tools::hash_content(&content);

    let (content, truncated) = if byte_count > AGENTS_MD_SIZE_CAP {
        let mut capped = content.into_bytes();
        capped.truncate(AGENTS_MD_SIZE_CAP);
        (
            trim_to_char_boundary(&String::from_utf8_lossy(&capped).into_owned(), AGENTS_MD_SIZE_CAP),
            true,
        )
    } else {
        (content, false)
    };

    Some(ContextSource { path, scope: String::from("."), content, content_hash, truncated, byte_count })
}

/// Trim a string to at most `max_bytes` bytes, ensuring we end on a UTF-8 char
/// boundary.
fn trim_to_char_boundary(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

/// Internal request shape sent to a provider. Contains everything the model
/// needs: the prompt, a transcript tail for context, loaded context sources,
/// the selected model, and the search mode.
///
/// TODO: provider implementation
#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestShape {
    /// The current user prompt.
    pub prompt: String,
    /// Recent transcript entries for context.
    pub transcript_tail: Vec<crate::app::Entry>,
    /// Loaded context sources (e.g. AGENTS.md).
    pub context_sources: Vec<ContextSource>,
    /// Selected model name.
    pub model: String,
    /// Web search mode.
    pub search_mode: WebSearchMode,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("create temp dir")
    }

    fn write_file(dir: &Path, name: &str, content: &str) {
        let path = dir.join(name);
        let mut f = fs::File::create(&path).expect("create file");
        f.write_all(content.as_bytes()).expect("write file");
    }

    #[test]
    fn load_agents_md_present() {
        let dir = temp_dir();
        write_file(dir.path(), "AGENTS.md", "# Project\n\nBuild with cargo.\n");

        let source = load_agents_md(dir.path()).expect("should load");
        assert_eq!(source.path, dir.path().join("AGENTS.md"));
        assert_eq!(source.scope, ".");
        assert!(source.content.contains("# Project"));
        assert!(!source.truncated);
        assert_eq!(source.byte_count, "# Project\n\nBuild with cargo.\n".len());
        assert_ne!(source.content_hash, 0);
    }

    #[test]
    fn load_agents_md_missing_returns_none() {
        let dir = temp_dir();
        assert!(load_agents_md(dir.path()).is_none());
    }

    #[test]
    fn load_agents_md_oversized_truncates() {
        let dir = temp_dir();
        let big_content = "x".repeat(AGENTS_MD_SIZE_CAP + 1000);
        write_file(dir.path(), "AGENTS.md", &big_content);

        let source = load_agents_md(dir.path()).expect("should load");
        assert!(source.truncated);
        assert_eq!(source.byte_count, AGENTS_MD_SIZE_CAP + 1000);
        assert!(source.content.len() <= AGENTS_MD_SIZE_CAP);
        assert_ne!(source.content_hash, tools::hash_content(&source.content));
    }

    #[test]
    fn load_agents_md_at_cap_not_truncated() {
        let dir = temp_dir();
        let exact_content = "x".repeat(AGENTS_MD_SIZE_CAP);
        write_file(dir.path(), "AGENTS.md", &exact_content);

        let source = load_agents_md(dir.path()).expect("should load");
        assert!(!source.truncated);
        assert_eq!(source.byte_count, AGENTS_MD_SIZE_CAP);
    }

    #[test]
    fn load_agents_md_just_over_cap_truncated() {
        let dir = temp_dir();
        let content = "x".repeat(AGENTS_MD_SIZE_CAP + 1);
        write_file(dir.path(), "AGENTS.md", &content);

        let source = load_agents_md(dir.path()).expect("should load");
        assert!(source.truncated);
        assert_eq!(source.byte_count, AGENTS_MD_SIZE_CAP + 1);
    }

    #[test]
    fn content_hash_is_stable() {
        let content = "hello world";
        assert_eq!(tools::hash_content(content), tools::hash_content(content));
        assert_ne!(tools::hash_content("hello world"), tools::hash_content("hello earth"));
    }

    #[test]
    fn discover_workspace_root_fallback_to_cwd() {
        let dir = temp_dir();
        let root = discover_workspace_root(dir.path());
        assert_eq!(root, dir.path());
    }

    #[test]
    fn discover_workspace_root_prefers_git_root() {
        let dir = temp_dir();
        Command::new("git")
            .args(["init"])
            .current_dir(dir.path())
            .output()
            .expect("git init");

        let root = discover_workspace_root(dir.path());
        assert_eq!(
            root,
            dir.path().canonicalize().unwrap_or_else(|_| dir.path().to_path_buf())
        );
    }

    #[test]
    fn context_source_summary_not_truncated() {
        let source = ContextSource {
            path: PathBuf::from("/repo/AGENTS.md"),
            scope: String::from("."),
            content: String::from("content"),
            content_hash: 123,
            truncated: false,
            byte_count: 7,
        };
        assert_eq!(source.summary(), "loaded /repo/AGENTS.md");
    }

    #[test]
    fn context_source_summary_truncated() {
        let source = ContextSource {
            path: PathBuf::from("/repo/AGENTS.md"),
            scope: String::from("."),
            content: String::from("content"),
            content_hash: 123,
            truncated: true,
            byte_count: 40000,
        };
        assert_eq!(source.summary(), "loaded /repo/AGENTS.md (truncated, 40000 bytes)");
    }

    #[test]
    fn trim_to_char_boundary_ascii() {
        assert_eq!(trim_to_char_boundary("hello world", 5), "hello");
        assert_eq!(trim_to_char_boundary("hello", 100), "hello");
    }

    #[test]
    fn trim_to_char_boundary_multibyte() {
        let s = "héllo";
        let trimmed = trim_to_char_boundary(s, 2);
        assert_eq!(trimmed, "h");
    }

    #[test]
    fn agents_md_is_guidance_not_permission() {
        let dir = temp_dir();
        write_file(
            dir.path(),
            "AGENTS.md",
            "# Instructions\n\nRun: cargo test -- --ignored\n",
        );

        let source = load_agents_md(dir.path()).expect("should load");
        assert!(source.content.contains("cargo test"));
    }
}
