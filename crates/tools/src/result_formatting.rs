//! Result formatting for model consumption
//!
//! This module provides utilities for formatting tool results in a structured
//! way that is easy for LLMs to parse and understand. The formats emphasize
//! clarity, context, and actionable information.

use serde::{Deserialize, Serialize};

/// Formatted tool result with structured metadata
///
/// Wraps tool outputs with additional context that helps models understand
/// what happened and what to do next.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattedResult {
    /// The tool that was executed
    pub tool: String,
    /// Whether the operation succeeded
    pub success: bool,
    /// A brief, human-readable summary of the result
    pub summary: String,
    /// Detailed output or error message
    pub details: String,
    /// Exit code (0-255, optional)
    pub exit_code: Option<i32>,
    /// Suggested next steps (optional)
    pub next_steps: Option<Vec<String>>,
    /// Structured data for programmatic access (optional)
    pub data: Option<serde_json::Value>,
}

impl FormattedResult {
    /// Create a new successful result
    pub fn success(tool: impl Into<String>, summary: impl Into<String>, details: impl Into<String>) -> Self {
        Self {
            tool: tool.into(),
            success: true,
            summary: summary.into(),
            details: details.into(),
            exit_code: Some(0),
            next_steps: None,
            data: None,
        }
    }

    /// Create a new error result
    pub fn error(tool: impl Into<String>, summary: impl Into<String>, details: impl Into<String>) -> Self {
        Self {
            tool: tool.into(),
            success: false,
            summary: summary.into(),
            details: details.into(),
            exit_code: Some(1),
            next_steps: None,
            data: None,
        }
    }

    /// Create a result with a specific exit code
    pub fn with_exit_code(
        tool: impl Into<String>, summary: impl Into<String>, details: impl Into<String>, exit_code: i32,
    ) -> Self {
        Self {
            tool: tool.into(),
            success: exit_code == 0,
            summary: summary.into(),
            details: details.into(),
            exit_code: Some(exit_code),
            next_steps: None,
            data: None,
        }
    }

    /// Add next steps to the result
    pub fn with_next_steps(mut self, steps: Vec<String>) -> Self {
        self.next_steps = Some(steps);
        self
    }

    /// Add structured data to the result
    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }

    /// Format as a markdown-style output
    pub fn to_markdown(&self) -> String {
        let mut output = String::new();

        output.push_str(&format!("**{}**: {}\n", self.tool, self.summary));
        output.push_str(&format!(
            "Status: {}\n",
            if self.success { "✓ Success" } else { "✗ Failed" }
        ));

        if let Some(code) = self.exit_code {
            output.push_str(&format!("Exit code: {}\n", code));
        }

        if !self.details.is_empty() {
            output.push_str("\n```\n");
            output.push_str(&self.details);
            output.push_str("\n```\n");
        }

        if let Some(ref steps) = self.next_steps {
            output.push_str("\n**Next Steps**:\n");
            for (i, step) in steps.iter().enumerate() {
                output.push_str(&format!("{}. {}\n", i + 1, step));
            }
        }

        output
    }

    /// Format as a compact single-line output
    pub fn to_compact(&self) -> String {
        if self.success {
            format!("{}: {}", self.tool, self.summary)
        } else {
            format!("{} failed: {}", self.tool, self.summary)
        }
    }
}

/// Formatter for Grep tool results
pub struct GrepFormatter;

impl GrepFormatter {
    /// Format grep results with match statistics
    pub fn format(matches: &str, file_count: usize, line_count: usize) -> String {
        format!(
            "Found {} matches across {} file(s)\n\n{}",
            line_count, file_count, matches
        )
    }

    /// Format empty result
    pub fn format_empty(pattern: &str) -> String {
        format!(
            "No matches found for pattern: '{}'\n\nTip: Check your regex syntax or try a broader search pattern.",
            pattern
        )
    }
}

/// Formatter for Glob tool results
pub struct GlobFormatter;

impl GlobFormatter {
    /// Format glob results with file count
    pub fn format(files: &[String], pattern: &str) -> String {
        if files.is_empty() {
            format!("No files found matching pattern: '{}'", pattern)
        } else {
            format!(
                "Found {} file(s) matching '{}':\n\n{}",
                files.len(),
                pattern,
                files.iter().map(|f| format!("- {}", f)).collect::<Vec<_>>().join("\n")
            )
        }
    }
}

/// Formatter for Read tool results
pub struct ReadFormatter;

impl ReadFormatter {
    /// Format read result with line range
    pub fn format(content: &str, file_path: &str, line_start: usize, line_end: usize) -> String {
        format!(
            "Read {} lines from {} (lines {}-{}):\n\n```\n{}\n```",
            line_end - line_start + 1,
            file_path,
            line_start,
            line_end,
            content
        )
    }

    /// Format truncated result warning
    pub fn format_truncated(file_path: &str, total_lines: usize, shown_lines: usize) -> String {
        format!(
            "Note: Showing {} of {} lines from {}. Use offset/limit parameters to read more.\n",
            shown_lines, total_lines, file_path
        )
    }
}

/// Formatter for Edit tool results
pub struct EditFormatter;

impl EditFormatter {
    /// Format successful edit result
    pub fn format_success(file_path: &str, old_string: &str, new_string: &str) -> String {
        format!(
            "Successfully edited: {}\nReplaced '{}' with '{}'",
            file_path,
            Self::truncate_for_display(old_string, 50),
            Self::truncate_for_display(new_string, 50)
        )
    }

    /// Format multiple replacement result
    pub fn format_multiple(file_path: &str, count: usize, old_string: &str, new_string: &str) -> String {
        format!(
            "Successfully edited: {}\nReplaced {} occurrence(s) of '{}' with '{}'",
            file_path,
            count,
            Self::truncate_for_display(old_string, 50),
            Self::truncate_for_display(new_string, 50)
        )
    }

    /// Truncate a string for display purposes
    fn truncate_for_display(s: &str, max_len: usize) -> String {
        if s.len() <= max_len { s.to_string() } else { format!("{}...", &s[..max_len.saturating_sub(3)]) }
    }
}

/// Formatter for MultiEdit tool results
pub struct MultiEditFormatter;

impl MultiEditFormatter {
    /// Format successful multi-edit result
    pub fn format_success(file_path: &str, edit_count: usize, total_replacements: usize) -> String {
        format!(
            "Successfully applied {} edit operation(s) to {}: {} total replacement(s)",
            edit_count, file_path, total_replacements
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_formatted_result_success() {
        let result = FormattedResult::success("grep", "Found 3 matches", "line 1\nline 2\nline 3");
        assert!(result.success);
        assert_eq!(result.tool, "grep");
        assert!(result.to_compact().contains("grep"));
        assert!(result.to_compact().contains("Found 3 matches"));
    }

    #[test]
    fn test_formatted_result_error() {
        let result = FormattedResult::error("edit", "Edit failed", "old_string not found");
        assert!(!result.success);
        assert_eq!(result.tool, "edit");
        assert!(result.to_compact().contains("failed"));
    }

    #[test]
    fn test_formatted_result_with_next_steps() {
        let result = FormattedResult::error("edit", "Edit failed", "old_string not found").with_next_steps(vec![
            "Read the file to see current content".to_string(),
            "Check the old_string matches exactly".to_string(),
        ]);

        let markdown = result.to_markdown();
        assert!(markdown.contains("Next Steps"));
        assert!(markdown.contains("Read the file"));
    }

    #[test]
    fn test_grep_formatter_empty() {
        let output = GrepFormatter::format_empty("test_pattern");
        assert!(output.contains("No matches found"));
        assert!(output.contains("test_pattern"));
        assert!(output.contains("Tip"));
    }

    #[test]
    fn test_glob_formatter_empty() {
        let files: Vec<String> = vec![];
        let output = GlobFormatter::format(&files, "*.rs");
        assert!(output.contains("No files found"));
        assert!(output.contains("*.rs"));
    }

    #[test]
    fn test_glob_formatter_with_files() {
        let files = vec!["src/main.rs".to_string(), "src/lib.rs".to_string()];
        let output = GlobFormatter::format(&files, "*.rs");
        assert!(output.contains("2 file(s)"));
        assert!(output.contains("src/main.rs"));
        assert!(output.contains("src/lib.rs"));
    }

    #[test]
    fn test_read_formatter() {
        let content = "line 1\nline 2\nline 3";
        let output = ReadFormatter::format(content, "src/main.rs", 1, 3);
        assert!(output.contains("src/main.rs"));
        assert!(output.contains("lines 1-3"));
        assert!(output.contains("line 1"));
    }

    #[test]
    fn test_edit_formatter_truncation() {
        let long_string = "a".repeat(100);
        let output = EditFormatter::format_success("/path/to/file", &long_string, "replacement");
        assert!(output.contains("..."));
    }

    #[test]
    fn test_multiedit_formatter() {
        let output = MultiEditFormatter::format_success("/path/to/file", 3, 5);
        assert!(output.contains("3 edit operation"));
        assert!(output.contains("5 total replacement"));
    }
}
