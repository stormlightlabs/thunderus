use ratatui::style::{Color, Style};
use ratatui::text::Span;

/// Iceberg color theme for Thunderus TUI
///
/// Based on iceberg.vim color scheme (https://github.com/cocopon/iceberg.vim)
/// Bluish dark theme designed for extended coding sessions with eye-friendly colors.
#[derive(Debug, Clone, Copy)]
pub struct Theme;

impl Theme {
    /// Primary background: deep blue-black (fills terminal)
    pub const BG: Color = Color::Rgb(22, 24, 33);

    /// Foreground: light blue-gray (primary text)
    pub const FG: Color = Color::Rgb(198, 200, 209);

    /// Secondary background: lighter blue-black (panels, cards, input)
    pub const PANEL_BG: Color = Color::Rgb(30, 33, 50);

    /// Hover/active states: visual selection
    pub const ACTIVE: Color = Color::Rgb(39, 44, 66);

    /// Comments: dim blue-gray
    pub const COMMENT: Color = Color::Rgb(107, 112, 137);

    /// Primary accent: blue
    pub const BLUE: Color = Color::Rgb(132, 160, 198);

    /// Secondary accent: cyan
    pub const CYAN: Color = Color::Rgb(137, 184, 194);

    /// Tertiary accent: purple
    pub const PURPLE: Color = Color::Rgb(160, 147, 199);

    /// Safe operations: green (borders, success)
    pub const GREEN: Color = Color::Rgb(180, 190, 130);

    /// Risky operations: yellow (requires approval)
    pub const YELLOW: Color = Color::Rgb(226, 164, 120);

    /// Errors: red (failures, destructive actions)
    pub const RED: Color = Color::Rgb(226, 120, 120);

    /// Muted text: dimmed foreground
    pub const MUTED: Color = Color::Rgb(107, 112, 137);

    /// Border color
    pub const BORDER: Color = Color::Rgb(60, 65, 90);

    /// Base style for all text
    pub fn base() -> Style {
        Style::default().fg(Self::FG).bg(Self::BG)
    }

    /// Primary accent style
    pub fn primary() -> Style {
        Style::default().fg(Self::BLUE).bg(Self::BG)
    }

    /// Success style
    pub fn success() -> Style {
        Style::default().fg(Self::GREEN).bg(Self::BG)
    }

    /// Warning style (for approval prompts)
    pub fn warning() -> Style {
        Style::default().fg(Self::YELLOW).bg(Self::BG)
    }

    /// Error style
    pub fn error() -> Style {
        Style::default().fg(Self::RED).bg(Self::BG)
    }

    /// Muted style (for secondary text)
    pub fn muted() -> Style {
        Style::default().fg(Self::MUTED).bg(Self::BG)
    }

    /// Panel style
    pub fn panel() -> Style {
        Style::default().fg(Self::FG).bg(Self::PANEL_BG)
    }

    /// Border style
    pub fn border() -> Style {
        Style::default().fg(Self::BORDER)
    }

    /// Active (selected) style
    pub fn active() -> Style {
        Style::default().fg(Self::FG).bg(Self::ACTIVE)
    }

    /// Get approval mode color
    pub fn approval_mode_color(mode: &str) -> Color {
        match mode {
            "read-only" => Self::CYAN,
            "auto" => Self::BLUE,
            "full-access" => Self::YELLOW,
            _ => Self::MUTED,
        }
    }

    /// Get span with approval mode styling
    pub fn approval_mode_span(mode: &str) -> Span<'_> {
        Span::styled(mode, Style::default().fg(Self::approval_mode_color(mode)))
    }

    /// Get tool risk level color
    pub fn risk_level_color(risk: &str) -> Color {
        match risk {
            "safe" => Self::GREEN,
            "risky" => Self::YELLOW,
            "dangerous" => Self::RED,
            _ => Self::MUTED,
        }
    }

    /// Get span with risk level styling
    pub fn risk_level_span(risk: &str) -> Span<'_> {
        Span::styled(risk, Style::default().fg(Self::risk_level_color(risk)))
    }

    /// Get sandbox mode color
    pub fn sandbox_mode_color(mode: &str) -> Color {
        match mode {
            "policy" => Self::GREEN,
            "os" => Self::YELLOW,
            "none" => Self::RED,
            _ => Self::MUTED,
        }
    }

    /// Get span with sandbox mode styling
    pub fn sandbox_mode_span(mode: &str) -> Span<'_> {
        Span::styled(mode, Style::default().fg(Self::sandbox_mode_color(mode)))
    }

    /// Get verbosity level color
    pub fn verbosity_color(level: &str) -> Color {
        match level {
            "quiet" => Self::MUTED,
            "default" => Self::BLUE,
            "verbose" => Self::PURPLE,
            _ => Self::MUTED,
        }
    }

    /// Get span with verbosity level styling
    pub fn verbosity_span(level: &str) -> Span<'_> {
        Span::styled(level, Style::default().fg(Self::verbosity_color(level)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_values() {
        assert!(matches!(Theme::BG, Color::Rgb(_, _, _)));
        assert!(matches!(Theme::FG, Color::Rgb(_, _, _)));
        assert!(matches!(Theme::PANEL_BG, Color::Rgb(_, _, _)));
    }

    #[test]
    fn test_approval_mode_colors() {
        assert_eq!(Theme::approval_mode_color("read-only"), Theme::CYAN);
        assert_eq!(Theme::approval_mode_color("auto"), Theme::BLUE);
        assert_eq!(Theme::approval_mode_color("full-access"), Theme::YELLOW);
        assert_eq!(Theme::approval_mode_color("unknown"), Theme::MUTED);
    }

    #[test]
    fn test_risk_level_colors() {
        assert_eq!(Theme::risk_level_color("safe"), Theme::GREEN);
        assert_eq!(Theme::risk_level_color("risky"), Theme::YELLOW);
        assert_eq!(Theme::risk_level_color("dangerous"), Theme::RED);
        assert_eq!(Theme::risk_level_color("unknown"), Theme::MUTED);
    }

    #[test]
    fn test_styles() {
        let base = Theme::base();
        assert_eq!(base.fg, Some(Theme::FG));
        assert_eq!(base.bg, Some(Theme::BG));

        let panel = Theme::panel();
        assert_eq!(panel.fg, Some(Theme::FG));
        assert_eq!(panel.bg, Some(Theme::PANEL_BG));
    }
}
