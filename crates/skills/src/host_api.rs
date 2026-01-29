//! Host API for plugin runtimes (WASM, Lua, MCP).
//!
//! This module provides the permission-checked host functions that plugins
//! can call to interact with the system. Currently a stub for future integration
//! with Extism (WASM) and mlua (Lua) runtimes.

use crate::types::SkillPermissions;

use std::path::{Path, PathBuf};
use thiserror::Error;

/// Errors that can occur during permission checks or host API operations.
#[derive(Debug, Error)]
pub enum HostApiError {
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Path traversal attempt: {0}")]
    PathTraversal(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Path not allowed: {0}")]
    PathNotAllowed(String),

    #[error("Host not allowed: {0}")]
    HostNotAllowed(String),

    #[error("Environment variable not allowed: {0}")]
    EnvVarNotAllowed(String),

    #[error("Operation failed: {0}")]
    OperationFailed(String),
}

/// Result type for host API operations.
pub type Result<T> = std::result::Result<T, HostApiError>;

/// Context for host function execution.
///
/// This context is passed to host functions to provide them with the
/// necessary information to perform permission checks and execute operations.
#[derive(Clone)]
pub struct HostContext {
    /// Plugin name (for logging and error messages)
    pub plugin_name: String,

    /// Workspace root directory (for resolving relative paths)
    pub workspace_root: PathBuf,

    /// Permissions granted to this plugin
    pub permissions: SkillPermissions,

    /// Plugin-scoped key-value store (for persistence)
    pub kv_store: KvStore,
}

/// Simple key-value store for plugin-scoped persistence.
#[derive(Debug, Clone, Default)]
pub struct KvStore {
    // TODO: Implement persistent storage
    data: std::collections::HashMap<String, String>,
}

impl KvStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, key: &str) -> Option<String> {
        self.data.get(key).cloned()
    }

    pub fn set(&mut self, key: String, value: String) {
        self.data.insert(key, value);
    }

    pub fn delete(&mut self, key: &str) {
        self.data.remove(key);
    }
}

impl HostContext {
    /// Create a new host context.
    pub fn new(plugin_name: String, workspace_root: PathBuf, permissions: SkillPermissions) -> Self {
        Self { plugin_name, workspace_root, permissions, kv_store: KvStore::new() }
    }

    /// Get the canonicalized workspace root for comparison.
    /// This is cached to avoid repeated canonicalization.
    fn canonical_workspace_root(&self) -> Option<PathBuf> {
        self.workspace_root.canonicalize().ok()
    }

    /// Check if a path is allowed for read access.
    ///
    /// Returns the canonicalized path if allowed, or an error if not.
    pub fn check_read_permission(&self, path: &str) -> Result<PathBuf> {
        let resolved = self.resolve_path(path)?;
        if !self.matches_patterns(&resolved, &self.permissions.filesystem.read) {
            return Err(HostApiError::PermissionDenied(format!(
                "read: {} (not in allowed paths for plugin '{}')",
                path, self.plugin_name
            )));
        }
        Ok(resolved)
    }

    /// Check if a path is allowed for write access.
    ///
    /// Returns the canonicalized path if allowed, or an error if not.
    pub fn check_write_permission(&self, path: &str) -> Result<PathBuf> {
        let resolved = self.resolve_path(path)?;
        if !self.matches_patterns(&resolved, &self.permissions.filesystem.write) {
            return Err(HostApiError::PermissionDenied(format!(
                "write: {} (not in allowed paths for plugin '{}')",
                path, self.plugin_name
            )));
        }
        Ok(resolved)
    }

    /// Check if a host is allowed for network access.
    pub fn check_network_permission(&self, host: &str) -> Result<()> {
        if self.matches_host(host, &self.permissions.network.allowed_hosts) {
            Ok(())
        } else {
            Err(HostApiError::HostNotAllowed(format!(
                "{} (not in allowed hosts for plugin '{}')",
                host, self.plugin_name
            )))
        }
    }

    /// Check if an environment variable is allowed to be read.
    pub fn check_env_var_permission(&self, var: &str) -> Result<()> {
        if self.permissions.env_vars.contains(&var.to_string()) {
            Ok(())
        } else {
            Err(HostApiError::EnvVarNotAllowed(format!(
                "{} (not in allowed env vars for plugin '{}')",
                var, self.plugin_name
            )))
        }
    }

    /// Resolve a path relative to the workspace root.
    ///
    /// Returns the canonicalized path if valid, or an error if the path
    /// attempts to traverse outside the workspace.
    fn resolve_path(&self, path: &str) -> Result<PathBuf> {
        let path = PathBuf::from(path);
        let resolved = if path.is_absolute() { path.clone() } else { self.workspace_root.join(path.clone()) };
        let canonical = resolved
            .canonicalize()
            .map_err(|e| HostApiError::Io(std::io::Error::new(e.kind(), format!("Failed to resolve path: {e}"))))?;

        let workspace_canonical = self
            .canonical_workspace_root()
            .ok_or_else(|| HostApiError::PathTraversal("Failed to canonicalize workspace root".to_string()))?;

        if !canonical.starts_with(&workspace_canonical) {
            return Err(HostApiError::PathTraversal(format!(
                "path '{}' escapes workspace root",
                path.display()
            )));
        }

        Ok(canonical)
    }

    /// Check if a path matches any of the allowed glob patterns.
    fn matches_patterns(&self, path: &Path, patterns: &[String]) -> bool {
        if patterns.is_empty() {
            return false;
        }

        let path_str = if let Some(workspace_canonical) = self.canonical_workspace_root() {
            path.strip_prefix(&workspace_canonical)
                .or_else(|_| path.strip_prefix(&self.workspace_root))
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| path.to_string_lossy().to_string())
        } else {
            path.strip_prefix(&self.workspace_root)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| path.to_string_lossy().to_string())
        };

        let path_normalized = path_str.replace('\\', "/");

        for pattern in patterns {
            if self.match_pattern(&path_normalized, pattern) {
                return true;
            }
        }

        false
    }

    /// Simple glob pattern matching.
    ///
    /// Supports:
    /// - `*` matches any sequence of characters (except `/`)
    /// - `**` matches any sequence including `/`
    /// - `?` matches any single character
    fn match_pattern(&self, text: &str, pattern: &str) -> bool {
        let text_normalized = text.strip_prefix("./").unwrap_or(text);
        let pattern_normalized = pattern.strip_prefix("./").unwrap_or(pattern);

        let placeholder = "\x00DOUBLESTAR\x00";
        let regex_pattern = pattern_normalized
            .replace('.', r"\.")
            .replace("**", placeholder)
            .replace('*', "[^/]*")
            .replace(placeholder, ".*")
            .replace('?', ".");

        match regex::Regex::new(&format!("^{}$", regex_pattern)) {
            Ok(re) => re.is_match(text_normalized),
            Err(_) => pattern_normalized == text_normalized,
        }
    }

    /// Check if a host matches any of the allowed host patterns.
    fn matches_host(&self, host: &str, patterns: &[String]) -> bool {
        if patterns.is_empty() {
            return false;
        }

        for pattern in patterns {
            if self.match_host(host, pattern) {
                return true;
            }
        }

        false
    }

    /// Simple host pattern matching with wildcard support.
    fn match_host(&self, host: &str, pattern: &str) -> bool {
        if pattern == "*" {
            return true;
        }

        if let Some(domain) = pattern.strip_prefix("*.") {
            return host.ends_with(&format!(".{domain}")) || host == domain;
        }

        host == pattern
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_context() -> HostContext {
        let mut permissions = SkillPermissions::default();
        permissions.filesystem.read = vec!["./data/**".to_string(), "./README.md".to_string()];
        permissions.filesystem.write = vec!["./output/**".to_string()];
        permissions.network.allowed_hosts = vec!["api.example.com".to_string(), "*.github.com".to_string()];
        permissions.env_vars = vec!["API_KEY".to_string()];

        HostContext::new("test-plugin".to_string(), PathBuf::from("/workspace"), permissions)
    }

    #[test]
    fn test_check_read_permission_allowed() {
        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path();

        fs::create_dir_all(workspace.join("data")).unwrap();
        fs::write(workspace.join("data").join("file.txt"), "test").unwrap();

        let mut permissions = SkillPermissions::default();
        permissions.filesystem.read = vec!["data/**".to_string()];

        let ctx = HostContext::new("test-plugin".to_string(), workspace.to_path_buf(), permissions);
        assert!(ctx.check_read_permission("data/file.txt").is_ok());
    }

    #[test]
    fn test_check_read_permission_denied() {
        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path();

        fs::write(workspace.join("secret.txt"), "test").unwrap();

        let mut permissions = SkillPermissions::default();
        permissions.filesystem.read = vec!["data/**".to_string()];

        let ctx = HostContext::new("test-plugin".to_string(), workspace.to_path_buf(), permissions);

        assert!(ctx.check_read_permission("secret.txt").is_err());
    }

    #[test]
    fn test_check_network_permission_allowed() {
        let ctx = create_test_context();
        assert!(ctx.check_network_permission("api.example.com").is_ok());
    }

    #[test]
    fn test_check_network_permission_denied() {
        let ctx = create_test_context();
        assert!(ctx.check_network_permission("evil.com").is_err());
    }

    #[test]
    fn test_host_wildcard_matching() {
        let ctx = create_test_context();
        assert!(ctx.check_network_permission("api.github.com").is_ok());
        assert!(ctx.check_network_permission("github.com").is_ok());
    }

    #[test]
    fn test_env_var_permission() {
        let ctx = create_test_context();
        assert!(ctx.check_env_var_permission("API_KEY").is_ok());
        assert!(ctx.check_env_var_permission("SECRET_KEY").is_err());
    }

    #[test]
    fn test_kv_store() {
        let mut store = KvStore::new();
        assert!(store.get("test").is_none());

        store.set("test".to_string(), "value".to_string());
        assert_eq!(store.get("test"), Some("value".to_string()));

        store.delete("test");
        assert!(store.get("test").is_none());
    }

    #[test]
    fn test_pattern_matching() {
        let ctx = create_test_context();
        assert!(ctx.match_pattern("data/file.txt", "./data/**"));
        assert!(ctx.match_pattern("data/sub/file.txt", "./data/**"));
        assert!(!ctx.match_pattern("other/file.txt", "./data/**"));

        assert!(ctx.match_pattern("README.md", "./README.md"));

        assert!(ctx.match_pattern("file.txt", "*.txt"));
        assert!(!ctx.match_pattern("sub/file.txt", "*.txt"));

        assert!(ctx.match_pattern("data/file.txt", "data/**"));
        assert!(ctx.match_pattern("./data/file.txt", "./data/**"));
    }

    #[test]
    fn test_path_traversal_protection() {
        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path();

        fs::create_dir_all(workspace.join("data")).unwrap();

        let mut permissions = SkillPermissions::default();
        permissions.filesystem.read = vec!["data/**".to_string()];

        let ctx = HostContext::new("test-plugin".to_string(), workspace.to_path_buf(), permissions);
        assert!(ctx.check_read_permission("data/../../etc/passwd").is_err());
    }
}
