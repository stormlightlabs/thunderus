//! Read history tracking for session state integration
//!
//! This module provides functionality to track file reads in session history,
//! enabling validation for edit operations (ensuring files are read before editing).

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Tracks which files have been read in the current session
///
/// This in-memory tracker provides fast lookups for edit validation.
/// It's backed by persistent session events for long-term storage.
#[derive(Debug, Clone)]
pub struct ReadHistory {
    /// Inner state with Arc for cheap cloning across tool executions
    inner: Arc<RwLock<ReadHistoryInner>>,
}

/// Inner state of read history
#[derive(Debug)]
struct ReadHistoryInner {
    /// Map of file path to (line_count, offset) tuples
    /// Only tracks successful reads
    reads: HashMap<String, (usize, usize)>,
}

impl ReadHistory {
    /// Creates a new empty read history
    pub fn new() -> Self {
        Self { inner: Arc::new(RwLock::new(ReadHistoryInner { reads: HashMap::new() })) }
    }

    /// Records a successful file read
    pub fn record_read(&self, file_path: &str, line_count: usize, offset: usize) {
        if let Ok(mut inner) = self.inner.write() {
            inner.reads.insert(file_path.to_string(), (line_count, offset));
        }
    }

    /// Records a failed file read attempt
    ///
    /// Failed reads are removed from history if they were previously read successfully.
    /// This ensures that only successful reads are tracked.
    pub fn record_failed_read(&self, file_path: &str) {
        if let Ok(mut inner) = self.inner.write() {
            inner.reads.remove(file_path);
        }
    }

    /// Checks if a file has been successfully read
    pub fn was_read(&self, file_path: &str) -> Option<(usize, usize)> {
        if let Ok(inner) = self.inner.read() { inner.reads.get(file_path).copied() } else { None }
    }

    /// Returns all files that have been successfully read
    pub fn read_files(&self) -> Vec<(String, usize, usize)> {
        if let Ok(inner) = self.inner.read() {
            inner
                .reads
                .iter()
                .map(|(path, &(line_count, offset))| (path.clone(), line_count, offset))
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Clears all read history
    ///
    /// Useful for testing or session resets
    pub fn clear(&self) {
        if let Ok(mut inner) = self.inner.write() {
            inner.reads.clear();
        }
    }

    /// Returns the number of files that have been read
    pub fn len(&self) -> usize {
        if let Ok(inner) = self.inner.read() { inner.reads.len() } else { 0 }
    }

    /// Returns true if no files have been read
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for ReadHistory {
    fn default() -> Self {
        Self::new()
    }
}

/// Validates that a file has been read before editing
///
/// This function is intended to be called by the Edit tool before performing edits.
/// It enforces the "read before edit" safety constraint.
pub fn validate_read_before_edit(history: &ReadHistory, file_path: &str) -> Result<(), String> {
    if history.was_read(file_path).is_some() {
        Ok(())
    } else {
        Err(format!(
            "Cannot edit file '{}': File must be read before editing. Use the Read tool to view the file content first.",
            file_path
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_history_new() {
        let history = ReadHistory::new();
        assert!(history.is_empty());
        assert_eq!(history.len(), 0);
    }

    #[test]
    fn test_record_read() {
        let history = ReadHistory::new();

        history.record_read("/path/to/file.txt", 100, 0);

        assert!(!history.is_empty());
        assert_eq!(history.len(), 1);
        assert_eq!(history.was_read("/path/to/file.txt"), Some((100, 0)));
    }

    #[test]
    fn test_record_multiple_reads() {
        let history = ReadHistory::new();

        history.record_read("/path/to/file1.txt", 100, 0);
        history.record_read("/path/to/file2.rs", 200, 10);
        history.record_read("/path/to/file3.md", 50, 0);

        assert_eq!(history.len(), 3);
        assert_eq!(history.was_read("/path/to/file1.txt"), Some((100, 0)));
        assert_eq!(history.was_read("/path/to/file2.rs"), Some((200, 10)));
        assert_eq!(history.was_read("/path/to/file3.md"), Some((50, 0)));
    }

    #[test]
    fn test_record_read_overwrites() {
        let history = ReadHistory::new();

        history.record_read("/path/to/file.txt", 100, 0);
        history.record_read("/path/to/file.txt", 200, 50);

        assert_eq!(history.len(), 1);
        assert_eq!(history.was_read("/path/to/file.txt"), Some((200, 50)));
    }

    #[test]
    fn test_record_failed_read() {
        let history = ReadHistory::new();

        history.record_read("/path/to/file.txt", 100, 0);
        assert_eq!(history.len(), 1);

        history.record_failed_read("/path/to/file.txt");
        assert_eq!(history.len(), 0);
        assert!(history.was_read("/path/to/file.txt").is_none());
    }

    #[test]
    fn test_was_read_nonexistent() {
        let history = ReadHistory::new();
        assert!(history.was_read("/nonexistent/file.txt").is_none());
    }

    #[test]
    fn test_read_files() {
        let history = ReadHistory::new();

        history.record_read("/path/to/file1.txt", 100, 0);
        history.record_read("/path/to/file2.rs", 200, 10);

        let files = history.read_files();
        assert_eq!(files.len(), 2);

        let mut files = files;
        files.sort_by(|a, b| a.0.cmp(&b.0));

        assert_eq!(files[0], ("/path/to/file1.txt".to_string(), 100, 0));
        assert_eq!(files[1], ("/path/to/file2.rs".to_string(), 200, 10));
    }

    #[test]
    fn test_clear() {
        let history = ReadHistory::new();

        history.record_read("/path/to/file.txt", 100, 0);
        assert_eq!(history.len(), 1);

        history.clear();
        assert!(history.is_empty());
        assert_eq!(history.len(), 0);
    }

    #[test]
    fn test_validate_read_before_edit_success() {
        let history = ReadHistory::new();
        history.record_read("/path/to/file.txt", 100, 0);

        let result = validate_read_before_edit(&history, "/path/to/file.txt");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_read_before_edit_failure() {
        let history = ReadHistory::new();

        let result = validate_read_before_edit(&history, "/path/to/file.txt");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must be read before editing"));
    }

    #[test]
    fn test_read_history_clone() {
        let history1 = ReadHistory::new();
        history1.record_read("/path/to/file.txt", 100, 0);

        let history2 = history1.clone();
        assert_eq!(history2.was_read("/path/to/file.txt"), Some((100, 0)));

        history2.record_read("/another/file.rs", 200, 0);
        assert_eq!(history1.was_read("/another/file.rs"), Some((200, 0)));
        assert_eq!(history1.len(), 2);
    }

    #[test]
    fn test_default() {
        let history: ReadHistory = Default::default();
        assert!(history.is_empty());
    }
}
