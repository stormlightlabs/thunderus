//! Renderer style primitives.
//!
//! [`CellStyle`] and [`Span`] are renderer-owned because wrapping, padding,
//! truncation, cursor calculation, and snapshots operate on them directly.
//! Color is Crossterm's color type; keeping a separate mirror enum adds
//! conversion code without buying backend independence today.

use std::fmt;
use std::sync::atomic::{AtomicU8, Ordering};

pub use crossterm::style::Color;

use crate::cli::Theme;

const SPINNER_FRAME_INTERVAL_MS: u64 = 66;

/// Style attributes applied to a cell's text and background.
///
/// Mirrors the subset of ANSI attributes `thndrs` uses today: foreground and
/// background color plus bold, italic, underlined, and dim modifiers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CellStyle {
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    pub italic: bool,
    pub underlined: bool,
    pub dim: bool,
}

impl Default for CellStyle {
    fn default() -> Self {
        CellStyle { fg: Color::Reset, bg: Color::Reset, bold: false, italic: false, underlined: false, dim: false }
    }
}

impl CellStyle {
    /// Create a default (reset) style.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the foreground color.
    pub const fn fg(mut self, color: Color) -> Self {
        self.fg = color;
        self
    }

    /// Set the background color.
    pub const fn bg(mut self, color: Color) -> Self {
        self.bg = color;
        self
    }

    /// Enable bold.
    pub const fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    /// Enable italic.
    pub const fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    /// Enable underline.
    pub const fn underlined(mut self) -> Self {
        self.underlined = true;
        self
    }

    /// Return a copy with the background color changed.
    pub const fn with_bg(mut self, color: Color) -> Self {
        self.bg = color;
        self
    }
}

/// A styled run of text.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct Span {
    pub text: String,
    pub style: CellStyle,
}

/// Semantic color roles shared by every renderer surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThemeRole {
    Primary,
    Secondary,
    Accent,
    Active,
    Success,
    Warning,
    Failure,
    Selection,
    Input,
    Focus,
    Surface,
    SurfaceMuted,
    Border,
    Link,
    Reasoning,
    DiffAdded,
    DiffRemoved,
}

/// Theme palette expressed as semantic renderer roles.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Palette {
    pub accent: Color,
    pub active: Color,
    pub border: Color,
    pub failure: Color,
    pub focus: Color,
    pub input: Color,
    pub link: Color,
    pub primary: Color,
    pub reasoning: Color,
    pub secondary: Color,
    pub selection: Color,
    pub success: Color,
    pub surface: Color,
    pub surface_muted: Color,
    pub warning: Color,
}

static CURRENT_THEME: AtomicU8 = AtomicU8::new(0);

pub const ICEBERG_DARK: Palette = Palette {
    accent: Color::Rgb { r: 132, g: 160, b: 198 },
    active: Color::Rgb { r: 226, g: 164, b: 120 },
    border: Color::Rgb { r: 68, g: 75, b: 113 },
    failure: Color::Rgb { r: 226, g: 120, b: 120 },
    focus: Color::Rgb { r: 137, g: 184, b: 194 },
    input: Color::Rgb { r: 22, g: 24, b: 33 },
    link: Color::Rgb { r: 132, g: 160, b: 198 },
    primary: Color::Rgb { r: 198, g: 200, b: 209 },
    reasoning: Color::Rgb { r: 160, g: 147, b: 199 },
    secondary: Color::Rgb { r: 129, g: 133, b: 150 },
    selection: Color::Rgb { r: 39, g: 44, b: 66 },
    success: Color::Rgb { r: 180, g: 190, b: 130 },
    surface: Color::Rgb { r: 30, g: 33, b: 50 },
    surface_muted: Color::Rgb { r: 15, g: 17, b: 23 },
    warning: Color::Rgb { r: 226, g: 164, b: 120 },
};

pub const ELDRITCH_MINIMAL: Palette = Palette {
    accent: Color::Rgb { r: 55, g: 244, b: 153 },
    active: Color::Rgb { r: 247, g: 198, b: 127 },
    border: Color::Rgb { r: 59, g: 66, b: 97 },
    failure: Color::Rgb { r: 241, g: 108, b: 117 },
    focus: Color::Rgb { r: 4, g: 209, b: 249 },
    input: Color::Rgb { r: 23, g: 25, b: 40 },
    link: Color::Rgb { r: 4, g: 209, b: 249 },
    primary: Color::Rgb { r: 235, g: 250, b: 250 },
    reasoning: Color::Rgb { r: 164, g: 140, b: 242 },
    secondary: Color::Rgb { r: 171, g: 180, b: 218 },
    selection: Color::Rgb { r: 41, g: 46, b: 66 },
    success: Color::Rgb { r: 55, g: 244, b: 153 },
    surface: Color::Rgb { r: 33, g: 35, b: 55 },
    surface_muted: Color::Rgb { r: 23, g: 25, b: 40 },
    warning: Color::Rgb { r: 241, g: 252, b: 121 },
};

pub const CATPPUCCIN_MOCHA: Palette = Palette {
    accent: Color::Rgb { r: 203, g: 166, b: 247 },
    active: Color::Rgb { r: 250, g: 179, b: 135 },
    border: Color::Rgb { r: 108, g: 112, b: 134 },
    failure: Color::Rgb { r: 243, g: 139, b: 168 },
    focus: Color::Rgb { r: 148, g: 226, b: 213 },
    input: Color::Rgb { r: 30, g: 30, b: 46 },
    link: Color::Rgb { r: 137, g: 180, b: 250 },
    primary: Color::Rgb { r: 205, g: 214, b: 244 },
    reasoning: Color::Rgb { r: 203, g: 166, b: 247 },
    secondary: Color::Rgb { r: 166, g: 173, b: 200 },
    selection: Color::Rgb { r: 69, g: 71, b: 90 },
    success: Color::Rgb { r: 166, g: 227, b: 161 },
    surface: Color::Rgb { r: 49, g: 50, b: 68 },
    surface_muted: Color::Rgb { r: 24, g: 24, b: 37 },
    warning: Color::Rgb { r: 249, g: 226, b: 175 },
};

impl Palette {
    /// Resolve a semantic role to the current theme's concrete color.
    pub const fn color(self, role: ThemeRole) -> Color {
        match role {
            ThemeRole::Primary => self.primary,
            ThemeRole::Secondary => self.secondary,
            ThemeRole::Accent => self.accent,
            ThemeRole::Active => self.active,
            ThemeRole::Success | ThemeRole::DiffAdded => self.success,
            ThemeRole::Warning => self.warning,
            ThemeRole::Failure | ThemeRole::DiffRemoved => self.failure,
            ThemeRole::Selection => self.selection,
            ThemeRole::Input => self.input,
            ThemeRole::Focus => self.focus,
            ThemeRole::Surface => self.surface,
            ThemeRole::SurfaceMuted => self.surface_muted,
            ThemeRole::Border => self.border,
            ThemeRole::Link => self.link,
            ThemeRole::Reasoning => self.reasoning,
        }
    }
}

impl Theme {
    fn renderer_palette(self) -> Palette {
        match self {
            Theme::EldritchMinimal => ELDRITCH_MINIMAL,
            Theme::IcebergDark => ICEBERG_DARK,
            Theme::CatppuccinMocha => CATPPUCCIN_MOCHA,
        }
    }
}

pub fn set_theme(theme: Theme) {
    CURRENT_THEME.store(theme as u8, Ordering::Relaxed);
}

pub fn palette() -> Palette {
    match CURRENT_THEME.load(Ordering::Relaxed) {
        1 => Theme::IcebergDark.renderer_palette(),
        2 => Theme::CatppuccinMocha.renderer_palette(),
        _ => Theme::EldritchMinimal.renderer_palette(),
    }
}

pub fn status_color(label: &str) -> Color {
    let p = palette();
    match label {
        "Ready" => p.success,
        "Stopped" => p.focus,
        "Failed" => p.failure,
        "Working" | "Compacting" | "Stopping" => p.active,
        _ if label.starts_with("Running ") => p.active,
        _ => p.secondary,
    }
}

pub fn status_icon(label: &str, tick: u64) -> &'static str {
    match label {
        "Working" | "Compacting" | "Stopping" => spinner_frame(tick),
        "Ready" => "✓",
        "Failed" => "✕",
        "Stopped" => "○",
        "Waiting for permission" => "!",
        _ if label.starts_with("Running ") => spinner_frame(tick),
        _ => "·",
    }
}

/// Convert UI ticks to a spinner frame that advances about every 66 milliseconds.
///
/// The event/render loop may run faster than the spinner so streaming output
/// stays responsive without making the activity affordance visually frantic.
pub fn spinner_tick(ui_tick: u64, tick_rate_ms: u64) -> u64 {
    ui_tick.saturating_mul(tick_rate_ms.max(1)) / SPINNER_FRAME_INTERVAL_MS
}

pub fn spinner_frame(tick: u64) -> &'static str {
    const FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    FRAMES[(tick as usize) % FRAMES.len()]
}

impl Span {
    /// Create a plain (unstyled) span.
    pub fn plain(text: impl Into<String>) -> Self {
        Span { text: text.into(), style: CellStyle::default() }
    }

    /// Create a styled span.
    pub fn styled(text: impl Into<String>, style: CellStyle) -> Self {
        Span { text: text.into(), style }
    }
}

impl From<&str> for Span {
    fn from(s: &str) -> Self {
        Span::plain(s)
    }
}

impl From<String> for Span {
    fn from(s: String) -> Self {
        Span::plain(s)
    }
}

/// Compact, human-readable representation of a [`Color`] for snapshot
/// debug output.
fn color_debug(color: Color) -> String {
    match color {
        Color::Reset => "-".to_string(),
        Color::Black => "black".to_string(),
        Color::DarkRed => "darkred".to_string(),
        Color::DarkGreen => "darkgreen".to_string(),
        Color::DarkYellow => "darkyellow".to_string(),
        Color::DarkBlue => "darkblue".to_string(),
        Color::DarkMagenta => "darkmagenta".to_string(),
        Color::DarkCyan => "darkcyan".to_string(),
        Color::Grey => "grey".to_string(),
        Color::DarkGrey => "darkgrey".to_string(),
        Color::Red => "red".to_string(),
        Color::Green => "green".to_string(),
        Color::Yellow => "yellow".to_string(),
        Color::Blue => "blue".to_string(),
        Color::Magenta => "magenta".to_string(),
        Color::Cyan => "cyan".to_string(),
        Color::White => "white".to_string(),
        Color::Rgb { r, g, b } => format!("#{r:02x}{g:02x}{b:02x}"),
        Color::AnsiValue(value) => format!("ansi{value}"),
    }
}

/// Render a [`CellStyle`] as a compact attribute string for snapshots.
///
/// Produces strings like `fg=#aabbcc bg=#112233 biu` where the trailing letters
/// are `b`old, `i`talic, `u`nderlined, `d`im. A fully default style renders as
/// the single letter `-`.
impl fmt::Display for CellStyle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts: Vec<String> = Vec::new();
        if self.fg != Color::Reset {
            parts.push(format!("fg={}", color_debug(self.fg)));
        }
        if self.bg != Color::Reset {
            parts.push(format!("bg={}", color_debug(self.bg)));
        }
        let mut attrs = String::new();
        if self.bold {
            attrs.push('b');
        }
        if self.italic {
            attrs.push('i');
        }
        if self.underlined {
            attrs.push('u');
        }
        if self.dim {
            attrs.push('d');
        }
        if !attrs.is_empty() {
            parts.push(attrs);
        }
        if parts.is_empty() {
            return f.write_str("-");
        }
        f.write_str(&parts.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_style_is_reset() {
        let s = CellStyle::default();
        assert_eq!(s.fg, Color::Reset);
        assert_eq!(s.bg, Color::Reset);
        assert!(!s.bold);
        assert!(!s.italic);
        assert!(!s.underlined);
        assert!(!s.dim);
    }

    #[test]
    fn builder_sets_attributes() {
        let s = CellStyle::new()
            .fg(Color::Red)
            .bg(Color::Blue)
            .bold()
            .italic()
            .underlined();
        assert_eq!(s.fg, Color::Red);
        assert_eq!(s.bg, Color::Blue);
        assert!(s.bold && s.italic && s.underlined);
    }

    #[test]
    fn display_default_style_is_dash() {
        assert_eq!(CellStyle::default().to_string(), "-");
    }

    #[test]
    fn display_shows_fg_bg_and_attrs() {
        let s = CellStyle::new().fg(Color::Red).bg(Color::Blue).bold().italic();
        assert_eq!(s.to_string(), "fg=red bg=blue bi");
    }

    #[test]
    fn display_rgb_color_hex() {
        let s = CellStyle::new().fg(Color::Rgb { r: 0xaa, g: 0xbb, b: 0xcc });
        assert_eq!(s.to_string(), "fg=#aabbcc");
    }

    #[test]
    fn spinner_frame_wraps() {
        assert_eq!(spinner_frame(0), "⠋");
        assert_eq!(spinner_frame(10), "⠋");
        assert_eq!(spinner_frame(11), "⠙");
    }

    #[test]
    fn spinner_tick_advances_at_a_smooth_66_ms_cadence() {
        assert_eq!(spinner_tick(0, 33), 0);
        assert_eq!(spinner_tick(1, 33), 0);
        assert_eq!(spinner_tick(2, 33), 1);
        assert_eq!(spinner_tick(1, 100), 1);
    }

    #[test]
    fn status_markers_have_stable_single_cell_geometry() {
        use unicode_width::UnicodeWidthStr;

        for label in [
            "Ready",
            "Working",
            "Compacting",
            "Running cargo test",
            "Stopped",
            "Failed",
        ] {
            assert_eq!(UnicodeWidthStr::width(status_icon(label, 0)), 1, "{label}");
        }
        assert!(status_icon("Ready", 0) != status_icon("Failed", 0));
        assert!(status_icon("Stopped", 0) != status_icon("Working", 0));
    }

    #[test]
    fn theme_selects_palette() {
        assert_eq!(Theme::IcebergDark.renderer_palette(), ICEBERG_DARK);
        assert_eq!(Theme::EldritchMinimal.renderer_palette(), ELDRITCH_MINIMAL);
        assert_eq!(Theme::CatppuccinMocha.renderer_palette(), CATPPUCCIN_MOCHA);
    }

    #[test]
    fn eldritch_minimal_uses_the_terminal_label_palette() {
        assert_eq!(ELDRITCH_MINIMAL.focus, Color::Rgb { r: 4, g: 209, b: 249 });
        assert_eq!(ELDRITCH_MINIMAL.success, Color::Rgb { r: 55, g: 244, b: 153 });
        assert_eq!(ELDRITCH_MINIMAL.reasoning, Color::Rgb { r: 164, g: 140, b: 242 });
        assert_eq!(ELDRITCH_MINIMAL.warning, Color::Rgb { r: 241, g: 252, b: 121 });
        assert_eq!(ELDRITCH_MINIMAL.active, Color::Rgb { r: 247, g: 198, b: 127 });
        assert_eq!(ELDRITCH_MINIMAL.failure, Color::Rgb { r: 241, g: 108, b: 117 });
    }

    #[test]
    fn every_theme_owns_the_required_semantic_roles() {
        let required = [
            ThemeRole::Primary,
            ThemeRole::Secondary,
            ThemeRole::Accent,
            ThemeRole::Active,
            ThemeRole::Success,
            ThemeRole::Warning,
            ThemeRole::Failure,
            ThemeRole::Selection,
            ThemeRole::Input,
            ThemeRole::Focus,
        ];

        for theme in [Theme::EldritchMinimal, Theme::IcebergDark, Theme::CatppuccinMocha] {
            let palette = theme.renderer_palette();
            assert!(required.into_iter().all(|role| palette.color(role) != Color::Reset));
            assert_ne!(palette.primary, palette.selection);
            assert_ne!(palette.primary, palette.input);
            assert_ne!(palette.focus, palette.input);
        }
    }
}
