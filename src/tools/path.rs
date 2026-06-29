use std::{
    io,
    path::{Path, PathBuf},
};

/// Check whether `path` is within `root` after normalization/canonicalization.
///
/// Both paths are canonicalized if possible. If canonicalization fails (e.g.
/// the path doesn't exist yet), the raw path is checked with `starts_with`.
pub fn is_within_root(path: &Path, root: &Path) -> bool {
    let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let canonical_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    canonical_path.starts_with(&canonical_root)
}

/// Normalize a relative path against a root, then verify containment.
///
/// Returns the absolute path if it is within root, or an error otherwise.
///
/// TODO: tool dispatch
#[allow(dead_code)]
pub fn resolve_within_root(root: &Path, relative: &str) -> io::Result<PathBuf> {
    let candidate = if Path::new(relative).is_absolute() { PathBuf::from(relative) } else { root.join(relative) };
    if !is_within_root(&candidate, root) {
        let kind = io::ErrorKind::PermissionDenied;
        Err(io::Error::new(kind, format!("path escapes workspace root: {relative}")))
    } else {
        Ok(candidate)
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
}
