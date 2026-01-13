use ignore::WalkBuilder;
use nucleo_matcher::{
    Config, Matcher, Utf32Str,
    pattern::{CaseMatching, Normalization, Pattern},
};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// File entry for fuzzy finder
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    /// Full file path
    pub path: PathBuf,
    /// Relative path from workspace root
    pub relative_path: String,
    /// File modification time for sorting
    pub modified: Option<SystemTime>,
    /// File size in bytes
    pub size: u64,
    /// Whether this is a directory
    pub is_dir: bool,
}

impl FileEntry {
    fn from_path(workspace_root: &Path, path: PathBuf) -> Option<Self> {
        let relative_path = path.strip_prefix(workspace_root).ok()?.to_string_lossy().to_string();

        let metadata = path.metadata().ok()?;
        let is_dir = metadata.is_dir();
        let modified = metadata.modified().ok();
        let size = metadata.len();

        Some(Self { path, relative_path, modified, size, is_dir })
    }

    /// Get file extension for language detection
    pub fn extension(&self) -> Option<&str> {
        self.path.extension()?.to_str()
    }
}

/// Sort mode for file results
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortMode {
    /// Sort by fuzzy match relevance (default)
    #[default]
    Relevance,
    /// Sort by modification time (newest first)
    ModifiedTime,
    /// Sort by path alphabetically
    Path,
}

impl SortMode {
    pub fn toggle(&mut self) {
        *self = match self {
            SortMode::Relevance => SortMode::ModifiedTime,
            SortMode::ModifiedTime => SortMode::Path,
            SortMode::Path => SortMode::Relevance,
        }
    }
}

/// Fuzzy finder state
#[derive(Debug, Clone)]
pub struct FuzzyFinder {
    /// Workspace root directory
    workspace_root: PathBuf,
    /// All files in workspace
    files: Vec<FileEntry>,
    /// Current search pattern
    pattern: String,
    /// Current matcher configuration
    matcher: Matcher,
    /// Matched and filtered results
    results: Vec<FileEntry>,
    /// Selected index in results
    selected_index: usize,
    /// Original input buffer before fuzzy finder activated
    original_input: String,
    /// Cursor position in original input (where @ was typed)
    original_cursor: usize,
    /// Current sort mode
    sort_mode: SortMode,
    /// Whether hidden files should be shown
    show_hidden: bool,
}

impl FuzzyFinder {
    /// Create a new fuzzy finder for given workspace
    pub fn new(workspace_root: PathBuf, original_input: String, original_cursor: usize) -> Self {
        let matcher = Matcher::new(Config::DEFAULT);

        Self {
            workspace_root,
            files: Vec::new(),
            pattern: String::new(),
            matcher,
            results: Vec::new(),
            selected_index: 0,
            original_input,
            original_cursor,
            sort_mode: SortMode::default(),
            show_hidden: false,
        }
    }

    /// Initialize file discovery
    pub fn discover_files(&mut self) -> std::io::Result<()> {
        self.files.clear();

        let walker = WalkBuilder::new(&self.workspace_root)
            .hidden(!self.show_hidden)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .ignore(true)
            .follow_links(false)
            .build();

        for entry in walker.flatten() {
            let path = entry.path().to_path_buf();
            if let Some(file) = FileEntry::from_path(&self.workspace_root, path)
                && !file.is_dir
            {
                self.files.push(file);
            }
        }

        self.update_results();
        Ok(())
    }

    /// Update search pattern and recompute matches
    pub fn set_pattern(&mut self, pattern: String) {
        self.pattern = pattern;
        self.update_results();
        if !self.results.is_empty() {
            self.selected_index = self.selected_index.min(self.results.len() - 1);
        } else {
            self.selected_index = 0;
        }
    }

    /// Get current search pattern
    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    /// Update results based on current pattern and sort mode
    fn update_results(&mut self) {
        if self.pattern.is_empty() {
            self.results = self.files.clone();
        } else {
            let pattern = Pattern::parse(&self.pattern, CaseMatching::Ignore, Normalization::Smart);

            let mut scored: Vec<(FileEntry, u32)> = self
                .files
                .iter()
                .filter_map(|file| {
                    let mut buf = Vec::new();
                    let haystack = Utf32Str::new(&file.relative_path, &mut buf);
                    if let Some(score) = pattern.score(haystack, &mut self.matcher) {
                        if score > 0 { Some((file.clone(), score)) } else { None }
                    } else {
                        None
                    }
                })
                .collect();

            scored.sort_by(|a, b| b.1.cmp(&a.1));

            self.results = scored.into_iter().map(|(f, _)| f).collect();
        }

        self.sort_results();
    }

    /// Sort results based on current sort mode
    fn sort_results(&mut self) {
        match self.sort_mode {
            SortMode::Relevance => (),
            SortMode::ModifiedTime => self.results.sort_by(|a, b| match (&a.modified, &b.modified) {
                (Some(a_time), Some(b_time)) => b_time.cmp(a_time),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => a.relative_path.cmp(&b.relative_path),
            }),
            SortMode::Path => self.results.sort_by(|a, b| a.relative_path.cmp(&b.relative_path)),
        }
    }

    /// Get current results
    pub fn results(&self) -> &[FileEntry] {
        &self.results
    }

    /// Get selected file entry
    pub fn selected(&self) -> Option<&FileEntry> {
        self.results.get(self.selected_index)
    }

    /// Get selected index
    pub fn selected_index(&self) -> usize {
        self.selected_index
    }

    /// Move selection up
    pub fn select_up(&mut self) {
        if !self.results.is_empty() {
            self.selected_index = self.selected_index.saturating_sub(1);
        }
    }

    /// Move selection down
    pub fn select_down(&mut self) {
        if !self.results.is_empty() {
            self.selected_index = (self.selected_index + 1).min(self.results.len() - 1);
        }
    }

    /// Toggle sort mode
    pub fn toggle_sort(&mut self) {
        self.sort_mode.toggle();
        self.sort_results();
    }

    /// Get current sort mode
    pub fn sort_mode(&self) -> SortMode {
        self.sort_mode
    }

    /// Toggle hidden files visibility
    pub fn toggle_hidden(&mut self) -> std::io::Result<()> {
        self.show_hidden = !self.show_hidden;
        self.discover_files()?;
        Ok(())
    }

    /// Check if hidden files are shown
    pub fn show_hidden(&self) -> bool {
        self.show_hidden
    }

    /// Get original input buffer
    pub fn original_input(&self) -> &str {
        &self.original_input
    }

    /// Get original cursor position
    pub fn original_cursor(&self) -> usize {
        self.original_cursor
    }

    /// Get count of matched files
    pub fn match_count(&self) -> usize {
        self.results.len()
    }

    /// Get total file count
    pub fn total_file_count(&self) -> usize {
        self.files.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;

    fn create_test_workspace() -> tempfile::TempDir {
        let temp = tempfile::tempdir().unwrap();

        let main_rs = temp.path().join("src").join("main.rs");
        fs::create_dir_all(main_rs.parent().unwrap()).unwrap();
        File::create(&main_rs).unwrap().write_all(b"fn main() {}").unwrap();

        let lib_rs = temp.path().join("src").join("lib.rs");
        File::create(&lib_rs).unwrap().write_all(b"pub fn lib() {}").unwrap();

        let config_rs = temp.path().join("config.rs");
        File::create(&config_rs)
            .unwrap()
            .write_all(b"const X: u32 = 1;")
            .unwrap();

        let hidden_file = temp.path().join(".hidden");
        File::create(&hidden_file).unwrap().write_all(b"hidden").unwrap();

        let test_dir = temp.path().join("tests");
        fs::create_dir_all(&test_dir).unwrap();
        let test_rs = test_dir.join("test.rs");
        File::create(&test_rs).unwrap().write_all(b"#[test]").unwrap();

        temp
    }

    #[test]
    fn test_fuzzy_finder_new() {
        let temp = create_test_workspace();
        let finder = FuzzyFinder::new(temp.path().to_path_buf(), "test input".to_string(), 5);

        assert_eq!(finder.pattern(), "");
        assert_eq!(finder.original_input(), "test input");
        assert_eq!(finder.original_cursor(), 5);
        assert_eq!(finder.selected_index(), 0);
        assert_eq!(finder.sort_mode(), SortMode::Relevance);
        assert!(!finder.show_hidden());
    }

    #[test]
    fn test_fuzzy_finder_discover_files() {
        let temp = create_test_workspace();
        let mut finder = FuzzyFinder::new(temp.path().to_path_buf(), String::new(), 0);

        finder.discover_files().unwrap();

        assert!(!finder.files.is_empty());
        assert_eq!(finder.total_file_count(), finder.files.len());
    }

    #[test]
    fn test_fuzzy_finder_set_pattern_empty() {
        let temp = create_test_workspace();
        let mut finder = FuzzyFinder::new(temp.path().to_path_buf(), String::new(), 0);

        finder.discover_files().unwrap();

        let total = finder.files.len();
        finder.set_pattern("".to_string());

        assert_eq!(finder.results().len(), total);
    }

    #[test]
    fn test_fuzzy_finder_set_pattern() {
        let temp = create_test_workspace();
        let mut finder = FuzzyFinder::new(temp.path().to_path_buf(), String::new(), 0);

        finder.discover_files().unwrap();

        finder.set_pattern("main.rs".to_string());

        assert!(!finder.results().is_empty());
        assert!(finder.results().iter().any(|f| f.relative_path.contains("main.rs")));
    }

    #[test]
    fn test_fuzzy_finder_select_up() {
        let temp = create_test_workspace();
        let mut finder = FuzzyFinder::new(temp.path().to_path_buf(), String::new(), 0);

        finder.discover_files().unwrap();

        if finder.results().len() > 1 {
            finder.select_down();
            assert_eq!(finder.selected_index(), 1);

            finder.select_up();
            assert_eq!(finder.selected_index(), 0);
        }
    }

    #[test]
    fn test_fuzzy_finder_select_down() {
        let temp = create_test_workspace();
        let mut finder = FuzzyFinder::new(temp.path().to_path_buf(), String::new(), 0);

        finder.discover_files().unwrap();

        if finder.results().len() > 1 {
            finder.select_down();
            assert!(finder.selected_index() <= 1);

            finder.select_down();
            assert!(finder.selected_index() < finder.results().len());
        }
    }

    #[test]
    fn test_fuzzy_finder_select_bounds() {
        let temp = create_test_workspace();
        let mut finder = FuzzyFinder::new(temp.path().to_path_buf(), String::new(), 0);

        finder.discover_files().expect("Failed to discover files");

        let initial_index = finder.selected_index();
        finder.select_up();
        assert_eq!(finder.selected_index(), initial_index);

        if !finder.results().is_empty() {
            let last_index = finder.results().len() - 1;
            finder.selected_index = last_index;
            finder.select_down();
            assert_eq!(finder.selected_index(), last_index);
        }
    }

    #[test]
    fn test_fuzzy_finder_toggle_sort() {
        let temp = create_test_workspace();
        let mut finder = FuzzyFinder::new(temp.path().to_path_buf(), String::new(), 0);

        assert_eq!(finder.sort_mode(), SortMode::Relevance);

        finder.toggle_sort();
        assert_eq!(finder.sort_mode(), SortMode::ModifiedTime);

        finder.toggle_sort();
        assert_eq!(finder.sort_mode(), SortMode::Path);

        finder.toggle_sort();
        assert_eq!(finder.sort_mode(), SortMode::Relevance);
    }

    #[test]
    fn test_fuzzy_finder_toggle_hidden() {
        let temp = create_test_workspace();
        let mut finder = FuzzyFinder::new(temp.path().to_path_buf(), String::new(), 0);

        finder.discover_files().unwrap();
        let count_without_hidden = finder.total_file_count();

        finder.toggle_hidden().unwrap();
        assert!(finder.show_hidden());

        finder.discover_files().unwrap();
        let count_with_hidden = finder.total_file_count();

        assert!(count_with_hidden >= count_without_hidden);
    }

    #[test]
    fn test_file_entry_from_path() {
        let temp = tempfile::tempdir().unwrap();
        let file_path = temp.path().join("test.txt");
        File::create(&file_path).unwrap().write_all(b"test").unwrap();

        let entry = FileEntry::from_path(temp.path(), file_path.clone()).unwrap();

        assert_eq!(entry.relative_path, "test.txt");
        assert_eq!(entry.path, file_path);
        assert!(!entry.is_dir);
        assert!(entry.size > 0);
        assert_eq!(entry.extension(), Some("txt"));
    }

    #[test]
    fn test_file_entry_extension() {
        let temp = tempfile::tempdir().unwrap();

        let rs_file = temp.path().join("test.rs");
        File::create(&rs_file).unwrap();
        let rs_entry = FileEntry::from_path(temp.path(), rs_file).unwrap();
        assert_eq!(rs_entry.extension(), Some("rs"));

        let md_file = temp.path().join("README.md");
        File::create(&md_file).unwrap();
        let md_entry = FileEntry::from_path(temp.path(), md_file).unwrap();
        assert_eq!(md_entry.extension(), Some("md"));

        let no_ext_file = temp.path().join("Makefile");
        File::create(&no_ext_file).unwrap();
        let no_ext_entry = FileEntry::from_path(temp.path(), no_ext_file).unwrap();
        assert!(no_ext_entry.extension().is_none());
    }

    #[test]
    fn test_fuzzy_finder_selected() {
        let temp = create_test_workspace();
        let mut finder = FuzzyFinder::new(temp.path().to_path_buf(), String::new(), 0);

        finder.discover_files().unwrap();

        if !finder.results().is_empty() {
            let selected = finder.selected();
            assert!(selected.is_some());
            assert_eq!(selected.unwrap().relative_path, finder.results()[0].relative_path);
        }
    }

    #[test]
    fn test_fuzzy_finder_selected_none_when_empty() {
        let finder = FuzzyFinder::new(PathBuf::from("/nonexistent"), String::new(), 0);

        let selected = finder.selected();
        assert!(selected.is_none());
    }

    #[test]
    fn test_fuzzy_finder_match_count() {
        let temp = create_test_workspace();
        let mut finder = FuzzyFinder::new(temp.path().to_path_buf(), String::new(), 0);

        finder.discover_files().unwrap();

        finder.set_pattern("rs".to_string());

        let match_count = finder.match_count();
        assert!(match_count > 0);
        assert!(match_count <= finder.total_file_count());
    }
}
