//! Application-owned context discovery and instruction adapters.
//!
//! Pure context policy lives in [`thndrs_agent::context`]; this module performs
//! filesystem discovery and supplies the resulting application data.

pub mod export;
pub mod instructions;

pub use instructions::{
    InstructionDiagnostic, InstructionInventory, InstructionSelection, InstructionSeverity, InstructionSnapshot,
    discover_instructions, select_instructions,
};

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

fn hash_content(content: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

pub fn scope_depth(scope: &str) -> usize {
    if scope == "." || scope.is_empty() { 0 } else { scope.matches('/').count() + 1 }
}

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
            true => format!("loaded {path_display} (truncated, {} bytes)", self.byte_count),
            false => format!("loaded {path_display}"),
        }
    }
}

/// Discover the workspace root from `cwd`. Prefers the git top-level directory
/// when available; falls back to the canonicalized `cwd`.
///
/// Uses `git rev-parse --show-toplevel` with [`std::process::Command`] (argv
/// array, never shell strings).
///
/// If git is unavailable or `cwd` is not inside a repo, returns a
/// fully-qualified `cwd` when the path can be canonicalized.
pub fn discover_workspace_root(cwd: &Path) -> PathBuf {
    match Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(cwd)
        .output()
    {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let root = stdout.trim();
            let root = if root.is_empty() { cwd.to_path_buf() } else { PathBuf::from(root) };
            root.canonicalize().unwrap_or(root)
        }
        _ => cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf()),
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
    let content_hash = hash_content(&content);

    let (content, truncated) = if byte_count > AGENTS_MD_SIZE_CAP {
        let mut capped = content.into_bytes();
        capped.truncate(AGENTS_MD_SIZE_CAP);
        (
            trim_to_char_boundary(&String::from_utf8_lossy(&capped), AGENTS_MD_SIZE_CAP),
            true,
        )
    } else {
        (content, false)
    };

    Some(ContextSource { path, scope: String::from("."), content, content_hash, truncated, byte_count })
}

/// Trim a string to at most `max_bytes` bytes, ensuring we end on a UTF-8 char boundary.
pub fn trim_to_char_boundary(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}
