use std::sync::atomic::{AtomicU8, Ordering};

use ratatui::style::{Color, Modifier, Style};

use crate::cli::Theme;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Palette {
    pub accent: Color,
    pub panel_bg: Color,
    pub surface0: Color,
    pub surface1: Color,
    pub surface_dim: Color,
    pub overlay0: Color,
    pub overlay1: Color,
    pub text: Color,
    pub subtext0: Color,
    pub mauve: Color,
    pub green: Color,
    pub yellow: Color,
    pub red: Color,
    pub blue: Color,
    pub teal: Color,
    pub peach: Color,
}

static CURRENT_THEME: AtomicU8 = AtomicU8::new(0);

pub const ICEBERG_DARK: Palette = Palette {
    accent: Color::Rgb(132, 160, 198),
    panel_bg: Color::Rgb(22, 24, 33),
    surface0: Color::Rgb(30, 33, 50),
    surface1: Color::Rgb(39, 44, 66),
    surface_dim: Color::Rgb(15, 17, 23),
    overlay0: Color::Rgb(68, 75, 113),
    overlay1: Color::Rgb(107, 112, 137),
    text: Color::Rgb(198, 200, 209),
    subtext0: Color::Rgb(129, 133, 150),
    mauve: Color::Rgb(160, 147, 199),
    green: Color::Rgb(180, 190, 130),
    yellow: Color::Rgb(226, 164, 120),
    red: Color::Rgb(226, 120, 120),
    blue: Color::Rgb(132, 160, 198),
    teal: Color::Rgb(137, 184, 194),
    peach: Color::Rgb(226, 164, 120),
};

pub const ELDRITCH_MINIMAL: Palette = Palette {
    accent: Color::Rgb(55, 244, 153),
    panel_bg: Color::Rgb(23, 25, 40),
    surface0: Color::Rgb(33, 35, 55),
    surface1: Color::Rgb(41, 46, 66),
    surface_dim: Color::Rgb(23, 25, 40),
    overlay0: Color::Rgb(59, 66, 97),
    overlay1: Color::Rgb(100, 115, 183),
    text: Color::Rgb(235, 250, 250),
    subtext0: Color::Rgb(171, 180, 218),
    mauve: Color::Rgb(164, 140, 242),
    green: Color::Rgb(55, 244, 153),
    yellow: Color::Rgb(224, 224, 224),
    red: Color::Rgb(241, 108, 117),
    blue: Color::Rgb(4, 209, 249),
    teal: Color::Rgb(4, 209, 249),
    peach: Color::Rgb(224, 224, 224),
};

pub const CATPPUCCIN_MOCHA: Palette = Palette {
    accent: Color::Rgb(203, 166, 247),
    panel_bg: Color::Rgb(30, 30, 46),
    surface0: Color::Rgb(49, 50, 68),
    surface1: Color::Rgb(69, 71, 90),
    surface_dim: Color::Rgb(24, 24, 37),
    overlay0: Color::Rgb(108, 112, 134),
    overlay1: Color::Rgb(127, 132, 156),
    text: Color::Rgb(205, 214, 244),
    subtext0: Color::Rgb(166, 173, 200),
    mauve: Color::Rgb(203, 166, 247),
    green: Color::Rgb(166, 227, 161),
    yellow: Color::Rgb(249, 226, 175),
    red: Color::Rgb(243, 139, 168),
    blue: Color::Rgb(137, 180, 250),
    teal: Color::Rgb(148, 226, 213),
    peach: Color::Rgb(250, 179, 135),
};

impl Theme {
    fn palette(self) -> Palette {
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
        1 => Theme::IcebergDark.palette(),
        2 => Theme::CatppuccinMocha.palette(),
        _ => Theme::EldritchMinimal.palette(),
    }
}

pub fn panel_style() -> Style {
    let p = palette();
    Style::default().fg(p.text).bg(p.panel_bg)
}

pub fn title_style() -> Style {
    let p = palette();
    Style::default()
        .fg(p.accent)
        .bg(p.panel_bg)
        .add_modifier(Modifier::BOLD)
}

pub fn text_style() -> Style {
    let p = palette();
    Style::default().fg(p.text).bg(p.panel_bg)
}

pub fn muted_style() -> Style {
    let p = palette();
    Style::default().fg(p.overlay0).bg(p.panel_bg)
}

pub fn subtle_style() -> Style {
    let p = palette();
    Style::default().fg(p.subtext0).bg(p.panel_bg)
}

pub fn status_color(label: &str) -> Color {
    let p = palette();
    match label {
        "idle" => p.overlay0,
        "done" => p.green,
        "sending" | "thinking" | "streaming" | "running tool" | "stopping" => p.yellow,
        "cancelled" => p.teal,
        "failed" => p.red,
        _ => p.overlay0,
    }
}

pub fn status_icon(label: &str, tick: u64) -> &'static str {
    match label {
        "sending" | "thinking" | "streaming" | "running tool" | "stopping" => spinner_frame(tick),
        "done" => "✓",
        "failed" => "✕",
        "cancelled" => "○",
        "idle" => "·",
        _ => "·",
    }
}

pub fn spinner_frame(tick: u64) -> &'static str {
    const FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    FRAMES[(tick as usize) % FRAMES.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spinner_frame_wraps() {
        assert_eq!(spinner_frame(0), "⠋");
        assert_eq!(spinner_frame(10), "⠋");
        assert_eq!(spinner_frame(11), "⠙");
    }

    #[test]
    fn theme_selects_palette() {
        assert_eq!(Theme::IcebergDark.palette(), ICEBERG_DARK);
        assert_eq!(Theme::EldritchMinimal.palette(), ELDRITCH_MINIMAL);
        assert_eq!(Theme::CatppuccinMocha.palette(), CATPPUCCIN_MOCHA);
    }
}
