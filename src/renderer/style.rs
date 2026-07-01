//! Renderer-native style primitives.
//!
//! These types are intentionally independent of both Ratatui and crossterm so
//! that wrapping, padding, truncation, cursor calculation, and snapshots can be
//! unit-tested without any terminal dependency.
//!
//! [`Color`] stores RGB values directly.

#![allow(dead_code)]

use std::fmt;

/// An RGBA-free terminal color.
///
/// RGB is the canonical representation; named variants exist for the 16-color
/// ANSI palette and reset so styles can map cleanly onto any backend.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum Color {
    /// Transparent / inherited from the surrounding style.
    #[default]
    Reset,
    Black,
    DarkRed,
    DarkGreen,
    DarkYellow,
    DarkBlue,
    DarkMagenta,
    DarkCyan,
    Grey,
    DarkGrey,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    /// 24-bit RGB color.
    Rgb {
        r: u8,
        g: u8,
        b: u8,
    },
}

impl Color {
    /// Build an RGB color.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Color::Rgb { r, g, b }
    }
}

/// Style attributes applied to a cell's text and background.
///
/// Mirrors the subset of ANSI attributes `thndrs` uses today: foreground and
/// background color plus bold, italic, underlined, and dim modifiers.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub struct CellStyle {
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    pub italic: bool,
    pub underlined: bool,
    pub dim: bool,
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

    /// Enable dim.
    pub const fn dim(mut self) -> Self {
        self.dim = true;
        self
    }

    /// Overlay another style onto this one.
    ///
    /// Non-default values from `other` replace the corresponding values in
    /// `self`. Boolean attributes are OR-ed so a modifier set by either side
    /// stays set.
    pub fn patch(self, other: CellStyle) -> CellStyle {
        let fg = if other.fg != Color::Reset { other.fg } else { self.fg };
        let bg = if other.bg != Color::Reset { other.bg } else { self.bg };
        CellStyle {
            fg,
            bg,
            bold: self.bold || other.bold,
            italic: self.italic || other.italic,
            underlined: self.underlined || other.underlined,
            dim: self.dim || other.dim,
        }
    }
}

/// A styled run of text.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct Span {
    pub text: String,
    pub style: CellStyle,
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

    /// Width in display columns (char count).
    pub fn width(&self) -> usize {
        self.text.chars().count()
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
            .underlined()
            .dim();
        assert_eq!(s.fg, Color::Red);
        assert_eq!(s.bg, Color::Blue);
        assert!(s.bold && s.italic && s.underlined && s.dim);
    }

    #[test]
    fn patch_overrides_colors_and_or_attrs() {
        let base = CellStyle::new().fg(Color::Rgb { r: 1, g: 2, b: 3 }).bold();
        let over = CellStyle::new().fg(Color::Green).italic();
        let merged = base.patch(over);
        assert_eq!(merged.fg, Color::Green);
        assert!(merged.bold, "bold should be preserved from base");
        assert!(merged.italic, "italic should be added from over");
    }

    #[test]
    fn patch_reset_color_keeps_base() {
        let base = CellStyle::new().fg(Color::Red);
        let merged = base.patch(CellStyle::new());
        assert_eq!(merged.fg, Color::Red);
    }

    #[test]
    fn span_width_counts_chars() {
        let span = Span::plain("héllo");
        assert_eq!(span.width(), 5);
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
        let s = CellStyle::new().fg(Color::rgb(0xaa, 0xbb, 0xcc));
        assert_eq!(s.to_string(), "fg=#aabbcc");
    }
}
