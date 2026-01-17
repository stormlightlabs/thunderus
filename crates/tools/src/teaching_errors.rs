//! Teaching error messages for tool operations
//!
//! This module provides enhanced error messages that explain what went wrong
//! and how to fix it. Each error includes context, explanation, and actionable
//! next steps.

use std::fmt;

/// Error category for teaching messages
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    /// The tool was used incorrectly (wrong parameters, wrong order, etc.)
    Usage,
    /// The requested operation conflicts with safety constraints
    Safety,
    /// The requested data was not found
    NotFound,
    /// The operation would have ambiguous effects
    Ambiguity,
    /// File I/O or system-level error
    System,
}

/// A teaching error with context and guidance
#[derive(Debug, Clone)]
pub struct TeachingError {
    /// The tool that generated this error
    pub tool: String,
    /// Category of error
    pub category: ErrorCategory,
    /// Brief summary of what went wrong
    pub summary: String,
    /// Detailed explanation of the problem
    pub explanation: String,
    /// Actionable next steps to resolve the error
    pub next_steps: Vec<String>,
}

impl TeachingError {
    /// Create a new teaching error
    pub fn new(
        tool: impl Into<String>, category: ErrorCategory, summary: impl Into<String>, explanation: impl Into<String>,
        next_steps: Vec<String>,
    ) -> Self {
        Self { tool: tool.into(), category, summary: summary.into(), explanation: explanation.into(), next_steps }
    }

    /// Format as a complete teaching message
    pub fn format(&self) -> String {
        let mut output = String::new();

        output.push_str(&format!("## {}: {}\n", self.tool, self.summary));
        output.push_str(&format!("**Error Type**: {:?}\n\n", self.category));
        output.push_str(&format!("**Explanation**:\n{}\n\n", self.explanation));

        if !self.next_steps.is_empty() {
            output.push_str("**To fix this**:\n");
            for (i, step) in self.next_steps.iter().enumerate() {
                output.push_str(&format!("{}. {}\n", i + 1, step));
            }
        }

        output
    }

    /// Format as a compact message
    pub fn to_compact(&self) -> String {
        format!("{}: {}", self.tool, self.summary)
    }
}

impl fmt::Display for TeachingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_compact())
    }
}

/// Builder for common Edit tool errors
pub struct EditErrors;

impl EditErrors {
    /// Error: old_string not unique in file
    pub fn old_string_not_unique(file_path: &str, count: usize, old_string: &str) -> TeachingError {
        let truncated = Self::truncate_string(old_string, 60);
        TeachingError::new(
            "edit",
            ErrorCategory::Ambiguity,
            "old_string not unique",
            format!(
                "The text you're trying to replace appears {} time(s) in the file '{}'. \
                The Edit tool requires an exact, unique match to prevent accidental changes \
                when multiple similar sections exist.",
                count, file_path
            ),
            vec![
                format!("Include more surrounding context in old_string to make it unique"),
                "Use replace_all=true if you intend to replace all occurrences".to_string(),
                format!("Use the Read tool to examine the file and identify the specific occurrence"),
                format!("Current old_string: '{}'", truncated),
            ],
        )
    }

    /// Error: old_string not found in file
    pub fn old_string_not_found(file_path: &str, old_string: &str) -> TeachingError {
        let truncated = Self::truncate_string(old_string, 60);
        TeachingError::new(
            "edit",
            ErrorCategory::NotFound,
            "old_string not found in file",
            format!(
                "The text you specified could not be found in '{}'. \
                This can happen if the file has changed since you last read it, or if \
                the old_string doesn't match exactly (including whitespace).",
                file_path
            ),
            vec![
                "Read the file again to see its current contents".to_string(),
                "Check that old_string matches exactly, including indentation and whitespace".to_string(),
                format!("Your old_string: '{}'", truncated),
            ],
        )
    }

    /// Error: file not read before edit
    pub fn file_not_read_first(file_path: &str) -> TeachingError {
        TeachingError::new(
            "edit",
            ErrorCategory::Safety,
            "Read required before Edit",
            format!(
                "You must use the Read tool on '{}' before using Edit. This safety check \
                ensures you've seen the current file contents and understand its structure \
                before making changes.",
                file_path
            ),
            vec![
                format!("Use Read tool on '{}' first", file_path),
                "This prevents blind edits that could break code".to_string(),
                "The read history is tracked per session for safety".to_string(),
            ],
        )
    }

    /// Error: file path doesn't exist
    pub fn file_not_found(file_path: &str) -> TeachingError {
        TeachingError::new(
            "edit",
            ErrorCategory::NotFound,
            "File not found",
            format!("The file '{}' does not exist or is not accessible.", file_path),
            vec![
                "Check the file path is correct and absolute".to_string(),
                "Use Glob tool to find the correct file path".to_string(),
                "Ensure you have read permissions for the file".to_string(),
            ],
        )
    }

    /// Helper to truncate strings for display
    fn truncate_string(s: &str, max_len: usize) -> String {
        if s.len() <= max_len { s.to_string() } else { format!("{}...", &s[..max_len.saturating_sub(3)]) }
    }
}

/// Builder for common MultiEdit tool errors
pub struct MultiEditErrors;

impl MultiEditErrors {
    /// Error: overlapping edits detected
    pub fn overlapping_edits(index1: usize, index2: usize) -> TeachingError {
        TeachingError::new(
            "multiedit",
            ErrorCategory::Ambiguity,
            "Overlapping edit operations",
            format!(
                "Edit operations {} and {} overlap or would conflict. MultiEdit requires \
                all edits to be independent so they can be applied atomically.",
                index1 + 1,
                index2 + 1
            ),
            vec![
                "Separate these into independent Edit operations".to_string(),
                "Ensure old_string values don't overlap".to_string(),
                "Apply edits in sequence using individual Edit calls".to_string(),
            ],
        )
    }

    /// Error: empty edits array
    pub fn no_edits_provided() -> TeachingError {
        TeachingError::new(
            "multiedit",
            ErrorCategory::Usage,
            "No edit operations provided",
            "The MultiEdit tool requires at least one edit operation to apply.".to_string(),
            vec![
                "Provide at least one edit in the 'edits' array".to_string(),
                "Each edit must have 'old_string' and 'new_string' fields".to_string(),
                "For single edits, use the Edit tool instead".to_string(),
            ],
        )
    }

    /// Error: edit old_string empty
    pub fn empty_old_string(index: usize) -> TeachingError {
        TeachingError::new(
            "multiedit",
            ErrorCategory::Usage,
            "Empty old_string in edit operation",
            format!(
                "Edit operation {} has an empty old_string, which is not allowed.",
                index + 1
            ),
            vec![
                "Provide a non-empty old_string for each edit".to_string(),
                "old_string must match exactly in the file to be replaced".to_string(),
            ],
        )
    }

    /// Error: old_string not found in file
    pub fn old_string_not_found(index: usize, old_string: &str) -> TeachingError {
        let truncated = EditErrors::truncate_string(old_string, 60);
        TeachingError::new(
            "multiedit",
            ErrorCategory::NotFound,
            format!("Edit operation {}: old_string not found", index + 1),
            format!(
                "The specified old_string in edit {} could not be found in the file.",
                index + 1
            ),
            vec![
                "Read the file again to verify its contents".to_string(),
                "Check the old_string matches exactly".to_string(),
                format!("Old string: '{}'", truncated),
            ],
        )
    }
}

/// Builder for common Grep tool errors
pub struct GrepErrors;

impl GrepErrors {
    /// Error: empty pattern
    pub fn empty_pattern() -> TeachingError {
        TeachingError::new(
            "grep",
            ErrorCategory::Usage,
            "Empty search pattern",
            "The Grep tool requires a non-empty pattern to search for.".to_string(),
            vec![
                "Provide a 'pattern' parameter with your search term".to_string(),
                "Patterns can be plain text or regular expressions".to_string(),
            ],
        )
    }

    /// Error: path doesn't exist
    pub fn path_not_found(path: &str) -> TeachingError {
        TeachingError::new(
            "grep",
            ErrorCategory::NotFound,
            "Search path not found",
            format!("The path '{}' does not exist or is not accessible.", path),
            vec![
                "Check the path is correct".to_string(),
                "Use Glob tool to find valid paths".to_string(),
                "Ensure you have read permissions".to_string(),
            ],
        )
    }

    /// Error: invalid regex pattern
    pub fn invalid_regex(pattern: &str, error: &str) -> TeachingError {
        TeachingError::new(
            "grep",
            ErrorCategory::Usage,
            "Invalid regular expression",
            format!("The pattern '{}' is not a valid regular expression.", pattern),
            vec![
                format!("Error: {}", error),
                "Check for unclosed brackets, parentheses, or special characters".to_string(),
                "Use plain text search by escaping special characters".to_string(),
            ],
        )
    }
}

/// Builder for common Glob tool errors
pub struct GlobErrors;

impl GlobErrors {
    /// Error: empty pattern
    pub fn empty_pattern() -> TeachingError {
        TeachingError::new(
            "glob",
            ErrorCategory::Usage,
            "Empty glob pattern",
            "The Glob tool requires a non-empty pattern to match files.".to_string(),
            vec![
                "Provide a 'pattern' parameter (e.g., '**/*.rs', 'src/**/*.ts')".to_string(),
                "Patterns support *, **, and ? wildcards".to_string(),
            ],
        )
    }

    /// Error: path doesn't exist
    pub fn path_not_found(path: &str) -> TeachingError {
        TeachingError::new(
            "glob",
            ErrorCategory::NotFound,
            "Search path not found",
            format!("The directory '{}' does not exist or is not accessible.", path),
            vec![
                "Check the directory path is correct".to_string(),
                "Use '.' to search the current directory".to_string(),
                "Use absolute paths for clarity".to_string(),
            ],
        )
    }
}

/// Builder for common Read tool errors
pub struct ReadErrors;

impl ReadErrors {
    /// Error: file not found
    pub fn file_not_found(file_path: &str) -> TeachingError {
        TeachingError::new(
            "read",
            ErrorCategory::NotFound,
            "File not found",
            format!("The file '{}' does not exist or is not accessible.", file_path),
            vec![
                "Check the file path is correct and absolute".to_string(),
                "Use Glob tool to find the file".to_string(),
                "Ensure you have read permissions".to_string(),
            ],
        )
    }

    /// Error: file is binary
    pub fn binary_file(file_path: &str) -> TeachingError {
        TeachingError::new(
            "read",
            ErrorCategory::Usage,
            "Binary file detected",
            format!("The file '{}' appears to be a binary file.", file_path),
            vec![
                "The Read tool only supports text files".to_string(),
                "Binary files cannot be safely displayed as text".to_string(),
                "Use specialized tools for this file type".to_string(),
            ],
        )
    }

    /// Error: invalid offset/limit
    pub fn invalid_line_range(offset: usize, limit: usize) -> TeachingError {
        TeachingError::new(
            "read",
            ErrorCategory::Usage,
            "Invalid line range",
            format!("Invalid offset ({}) or limit ({}) specified.", offset, limit),
            vec![
                "offset must be >= 1 (line numbers start at 1)".to_string(),
                "limit must be >= 1".to_string(),
                "Use default values (offset: 1, limit: 2000) for standard reads".to_string(),
            ],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edit_error_not_unique() {
        let err = EditErrors::old_string_not_unique("/path/to/file.rs", 3, "function foo()");
        let formatted = err.format();

        assert!(formatted.contains("edit"));
        assert!(formatted.contains("old_string not unique"));
        assert!(formatted.contains("3 time(s)"));
        assert!(formatted.contains("replace_all=true"));
    }

    #[test]
    fn test_edit_error_not_found() {
        let err = EditErrors::old_string_not_found("/path/to/file.rs", "function bar()");
        let formatted = err.format();

        assert!(formatted.contains("old_string not found"));
        assert!(formatted.contains("Read the file again"));
    }

    #[test]
    fn test_edit_error_not_read_first() {
        let err = EditErrors::file_not_read_first("/path/to/file.rs");
        let formatted = err.format();

        assert!(formatted.contains("Read required before Edit"));
        assert!(formatted.contains("safety check"));
    }

    #[test]
    fn test_multiedit_error_overlapping() {
        let err = MultiEditErrors::overlapping_edits(0, 1);
        let formatted = err.format();

        assert!(formatted.contains("Overlapping edit operations"));
        assert!(formatted.contains("independent"));
    }

    #[test]
    fn test_grep_error_empty_pattern() {
        let err = GrepErrors::empty_pattern();
        let formatted = err.format();

        assert!(formatted.contains("Empty search pattern"));
        assert!(formatted.contains("non-empty"));
    }

    #[test]
    fn test_glob_error_empty_pattern() {
        let err = GlobErrors::empty_pattern();
        assert!(err.to_compact().contains("glob"));
    }

    #[test]
    fn test_read_error_binary_file() {
        let err = ReadErrors::binary_file("/path/to/binary");
        let formatted = err.format();

        assert!(formatted.contains("Binary file detected"));
        assert!(formatted.contains("text files"));
    }

    #[test]
    fn test_error_to_compact() {
        let err = EditErrors::file_not_found("/test.rs");
        assert!(err.to_compact().contains("edit: File not found"));
    }

    #[test]
    fn test_error_format_has_next_steps() {
        let err = EditErrors::old_string_not_unique("/test.rs", 2, "foo");
        let formatted = err.format();

        assert!(formatted.contains("To fix this"));
        assert!(formatted.contains("1."));
    }
}
