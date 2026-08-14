//! Search backend selection and bounded filesystem fallback support.

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

const MAX_FALLBACK_FILES: usize = 10_000;

/// Executables available to repository search tools.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SearchPrograms {
    fd: Option<PathBuf>,
    rg: Option<PathBuf>,
}

impl SearchPrograms {
    /// Discover search programs from the process path.
    pub fn discover() -> Self {
        let paths = env::split_paths(&env::var_os("PATH").unwrap_or_default()).collect::<Vec<_>>();
        Self::discover_in(&paths)
    }

    /// Discover search programs in an explicit path list.
    pub fn discover_in(paths: &[PathBuf]) -> Self {
        Self { fd: find_program(paths, "fd"), rg: find_program(paths, "rg") }
    }

    pub fn fd(&self) -> Option<&Path> {
        self.fd.as_deref()
    }

    pub fn rg(&self) -> Option<&Path> {
        self.rg.as_deref()
    }

    pub fn file_discovery_label(&self) -> &'static str {
        if self.fd.is_some() {
            "fd"
        } else if self.rg.is_some() {
            "rg --files (degraded)"
        } else {
            "in-process fallback (degraded)"
        }
    }

    pub fn content_search_label(&self) -> &'static str {
        if self.rg.is_some() { "rg --json" } else { "in-process fallback (degraded)" }
    }

    pub fn is_degraded(&self) -> bool {
        self.fd.is_none() || self.rg.is_none()
    }
}

/// Prepend stable implementation metadata to a bounded tool projection.
pub fn with_implementation_line(label: &str, mut lines: Vec<String>) -> Vec<String> {
    lines.insert(0, format!("[implementation: {label}]"));
    lines
}

/// Enumerate regular files without following symlinks.
///
/// Traversal is deterministic, honors repository ignore rules, and stops after
/// a fixed file cap. Canonicalizing the root before walking and refusing to
/// follow symlinks keeps every returned path contained to that root.
pub fn fallback_files(root: &Path, include_hidden: bool, max_depth: Option<u32>) -> io::Result<Vec<PathBuf>> {
    fallback_paths(root, include_hidden, max_depth, false)
}

/// Enumerate workspace files and, when requested, directories without following symlinks.
pub fn fallback_paths(
    root: &Path, include_hidden: bool, max_depth: Option<u32>, include_directories: bool,
) -> io::Result<Vec<PathBuf>> {
    let canonical_root = fs::canonicalize(root)?;
    let metadata = fs::symlink_metadata(&canonical_root)?;
    if metadata.is_file() {
        return Ok(vec![canonical_root]);
    }
    if !metadata.is_dir() {
        return Ok(Vec::new());
    }

    let mut builder = WalkBuilder::new(&canonical_root);
    builder
        .hidden(!include_hidden)
        .follow_links(false)
        .require_git(false)
        .sort_by_file_path(|left, right| left.cmp(right));
    if let Some(max_depth) = max_depth {
        builder.max_depth(Some(max_depth as usize + 1));
    }

    let mut paths = Vec::new();
    for entry in builder.build() {
        let entry = entry.map_err(|error| io::Error::other(error.to_string()))?;
        let is_file = entry.file_type().is_some_and(|file_type| file_type.is_file());
        let is_directory = entry.depth() > 0 && entry.file_type().is_some_and(|file_type| file_type.is_dir());
        if is_file || (include_directories && is_directory) {
            paths.push(entry.into_path());
            if paths.len() >= MAX_FALLBACK_FILES {
                break;
            }
        }
    }
    Ok(paths)
}

/// Render a fallback path using the same relative/absolute shape as the root.
pub fn display_path(root: &Path, file: &Path) -> String {
    if root.is_file() {
        return root.display().to_string();
    }
    let canonical_root = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let relative = file.strip_prefix(&canonical_root).unwrap_or(file);
    root.join(relative).display().to_string()
}

/// Match the small glob subset accepted by repository discovery fallbacks.
pub fn matches_glob(path: &str, glob: &str) -> bool {
    if !glob.contains('*') {
        return path.contains(glob);
    }

    let parts = glob.split('*').collect::<Vec<_>>();
    let mut position = 0;
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if index == 0 && !path.starts_with(part) {
            return false;
        }
        let Some(found) = path[position..].find(part) else {
            return false;
        };
        position += found + part.len();
    }
    parts.last().is_none_or(|last| last.is_empty() || path.ends_with(last))
}

fn find_program(paths: &[PathBuf], name: &str) -> Option<PathBuf> {
    paths
        .iter()
        .map(|path| path.join(name))
        .find(|candidate| is_executable(candidate))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    fs::metadata(path).is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_paths_select_native_and_degraded_backends() {
        let dir = tempfile::tempdir().expect("temp dir");
        fs::write(dir.path().join("fd"), "").expect("fd fixture");
        fs::write(dir.path().join("rg"), "").expect("rg fixture");
        make_executable(&dir.path().join("fd"));
        make_executable(&dir.path().join("rg"));

        let native = SearchPrograms::discover_in(&[dir.path().to_path_buf()]);
        assert_eq!(native.file_discovery_label(), "fd");
        assert_eq!(native.content_search_label(), "rg --json");
        assert!(!native.is_degraded());

        let fallback = SearchPrograms::discover_in(&[]);
        assert_eq!(fallback.file_discovery_label(), "in-process fallback (degraded)");
        assert_eq!(fallback.content_search_label(), "in-process fallback (degraded)");
        assert!(fallback.is_degraded());
    }

    #[cfg(unix)]
    fn make_executable(path: &Path) {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(path).expect("fixture metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).expect("executable fixture");
    }

    #[cfg(not(unix))]
    fn make_executable(_path: &Path) {}

    #[test]
    fn fallback_stays_contained_and_honors_ignore_rules() {
        let dir = tempfile::tempdir().expect("temp dir");
        fs::create_dir_all(dir.path().join("src")).expect("src");
        fs::create_dir_all(dir.path().join("vendor/pkg")).expect("vendor");
        fs::write(dir.path().join(".gitignore"), "vendor/\n").expect("ignore rules");
        fs::write(dir.path().join("src/lib.rs"), "safe").expect("source");
        fs::write(dir.path().join("vendor/pkg/lib.rs"), "generated").expect("vendor source");

        #[cfg(unix)]
        std::os::unix::fs::symlink("/etc", dir.path().join("src/outside")).expect("symlink");

        let files = fallback_files(dir.path(), false, None).expect("walk");
        let canonical_root = fs::canonicalize(dir.path()).expect("canonical root");
        assert_eq!(files.len(), 1);
        assert!(files.iter().all(|path| path.starts_with(&canonical_root)));
        assert!(files[0].ends_with("src/lib.rs"));
    }
}
