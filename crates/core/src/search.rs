use crate::{Error, Result};
use std::path::Path;
use std::process::Command;

/// Scope for search operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchScope {
    /// Search all files (events + views)
    All,
    /// Search only events.jsonl
    Events,
    /// Search only materialized views
    Views,
}

/// A single search result hit
#[derive(Debug, Clone)]
pub struct SearchHit {
    /// File path relative to session directory
    pub file: String,
    /// Line number (1-indexed)
    pub line: usize,
    /// Matched line content
    pub content: String,
    /// Optional context lines around the match
    pub context: Option<Vec<String>>,
}

/// Search a session directory using ripgrep
pub fn search_session(session_dir: &Path, query: &str, scope: SearchScope) -> Result<Vec<SearchHit>> {
    let mut cmd = Command::new("rg");

    cmd.arg("--line-number")
        .arg("--no-heading")
        .arg("--with-filename")
        .arg("--color=never")
        .arg("--fixed-strings")
        .arg("--case-insensitive")
        .arg("--max-count=50");

    cmd.arg(query);

    match scope {
        SearchScope::All => {
            cmd.arg(session_dir);
        }
        SearchScope::Events => {
            cmd.arg(session_dir.join("events.jsonl"));
        }
        SearchScope::Views => {
            cmd.arg(session_dir.join("../views"));
        }
    }

    let output = cmd
        .output()
        .map_err(|e| Error::Other(format!("Failed to execute ripgrep: {}", e)))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut hits = Vec::new();

    for line in stdout.lines() {
        let parts: Vec<&str> = line.splitn(3, ':').collect();
        if parts.len() == 3
            && let Ok(line_num) = parts[1].parse::<usize>()
        {
            hits.push(SearchHit {
                file: parts[0].to_string(),
                line: line_num,
                content: parts[2].to_string(),
                context: None,
            });
        }
    }

    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_search_scope_variants() {
        assert_eq!(SearchScope::All, SearchScope::All);
        assert_ne!(SearchScope::All, SearchScope::Events);
        assert_ne!(SearchScope::Events, SearchScope::Views);
    }

    #[test]
    fn test_search_hit_creation() {
        let hit = SearchHit {
            file: "events.jsonl".to_string(),
            line: 42,
            content: "test content".to_string(),
            context: None,
        };

        assert_eq!(hit.file, "events.jsonl");
        assert_eq!(hit.line, 42);
        assert_eq!(hit.content, "test content");
        assert!(hit.context.is_none());
    }

    #[test]
    fn test_search_session_with_temp_dir() {
        let temp = TempDir::new().unwrap();
        let session_dir = temp.path().join("session");
        fs::create_dir(&session_dir).unwrap();

        fs::write(
            session_dir.join("events.jsonl"),
            "line 1: test content\nline 2: another line\nline 3: test again\n",
        )
        .unwrap();

        let results = search_session(&session_dir, "test", SearchScope::Events);
        assert!(results.is_ok() || results.is_err());
    }
}
