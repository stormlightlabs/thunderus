use std::rc::Rc;

use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// Layout breakpoints for responsive TUI
///
/// Based on terminal width, we render different layouts:
/// - >= 100 cols: Full layout with sidebar
/// - 80-99 cols: Medium layout, sidebar hidden
/// - < 80 cols: Compact layout, minimal chrome
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutMode {
    /// Full layout with sidebar (>= 100 columns)
    Full,
    /// Medium layout without sidebar (80-99 columns)
    Medium,
    /// Compact layout (<= 79 columns)
    Compact,
}

impl From<u16> for LayoutMode {
    fn from(width: u16) -> Self {
        match width {
            w if w >= 100 => Self::Full,
            w if w >= 80 => Self::Medium,
            _ => Self::Compact,
        }
    }
}

impl LayoutMode {
    /// Check if sidebar should be shown
    pub fn has_sidebar(&self) -> bool {
        matches!(self, Self::Full)
    }
}

/// Calculated layout for the TUI
#[derive(Debug, Clone)]
pub struct TuiLayout {
    /// Layout mode based on terminal width
    pub mode: LayoutMode,
    /// Header area (1 line)
    pub header: Rect,
    /// Main transcript area
    pub transcript: Rect,
    /// Left sidebar (only in Full mode)
    pub sidebar: Option<Rect>,
    /// Footer area (1-3 lines)
    pub footer: Rect,
}

impl TuiLayout {
    /// Calculate layout based on terminal size and sidebar visibility preference
    pub fn calculate(area: Rect, sidebar_visible: bool) -> Self {
        let mode = LayoutMode::from(area.width);
        let effective_sidebar_visible = sidebar_visible && mode.has_sidebar();

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0), Constraint::Length(3)])
            .split(area);

        let header = chunks[0];
        let main = chunks[1];
        let footer = chunks[2];

        let (sidebar, transcript) = if effective_sidebar_visible {
            let width = main.width.saturating_sub(2);
            let sidebar_width = width.min(25);

            let main_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(sidebar_width), Constraint::Min(0)])
                .split(main);

            (Some(main_chunks[0]), main_chunks[1])
        } else {
            (None, main)
        };

        Self { mode, header, transcript, sidebar, footer }
    }

    /// Get footer input area (single line)
    pub fn footer_input(&self) -> Rect {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0), Constraint::Length(20)])
            .split(self.footer);

        chunks[0]
    }

    /// Get footer hints area
    pub fn footer_hints(&self) -> Rect {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0), Constraint::Length(20)])
            .split(self.footer);

        chunks[1]
    }

    /// Get sidebar sections layout (4 sections)
    ///
    /// Returns: (session_events, modified_files, git_diff, lsp_mcp_status)
    pub fn sidebar_sections(&self) -> Option<SidebarSections> {
        self.sidebar.map(SidebarSections::new)
    }
}

pub struct SidebarSections {
    pub session_events: Rect,
    pub modified_files: Rect,
    pub git_diff: Rect,
    pub lsp_mcp_status: Rect,
}

impl SidebarSections {
    fn new(area: Rect) -> Self {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(4),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Min(0),
            ])
            .split(area);

        Self { session_events: chunks[0], modified_files: chunks[1], git_diff: chunks[2], lsp_mcp_status: chunks[3] }
    }
}

enum HeaderSize {
    Full,
    Large,
    Medium,
    Narrow,
    Compact,
}

impl From<u16> for HeaderSize {
    fn from(width: u16) -> Self {
        match width {
            w if w >= 140 => Self::Full,
            w if w >= 120 => Self::Large,
            w if w >= 100 => Self::Medium,
            w if w >= 80 => Self::Narrow,
            _ => Self::Compact,
        }
    }
}

#[derive(Default)]
pub struct HeaderSections {
    pub cwd: Rect,
    pub profile: Rect,
    pub provider: Rect,
    pub approval: Rect,
    pub git: Rect,
    pub sandbox: Rect,
    pub network: Rect,
    pub verbosity: Rect,
}

impl HeaderSections {
    fn layout(area: Rect) -> Rc<[Rect]> {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints(Self::constraints(area))
            .split(area)
    }

    fn constraints(area: Rect) -> Vec<Constraint> {
        match HeaderSize::from(area.width) {
            HeaderSize::Full => vec![
                Constraint::Length(20),
                Constraint::Length(12),
                Constraint::Length(20),
                Constraint::Length(12),
                Constraint::Length(12),
                Constraint::Length(10),
                Constraint::Length(8),
                Constraint::Min(0),
            ],
            HeaderSize::Large => vec![
                Constraint::Length(20),
                Constraint::Length(12),
                Constraint::Length(20),
                Constraint::Length(12),
                Constraint::Length(12),
                Constraint::Length(10),
                Constraint::Length(8),
            ],
            HeaderSize::Medium => vec![
                Constraint::Length(12),
                Constraint::Length(20),
                Constraint::Length(12),
                Constraint::Length(12),
                Constraint::Length(10),
                Constraint::Length(8),
            ],
            HeaderSize::Narrow => vec![
                Constraint::Length(12),
                Constraint::Length(20),
                Constraint::Length(12),
                Constraint::Length(12),
                Constraint::Min(0),
            ],
            HeaderSize::Compact => vec![Constraint::Length(12), Constraint::Min(0)],
        }
    }

    /// Calculate header section widths
    ///
    /// Header layout (responsive):
    /// - Full (>= 140): cwd | profile | provider/model | approval | git | sandbox | network | verbosity
    /// - Large (>= 120): cwd | profile | provider/model | approval | git | sandbox | network
    /// - Medium (>= 100): profile | provider/model | approval | git | sandbox | network
    /// - Narrow (>= 80): profile | provider/model | approval | git
    /// - Compact (< 80): profile | approval
    pub fn new(area: Rect) -> Self {
        let chunks = Self::layout(area);

        match HeaderSize::from(area.width) {
            HeaderSize::Full => Self {
                cwd: chunks[0],
                profile: chunks[1],
                provider: chunks[2],
                approval: chunks[3],
                git: chunks[4],
                sandbox: chunks[5],
                network: chunks[6],
                verbosity: chunks[7],
            },
            HeaderSize::Large => Self {
                cwd: chunks[0],
                profile: chunks[1],
                provider: chunks[2],
                approval: chunks[3],
                git: chunks[4],
                sandbox: chunks[5],
                network: chunks[6],
                ..Default::default()
            },
            HeaderSize::Medium => Self {
                profile: chunks[0],
                provider: chunks[1],
                approval: chunks[2],
                git: chunks[3],
                sandbox: chunks[4],
                network: chunks[5],
                ..Default::default()
            },
            HeaderSize::Narrow => Self {
                profile: chunks[0],
                provider: chunks[1],
                approval: chunks[2],
                git: chunks[3],
                ..Default::default()
            },
            HeaderSize::Compact => Self { profile: chunks[0], approval: chunks[1], ..Default::default() },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layout_mode_from_width() {
        assert_eq!(LayoutMode::from(100), LayoutMode::Full);
        assert_eq!(LayoutMode::from(120), LayoutMode::Full);
        assert_eq!(LayoutMode::from(99), LayoutMode::Medium);
        assert_eq!(LayoutMode::from(80), LayoutMode::Medium);
        assert_eq!(LayoutMode::from(79), LayoutMode::Compact);
        assert_eq!(LayoutMode::from(60), LayoutMode::Compact);
    }

    #[test]
    fn test_layout_mode_has_sidebar() {
        assert!(LayoutMode::Full.has_sidebar());
        assert!(!LayoutMode::Medium.has_sidebar());
        assert!(!LayoutMode::Compact.has_sidebar());
    }

    #[test]
    fn test_tui_layout_full_mode() {
        let area = Rect::new(0, 0, 100, 30);
        let layout = TuiLayout::calculate(area, true);
        assert_eq!(layout.mode, LayoutMode::Full);
        assert!(layout.sidebar.is_some());
        assert_eq!(layout.header.height, 1);
        assert_eq!(layout.header.width, 100);
        assert_eq!(layout.footer.height, 3);
        assert_eq!(layout.footer.width, 100);

        let sidebar = layout.sidebar.unwrap();
        assert!(sidebar.width > 0);
        assert!(sidebar.width < 30);
    }

    #[test]
    fn test_tui_layout_medium_mode() {
        let area = Rect::new(0, 0, 85, 30);
        let layout = TuiLayout::calculate(area, true);

        assert_eq!(layout.mode, LayoutMode::Medium);
        assert!(layout.sidebar.is_none());

        assert_eq!(layout.transcript.width, 85);
    }

    #[test]
    fn test_tui_layout_compact_mode() {
        let area = Rect::new(0, 0, 70, 20);
        let layout = TuiLayout::calculate(area, true);
        assert_eq!(layout.mode, LayoutMode::Compact);
        assert!(layout.sidebar.is_none());
        assert_eq!(layout.transcript.width, 70);
    }

    #[test]
    fn test_tui_layout_sidebar_hidden() {
        let area = Rect::new(0, 0, 100, 30);
        let layout = TuiLayout::calculate(area, false);
        assert!(layout.sidebar.is_none());
        assert_eq!(layout.transcript.width, 100);
    }

    #[test]
    fn test_footer_sections() {
        let area = Rect::new(0, 0, 100, 30);
        let layout = TuiLayout::calculate(area, true);
        let input = layout.footer_input();
        let hints = layout.footer_hints();

        assert_eq!(input.y, layout.footer.y);
        assert_eq!(hints.y, layout.footer.y);
        assert_eq!(hints.width, 20);
        assert_eq!(input.width, 80);
    }

    #[test]
    fn test_sidebar_sections() {
        let area = Rect::new(0, 0, 100, 30);
        let layout = TuiLayout::calculate(area, true);
        let sections = layout.sidebar_sections();
        assert!(sections.is_some());

        let sidebar = sections.unwrap();
        assert_eq!(sidebar.session_events.height, 4);
        assert_eq!(sidebar.modified_files.height, 3);
        assert_eq!(sidebar.git_diff.height, 3);
    }

    #[test]
    fn test_header_sections_full() {
        let area = Rect::new(0, 0, 140, 1);
        let header = HeaderSections::new(area);

        assert_ne!(header.cwd.width, 0);
        assert_ne!(header.profile.width, 0);
        assert_ne!(header.provider.width, 0);
        assert_ne!(header.approval.width, 0);
        assert_ne!(header.git.width, 0);
        assert_ne!(header.sandbox.width, 0);
        assert_ne!(header.network.width, 0);
        assert_ne!(header.verbosity.width, 0);
    }

    #[test]
    fn test_header_sections_large() {
        let area = Rect::new(0, 0, 120, 1);
        let header = HeaderSections::new(area);

        assert_ne!(header.cwd.width, 0);
        assert_ne!(header.profile.width, 0);
        assert_ne!(header.provider.width, 0);
        assert_ne!(header.approval.width, 0);
        assert_ne!(header.git.width, 0);
        assert_ne!(header.sandbox.width, 0);
        assert_ne!(header.network.width, 0);
    }

    #[test]
    fn test_header_sections_medium() {
        let area = Rect::new(0, 0, 100, 1);
        let header = HeaderSections::new(area);

        assert_eq!(header.cwd, Rect::default());
        assert_ne!(header.profile.width, 0);
        assert_ne!(header.provider.width, 0);
        assert_ne!(header.approval.width, 0);
        assert_ne!(header.git.width, 0);
        assert_ne!(header.sandbox.width, 0);
        assert_ne!(header.network.width, 0);
    }

    #[test]
    fn test_header_sections_compact() {
        let area = Rect::new(0, 0, 70, 1);
        let header = HeaderSections::new(area);

        assert_eq!(header.cwd, Rect::default());
        assert_eq!(header.provider, Rect::default());
        assert_eq!(header.git, Rect::default());
        assert_eq!(header.sandbox, Rect::default());
        assert_eq!(header.network, Rect::default());
        assert_eq!(header.verbosity, Rect::default());

        assert_ne!(header.profile.width, 0);
        assert_ne!(header.approval.width, 0);
    }

    #[test]
    fn test_tui_layout_small_terminal() {
        let area = Rect::new(0, 0, 40, 15);
        let layout = TuiLayout::calculate(area, true);

        assert_eq!(layout.mode, LayoutMode::Compact);
        assert!(layout.sidebar.is_none());

        assert_eq!(layout.header.height, 1);
        assert_eq!(layout.footer.height, 3);
        assert_eq!(layout.transcript.height, 11);
    }
}
