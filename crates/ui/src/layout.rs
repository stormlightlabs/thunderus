use ratatui::layout::{Constraint, Direction, Layout, Rect};
use std::rc::Rc;

/// Layout breakpoints for responsive TUI
///
/// Based on terminal width, we render different layouts:
/// - >= 140 cols: Full layout with sidebar (wide terminals)
/// - 100-139 cols: Medium layout, sidebar hidden (typical terminals)
/// - < 100 cols: Compact layout, minimal chrome (narrow terminals)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutMode {
    /// Full layout with sidebar (>= 140 columns)
    Full,
    /// Medium layout without sidebar (100-139 columns)
    Medium,
    /// Compact layout (< 100 columns)
    Compact,
    /// Inspector layout (Provenance & Trajectory)
    Inspector,
}

impl From<u16> for LayoutMode {
    fn from(width: u16) -> Self {
        match width {
            w if w >= 140 => Self::Full,
            w if w >= 100 => Self::Medium,
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
    /// Evidence List area (in Inspector mode)
    pub evidence_list: Option<Rect>,
    /// Evidence Detail area (in Inspector mode)
    pub evidence_detail: Option<Rect>,
    /// Left sidebar (only in Full mode)
    pub sidebar: Option<Rect>,
    /// Footer area (1-3 lines)
    pub footer: Rect,
}

impl TuiLayout {
    /// Calculate layout based on terminal size and sidebar visibility preference
    pub fn calculate(area: Rect, sidebar_visible: bool, sidebar_width_override: Option<u16>) -> Self {
        let mode = LayoutMode::from(area.width);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0), Constraint::Length(6)])
            .split(area);

        let header = chunks[0];
        let main = chunks[1];
        let footer = chunks[2];

        let sidebar_width = sidebar_width_override.unwrap_or(20);
        let effective_sidebar_visible = sidebar_visible && mode.has_sidebar() && sidebar_width > 0;

        let (sidebar, transcript) = if effective_sidebar_visible {
            let width = main.width.saturating_sub(2);
            let sidebar_width = sidebar_width.min(width);

            let main_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(sidebar_width), Constraint::Min(0)])
                .split(main);

            (Some(main_chunks[0]), main_chunks[1])
        } else {
            (None, main)
        };

        Self { mode, header, transcript, evidence_list: None, evidence_detail: None, sidebar, footer }
    }

    /// Calculate Inspector layout
    pub fn calculate_inspector(area: Rect) -> Self {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0), Constraint::Length(6)])
            .split(area);

        let header = chunks[0];
        let main = chunks[1];
        let footer = chunks[2];

        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
            .split(main);

        Self {
            mode: LayoutMode::Inspector,
            header,
            transcript: Rect::default(), // Not used in Inspector mode
            evidence_list: Some(main_chunks[0]),
            evidence_detail: Some(main_chunks[1]),
            sidebar: None,
            footer,
        }
    }

    /// Get footer input area (single line)
    pub fn footer_input(&self) -> Rect {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0), Constraint::Length(40)])
            .split(self.footer);

        chunks[0]
    }

    /// Get footer hints area
    pub fn footer_hints(&self) -> Rect {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0), Constraint::Length(40)])
            .split(self.footer);

        chunks[1]
    }

    /// Get sidebar sections layout (7 sections)
    ///
    /// Returns: (token_usage, session_events, modified_files, git_diff, lsp_mcp_status, context, files)
    pub fn sidebar_sections(&self) -> Option<SidebarSections> {
        self.sidebar.map(SidebarSections::new)
    }
}

pub struct SidebarSections {
    pub token_usage: Rect,
    pub session_events: Rect,
    pub modified_files: Rect,
    pub git_diff: Rect,
    pub lsp_mcp_status: Rect,
    pub context: Rect,
    pub files: Rect,
}

impl SidebarSections {
    fn new(area: Rect) -> Self {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(4),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(4),
                Constraint::Min(0),
            ])
            .split(area);

        Self {
            token_usage: chunks[0],
            session_events: chunks[1],
            modified_files: chunks[2],
            git_diff: chunks[3],
            lsp_mcp_status: chunks[4],
            context: chunks[5],
            files: chunks[6],
        }
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

/// Layout for the welcome screen
///
/// Provides a centered, clean layout without header/sidebar for first session.
#[derive(Debug, Clone)]
pub struct WelcomeLayout {
    /// Header area (git branch + working directory)
    pub header: Rect,
    /// Centered logo area (upper portion)
    pub logo: Rect,
    /// Input card with blue accent bar
    pub input_card: Rect,
    /// Recent sessions row (below input card)
    pub recent_sessions: Rect,
    /// Centered keyboard shortcuts row
    pub shortcuts: Rect,
    /// Tip with colored indicator
    pub tips: Rect,
    /// Bottom status bar (github.com/stormlightlabs/thunderus | version)
    pub status_bar: Rect,
}

impl WelcomeLayout {
    /// Calculate layout based on terminal size and layout mode
    pub fn calculate(area: Rect, mode: LayoutMode) -> Self {
        let content_width = match mode {
            LayoutMode::Full | LayoutMode::Inspector => 120.min(area.width.saturating_sub(4)),
            LayoutMode::Medium => 100.min(area.width.saturating_sub(4)),
            LayoutMode::Compact => area.width.saturating_sub(4),
        };

        let center_x = area.x + (area.width.saturating_sub(content_width)) / 2;
        let header_height = 1;
        let status_bar_height = 1;
        let tips_height = 1;
        let shortcuts_height = 1;
        let recent_sessions_height: u16 = if mode == LayoutMode::Compact { 0 } else { 1 };
        let input_card_height = 3;
        let logo_height = 6;
        let header = Rect { x: area.x, y: area.y, width: area.width, height: header_height };

        let total_content_height =
            logo_height + 1 + input_card_height + 1 + recent_sessions_height + 1 + shortcuts_height + 1 + tips_height;

        let available_height = area.height.saturating_sub(status_bar_height + header_height);
        let content_start_y = area.y + header_height + available_height.saturating_sub(total_content_height) / 2;

        let mut current_y = content_start_y;

        let logo = Rect { x: center_x, y: current_y, width: content_width, height: logo_height };
        current_y += logo_height + 1;

        let input_card = Rect { x: center_x, y: current_y, width: content_width, height: input_card_height };
        current_y += input_card_height + 1;

        let shortcuts = Rect { x: center_x, y: current_y, width: content_width, height: shortcuts_height };
        current_y += shortcuts_height + 1;

        let recent_sessions = if mode == LayoutMode::Compact {
            Rect { x: center_x, y: current_y, width: content_width, height: 0 }
        } else {
            let rect = Rect { x: center_x, y: current_y, width: content_width, height: recent_sessions_height };
            current_y += recent_sessions_height + 1;
            rect
        };

        let tips = Rect { x: center_x, y: current_y, width: content_width, height: tips_height };

        let status_bar = Rect {
            x: area.x,
            y: area.y + area.height.saturating_sub(status_bar_height),
            width: area.width,
            height: status_bar_height,
        };

        Self { header, logo, input_card, recent_sessions, shortcuts, tips, status_bar }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layout_mode_from_width() {
        assert_eq!(LayoutMode::from(140), LayoutMode::Full);
        assert_eq!(LayoutMode::from(160), LayoutMode::Full);

        assert_eq!(LayoutMode::from(139), LayoutMode::Medium);
        assert_eq!(LayoutMode::from(120), LayoutMode::Medium);
        assert_eq!(LayoutMode::from(100), LayoutMode::Medium);

        assert_eq!(LayoutMode::from(99), LayoutMode::Compact);
        assert_eq!(LayoutMode::from(80), LayoutMode::Compact);
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
        let area = Rect::new(0, 0, 140, 30);
        let layout = TuiLayout::calculate(area, true, Some(20));
        assert_eq!(layout.mode, LayoutMode::Full);
        assert!(layout.sidebar.is_some());
        assert_eq!(layout.header.height, 1);
        assert_eq!(layout.header.width, 140);
        assert_eq!(layout.footer.height, 6);
        assert_eq!(layout.footer.width, 140);

        let sidebar = layout.sidebar.unwrap();
        assert!(sidebar.width > 0);
        assert!(sidebar.width < 30);
    }

    #[test]
    fn test_tui_layout_medium_mode() {
        let area = Rect::new(0, 0, 120, 30);
        let layout = TuiLayout::calculate(area, true, Some(20));

        assert_eq!(layout.mode, LayoutMode::Medium);
        assert!(layout.sidebar.is_none());

        assert_eq!(layout.transcript.width, 120);
    }

    #[test]
    fn test_tui_layout_compact_mode() {
        let area = Rect::new(0, 0, 80, 20);
        let layout = TuiLayout::calculate(area, true, Some(20));
        assert_eq!(layout.mode, LayoutMode::Compact);
        assert!(layout.sidebar.is_none());
        assert_eq!(layout.transcript.width, 80);
    }

    #[test]
    fn test_tui_layout_sidebar_hidden() {
        let area = Rect::new(0, 0, 140, 30);
        let layout = TuiLayout::calculate(area, false, None);
        assert!(layout.sidebar.is_none());
        assert_eq!(layout.transcript.width, 140);
    }

    #[test]
    fn test_footer_sections() {
        let area = Rect::new(0, 0, 120, 30);
        let layout = TuiLayout::calculate(area, true, Some(20));
        let input = layout.footer_input();
        let hints = layout.footer_hints();

        assert_eq!(input.y, layout.footer.y);
        assert_eq!(hints.y, layout.footer.y);
        assert_eq!(hints.width, 40);
        assert_eq!(input.width, 80);
    }

    #[test]
    fn test_sidebar_sections() {
        let area = Rect::new(0, 0, 140, 30);
        let layout = TuiLayout::calculate(area, true, Some(20));
        let sections = layout.sidebar_sections();
        assert!(sections.is_some());

        let sidebar = sections.unwrap();
        assert_eq!(sidebar.session_events.height, 4);
        assert_eq!(sidebar.modified_files.height, 3);
        assert_eq!(sidebar.git_diff.height, 3);
        assert_eq!(sidebar.context.height, 4);
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
        let layout = TuiLayout::calculate(area, true, Some(20));

        assert_eq!(layout.mode, LayoutMode::Compact);
        assert!(layout.sidebar.is_none());

        assert_eq!(layout.header.height, 1);
        assert_eq!(layout.footer.height, 6);
        assert_eq!(layout.transcript.height, 8);
    }

    #[test]
    fn test_welcome_layout_full() {
        let area = Rect::new(0, 0, 120, 30);
        let layout = WelcomeLayout::calculate(area, LayoutMode::Full);
        assert_eq!(layout.logo.width, 116);
        assert_eq!(layout.logo.height, 6);

        assert_eq!(layout.input_card.width, 116);
        assert_eq!(layout.input_card.height, 3);

        assert_eq!(layout.recent_sessions.height, 1);
        assert_eq!(layout.shortcuts.height, 1);
        assert_eq!(layout.tips.height, 1);

        assert_eq!(layout.status_bar.height, 1);
        assert_eq!(layout.status_bar.width, 120);
        assert_eq!(layout.status_bar.y, 29);
    }

    #[test]
    fn test_welcome_layout_compact() {
        let area = Rect::new(0, 0, 70, 25);
        let layout = WelcomeLayout::calculate(area, LayoutMode::Compact);
        assert_eq!(layout.logo.width, 66);
        assert_eq!(layout.input_card.width, 66);
        assert_eq!(layout.recent_sessions.height, 0);
        assert_eq!(layout.status_bar.height, 1);
        assert_eq!(layout.status_bar.y, 24);
    }

    #[test]
    fn test_welcome_layout_centered() {
        let area = Rect::new(0, 0, 100, 30);
        let layout = WelcomeLayout::calculate(area, LayoutMode::Full);
        assert_eq!(layout.logo.x, 2);
        assert_eq!(layout.input_card.x, 2);
        assert_eq!(layout.shortcuts.x, 2);
    }
}
