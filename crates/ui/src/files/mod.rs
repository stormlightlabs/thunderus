//! Workspace file browser, syntax highlighting, and fuzzy file finder.

mod highlight;
mod screen;
mod tree;

use crate::finder::FuzzyFinder;
use crate::scroll::ScrollState;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub(crate) use highlight::{HighlightSegment, HighlightedLine};
pub(crate) use screen::FileBrowser;

const MAX_FINDER_RESULTS: usize = 12;
const MAX_PROMPT_FILE_BYTES: usize = 16_384;
const DEFAULT_TREE_PAGE_SIZE: usize = 10;
const DEFAULT_CONTENT_PAGE_SIZE: usize = 20;

#[derive(Debug, Clone)]
pub(crate) struct FileBrowserApp {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileTreeRow {
    pub name: String,
    pub depth: u16,
    pub is_dir: bool,
    pub expanded: bool,
    pub selected: bool,
    pub active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileBrowserAction {
    None,
    Quit,
    ExitToChat,
}

impl Default for FileBrowserApp {
    fn default() -> Self {
        let workspace_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self::new(workspace_root)
    }
}

impl FileBrowserApp {
    pub(crate) fn new(workspace_root: PathBuf) -> Self {
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

    pub(crate) fn reload_workspace(&mut self) -> std::io::Result<()> {
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

    pub(crate) fn load_debug_fixture(&mut self) {
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

    pub(crate) fn handle_input(&mut self, key: KeyEvent) -> FileBrowserAction {
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

    pub(crate) fn sync_viewports(&mut self, tree_page_size: usize, content_page_size: usize) -> bool {
        let tree_page_size = tree_page_size.max(1);
        let content_page_size = content_page_size.max(1);
        let changed =
            self.tree_scroll.page_size != tree_page_size || self.content_scroll.page_size != content_page_size;

        if changed {
            self.tree_scroll.set_page_size(tree_page_size);
            self.content_scroll.set_page_size(content_page_size);
        }

        changed
    }

    pub(crate) fn workspace_title(&self) -> String {
        self.workspace_root.display().to_string()
    }

    pub(crate) fn tree_rows(&self) -> Vec<FileTreeRow> {
        self.visible_entries
            .iter()
            .enumerate()
            .skip(self.tree_scroll.offset)
            .take(self.tree_scroll.page_size.max(1))
            .map(|(idx, entry)| FileTreeRow {
                name: entry.name.clone(),
                depth: entry.depth,
                is_dir: entry.is_dir,
                expanded: entry.expanded,
                selected: idx == self.selected_index,
                active: self.active_file.as_deref() == Some(entry.path.as_path()),
            })
            .collect()
    }

    pub(crate) fn breadcrumb(&self) -> String {
        let root = self
            .workspace_root
            .file_name()
            .and_then(|name| name.to_str())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| self.workspace_root.display().to_string());

        let Some(active) = self.active_file.as_ref() else {
            return format!("{root} > (no file selected)");
        };

        let mut parts = vec![root];
        parts.extend(
            active
                .components()
                .map(|component| component.as_os_str().to_string_lossy().into_owned()),
        );

        parts.join(" > ")
    }

    pub(crate) fn content_rows(&self) -> Vec<HighlightedLine> {
        self.highlighted_lines
            .iter()
            .skip(self.content_scroll.offset)
            .take(self.content_scroll.page_size.max(1))
            .cloned()
            .collect()
    }

    pub(crate) fn content_line_number_width(&self) -> usize {
        self.highlighted_lines.len().to_string().len().max(3)
    }

    pub(crate) fn status_line(&self) -> &str {
        &self.status_line
    }

    pub(crate) fn is_finder_active(&self) -> bool {
        self.finder.active
    }

    pub(crate) fn finder_query(&self) -> &str {
        &self.finder.query
    }

    pub(crate) fn finder_rows(&self) -> Vec<(bool, String)> {
        self.finder
            .filtered_items()
            .enumerate()
            .map(|(idx, path)| (idx == self.finder.selected, path.display().to_string()))
            .collect()
    }
}

pub(crate) fn workspace_files(root: &Path) -> Vec<PathBuf> {
    tree::workspace_files(root)
}

pub(crate) fn read_file_for_prompt_result(root: &Path, relative_path: &Path) -> std::io::Result<String> {
    let absolute = root.join(relative_path);
    let mut content = std::fs::read_to_string(absolute)?;
    if content.len() > MAX_PROMPT_FILE_BYTES {
        content.truncate(MAX_PROMPT_FILE_BYTES);
    }
    Ok(content)
}
