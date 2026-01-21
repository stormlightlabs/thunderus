//! Context file loader for CLAUDE.md compatibility
//!
//! Automatically discovers and loads context files (CLAUDE.md, AGENTS.md, etc.)
//! from the repository root and current working directory.

use crate::error::Result;
use crate::session::Session;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Context file sources with their priority (lower = higher priority)
pub const CONTEXT_FILES: &[(&str, u8)] = &[
    ("CLAUDE.md", 1),
    ("AGENTS.md", 2),
    ("GEMINI.md", 3),
    ("THUNDERUS.md", 4),
    ("THNDRS.md", 5),
    ("QWEN.md", 6),
];

/// User-specific context files (overlay, gitignored)
pub const LOCAL_CONTEXT_PATTERN: &str = "*.local.md";

/// A loaded context file with metadata
#[derive(Debug, Clone)]
pub struct LoadedContext {
    /// Source filename (e.g., "CLAUDE.md")
    pub source: String,
    /// Absolute path to the file
    pub path: PathBuf,
    /// File content
    pub content: String,
    /// Content hash for deduplication
    pub content_hash: String,
    /// Priority (lower = higher priority)
    pub priority: u8,
}

impl LoadedContext {
    /// Create a new LoadedContext from a file path
    pub fn from_path(path: PathBuf, priority: u8) -> Result<Self> {
        let source = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        let content = fs::read_to_string(&path)?;
        let content_hash = Self::compute_hash(&content);

        Ok(Self { source, path, content, content_hash, priority })
    }

    /// Compute a simple hash of the content for deduplication
    fn compute_hash(content: &str) -> String {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        content.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }
}

/// Context loader that discovers and loads context files
#[derive(Debug, Clone)]
pub struct ContextLoader {
    /// Loaded context files, keyed by source name
    loaded: HashMap<String, LoadedContext>,
    /// Current working directory
    cwd: PathBuf,
    /// Git repository root (if found)
    git_root: Option<PathBuf>,
}

impl ContextLoader {
    /// Create a new context loader
    pub fn new(cwd: PathBuf) -> Self {
        let git_root = Self::find_git_root(&cwd);
        Self { loaded: HashMap::new(), cwd, git_root }
    }

    /// Scan for and load all context files
    ///
    /// Scans in priority order:
    /// 1. Repository root
    /// 2. Current working directory
    /// 3. Parent directories (up to git root)
    pub fn load_all(&mut self) -> Vec<LoadedContext> {
        self.loaded.clear();

        if let Some(root) = self.git_root.clone() {
            self.scan_directory(&root);
        }

        self.scan_directory(&self.cwd.clone());
        self.scan_local_files();
        let mut contexts: Vec<LoadedContext> = self.loaded.values().cloned().collect();
        contexts.sort_by_key(|c| c.priority);
        contexts
    }

    /// Get merged context content
    ///
    /// Higher priority files override conflicting content from lower priority files.
    pub fn merged_content(&self) -> String {
        let mut merged = String::new();
        let mut contexts: Vec<_> = self.loaded.values().cloned().collect();
        contexts.sort_by_key(|c| c.priority);

        for ctx in contexts {
            merged.push_str("<!-- ");
            merged.push_str(&ctx.source);
            merged.push_str(" from ");
            merged.push_str(ctx.path.display().to_string().as_str());
            merged.push_str(" -->\n\n");
            merged.push_str(&ctx.content);
            merged.push_str("\n\n");
        }

        merged
    }

    /// Get loaded context by source name
    pub fn get(&self, source: &str) -> Option<&LoadedContext> {
        self.loaded.get(source)
    }

    /// Get all loaded contexts
    pub fn all(&self) -> Vec<&LoadedContext> {
        let mut contexts: Vec<_> = self.loaded.values().collect();
        contexts.sort_by_key(|c| c.priority);
        contexts
    }

    /// Load all contexts and append events to session
    pub fn append_to_session(&mut self, session: &mut Session) -> Result<usize> {
        let contexts = self.load_all();
        let mut count = 0;

        for ctx in contexts {
            session.append_context_load(&ctx.source, ctx.path.to_string_lossy(), &ctx.content_hash)?;
            count += 1;
        }

        Ok(count)
    }

    /// Scan a single directory for context files
    fn scan_directory(&mut self, dir: &Path) {
        for &(filename, priority) in CONTEXT_FILES {
            let path = dir.join(filename);
            if path.exists()
                && !self.loaded.contains_key(filename)
                && let Ok(ctx) = LoadedContext::from_path(path.clone(), priority)
            {
                self.loaded.insert(filename.to_string(), ctx);
            }
        }
    }

    /// Scan for local overlay files (*.local.md)
    fn scan_local_files(&mut self) {
        let mut dirs = vec![self.cwd.clone()];
        if let Some(ref root) = self.git_root
            && root != &self.cwd
        {
            dirs.push(root.clone());
        }

        for dir in dirs {
            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if let Some(name) = path.file_name() {
                        let name_str = name.to_string_lossy();
                        if name_str.ends_with(".local.md") {
                            let priority = u8::MAX;
                            if let Ok(ctx) = LoadedContext::from_path(path.clone(), priority) {
                                self.loaded.insert(name_str.to_string(), ctx);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Find the git repository root by traversing upward
    fn find_git_root(start: &Path) -> Option<PathBuf> {
        let mut current = start.canonicalize().ok()?;

        loop {
            let git_dir = current.join(".git");
            if git_dir.exists() {
                return Some(current);
            }

            if !current.pop() {
                return None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::AgentDir;
    use tempfile::TempDir;

    #[test]
    fn test_loaded_context_from_path() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("TEST.md");
        fs::write(&file_path, "# Test content\n\nSome text here").unwrap();

        let ctx = LoadedContext::from_path(file_path.clone(), 1).unwrap();

        assert_eq!(ctx.source, "TEST.md");
        assert_eq!(ctx.path, file_path);
        assert!(ctx.content.contains("Test content"));
        assert!(!ctx.content_hash.is_empty());
        assert_eq!(ctx.priority, 1);
    }

    #[test]
    fn test_loaded_context_compute_hash() {
        let hash1 = LoadedContext::compute_hash("same content");
        let hash2 = LoadedContext::compute_hash("same content");
        let hash3 = LoadedContext::compute_hash("different content");

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_context_loader_new() {
        let temp = TempDir::new().unwrap();
        let loader = ContextLoader::new(temp.path().to_path_buf());
        assert!(loader.loaded.is_empty());
        assert_eq!(loader.cwd, temp.path());
        assert!(loader.git_root.is_none());
    }

    #[test]
    fn test_context_loader_with_git_repo() {
        let temp = TempDir::new().unwrap();
        let git_dir = temp.path().join(".git");
        fs::create_dir(&git_dir).unwrap();

        let loader = ContextLoader::new(temp.path().to_path_buf());
        let canonical_temp = temp.path().canonicalize().unwrap();
        assert_eq!(loader.git_root, Some(canonical_temp));
    }

    #[test]
    fn test_find_git_root() {
        let temp = TempDir::new().unwrap();
        assert!(ContextLoader::find_git_root(temp.path()).is_none());
        fs::create_dir(temp.path().join(".git")).unwrap();

        let canonical_temp = temp.path().canonicalize().unwrap();
        assert_eq!(ContextLoader::find_git_root(temp.path()), Some(canonical_temp.clone()));

        let subdir = temp.path().join("subdir");
        fs::create_dir(&subdir).unwrap();
        assert_eq!(ContextLoader::find_git_root(&subdir), Some(canonical_temp));
    }

    #[test]
    fn test_context_loader_scan_directory() {
        let temp = TempDir::new().unwrap();
        let claude_path = temp.path().join("CLAUDE.md");
        fs::write(&claude_path, "# Claude Context").unwrap();

        let agents_path = temp.path().join("AGENTS.md");
        fs::write(&agents_path, "# Agents Context").unwrap();

        let mut loader = ContextLoader::new(temp.path().to_path_buf());
        loader.scan_directory(temp.path());

        assert!(loader.loaded.contains_key("CLAUDE.md"));
        assert!(loader.loaded.contains_key("AGENTS.md"));

        let claude = loader.get("CLAUDE.md").unwrap();
        assert_eq!(claude.priority, 1);
        assert!(claude.content.contains("Claude Context"));

        let agents = loader.get("AGENTS.md").unwrap();
        assert_eq!(agents.priority, 2);
    }

    #[test]
    fn test_context_loader_scan_local_files() {
        let temp = TempDir::new().unwrap();
        let local_path = temp.path().join("CLAUDE.local.md");
        fs::write(&local_path, "# Local override").unwrap();

        let mut loader = ContextLoader::new(temp.path().to_path_buf());
        loader.scan_local_files();
        assert!(loader.loaded.contains_key("CLAUDE.local.md"));

        let local = loader.get("CLAUDE.local.md").unwrap();
        assert_eq!(local.priority, u8::MAX);
        assert!(local.content.contains("Local override"));
    }

    #[test]
    fn test_context_loader_load_all() {
        let temp = TempDir::new().unwrap();
        fs::create_dir(temp.path().join(".git")).unwrap();
        fs::write(temp.path().join("CLAUDE.md"), "# Root Claude").unwrap();

        let subdir = temp.path().join("subdir");
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join("AGENTS.md"), "# Subdir Agents").unwrap();

        let mut loader = ContextLoader::new(subdir.clone());
        let contexts = loader.load_all();

        assert!(!contexts.is_empty());
        assert_eq!(loader.get("CLAUDE.md").unwrap().priority, 1);
        assert_eq!(loader.get("AGENTS.md").unwrap().priority, 2);
    }

    #[test]
    fn test_context_loader_merged_content() {
        let temp = TempDir::new().unwrap();

        fs::write(temp.path().join("CLAUDE.md"), "# Claude\n\nContent 1").unwrap();
        fs::write(temp.path().join("AGENTS.md"), "# Agents\n\nContent 2").unwrap();

        let mut loader = ContextLoader::new(temp.path().to_path_buf());
        loader.load_all();

        let merged = loader.merged_content();

        assert!(merged.contains("<!-- CLAUDE.md"));
        assert!(merged.contains("Content 1"));
        assert!(merged.contains("<!-- AGENTS.md"));
        assert!(merged.contains("Content 2"));
    }

    #[test]
    fn test_context_loader_all_sorted() {
        let temp = TempDir::new().unwrap();

        fs::write(temp.path().join("QWEN.md"), "# Qwen").unwrap();
        fs::write(temp.path().join("CLAUDE.md"), "# Claude").unwrap();
        fs::write(temp.path().join("GEMINI.md"), "# Gemini").unwrap();

        let mut loader = ContextLoader::new(temp.path().to_path_buf());
        loader.load_all();

        let all = loader.all();

        assert_eq!(all[0].source, "CLAUDE.md");
        assert_eq!(all[0].priority, 1);
        assert_eq!(all[1].source, "GEMINI.md");
        assert_eq!(all[1].priority, 3);
        assert_eq!(all[2].source, "QWEN.md");
        assert_eq!(all[2].priority, 6);
    }

    #[test]
    fn test_context_loader_append_to_session() {
        let temp = TempDir::new().unwrap();
        let agent_dir = AgentDir::new(temp.path());

        fs::write(temp.path().join("CLAUDE.md"), "# Claude Context").unwrap();

        let mut session = Session::new(agent_dir).unwrap();
        let mut loader = ContextLoader::new(temp.path().to_path_buf());

        let count = loader.append_to_session(&mut session).unwrap();

        assert_eq!(count, 1);

        let events = session.read_events().unwrap();
        assert_eq!(events.len(), 1);

        if let crate::session::Event::ContextLoad { source, path, content_hash } = &events[0].event {
            assert_eq!(source, "CLAUDE.md");
            assert!(path.contains("CLAUDE.md"));
            assert!(!content_hash.is_empty());
        } else {
            panic!("Expected ContextLoad event");
        }
    }

    #[test]
    fn test_context_loader_no_duplicate_sources() {
        let temp = TempDir::new().unwrap();

        fs::create_dir(temp.path().join(".git")).unwrap();
        fs::write(temp.path().join("CLAUDE.md"), "# Root Claude").unwrap();

        let subdir = temp.path().join("subdir");
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join("CLAUDE.md"), "# Subdir Claude").unwrap();

        let mut loader = ContextLoader::new(subdir.clone());
        loader.load_all();

        assert_eq!(loader.loaded.keys().filter(|k| k.contains("CLAUDE")).count(), 1);
    }

    #[test]
    fn test_context_files_constant() {
        assert_eq!(CONTEXT_FILES[0].0, "CLAUDE.md");
        assert_eq!(CONTEXT_FILES[0].1, 1);
    }

    #[test]
    fn test_local_context_pattern() {
        assert!(LOCAL_CONTEXT_PATTERN.contains("*.local.md"));
    }
}
