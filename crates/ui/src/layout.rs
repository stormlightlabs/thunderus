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

impl LayoutMode {
    /// Determine layout mode based on terminal width
    pub fn from_width(width: u16) -> Self {
        if width >= 100 {
            Self::Full
        } else if width >= 80 {
            Self::Medium
        } else {
            Self::Compact
        }
    }

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
        let mode = LayoutMode::from_width(area.width);
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

    /// Get sidebar sections layout (stats, approvals)
    pub fn sidebar_sections(&self) -> Option<(Rect, Rect)> {
        let sidebar = self.sidebar?;

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(sidebar);

        Some((chunks[0], chunks[1]))
    }
}

/// Calculate header section widths
///
/// Header layout:
/// - cwd | profile | provider/model | approval mode
/// - On narrow terminals, less critical info is dropped
pub fn header_sections(area: Rect) -> (Rect, Rect, Rect, Rect) {
    let sections = match area.width {
        w if w >= 100 => Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(25),
                Constraint::Length(15),
                Constraint::Length(20),
                Constraint::Min(0),
            ])
            .split(area),
        w if w >= 80 => Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(15), Constraint::Length(20), Constraint::Min(0)])
            .split(area),
        _ => Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(15), Constraint::Min(0)])
            .split(area),
    };

    match sections.len() {
        4 => (sections[0], sections[1], sections[2], sections[3]),
        3 => (Rect::default(), sections[0], sections[1], sections[2]),
        2 => (Rect::default(), sections[0], Rect::default(), sections[1]),
        _ => (Rect::default(), Rect::default(), Rect::default(), Rect::default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layout_mode_from_width() {
        assert_eq!(LayoutMode::from_width(100), LayoutMode::Full);
        assert_eq!(LayoutMode::from_width(120), LayoutMode::Full);
        assert_eq!(LayoutMode::from_width(99), LayoutMode::Medium);
        assert_eq!(LayoutMode::from_width(80), LayoutMode::Medium);
        assert_eq!(LayoutMode::from_width(79), LayoutMode::Compact);
        assert_eq!(LayoutMode::from_width(60), LayoutMode::Compact);
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

        let (token_stats, approval_stats) = sections.unwrap();

        assert_eq!(token_stats.height, 1);
        assert!(approval_stats.height > 0);
    }

    #[test]
    fn test_header_sections_full() {
        let area = Rect::new(0, 0, 100, 1);
        let (cwd, profile, provider, approval) = header_sections(area);

        assert_ne!(cwd.width, 0);
        assert_ne!(profile.width, 0);
        assert_ne!(provider.width, 0);
        assert_ne!(approval.width, 0);
        assert_eq!(cwd.width, 25);
    }

    #[test]
    fn test_header_sections_medium() {
        let area = Rect::new(0, 0, 85, 1);
        let (cwd, profile, provider, approval) = header_sections(area);

        assert_eq!(cwd, Rect::default());
        assert_ne!(profile.width, 0);
        assert_ne!(provider.width, 0);
        assert_ne!(approval.width, 0);
    }

    #[test]
    fn test_header_sections_compact() {
        let area = Rect::new(0, 0, 70, 1);
        let (cwd, profile, provider, approval) = header_sections(area);

        assert_eq!(cwd, Rect::default());
        assert_eq!(provider, Rect::default());

        assert_ne!(profile.width, 0);
        assert_ne!(approval.width, 0);
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
