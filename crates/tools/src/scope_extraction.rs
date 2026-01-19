//! Scope extraction from tool arguments for the "SCOPE" field
//!
//! This module analyzes tool arguments to extract information about what files,
//! paths, or resources will be affected by a tool call. This information is
//! displayed in action cards to help users understand the blast radius of operations.

use serde_json::Value;
use std::collections::HashSet;

/// Extracted scope information from tool arguments
#[derive(Debug, Clone, PartialEq)]
pub struct ScopeInfo {
    /// Files that will be affected
    pub files: Vec<String>,
    /// Directories that will be affected
    pub directories: Vec<String>,
    /// Patterns used for matching (e.g., glob patterns, regex patterns)
    pub patterns: Vec<String>,
    /// Whether this affects the entire project
    pub is_project_wide: bool,
    /// Whether this affects external systems (network, APIs, etc.)
    pub is_external: bool,
}

impl ScopeInfo {
    /// Create empty scope info
    pub fn new() -> Self {
        Self {
            files: Vec::new(),
            directories: Vec::new(),
            patterns: Vec::new(),
            is_project_wide: false,
            is_external: false,
        }
    }

    /// Check if this scope affects any resources
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
            && self.directories.is_empty()
            && self.patterns.is_empty()
            && !self.is_project_wide
            && !self.is_external
    }

    /// Get a brief description of the scope
    pub fn to_brief(&self) -> String {
        if self.is_external {
            return "External system".to_string();
        }

        if self.is_project_wide {
            return "Project-wide".to_string();
        }

        let mut parts = Vec::new();

        if !self.files.is_empty() {
            let count = self.files.len();
            parts.push(if count == 1 { "1 file".to_string() } else { format!("{} files", count) });
        }

        if !self.directories.is_empty() {
            let count = self.directories.len();
            parts.push(if count == 1 { "1 directory".to_string() } else { format!("{} directories", count) });
        }

        if !self.patterns.is_empty() {
            let count = self.patterns.len();
            parts.push(if count == 1 { "1 pattern".to_string() } else { format!("{} patterns", count) });
        }

        if parts.is_empty() { "No specific scope".to_string() } else { parts.join(", ") }
    }

    /// Get a detailed description of the scope with file lists
    pub fn to_detailed(&self) -> String {
        if self.is_external {
            return "External system call (network/API)".to_string();
        }

        if self.is_project_wide {
            return "Affects entire project (multiple files/directories)".to_string();
        }

        let mut lines = Vec::new();

        if !self.files.is_empty() {
            lines.push("Files:".to_string());
            for file in &self.files {
                lines.push(format!("  - {}", file));
            }
        }

        if !self.directories.is_empty() {
            lines.push("Directories:".to_string());
            for dir in &self.directories {
                lines.push(format!("  - {}", dir));
            }
        }

        if !self.patterns.is_empty() {
            lines.push("Patterns:".to_string());
            for pattern in &self.patterns {
                lines.push(format!("  - {}", pattern));
            }
        }

        if lines.is_empty() { "No specific scope".to_string() } else { lines.join("\n") }
    }

    /// Add a file to the scope
    pub fn add_file(mut self, file: impl Into<String>) -> Self {
        self.files.push(file.into());
        self
    }

    /// Add multiple files to the scope
    pub fn add_files(mut self, files: Vec<String>) -> Self {
        self.files.extend(files);
        self
    }

    /// Add a directory to the scope
    pub fn add_directory(mut self, dir: impl Into<String>) -> Self {
        self.directories.push(dir.into());
        self
    }

    /// Add a pattern to the scope
    pub fn add_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.patterns.push(pattern.into());
        self
    }

    /// Mark as project-wide
    pub fn with_project_wide(mut self) -> Self {
        self.is_project_wide = true;
        self
    }

    /// Mark as external
    pub fn with_external(mut self) -> Self {
        self.is_external = true;
        self
    }
}

impl Default for ScopeInfo {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract scope information from tool arguments
pub fn extract_scope(tool_name: &str, arguments: &Value) -> ScopeInfo {
    match tool_name {
        t if t.contains("read") => extract_read_scope(arguments),
        t if t.contains("write") || t.contains("edit") => extract_write_scope(arguments),
        t if t.contains("multiedit") => extract_multiedit_scope(arguments),
        t if t.contains("delete") || t.contains("remove") => extract_delete_scope(arguments),

        t if t.contains("grep") || t.contains("search") => extract_search_scope(arguments),
        t if t.contains("glob") || t.contains("find") => extract_glob_scope(arguments),

        t if t.contains("shell") || t.contains("exec") || t.contains("command") => extract_shell_scope(arguments),

        t if t.contains("http") || t.contains("fetch") || t.contains("request") || t.contains("curl") => {
            ScopeInfo::new().with_external()
        }

        t if t.contains("git") => extract_git_scope(arguments),
        t if t.contains("npm") || t.contains("yarn") || t.contains("pip") || t.contains("cargo") => {
            ScopeInfo::new().with_project_wide()
        }

        _ => extract_generic_scope(arguments),
    }
}

/// Extract scope from read operations
fn extract_read_scope(args: &Value) -> ScopeInfo {
    if let Some(obj) = args.as_object() {
        if let Some(file_path) = obj.get("file_path").and_then(|v| v.as_str()) {
            return ScopeInfo::new().add_file(file_path);
        }

        if let Some(path) = obj.get("path").and_then(|v| v.as_str()) {
            return ScopeInfo::new().add_file(path);
        }

        if let Some(paths) = obj.get("paths").and_then(|v| v.as_array()) {
            let files: Vec<String> = paths.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
            if !files.is_empty() {
                return ScopeInfo::new().add_files(files);
            }
        }
    }

    ScopeInfo::new()
}

/// Extract scope from write/edit operations
fn extract_write_scope(args: &Value) -> ScopeInfo {
    if let Some(obj) = args.as_object() {
        if let Some(file_path) = obj.get("file_path").and_then(|v| v.as_str()) {
            return ScopeInfo::new().add_file(file_path);
        }
        if let Some(path) = obj.get("path").and_then(|v| v.as_str()) {
            return ScopeInfo::new().add_file(path);
        }
    }

    ScopeInfo::new()
}

/// Extract scope from multi-edit operations
fn extract_multiedit_scope(args: &Value) -> ScopeInfo {
    if let Some(obj) = args.as_object()
        && let Some(file_path) = obj.get("file_path").and_then(|v| v.as_str())
    {
        return ScopeInfo::new().add_file(file_path);
    }

    ScopeInfo::new()
}

/// Extract scope from delete/remove operations
fn extract_delete_scope(args: &Value) -> ScopeInfo {
    if let Some(obj) = args.as_object() {
        if let Some(path) = obj.get("path").and_then(|v| v.as_str()) {
            if path.ends_with('/') || obj.get("recursive").is_some() {
                return ScopeInfo::new().add_directory(path);
            }
            return ScopeInfo::new().add_file(path);
        }

        if let Some(paths) = obj.get("paths").and_then(|v| v.as_array()) {
            let items: Vec<String> = paths.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
            if !items.is_empty() {
                let mut scope = ScopeInfo::new();
                for item in items {
                    if item.ends_with('/') {
                        scope = scope.add_directory(item);
                    } else {
                        scope = scope.add_file(item);
                    }
                }
                return scope;
            }
        }
    }

    ScopeInfo::new()
}

/// Extract scope from search/grep operations
fn extract_search_scope(args: &Value) -> ScopeInfo {
    let mut scope = ScopeInfo::new();

    if let Some(obj) = args.as_object() {
        if let Some(path) = obj.get("path").and_then(|v| v.as_str()) {
            scope = scope.add_directory(path);
        }

        if let Some(pattern) = obj.get("pattern").and_then(|v| v.as_str()) {
            scope = scope.add_pattern(pattern);
        }

        if let Some(paths) = obj.get("paths").and_then(|v| v.as_array()) {
            let dirs: Vec<String> = paths.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
            scope = scope.add_files(dirs);
        }
    }

    scope
}

/// Extract scope from glob/find operations
fn extract_glob_scope(args: &Value) -> ScopeInfo {
    let mut scope = ScopeInfo::new();

    if let Some(obj) = args.as_object() {
        if let Some(pattern) = obj.get("pattern").and_then(|v| v.as_str()) {
            scope = scope.add_pattern(pattern);
        }

        if let Some(path) = obj.get("path").and_then(|v| v.as_str()) {
            scope = scope.add_directory(path);
        }
    }

    scope
}

/// Extract scope from shell/command execution
fn extract_shell_scope(args: &Value) -> ScopeInfo {
    if let Some(obj) = args.as_object()
        && let Some(command) = obj.get("command").and_then(|v| v.as_str())
    {
        return analyze_shell_command_scope(command);
    }

    ScopeInfo::new().with_project_wide()
}

/// Analyze a shell command to determine its scope
fn analyze_shell_command_scope(command: &str) -> ScopeInfo {
    let cmd_lower = command.to_lowercase();

    if cmd_lower.contains("curl")
        || cmd_lower.contains("wget")
        || cmd_lower.contains("ssh")
        || cmd_lower.contains("git push")
        || cmd_lower.contains("git pull")
    {
        return ScopeInfo::new().with_external();
    }

    if cmd_lower.contains("cargo build")
        || cmd_lower.contains("cargo test")
        || cmd_lower.contains("npm install")
        || cmd_lower.contains("npm run")
        || cmd_lower.contains("yarn")
        || cmd_lower.contains("pip install")
        || cmd_lower.contains("make")
        || cmd_lower.contains("cmake")
    {
        return ScopeInfo::new().with_project_wide();
    }

    let mut files = HashSet::new();
    let mut directories = HashSet::new();

    for arg in command.split_whitespace() {
        if arg.starts_with('/') || arg.starts_with("./") || arg.starts_with("../") {
            if arg.ends_with('/') {
                directories.insert(arg.to_string());
            } else {
                files.insert(arg.to_string());
            }
        }
    }

    let mut scope = ScopeInfo::new();
    if !files.is_empty() {
        scope = scope.add_files(files.into_iter().collect());
    }
    if !directories.is_empty() {
        for dir in directories {
            scope = scope.add_directory(dir);
        }
    }

    if scope.is_empty() { ScopeInfo::new().with_project_wide() } else { scope }
}

/// Extract scope from git operations
fn extract_git_scope(args: &Value) -> ScopeInfo {
    if let Some(obj) = args.as_object()
        && let Some(command) = obj.get("command").and_then(|v| v.as_str())
    {
        if command.contains("push") || command.contains("pull") || command.contains("fetch") {
            return ScopeInfo::new().with_external();
        }

        return ScopeInfo::new().with_project_wide();
    }

    ScopeInfo::new().with_project_wide()
}

/// Extract scope from generic arguments (looks for common field names)
fn extract_generic_scope(args: &Value) -> ScopeInfo {
    let mut scope = ScopeInfo::new();

    if let Some(obj) = args.as_object() {
        let path_keys = [
            "path",
            "file",
            "file_path",
            "filePath",
            "filename",
            "source",
            "destination",
        ];

        for key in &path_keys {
            if let Some(value) = obj.get(*key)
                && let Some(path) = value.as_str()
            {
                if path.contains('*') || path.contains('?') {
                    scope = scope.add_pattern(path);
                } else if path.ends_with('/') {
                    scope = scope.add_directory(path);
                } else {
                    scope = scope.add_file(path);
                }
                break;
            }
        }

        let array_keys = ["paths", "files", "files_and_folders"];
        for key in &array_keys {
            if let Some(value) = obj.get(*key)
                && let Some(arr) = value.as_array()
            {
                let items: Vec<String> = arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
                if !items.is_empty() {
                    scope = scope.add_files(items);
                    break;
                }
            }
        }
    }

    scope
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scope_info_new() {
        let scope = ScopeInfo::new();
        assert!(scope.is_empty());
        assert_eq!(scope.to_brief(), "No specific scope");
    }

    #[test]
    fn test_scope_info_add_file() {
        let scope = ScopeInfo::new().add_file("/path/to/file.rs");
        assert_eq!(scope.files, vec!["/path/to/file.rs".to_string()]);
        assert_eq!(scope.to_brief(), "1 file");
    }

    #[test]
    fn test_scope_info_add_directory() {
        let scope = ScopeInfo::new().add_directory("/path/to/dir/");
        assert_eq!(scope.directories, vec!["/path/to/dir/".to_string()]);
        assert_eq!(scope.to_brief(), "1 directory");
    }

    #[test]
    fn test_scope_info_multiple_files() {
        let scope = ScopeInfo::new().add_file("file1.rs").add_file("file2.rs");
        assert_eq!(scope.to_brief(), "2 files");
    }

    #[test]
    fn test_scope_info_external() {
        let scope = ScopeInfo::new().with_external();
        assert!(scope.is_external);
        assert_eq!(scope.to_brief(), "External system");
    }

    #[test]
    fn test_scope_info_project_wide() {
        let scope = ScopeInfo::new().with_project_wide();
        assert!(scope.is_project_wide);
        assert_eq!(scope.to_brief(), "Project-wide");
    }

    #[test]
    fn test_extract_scope_read() {
        let args = serde_json::json!({"file_path": "/path/to/file.rs"});
        let scope = extract_scope("read", &args);
        assert_eq!(scope.files, vec!["/path/to/file.rs".to_string()]);
    }

    #[test]
    fn test_extract_scope_edit() {
        let args = serde_json::json!({"path": "/path/to/file.rs", "old_string": "foo", "new_string": "bar"});
        let scope = extract_scope("edit", &args);
        assert_eq!(scope.files, vec!["/path/to/file.rs".to_string()]);
    }

    #[test]
    fn test_extract_scope_glob() {
        let args = serde_json::json!({"pattern": "**/*.rs", "path": "/src"});
        let scope = extract_scope("glob", &args);
        assert_eq!(scope.patterns, vec!["**/*.rs".to_string()]);
        assert_eq!(scope.directories, vec!["/src".to_string()]);
    }

    #[test]
    fn test_extract_scope_grep() {
        let args = serde_json::json!({"pattern": "TODO", "path": "/src"});
        let scope = extract_scope("grep", &args);
        assert_eq!(scope.patterns, vec!["TODO".to_string()]);
        assert_eq!(scope.directories, vec!["/src".to_string()]);
    }

    #[test]
    fn test_extract_scope_delete() {
        let args = serde_json::json!({"path": "/tmp/file.txt"});
        let scope = extract_scope("delete", &args);
        assert_eq!(scope.files, vec!["/tmp/file.txt".to_string()]);
    }

    #[test]
    fn test_extract_scope_delete_directory() {
        let args = serde_json::json!({"path": "/tmp/dir/", "recursive": true});
        let scope = extract_scope("delete", &args);
        assert_eq!(scope.directories, vec!["/tmp/dir/".to_string()]);
    }

    #[test]
    fn test_extract_scope_shell_network() {
        let args = serde_json::json!({"command": "curl https://api.example.com"});
        let scope = extract_scope("shell", &args);
        assert!(scope.is_external);
    }

    #[test]
    fn test_extract_scope_shell_project_wide() {
        let args = serde_json::json!({"command": "cargo build"});
        let scope = extract_scope("shell", &args);
        assert!(scope.is_project_wide);
    }

    #[test]
    fn test_extract_scope_http() {
        let args = serde_json::json!({"url": "https://example.com"});
        let scope = extract_scope("http_request", &args);
        assert!(scope.is_external);
    }

    #[test]
    fn test_extract_scope_git() {
        let args = serde_json::json!({"command": "git push"});
        let scope = extract_scope("git", &args);
        assert!(scope.is_external);
    }

    #[test]
    fn test_extract_scope_generic() {
        let args = serde_json::json!({"path": "/some/file.txt"});
        let scope = extract_scope("unknown_tool", &args);
        assert_eq!(scope.files, vec!["/some/file.txt".to_string()]);
    }

    #[test]
    fn test_scope_to_detailed() {
        let scope = ScopeInfo::new()
            .add_file("file1.rs")
            .add_file("file2.rs")
            .add_directory("/src/");

        let detailed = scope.to_detailed();
        assert!(detailed.contains("file1.rs"));
        assert!(detailed.contains("file2.rs"));
        assert!(detailed.contains("/src/"));
    }

    #[test]
    fn test_analyze_shell_command_file_paths() {
        let command = "cat /path/to/file.txt";
        let scope = analyze_shell_command_scope(command);
        assert_eq!(scope.files, vec!["/path/to/file.txt".to_string()]);
    }

    #[test]
    fn test_analyze_shell_command_directory() {
        let command = "ls -la ./src/";
        let scope = analyze_shell_command_scope(command);
        assert_eq!(scope.directories, vec!["./src/".to_string()]);
    }

    #[test]
    fn test_analyze_shell_command_project_wide() {
        let command = "npm install";
        let scope = analyze_shell_command_scope(command);
        assert!(scope.is_project_wide);
    }

    #[test]
    fn test_scope_info_to_brief_mixed() {
        let scope = ScopeInfo::new()
            .add_file("file1.rs")
            .add_file("file2.rs")
            .add_directory("/src/")
            .add_pattern("**/*.test.rs");

        let brief = scope.to_brief();
        assert!(brief.contains("2 files"));
        assert!(brief.contains("1 directory"));
        assert!(brief.contains("1 pattern"));
    }
}
