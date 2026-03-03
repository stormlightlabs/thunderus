//! Workspace file browser, syntax highlighting, and fuzzy file finder.

use super::{
    colors,
    components::{HintFooter, HintToken, TopBorderedInputRow},
    layout::{ConstraintSpec, split as split_rects},
};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ignore::WalkBuilder;
use nucleo_matcher::{
    Config, Matcher, Utf32Str,
    pattern::{CaseMatching, Normalization, Pattern},
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};
use syntect::{
    easy::HighlightLines,
    highlighting::{Theme, ThemeSet},
    parsing::SyntaxSet,
};
use thndrs_ui_macros::AreaSpec;

const MAX_FINDER_RESULTS: usize = 12;
const MAX_FILE_LINES: usize = 2_000;
const MAX_PROMPT_FILE_BYTES: usize = 16_384;

#[derive(Debug, Clone)]
struct FileNode {
    name: String,
    path: PathBuf,
    is_dir: bool,
    children: Vec<FileNode>,
}

impl FileNode {
    fn root(name: String) -> Self {
        Self { name, path: PathBuf::new(), is_dir: true, children: Vec::new() }
    }

    fn sort_recursive(&mut self) {
        self.children.sort_by(|left, right| {
            right
                .is_dir
                .cmp(&left.is_dir)
                .then_with(|| left.name.to_ascii_lowercase().cmp(&right.name.to_ascii_lowercase()))
        });
        for child in &mut self.children {
            child.sort_recursive();
        }
    }
}

#[derive(Debug, Clone)]
struct TreeEntry {
    name: String,
    path: PathBuf,
    depth: u16,
    is_dir: bool,
    expanded: bool,
}

#[derive(Debug, Clone)]
pub struct HighlightSegment {
    pub text: String,
    pub fg: Color,
    pub bold: bool,
    pub italic: bool,
}

#[derive(Debug, Clone)]
pub struct HighlightedLine {
    pub line_number: usize,
    pub segments: Vec<HighlightSegment>,
}

#[derive(Debug, Clone, Default)]
struct FuzzyFinderState {
    active: bool,
    query: String,
    selected: usize,
    matches: Vec<PathBuf>,
}

impl FuzzyFinderState {
    fn activate(&mut self, candidates: &[PathBuf]) {
        self.active = true;
        self.query.clear();
        self.selected = 0;
        self.matches = candidates.iter().take(MAX_FINDER_RESULTS).cloned().collect();
    }

    fn deactivate(&mut self) {
        self.active = false;
        self.query.clear();
        self.selected = 0;
        self.matches.clear();
    }
}

#[derive(Debug, Clone)]
pub struct FileBrowserApp {
    workspace_root: PathBuf,
    root: FileNode,
    files: Vec<PathBuf>,
    expanded_dirs: HashSet<PathBuf>,
    visible_entries: Vec<TreeEntry>,
    selected_index: usize,
    tree_scroll: u16,
    content_scroll: u16,
    active_file: Option<PathBuf>,
    highlighted_lines: Vec<HighlightedLine>,
    finder: FuzzyFinderState,
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
            root: FileNode::root(root_name),
            files: Vec::new(),
            expanded_dirs: HashSet::new(),
            visible_entries: Vec::new(),
            selected_index: 0,
            tree_scroll: 0,
            content_scroll: 0,
            active_file: None,
            highlighted_lines: Vec::new(),
            finder: FuzzyFinderState::default(),
            status_line: "Press @ to fuzzy-find files, Enter to open, Esc to return to chat.".to_string(),
        };

        if let Err(error) = app.reload_workspace() {
            app.status_line = format!("Failed to read workspace: {error}");
        }

        app
    }

    pub fn reload_workspace(&mut self) -> std::io::Result<()> {
        let (mut root, files) = build_workspace_index(&self.workspace_root)?;
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
        }

        Ok(())
    }

    pub fn load_debug_fixture(&mut self) {
        self.root = build_debug_tree();
        self.files = collect_files(&self.root);
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
        self.tree_scroll = 0;
        self.content_scroll = 0;
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
                self.finder.activate(&self.files);
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
                self.content_scroll = self.content_scroll.saturating_sub(20);
                FileBrowserAction::None
            }
            KeyCode::PageDown => {
                self.content_scroll = self.content_scroll.saturating_add(20);
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
                self.finder.selected = self.finder.selected.saturating_sub(1);
            }
            KeyCode::Down => {
                if self.finder.selected + 1 < self.finder.matches.len() {
                    self.finder.selected += 1;
                }
            }
            KeyCode::Enter => {
                if let Some(path) = self.finder.matches.get(self.finder.selected).cloned() {
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
        self.finder.matches = fuzzy_match_paths(&self.finder.query, &self.files, MAX_FINDER_RESULTS);
        self.finder.selected = self.finder.selected.min(self.finder.matches.len().saturating_sub(1));
    }

    fn move_selection_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }

        if (self.selected_index as u16) < self.tree_scroll {
            self.tree_scroll = self.selected_index as u16;
        }
    }

    fn move_selection_down(&mut self) {
        if self.selected_index + 1 < self.visible_entries.len() {
            self.selected_index += 1;
            if self.selected_index as u16 >= self.tree_scroll.saturating_add(10) {
                self.tree_scroll = self.tree_scroll.saturating_add(1);
            }
        }
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
        self.highlighted_lines = highlight_file(&absolute_path, &content);
        self.content_scroll = 0;
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
        flatten_tree_entries(&self.root, 0, &self.expanded_dirs, &mut entries);
        self.visible_entries = entries;
        self.selected_index = self.selected_index.min(self.visible_entries.len().saturating_sub(1));
        self.tree_scroll = self.tree_scroll.min(self.selected_index as u16);
    }
}

pub fn draw_file_browser_screen(frame: &mut Frame, app: &FileBrowserApp) {
    let size = frame.area();
    let clear = Block::default().style(Style::default().bg(colors::BG_TERMINAL));
    frame.render_widget(clear, size);

    let shell = FileBrowserShell.split(size);
    if shell.len() < 3 {
        return;
    }

    draw_file_browser_main(frame, shell[0], app);
    draw_file_browser_hints(frame, shell[1]);
    draw_file_browser_status(frame, shell[2], app);

    if app.finder.active {
        draw_fuzzy_overlay(frame, size, app);
    }
}

fn draw_file_browser_main(frame: &mut Frame, area: Rect, app: &FileBrowserApp) {
    let sidebar_width = area.width.clamp(24, 38);
    let layout = split_rects(
        area,
        Direction::Horizontal,
        vec![Constraint::Length(sidebar_width), Constraint::Min(0)],
    );

    if layout.len() < 2 {
        return;
    }

    draw_tree_pane(frame, layout[0], app);
    draw_content_pane(frame, layout[1], app);
}

fn draw_tree_pane(frame: &mut Frame, area: Rect, app: &FileBrowserApp) {
    let tree_title = format!(" {} ", app.workspace_root.display());
    let block = Block::default()
        .title(tree_title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(colors::BORDER_COLOR))
        .style(Style::default().bg(colors::BG_TERMINAL));
    frame.render_widget(block.clone(), area);

    let inner = block.inner(area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let lines = app
        .visible_entries
        .iter()
        .enumerate()
        .map(|(idx, entry)| {
            let indent = "  ".repeat(entry.depth as usize);
            let icon = if entry.is_dir { if entry.expanded { "v" } else { ">" } } else { "-" };

            let mut name_style = Style::default().fg(colors::TEXT_SECONDARY);
            if app.active_file.as_deref() == Some(entry.path.as_path()) {
                name_style = name_style.fg(colors::ACCENT_CYAN).add_modifier(Modifier::BOLD);
            }

            let mut row_style = Style::default().bg(colors::BG_TERMINAL);
            if idx == app.selected_index {
                row_style = row_style.fg(colors::TEXT_PRIMARY).add_modifier(Modifier::BOLD);
            }

            Line::from(vec![
                Span::styled(indent, row_style),
                Span::styled(
                    icon,
                    row_style.fg(if entry.is_dir { colors::ACCENT_YELLOW } else { colors::TEXT_MUTED }),
                ),
                Span::styled(" ", row_style),
                Span::styled(entry.name.clone(), name_style.patch(row_style)),
            ])
        })
        .collect::<Vec<_>>();

    let tree_text = Text::from(lines);
    let paragraph = Paragraph::new(tree_text)
        .style(Style::default().bg(colors::BG_TERMINAL))
        .scroll((app.tree_scroll, 0));
    frame.render_widget(paragraph, inner);
}

fn draw_content_pane(frame: &mut Frame, area: Rect, app: &FileBrowserApp) {
    let layout = split_rects(
        area,
        Direction::Vertical,
        vec![Constraint::Length(1), Constraint::Min(0)],
    );
    if layout.len() < 2 {
        return;
    }

    let breadcrumb = build_breadcrumb(app);
    let crumb_paragraph = Paragraph::new(Line::from(vec![
        Span::styled(" ", Style::default()),
        Span::styled(breadcrumb, Style::default().fg(colors::ACCENT_CYAN)),
    ]))
    .style(Style::default().bg(colors::BG_TERMINAL));
    frame.render_widget(crumb_paragraph, layout[0]);

    let content_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(colors::BORDER_COLOR))
        .style(Style::default().bg(colors::BG_TERMINAL));
    frame.render_widget(content_block.clone(), layout[1]);
    let inner = content_block.inner(layout[1]);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    if app.highlighted_lines.is_empty() {
        let placeholder = Paragraph::new("Select a file from the tree or open fuzzy finder with @")
            .style(Style::default().fg(colors::TEXT_MUTED).bg(colors::BG_TERMINAL))
            .wrap(Wrap { trim: true });
        frame.render_widget(placeholder, inner);
        return;
    }

    let line_width = app.highlighted_lines.len().to_string().len().max(3);
    let lines = app
        .highlighted_lines
        .iter()
        .map(|line| {
            let mut spans = Vec::with_capacity(line.segments.len() + 2);
            spans.push(Span::styled(
                format!("{:>line_width$} ", line.line_number, line_width = line_width),
                Style::default().fg(colors::TEXT_MUTED),
            ));

            for segment in &line.segments {
                let mut style = Style::default().fg(segment.fg);
                if segment.bold {
                    style = style.add_modifier(Modifier::BOLD);
                }
                if segment.italic {
                    style = style.add_modifier(Modifier::ITALIC);
                }
                spans.push(Span::styled(segment.text.clone(), style));
            }

            if line.segments.is_empty() {
                spans.push(Span::raw(" "));
            }

            Line::from(spans)
        })
        .collect::<Vec<_>>();

    let paragraph = Paragraph::new(Text::from(lines))
        .style(Style::default().bg(colors::BG_TERMINAL))
        .scroll((app.content_scroll, 0))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, inner);
}

fn draw_file_browser_hints(frame: &mut Frame, area: Rect) {
    let tokens = [
        HintToken::Text("Press "),
        HintToken::Key("@"),
        HintToken::Text(" for finder, "),
        HintToken::Key("Enter"),
        HintToken::Text(" to open/toggle, "),
        HintToken::Key("Esc"),
        HintToken::Text(" to return to chat"),
    ];
    HintFooter.render(frame, area, &tokens);
}

fn draw_file_browser_status(frame: &mut Frame, area: Rect, app: &FileBrowserApp) {
    TopBorderedInputRow.render(frame, area, &app.status_line, false);
}

fn draw_fuzzy_overlay(frame: &mut Frame, area: Rect, app: &FileBrowserApp) {
    let rows = split_rects(
        area,
        Direction::Vertical,
        vec![Constraint::Fill(1), Constraint::Length(12), Constraint::Fill(1)],
    );
    if rows.len() < 2 {
        return;
    }

    let cols = split_rects(
        rows[1],
        Direction::Horizontal,
        vec![
            Constraint::Fill(1),
            Constraint::Length(72.min(area.width.saturating_sub(2))),
            Constraint::Fill(1),
        ],
    );
    if cols.len() < 2 {
        return;
    }

    let panel = cols[1];
    let overlay = Block::default()
        .title(" open file ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(colors::ACCENT_CYAN))
        .style(Style::default().bg(colors::BG_TERMINAL));
    frame.render_widget(overlay.clone(), panel);
    let inner = overlay.inner(panel);

    let mut rows = Vec::with_capacity(app.finder.matches.len() + 2);
    rows.push(Constraint::Length(1));
    rows.extend((0..app.finder.matches.len()).map(|_| Constraint::Length(1)));
    rows.push(Constraint::Min(0));

    let layout = split_rects(inner, Direction::Vertical, rows);
    if layout.is_empty() {
        return;
    }

    let input = Paragraph::new(Line::from(vec![
        Span::styled(
            "@",
            Style::default().fg(colors::ACCENT_CYAN).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            &app.finder.query,
            Style::default().fg(colors::TEXT_PRIMARY).add_modifier(Modifier::BOLD),
        ),
    ]))
    .style(Style::default().bg(colors::BG_TERMINAL));
    frame.render_widget(input, layout[0]);

    for (idx, path) in app.finder.matches.iter().enumerate() {
        if let Some(slot) = layout.get(idx + 1).copied() {
            let selected = idx == app.finder.selected;
            let row_style = if selected {
                Style::default()
                    .bg(colors::BG_TERMINAL)
                    .fg(colors::TEXT_PRIMARY)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().bg(colors::BG_TERMINAL)
            };

            let line = Line::from(vec![
                Span::styled(if selected { "> " } else { "  " }, row_style.fg(colors::ACCENT_CYAN)),
                Span::styled(path.display().to_string(), row_style.fg(colors::TEXT_SECONDARY)),
            ]);
            let para = Paragraph::new(line).style(row_style);
            frame.render_widget(para, slot);
        }
    }
}

fn build_breadcrumb(app: &FileBrowserApp) -> String {
    let root = app
        .workspace_root
        .file_name()
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| app.workspace_root.display().to_string());

    let Some(active) = app.active_file.as_ref() else {
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

fn build_workspace_index(root: &Path) -> std::io::Result<(FileNode, Vec<PathBuf>)> {
    let root_name = root
        .file_name()
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| root.display().to_string());
    let mut tree = FileNode::root(root_name);
    let mut files = Vec::new();

    let mut walker = WalkBuilder::new(root);
    walker
        .standard_filters(true)
        .hidden(false)
        .git_ignore(true)
        .git_exclude(true)
        .git_global(true)
        .parents(true);

    for dent in walker.build() {
        let Ok(entry) = dent else {
            continue;
        };

        let path = entry.path();
        if path == root {
            continue;
        }

        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };

        if relative.as_os_str().is_empty() {
            continue;
        }

        let is_dir = entry.file_type().map(|ty| ty.is_dir()).unwrap_or(false);
        insert_node(&mut tree, relative, is_dir);

        if !is_dir {
            files.push(relative.to_path_buf());
        }
    }

    files.sort();
    Ok((tree, files))
}

pub fn workspace_files(root: &Path) -> Vec<PathBuf> {
    build_workspace_index(root).map(|(_, files)| files).unwrap_or_default()
}

fn insert_node(root: &mut FileNode, relative_path: &Path, is_dir: bool) {
    let components = relative_path
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>();

    if components.is_empty() {
        return;
    }

    let mut cursor = root;
    let mut current_path = PathBuf::new();

    for (idx, name) in components.iter().enumerate() {
        current_path.push(name);
        let last = idx + 1 == components.len();
        let node_is_dir = if last { is_dir } else { true };

        let child_index = if let Some(existing_idx) = cursor.children.iter().position(|child| child.name == *name) {
            existing_idx
        } else {
            cursor.children.push(FileNode {
                name: name.clone(),
                path: current_path.clone(),
                is_dir: node_is_dir,
                children: Vec::new(),
            });
            cursor.children.len() - 1
        };

        cursor = &mut cursor.children[child_index];
    }
}

fn flatten_tree_entries(node: &FileNode, depth: u16, expanded_dirs: &HashSet<PathBuf>, out: &mut Vec<TreeEntry>) {
    for child in &node.children {
        let expanded = child.is_dir && expanded_dirs.contains(&child.path);
        out.push(TreeEntry {
            name: child.name.clone(),
            path: child.path.clone(),
            depth,
            is_dir: child.is_dir,
            expanded,
        });

        if child.is_dir && expanded {
            flatten_tree_entries(child, depth.saturating_add(1), expanded_dirs, out);
        }
    }
}

fn build_debug_tree() -> FileNode {
    let mut root = FileNode::root("debug-workspace".to_string());

    for dir in 0..14 {
        let mut dir_node = FileNode {
            name: format!("module_{dir:02}"),
            path: PathBuf::from(format!("module_{dir:02}")),
            is_dir: true,
            children: Vec::new(),
        };

        for nested in 0..6 {
            let nested_name = format!("feature_{nested:02}");
            let mut nested_node = FileNode {
                name: nested_name.clone(),
                path: PathBuf::from(format!("module_{dir:02}/{nested_name}")),
                is_dir: true,
                children: Vec::new(),
            };

            for file in 0..8 {
                let name = format!("file_{file:02}.rs");
                nested_node.children.push(FileNode {
                    name: name.clone(),
                    path: PathBuf::from(format!("module_{dir:02}/{nested_name}/{name}")),
                    is_dir: false,
                    children: Vec::new(),
                });
            }

            dir_node.children.push(nested_node);
        }

        root.children.push(dir_node);
    }

    root.sort_recursive();
    root
}

fn collect_files(root: &FileNode) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_files_recursive(root, &mut files);
    files.sort();
    files
}

fn collect_files_recursive(node: &FileNode, out: &mut Vec<PathBuf>) {
    for child in &node.children {
        if child.is_dir {
            collect_files_recursive(child, out);
        } else {
            out.push(child.path.clone());
        }
    }
}

fn highlight_file(path: &Path, content: &str) -> Vec<HighlightedLine> {
    let syntax_set = SyntaxSet::load_defaults_newlines();
    let theme_set = ThemeSet::load_defaults();
    let theme = choose_theme(&theme_set);

    let syntax = syntax_set
        .find_syntax_for_file(path)
        .ok()
        .flatten()
        .unwrap_or_else(|| syntax_set.find_syntax_plain_text());

    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut lines = Vec::new();

    for (idx, line) in content.lines().take(MAX_FILE_LINES).enumerate() {
        let ranges = highlighter
            .highlight_line(line, &syntax_set)
            .unwrap_or_else(|_| vec![(syntect::highlighting::Style::default(), line)]);

        let segments = ranges
            .into_iter()
            .map(|(style, text)| HighlightSegment {
                text: text.to_string(),
                fg: Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b),
                bold: style.font_style.contains(syntect::highlighting::FontStyle::BOLD),
                italic: style.font_style.contains(syntect::highlighting::FontStyle::ITALIC),
            })
            .collect::<Vec<_>>();

        lines.push(HighlightedLine { line_number: idx + 1, segments });
    }

    if lines.is_empty() {
        lines.push(HighlightedLine { line_number: 1, segments: Vec::new() });
    }

    lines
}

pub fn fuzzy_match_paths(query: &str, candidates: &[PathBuf], limit: usize) -> Vec<PathBuf> {
    if query.trim().is_empty() {
        return candidates.iter().take(limit).cloned().collect();
    }

    let mut matcher = Matcher::new(Config::DEFAULT);
    let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
    let mut utf32_buf = Vec::new();
    let mut ranked = candidates
        .iter()
        .filter_map(|path| {
            let candidate = path.to_string_lossy();
            let score = pattern.score(Utf32Str::new(candidate.as_ref(), &mut utf32_buf), &mut matcher)?;
            Some((score, path.clone()))
        })
        .collect::<Vec<_>>();

    ranked.sort_by(|left, right| right.0.cmp(&left.0));
    ranked.into_iter().take(limit).map(|(_, path)| path).collect()
}

pub fn read_file_for_prompt(root: &Path, relative_path: &Path) -> Option<String> {
    let absolute = root.join(relative_path);
    let mut content = std::fs::read_to_string(absolute).ok()?;
    if content.len() > MAX_PROMPT_FILE_BYTES {
        content.truncate(MAX_PROMPT_FILE_BYTES);
    }
    Some(content)
}

fn choose_theme(themes: &ThemeSet) -> &Theme {
    if let Some(theme) = themes.themes.get("base16-mocha.dark") {
        return theme;
    }

    if let Some((_, theme)) = themes.themes.iter().next() {
        return theme;
    }

    panic!("syntect theme set is unexpectedly empty")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_node_creates_nested_nodes() {
        let mut root = FileNode::root("root".to_string());
        insert_node(&mut root, Path::new("src/main.rs"), false);

        assert_eq!(root.children.len(), 1);
        assert_eq!(root.children[0].name, "src");
        assert!(root.children[0].is_dir);
        assert_eq!(root.children[0].children[0].name, "main.rs");
        assert!(!root.children[0].children[0].is_dir);
    }

    #[test]
    fn test_collect_files_from_debug_tree() {
        let root = build_debug_tree();
        let files = collect_files(&root);
        assert!(!files.is_empty());
        assert!(
            files
                .iter()
                .all(|path| path.extension().and_then(|ext| ext.to_str()) == Some("rs"))
        );
    }

    #[test]
    fn test_highlight_file_returns_line_numbers() {
        let lines = highlight_file(Path::new("src/main.rs"), "fn main() {}\n");
        assert_eq!(lines[0].line_number, 1);
    }
}
