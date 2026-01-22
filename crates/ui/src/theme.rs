use ratatui::style::{Color, Style};
use ratatui::text::Span;
use std::fmt::Display;
use std::str::FromStr;

/// Theme variant options supported by Thunderus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeVariant {
    Iceberg,
    Oxocarbon,
}

impl ThemeVariant {
    pub fn as_str(&self) -> &'static str {
        match self {
            ThemeVariant::Iceberg => "iceberg",
            ThemeVariant::Oxocarbon => "oxocarbon",
        }
    }

    pub fn parse_str(value: &str) -> Option<Self> {
        match value.to_lowercase().as_str() {
            "iceberg" => Some(Self::Iceberg),
            "oxocarbon" => Some(Self::Oxocarbon),
            _ => None,
        }
    }
}

impl Display for ThemeVariant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for ThemeVariant {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ThemeVariant::parse_str(s).ok_or_else(|| s.to_string())
    }
}

/// Color palette for a given theme variant.
#[derive(Debug, Clone, Copy)]
pub struct ThemePalette {
    pub bg: Color,
    pub fg: Color,
    pub panel_bg: Color,
    pub active: Color,
    pub highlight: Color,
    pub comment: Color,
    pub blue: Color,
    pub cyan: Color,
    pub purple: Color,
    pub green: Color,
    pub yellow: Color,
    pub red: Color,
    pub muted: Color,
    pub border: Color,
    pub black: Color,
}

/// Theme helpers for Thunderus TUI.
pub struct Theme;

impl Theme {
    pub fn palette(variant: ThemeVariant) -> ThemePalette {
        match variant {
            ThemeVariant::Iceberg => ThemePalette {
                bg: Color::Rgb(22, 24, 33),
                fg: Color::Rgb(198, 200, 209),
                panel_bg: Color::Rgb(30, 33, 50),
                active: Color::Rgb(39, 44, 66),
                highlight: Color::Rgb(39, 44, 66),
                comment: Color::Rgb(107, 112, 137),
                blue: Color::Rgb(132, 160, 198),
                cyan: Color::Rgb(137, 184, 194),
                purple: Color::Rgb(160, 147, 199),
                green: Color::Rgb(180, 190, 130),
                yellow: Color::Rgb(226, 164, 120),
                red: Color::Rgb(226, 120, 120),
                muted: Color::Rgb(107, 112, 137),
                border: Color::Rgb(107, 112, 137),
                black: Color::Rgb(22, 24, 33),
            },
            ThemeVariant::Oxocarbon => ThemePalette {
                bg: Color::Rgb(22, 22, 22),
                fg: Color::Rgb(242, 244, 248),
                panel_bg: Color::Rgb(38, 38, 38),
                active: Color::Rgb(57, 57, 57),
                highlight: Color::Rgb(57, 57, 57),
                comment: Color::Rgb(82, 82, 82),
                blue: Color::Rgb(120, 169, 255),
                cyan: Color::Rgb(8, 189, 186),
                purple: Color::Rgb(190, 149, 255),
                green: Color::Rgb(66, 190, 101),
                yellow: Color::Rgb(255, 126, 182),
                red: Color::Rgb(238, 83, 150),
                muted: Color::Rgb(82, 82, 82),
                border: Color::Rgb(82, 82, 82),
                black: Color::Rgb(22, 22, 22),
            },
        }
    }

    pub fn base(palette: ThemePalette) -> Style {
        Style::default().fg(palette.fg).bg(palette.bg)
    }

    pub fn primary(palette: ThemePalette) -> Style {
        Style::default().fg(palette.blue).bg(palette.bg)
    }

    pub fn success(palette: ThemePalette) -> Style {
        Style::default().fg(palette.green).bg(palette.bg)
    }

    pub fn warning(palette: ThemePalette) -> Style {
        Style::default().fg(palette.yellow).bg(palette.bg)
    }

    pub fn error(palette: ThemePalette) -> Style {
        Style::default().fg(palette.red).bg(palette.bg)
    }

    pub fn muted(palette: ThemePalette) -> Style {
        Style::default().fg(palette.muted).bg(palette.bg)
    }

    pub fn panel(palette: ThemePalette) -> Style {
        Style::default().fg(palette.fg).bg(palette.panel_bg)
    }

    pub fn border(palette: ThemePalette) -> Style {
        Style::default().fg(palette.border)
    }

    pub fn active(palette: ThemePalette) -> Style {
        Style::default().fg(palette.fg).bg(palette.active)
    }

    pub fn approval_mode_color(palette: ThemePalette, mode: &str) -> Color {
        match mode {
            "read-only" => palette.cyan,
            "auto" => palette.blue,
            "full-access" => palette.yellow,
            _ => palette.muted,
        }
    }

    pub fn approval_mode_span(palette: ThemePalette, mode: &str) -> Span<'_> {
        Span::styled(mode, Style::default().fg(Self::approval_mode_color(palette, mode)))
    }

    pub fn risk_level_color(palette: ThemePalette, risk: &str) -> Color {
        match risk {
            "safe" => palette.green,
            "risky" => palette.yellow,
            "dangerous" => palette.red,
            _ => palette.muted,
        }
    }

    pub fn risk_level_span(palette: ThemePalette, risk: &str) -> Span<'_> {
        Span::styled(risk, Style::default().fg(Self::risk_level_color(palette, risk)))
    }

    pub fn sandbox_mode_color(palette: ThemePalette, mode: &str) -> Color {
        match mode {
            "policy" => palette.green,
            "os" => palette.yellow,
            "none" => palette.red,
            _ => palette.muted,
        }
    }

    pub fn sandbox_mode_span(palette: ThemePalette, mode: &str) -> Span<'_> {
        Span::styled(mode, Style::default().fg(Self::sandbox_mode_color(palette, mode)))
    }

    pub fn verbosity_color(palette: ThemePalette, level: &str) -> Color {
        match level {
            "quiet" => palette.muted,
            "default" => palette.blue,
            "verbose" => palette.purple,
            _ => palette.muted,
        }
    }

    pub fn verbosity_span(palette: ThemePalette, level: &str) -> Span<'_> {
        Span::styled(level, Style::default().fg(Self::verbosity_color(palette, level)))
    }
}

impl Default for ThemePalette {
    fn default() -> Self {
        Theme::palette(ThemeVariant::Iceberg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_values() {
        let iceberg = Theme::palette(ThemeVariant::Iceberg);
        assert!(matches!(iceberg.bg, Color::Rgb(_, _, _)));
        assert!(matches!(iceberg.fg, Color::Rgb(_, _, _)));
        assert!(matches!(iceberg.panel_bg, Color::Rgb(_, _, _)));
    }

    #[test]
    fn test_approval_mode_colors() {
        let palette = Theme::palette(ThemeVariant::Iceberg);
        assert_eq!(Theme::approval_mode_color(palette, "read-only"), palette.cyan);
        assert_eq!(Theme::approval_mode_color(palette, "auto"), palette.blue);
        assert_eq!(Theme::approval_mode_color(palette, "full-access"), palette.yellow);
        assert_eq!(Theme::approval_mode_color(palette, "unknown"), palette.muted);
    }

    #[test]
    fn test_risk_level_colors() {
        let palette = Theme::palette(ThemeVariant::Iceberg);
        assert_eq!(Theme::risk_level_color(palette, "safe"), palette.green);
        assert_eq!(Theme::risk_level_color(palette, "risky"), palette.yellow);
        assert_eq!(Theme::risk_level_color(palette, "dangerous"), palette.red);
        assert_eq!(Theme::risk_level_color(palette, "unknown"), palette.muted);
    }

    #[test]
    fn test_styles() {
        let palette = Theme::palette(ThemeVariant::Iceberg);
        let base = Theme::base(palette);
        assert_eq!(base.fg, Some(palette.fg));
        assert_eq!(base.bg, Some(palette.bg));

        let panel = Theme::panel(palette);
        assert_eq!(panel.fg, Some(palette.fg));
        assert_eq!(panel.bg, Some(palette.panel_bg));
    }
}
