use std::path::Path;

use crate::{tools::ToolOutput, utils};

/// Read a range of lines from a file, implemented in Rust.
///
/// Lines are 1-indexed. `start_line` is inclusive, `end_line` is inclusive
/// (defaults to `start_line + 20`). Enforces workspace-root containment and
/// line-length caps.
pub fn exec(path: &Path, root: &Path, start_line: u32, end_line: Option<u32>) -> ToolOutput {
    if !super::path::is_within_root(path, root) {
        return ToolOutput::failed(
            "read_file_range",
            format!("path escapes workspace root: {}", path.display()),
        );
    }

    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            return ToolOutput::failed("read_file_range", format!("read failed: {e}"));
        }
    };

    let start = start_line.max(1) as usize;
    let end = end_line.map(|e| e.max(start as u32) as usize).unwrap_or(start + 20);

    let lines: Vec<String> = content
        .lines()
        .enumerate()
        .skip(start.saturating_sub(1))
        .take(end.saturating_sub(start) + 1)
        .map(|(i, line)| format!("{}: {}", i + 1, utils::truncate_line(line)))
        .collect();

    if lines.is_empty() {
        return ToolOutput::failed("read_file_range", format!("no lines in range {start}-{end}"));
    }

    ToolOutput::ok("read_file_range", lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ToolStatus;

    #[test]
    fn read_file_range_reads_specific_lines() {
        let root = std::env::current_dir().unwrap();
        let path = root.join("Cargo.toml");
        let output = exec(&path, &root, 1, Some(3));
        assert_eq!(output.status, ToolStatus::Ok);
        assert_eq!(output.output.len(), 3);
        assert!(output.output[0].starts_with("1:"));
    }

    #[test]
    fn read_file_range_outside_root_fails() {
        let root = std::env::current_dir().unwrap();
        let outside = root.parent().unwrap().join("some_file.txt");
        let output = exec(&outside, &root, 1, Some(10));
        assert_eq!(output.status, ToolStatus::Failed);
        assert!(
            output
                .error
                .as_ref()
                .is_some_and(|e| e.contains("escapes workspace root"))
        );
    }

    #[test]
    fn read_file_range_nonexistent_file_fails() {
        let root = std::env::current_dir().unwrap();
        let path = root.join("nonexistent_file_zzz.rs");
        let output = exec(&path, &root, 1, Some(10));
        assert_eq!(output.status, ToolStatus::Failed);
    }
}
