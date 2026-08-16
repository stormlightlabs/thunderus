use std::{
    io,
    path::{Component, Path, PathBuf},
};

/// Resolve a process-accessible path, using `root` only as the base for relative paths.
pub fn resolve_from_root(root: &Path, path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() { path.to_path_buf() } else { root.join(path) }
}

/// Check whether `path` is within `root` after normalization.
///
/// The root is canonicalized to resolve symlinks (e.g. macOS `/var` → `/private/var`).
///
/// The candidate path is made absolute against the canonical root (if relative),
/// normalized lexically, and resolved through its nearest existing ancestor.
///
///  `.` and `..` are resolved without touching the filesystem so non-existent paths
/// (e.g. files about to be created) are handled correctly.
pub fn is_within_root(path: &Path, root: &Path) -> bool {
    let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let absolute = if path.is_absolute() { path.to_path_buf() } else { canonical_root.join(path) };
    let normalized = lexical_normalize(&absolute);
    let Some(existing) = nearest_existing_ancestor(&normalized) else {
        return normalized.starts_with(&canonical_root);
    };
    existing
        .canonicalize()
        .is_ok_and(|canonical_existing| canonical_existing.starts_with(&canonical_root))
}

/// Normalize a relative path against a root, then verify containment.
///
/// Returns the normalized absolute path if it is within root, or an error
/// otherwise. The root is canonicalized; the candidate is normalized lexically
/// so non-existent paths work correctly.
pub fn resolve_within_root(root: &Path, relative: &str) -> io::Result<PathBuf> {
    let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let candidate = if Path::new(relative).is_absolute() {
        PathBuf::from(relative)
    } else {
        canonical_root.join(relative)
    };
    let normalized = lexical_normalize(&candidate);

    if let Some(existing) = nearest_existing_ancestor(&normalized) {
        let canonical_existing = existing.canonicalize()?;
        if !canonical_existing.starts_with(&canonical_root) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("path escapes workspace root through symlink: {relative}"),
            ));
        }
        let suffix = normalized.strip_prefix(&existing).unwrap_or_else(|_| Path::new(""));
        if suffix.as_os_str().is_empty() {
            return Ok(canonical_existing);
        }
        return Ok(canonical_existing.join(suffix));
    }

    if !normalized.starts_with(&canonical_root) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("path escapes workspace root: {relative}"),
        ));
    }

    Ok(normalized)
}

/// Lexically normalize a path by resolving `.` and `..` components without
/// touching the filesystem.
///
/// This handles non-existent paths (e.g. files about to be created) and paths
/// under symlinked roots consistently. It does not resolve symlinks in the
/// path itself — that is acceptable for alpha containment checks.
fn lexical_normalize(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                result.pop();
            }
            Component::CurDir => (),
            other => result.push(other.as_os_str()),
        }
    }
    result
}

fn nearest_existing_ancestor(path: &Path) -> Option<PathBuf> {
    let mut current = path.to_path_buf();
    loop {
        if current.exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_within_root_same_path() {
        let root = std::env::current_dir().unwrap();
        assert!(is_within_root(&root, &root));
    }

    #[test]
    fn is_within_root_subpath() {
        let root = std::env::current_dir().unwrap();
        let sub = root.join("src");
        assert!(is_within_root(&sub, &root));
    }

    #[test]
    fn is_within_root_escape_detected() {
        let root = std::env::current_dir().unwrap();
        let outside = root.parent().unwrap().to_path_buf();
        assert!(!is_within_root(&outside, &root));
    }

    #[test]
    fn resolve_within_root_relative_path() {
        let root = std::env::current_dir().unwrap();
        let result = resolve_within_root(&root, "src/main.rs");
        assert!(result.is_ok());
        assert!(result.unwrap().starts_with(&root));
    }

    #[test]
    fn resolve_within_root_absolute_escape() {
        let root = std::env::current_dir().unwrap();
        let outside = root.parent().unwrap();
        let result = resolve_within_root(&root, outside.to_str().unwrap());
        assert!(result.is_err());
    }

    #[cfg(unix)]
    #[test]
    fn resolve_within_root_rejects_symlink_escape_parent() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path().join("root");
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&root).expect("root");
        std::fs::create_dir_all(&outside).expect("outside");
        std::os::unix::fs::symlink(&outside, root.join("link")).expect("symlink");

        let result = resolve_within_root(&root, "link/new.txt");

        assert!(result.is_err());
    }
}
