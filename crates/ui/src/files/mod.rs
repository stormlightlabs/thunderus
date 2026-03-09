//! Workspace file browser, syntax highlighting, and fuzzy file finder.

mod highlight;
mod render;
mod tree;

use super::layout::ConstraintSpec;
use super::screen::{Screen, ScreenAction};
use crate::finder::FuzzyFinder;
use crate::finder::fuzzy_match_items;
use crate::scroll::ScrollState;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Rect};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use thndrs_ui_macros::AreaSpec;

pub use highlight::{HighlightSegment, HighlightedLine};
pub use render::draw_file_browser_screen;

const MAX_FINDER_RESULTS: usize = 12;
const MAX_PROMPT_FILE_BYTES: usize = 16_384;
const DEFAULT_TREE_PAGE_SIZE: usize = 10;
const DEFAULT_CONTENT_PAGE_SIZE: usize = 20;

#[derive(Debug, Clone)]
pub struct FileBrowserApp {
    workspace_root: PathBuf,
    root: tree::FileNode,
    files: Vec<PathBuf>,
    expanded_dirs: HashSet<PathBuf>,
    visible_entries: Vec<tree::TreeEntry>,
    selected_index: usize,
    tree_scroll: ScrollState,
    content_scroll: ScrollState,
    active_file: Option<PathBuf>,
    highlighted_lines: Vec<HighlightedLine>,
    finder: FuzzyFinder<PathBuf>,
    status_line: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileBrowserAction {
    None,
    Quit,
    ExitToChat,
}

#[derive(AreaSpec)]
pub struct FileBrowserShell;

impl ConstraintSpec for FileBrowserShell {
    fn direction(&self) -> Direction {
        Direction::Vertical
    }

    fn constraints(&self, _area: Rect) -> Vec<Constraint> {
        vec![Constraint::Min(0), Constraint::Length(1), Constraint::Length(3)]
    }
}

impl Default for FileBrowserApp {
    fn default() -> Self {
        let workspace_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self::new(workspace_root)
    }
}

impl FileBrowserApp {
    pub fn new(workspace_root: PathBuf) -> Self {
        let root_name = workspace_root
            .file_name()
            .and_then(|name| name.to_str())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| workspace_root.display().to_string());

        let mut app = Self {
            workspace_root,
            root: tree::FileNode::root(root_name),
            files: Vec::new(),
            expanded_dirs: HashSet::new(),
            visible_entries: Vec::new(),
            selected_index: 0,
            tree_scroll: ScrollState::with_viewport(0, DEFAULT_TREE_PAGE_SIZE),
            content_scroll: ScrollState::with_viewport(0, DEFAULT_CONTENT_PAGE_SIZE),
            active_file: None,
            highlighted_lines: Vec::new(),
            finder: FuzzyFinder::default(),
            status_line: "Press @ to fuzzy-find files, Enter to open, Esc to return to chat.".to_string(),
        };

        if let Err(error) = app.reload_workspace() {
            app.status_line = format!("Failed to read workspace: {error}");
        }

        app
    }

    pub fn reload_workspace(&mut self) -> std::io::Result<()> {
        let (mut root, files) = tree::build_workspace_index(&self.workspace_root)?;
        root.sort_recursive();

        self.root = root;
        self.files = files;

        if self.expanded_dirs.is_empty() {
            for child in &self.root.children {
                if child.is_dir {
                    self.expanded_dirs.insert(child.path.clone());
                }
            }
        }

        self.rebuild_visible_entries();

        if self.active_file.is_none() {
            if let Some(path) = self.files.first().cloned() {
                self.open_file(&path);
            }
        } else if let Some(path) = self.active_file.clone()
            && !self.files.contains(&path)
        {
            self.active_file = None;
            self.highlighted_lines.clear();
            self.content_scroll.set_total(0);
        }

        Ok(())
    }

    pub fn load_debug_fixture(&mut self) {
        self.root = tree::build_debug_tree();
        self.files = tree::collect_files(&self.root);
        self.expanded_dirs.clear();
        for child in &self.root.children {
            self.expanded_dirs.insert(child.path.clone());
            if child.is_dir {
                for nested in &child.children {
                    if nested.is_dir {
                        self.expanded_dirs.insert(nested.path.clone());
                    }
                }
            }
        }

        self.rebuild_visible_entries();
        self.selected_index = 0;
        self.tree_scroll.set_offset(0);
        self.content_scroll.set_offset(0);
        self.finder.deactivate();
        self.status_line = "Debug fixture loaded. Use Up/Down to scroll the tree and content.".to_string();

        if let Some(path) = self.files.first().cloned() {
            self.open_file(&path);
        }
    }

    pub fn handle_input(&mut self, key: KeyEvent) -> FileBrowserAction {
        if key.kind != KeyEventKind::Press {
            return FileBrowserAction::None;
        }

        if self.finder.active {
            return self.handle_fuzzy_finder_input(key);
        }

        match key.code {
            KeyCode::Esc => FileBrowserAction::ExitToChat,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => FileBrowserAction::Quit,
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => FileBrowserAction::Quit,
            KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => FileBrowserAction::Quit,
            KeyCode::Char('@') => {
                self.finder.activate_with_items(self.files.clone());
                self.update_fuzzy_matches();
                FileBrowserAction::None
            }
            KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Err(error) = self.reload_workspace() {
                    self.status_line = format!("Workspace refresh failed: {error}");
                } else {
                    self.status_line = "Workspace refreshed".to_string();
                }
                FileBrowserAction::None
            }
            KeyCode::Up => {
                self.move_selection_up();
                FileBrowserAction::None
            }
            KeyCode::Down => {
                self.move_selection_down();
                FileBrowserAction::None
            }
            KeyCode::Left => {
                self.collapse_selected_or_parent();
                FileBrowserAction::None
            }
            KeyCode::Right => {
                self.expand_or_open_selected();
                FileBrowserAction::None
            }
            KeyCode::Enter => {
                self.expand_or_open_selected();
                FileBrowserAction::None
            }
            KeyCode::PageUp => {
                self.content_scroll.page_up();
                FileBrowserAction::None
            }
            KeyCode::PageDown => {
                self.content_scroll.page_down();
                FileBrowserAction::None
            }
            _ => FileBrowserAction::None,
        }
    }

    fn handle_fuzzy_finder_input(&mut self, key: KeyEvent) -> FileBrowserAction {
        match key.code {
            KeyCode::Esc => {
                self.finder.deactivate();
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.finder.deactivate();
                return FileBrowserAction::Quit;
            }
            KeyCode::Char(ch) => {
                self.finder.query.push(ch);
                self.update_fuzzy_matches();
            }
            KeyCode::Backspace => {
                self.finder.query.pop();
                self.update_fuzzy_matches();
            }
            KeyCode::Up => {
                self.finder.move_up();
            }
            KeyCode::Down => {
                self.finder.move_down();
            }
            KeyCode::Enter => {
                if let Some(path) = self.finder.selected_item().cloned() {
                    self.open_file(&path);
                    self.select_entry_by_path(&path);
                    self.expand_parents_for(&path);
                    self.rebuild_visible_entries();
                }
                self.finder.deactivate();
            }
            _ => {}
        }

        FileBrowserAction::None
    }

    fn update_fuzzy_matches(&mut self) {
        self.finder
            .refresh(|path| path.display().to_string(), MAX_FINDER_RESULTS);
    }

    fn move_selection_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }

        self.tree_scroll.ensure_visible(self.selected_index);
    }

    fn move_selection_down(&mut self) {
        if self.selected_index + 1 < self.visible_entries.len() {
            self.selected_index += 1;
        }

        self.tree_scroll.ensure_visible(self.selected_index);
    }

    fn collapse_selected_or_parent(&mut self) {
        let Some(entry) = self.visible_entries.get(self.selected_index).cloned() else {
            return;
        };

        if entry.is_dir && entry.expanded {
            self.expanded_dirs.remove(&entry.path);
            self.rebuild_visible_entries();
            return;
        }

        if let Some(parent) = entry.path.parent()
            && !parent.as_os_str().is_empty()
        {
            self.expanded_dirs.remove(parent);
            self.rebuild_visible_entries();
            self.select_entry_by_path(parent);
        }
    }

    fn expand_or_open_selected(&mut self) {
        let Some(entry) = self.visible_entries.get(self.selected_index).cloned() else {
            return;
        };

        if entry.is_dir {
            if entry.expanded {
                self.expanded_dirs.remove(&entry.path);
            } else {
                self.expanded_dirs.insert(entry.path);
            }
            self.rebuild_visible_entries();
            return;
        }

        self.open_file(&entry.path);
    }

    fn open_file(&mut self, relative_path: &Path) {
        let absolute_path = self.workspace_root.join(relative_path);
        let Ok(content) = std::fs::read_to_string(&absolute_path) else {
            self.status_line = format!("Could not read {}", relative_path.display());
            return;
        };

        self.active_file = Some(relative_path.to_path_buf());
        self.highlighted_lines = highlight::highlight_file(&absolute_path, &content);
        self.content_scroll.set_offset(0);
        self.content_scroll.set_total(self.highlighted_lines.len());
        self.status_line = format!("Opened {}", relative_path.display());
    }

    fn select_entry_by_path(&mut self, path: &Path) {
        if let Some((idx, _)) = self
            .visible_entries
            .iter()
            .enumerate()
            .find(|(_, entry)| entry.path == path)
        {
            self.selected_index = idx;
            self.tree_scroll.ensure_visible(self.selected_index);
        }
    }

    fn expand_parents_for(&mut self, path: &Path) {
        let mut cursor = path.parent();
        while let Some(parent) = cursor {
            if parent.as_os_str().is_empty() {
                break;
            }
            self.expanded_dirs.insert(parent.to_path_buf());
            cursor = parent.parent();
        }
    }

    fn rebuild_visible_entries(&mut self) {
        let mut entries = Vec::new();
        tree::flatten_tree_entries(&self.root, 0, &self.expanded_dirs, &mut entries);
        self.visible_entries = entries;
        self.selected_index = self.selected_index.min(self.visible_entries.len().saturating_sub(1));
        self.tree_scroll.set_total(self.visible_entries.len());
        self.tree_scroll.ensure_visible(self.selected_index);
    }
}

impl Screen for FileBrowserApp {
    fn handle_input(&mut self, key: KeyEvent) -> ScreenAction {
        match FileBrowserApp::handle_input(self, key) {
            FileBrowserAction::None => ScreenAction::None,
            FileBrowserAction::Quit => ScreenAction::Quit,
            FileBrowserAction::ExitToChat => ScreenAction::ReturnToPrevious,
        }
    }

    fn draw(&self, frame: &mut Frame) {
        draw_file_browser_screen(frame, self);
    }
}

pub fn workspace_files(root: &Path) -> Vec<PathBuf> {
    tree::workspace_files(root)
}

pub fn fuzzy_match_paths(query: &str, candidates: &[PathBuf], limit: usize) -> Vec<PathBuf> {
    fuzzy_match_items(query, candidates, limit, |path| path.display().to_string())
        .into_iter()
        .filter_map(|(idx, _)| candidates.get(idx).cloned())
        .collect()
}

pub fn read_file_for_prompt(root: &Path, relative_path: &Path) -> Option<String> {
    read_file_for_prompt_result(root, relative_path).ok()
}

pub fn read_file_for_prompt_result(root: &Path, relative_path: &Path) -> std::io::Result<String> {
    let absolute = root.join(relative_path);
    let mut content = std::fs::read_to_string(absolute)?;
    if content.len() > MAX_PROMPT_FILE_BYTES {
        content.truncate(MAX_PROMPT_FILE_BYTES);
    }
    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fuzzy_match_paths_returns_expected_matches() {
        let candidates = vec![
            PathBuf::from("src/main.rs"),
            PathBuf::from("src/lib.rs"),
            PathBuf::from("README.md"),
        ];

        let results = fuzzy_match_paths("main", &candidates, 10);
        assert!(results.iter().any(|path| path == Path::new("src/main.rs")));
    }
}
