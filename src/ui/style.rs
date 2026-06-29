use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

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

pub const P: Palette = ICEBERG_DARK;

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

#[allow(dead_code)]
pub const CATPPUCCIN_MOCHA: Palette = Palette {
    accent: Color::Rgb(137, 180, 250),
    panel_bg: Color::Rgb(24, 24, 37),
    surface0: Color::Rgb(49, 50, 68),
    surface1: Color::Rgb(69, 71, 90),
    surface_dim: Color::Rgb(30, 30, 46),
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

pub fn panel_style() -> Style {
    Style::default().fg(P.text).bg(P.panel_bg)
}

pub fn border_style() -> Style {
    Style::default().fg(P.overlay0).bg(P.panel_bg)
}

pub fn title_style() -> Style {
    Style::default()
        .fg(P.accent)
        .bg(P.panel_bg)
        .add_modifier(Modifier::BOLD)
}

pub fn text_style() -> Style {
    Style::default().fg(P.text).bg(P.panel_bg)
}

pub fn muted_style() -> Style {
    Style::default().fg(P.overlay0).bg(P.panel_bg)
}

pub fn subtle_style() -> Style {
    Style::default().fg(P.subtext0).bg(P.panel_bg)
}

pub fn label_chip(label: &str, fg: Color, bg: Color) -> Span<'static> {
    Span::styled(
        format!(" {label} "),
        Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD),
    )
}

pub fn muted_chip(label: &str) -> Span<'static> {
    label_chip(label, P.subtext0, P.surface0)
}

pub fn status_color(label: &str) -> Color {
    match label {
        "idle" => P.overlay0,
        "done" => P.green,
        "sending" | "thinking" | "streaming" | "running tool" => P.yellow,
        "cancelled" => P.teal,
        "failed" => P.red,
        _ => P.overlay0,
    }
}

pub fn status_icon(label: &str, tick: u64) -> &'static str {
    match label {
        "sending" | "thinking" | "streaming" | "running tool" => spinner_frame(tick),
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
    fn label_chip_adds_cell_padding() {
        assert_eq!(label_chip("tool", P.text, P.surface0).content.as_ref(), " tool ");
    }
}
