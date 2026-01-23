use ignore::Walk;
use serde_json::Value;
use std::path::{Path, PathBuf};
use thunderus_core::{Result, ToolRisk};
use thunderus_providers::{ToolParameter, ToolResult};

use crate::Tool;

/// Sort order for glob results
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobSortOrder {
    /// Sort by modification time (newest first)
    ModifiedTime,
    /// Sort by file path (alphabetical)
    Path,
    /// No sorting (useful for large directories)
    None,
}

impl GlobSortOrder {
    pub fn parse_str(s: &str) -> Option<Self> {
        match s {
            "modified" | "time" | "mtime" => Some(Self::ModifiedTime),
            "path" | "name" | "alpha" => Some(Self::Path),
            "none" | "unsorted" => Some(Self::None),
            _ => None,
        }
    }
}

/// Options for the Glob tool
pub struct GlobOptions<'a> {
    pub pattern: &'a str,
    pub path: &'a Path,
    pub sort_order: GlobSortOrder,
    pub respect_gitignore: bool,
    pub limit: Option<usize>,
}

/// A tool that finds files matching glob patterns
///
/// This tool provides fast file discovery with .gitignore awareness,
/// similar to the "fast file search" pattern in Claude Code.
#[derive(Debug)]
pub struct GlobTool;

impl GlobTool {
    /// Validates that the path exists and is accessible
    fn validate_path(path: &Path) -> Result<()> {
        if !path.exists() {
            return Err(thunderus_core::Error::Validation(format!(
                "Path does not exist: {}",
                path.display()
            )));
        }
        Ok(())
    }

    /// Executes the glob search and returns matching files
    fn execute_and_parse(options: &GlobOptions) -> Result<String> {
        Self::validate_path(options.path)?;

        let mut results: Vec<PathBuf> = Vec::new();

        if options.respect_gitignore {
            let walk_builder = Walk::new(options.path);
            let pattern = options.pattern.trim_start_matches('/');

            for entry in walk_builder {
                match entry {
                    Ok(entry) => {
                        let path = entry.path();

                        if path.is_dir() {
                            continue;
                        }

                        if let Ok(rel_path) = path.strip_prefix(options.path) {
                            let rel_str = rel_path.to_string_lossy();

                            if Self::matches_glob_pattern(&rel_str, pattern) {
                                results.push(path.to_path_buf());

                                if let Some(limit) = options.limit
                                    && results.len() >= limit
                                {
                                    break;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Warning: error reading path: {}", e);
                    }
                }
            }
        } else {
            let full_pattern = if options.path == Path::new(".") {
                options.pattern.to_string()
            } else {
                format!("{}/{}", options.path.display(), options.pattern.trim_start_matches('/'))
            };

            if let Ok(glob_iter) = glob::glob(&full_pattern) {
                for entry in glob_iter {
                    match entry {
                        Ok(path) => {
                            if path.is_dir() {
                                continue;
                            }

                            results.push(path);
                        }
                        Err(e) => {
                            eprintln!("Warning: error reading path: {}", e);
                        }
                    }

                    if let Some(limit) = options.limit
                        && results.len() >= limit
                    {
                        break;
                    }
                }
            }
        }

        match options.sort_order {
            GlobSortOrder::ModifiedTime => {
                results.sort_by_key(|path| {
                    std::fs::metadata(path)
                        .and_then(|m| m.modified())
                        .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                });
                results.reverse();
            }
            GlobSortOrder::Path => results.sort(),
            GlobSortOrder::None => (),
        }

        if results.is_empty() {
            Ok(format!("No files found matching pattern: {}", options.pattern))
        } else {
            let base_path = options
                .path
                .canonicalize()
                .unwrap_or_else(|_| options.path.to_path_buf());

            let formatted: Vec<String> = results
                .iter()
                .filter_map(|p| {
                    if let Ok(rel) = p.strip_prefix(&base_path) {
                        let rel_str = rel.display().to_string();
                        if rel_str.is_empty() {
                            p.file_name().and_then(|n| n.to_str()).map(|s| s.to_string())
                        } else {
                            Some(rel_str)
                        }
                    } else if let Ok(rel) = p.strip_prefix(options.path) {
                        let rel_str = rel.display().to_string();
                        if rel_str.is_empty() {
                            p.file_name().and_then(|n| n.to_str()).map(|s| s.to_string())
                        } else {
                            Some(rel_str)
                        }
                    } else {
                        p.to_str().map(|s| s.to_string())
                    }
                })
                .collect();

            Ok(formatted.join("\n"))
        }
    }

    /// Checks if a path matches a glob pattern
    ///
    /// This is a simplified glob matcher that supports:
    /// - * matches any sequence of non-separator characters
    /// - ** matches any sequence of characters (including separators)
    /// - ? matches any single non-separator character
    fn matches_glob_pattern(path: &str, pattern: &str) -> bool {
        if pattern == "*" || pattern == "**" {
            return true;
        }

        let regex_pattern = pattern
            .replace('\\', r"\\")
            .replace('.', r"\.")
            .replace('?', ".")
            .replace("**/", "(?:.*/)?")
            .replace("**", ".*")
            .replace('*', "[^/]*");

        if let Ok(re) = regex_lite::Regex::new(&regex_pattern) {
            re.is_match(path)
        } else {
            let simplified = pattern.replace(['*', '?'], "");
            path.contains(&simplified)
        }
    }
}

impl Tool for GlobTool {
    fn name(&self) -> &str {
        "glob"
    }

    fn description(&self) -> &str {
        "Find files matching glob patterns. Use this instead of bash find for file discovery."
    }

    fn parameters(&self) -> ToolParameter {
        ToolParameter::new_object(vec![
            (
                "pattern".to_string(),
                ToolParameter::new_string("Glob pattern").with_description(
                    "Glob pattern to match files (e.g., '**/*.rs', 'src/**/test_*.rs', '*.{ts,tsx}')",
                ),
            ),
            (
                "path".to_string(),
                ToolParameter::new_string("Directory to search")
                    .with_description("Directory path to search in (defaults to current directory)"),
            ),
            (
                "sort_order".to_string(),
                ToolParameter::new_string("Sort order").with_description(
                    "Sort order: 'modified' (default, newest first), 'path' (alphabetical), or 'none'",
                ),
            ),
            (
                "respect_gitignore".to_string(),
                ToolParameter::new_boolean("Respect .gitignore")
                    .with_description("Whether to respect .gitignore rules (default: true)"),
            ),
            (
                "limit".to_string(),
                ToolParameter::new_number("Max results")
                    .with_description("Maximum number of results to return (default: unlimited)"),
            ),
        ])
    }

    fn risk_level(&self) -> ToolRisk {
        ToolRisk::Safe
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn classification(&self) -> Option<thunderus_core::Classification> {
        Some(thunderus_core::Classification::new(
            ToolRisk::Safe,
            "Glob is a read-only file discovery operation that does not modify files or system state. It only reads directory listings to find matching paths.",
        ))
    }

    fn execute(&self, tool_call_id: String, arguments: &Value) -> Result<ToolResult> {
        let pattern = arguments
            .get("pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| thunderus_core::Error::Validation("Missing or invalid 'pattern' parameter".to_string()))?;

        if pattern.is_empty() {
            return Err(thunderus_core::Error::Validation("Pattern cannot be empty".to_string()));
        }

        let path_str = arguments.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let sort_order_str = arguments.get("sort_order").and_then(|v| v.as_str());
        let sort_order = sort_order_str
            .and_then(GlobSortOrder::parse_str)
            .unwrap_or(GlobSortOrder::ModifiedTime);
        let respect_gitignore = arguments
            .get("respect_gitignore")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let limit = arguments.get("limit").and_then(|v| v.as_u64()).map(|v| v as usize);

        let path = PathBuf::from(path_str);
        Self::validate_path(&path)?;

        let options = GlobOptions { pattern, path: &path, sort_order, respect_gitignore, limit };

        let result = Self::execute_and_parse(&options)?;

        Ok(ToolResult::success(tool_call_id, result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_tool_properties() {
        let tool = GlobTool;
        assert_eq!(tool.name(), "glob");
        assert_eq!(
            tool.description(),
            "Find files matching glob patterns. Use this instead of bash find for file discovery."
        );
    }

    #[test]
    fn test_glob_risk_level() {
        let tool = GlobTool;
        assert!(tool.risk_level().is_safe());
    }

    #[test]
    fn test_glob_classification() {
        let tool = GlobTool;
        let classification = tool.classification().unwrap();
        assert!(classification.risk.is_safe());
        assert!(classification.reasoning.contains("read-only"));
    }

    #[test]
    fn test_glob_spec() {
        let tool = GlobTool;
        let spec = tool.spec();

        assert_eq!(spec.name(), "glob");
        assert!(spec.description().is_some());
    }

    #[test]
    fn test_glob_execute_missing_pattern() {
        let tool = GlobTool;
        let args = serde_json::json!({});
        let result = tool.execute("call_glob_1".to_string(), &args);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Missing or invalid 'pattern'"));
    }

    #[test]
    fn test_glob_execute_empty_pattern() {
        let tool = GlobTool;
        let args = serde_json::json!({"pattern": ""});
        let result = tool.execute("call_glob_2".to_string(), &args);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Pattern cannot be empty"));
    }

    #[test]
    fn test_glob_execute_invalid_path() {
        let tool = GlobTool;
        let args = serde_json::json!({"pattern": "*.rs", "path": "/nonexistent/path/xyz"});
        let result = tool.execute("call_glob_3".to_string(), &args);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Path does not exist"));
    }

    #[test]
    fn test_glob_sort_order_from_str() {
        assert_eq!(GlobSortOrder::parse_str("modified"), Some(GlobSortOrder::ModifiedTime));
        assert_eq!(GlobSortOrder::parse_str("time"), Some(GlobSortOrder::ModifiedTime));
        assert_eq!(GlobSortOrder::parse_str("path"), Some(GlobSortOrder::Path));
        assert_eq!(GlobSortOrder::parse_str("alpha"), Some(GlobSortOrder::Path));
        assert_eq!(GlobSortOrder::parse_str("none"), Some(GlobSortOrder::None));
        assert_eq!(GlobSortOrder::parse_str("unsorted"), Some(GlobSortOrder::None));
        assert_eq!(GlobSortOrder::parse_str("invalid"), None);
    }

    #[test]
    fn test_glob_execute_simple_pattern() {
        let tool = GlobTool;

        let args = serde_json::json!({"pattern": "**/*.rs", "path": "src", "limit": 20});
        let result = tool.execute("call_glob_4".to_string(), &args);

        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert_eq!(tool_result.tool_call_id, "call_glob_4");
        assert!(tool_result.is_success());
        assert!(!tool_result.content.is_empty());
        assert!(!tool_result.content.contains("No files found"));
    }

    #[test]
    fn test_glob_execute_with_limit() {
        let tool = GlobTool;
        let args = serde_json::json!({"pattern": "**/*.rs", "path": "src", "limit": 3});
        let result = tool.execute("call_glob_5".to_string(), &args);

        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert!(tool_result.is_success());

        let line_count = tool_result.content.lines().count();
        assert!(line_count <= 3);
    }

    #[test]
    fn test_glob_execute_no_results() {
        let tool = GlobTool;
        let args = serde_json::json!({"pattern": "*.nonexistent", "path": "src"});
        let result = tool.execute("call_glob_6".to_string(), &args);

        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert!(tool_result.is_success());
        assert!(tool_result.content.contains("No files found"));
    }
}
