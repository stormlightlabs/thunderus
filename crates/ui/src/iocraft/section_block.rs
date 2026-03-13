use super::theme::resolve_theme;
use super::wrapped_line_count;
use ::iocraft::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SectionTone {
    #[default]
    Intent,
    Actions,
    Result,
    Next,
}

#[derive(Default, Props)]
pub struct SectionBlockProps {
    pub tone: SectionTone,
    pub icon: String,
    pub title: String,
    pub body: String,
}

#[component]
pub fn SectionBlock(hooks: Hooks, props: &SectionBlockProps) -> impl Into<AnyElement<'static>> {
    let theme = resolve_theme(&hooks);
    let accent = props.tone.accent(theme);

    element! {
        View(
            width: 100pct,
            flex_direction: FlexDirection::Column,
            border_style: BorderStyle::Single,
            border_edges: Edges::Left,
            border_color: accent,
            background_color: theme.bg_terminal,
            padding_left: 1,
        ) {
            MixedText(contents: vec![
                MixedTextContent::new(&props.icon).color(accent),
                MixedTextContent::new(" "),
                MixedTextContent::new(props.title.to_ascii_uppercase()).color(accent).weight(Weight::Bold),
            ])
            View(height: 1)
            Text(content: &props.body, color: theme.text_secondary, wrap: TextWrap::Wrap)
        }
    }
}

impl SectionTone {
    pub fn accent(self, theme: Theme) -> Color {
        match self {
            Self::Intent => theme.accent_purple,
            Self::Actions => theme.accent_yellow,
            Self::Result => theme.accent_green,
            Self::Next => theme.accent_cyan,
        }
    }
}

pub fn estimate_height(content: &str, width: u16, min_height: u16) -> u16 {
    let wrapped_lines = wrapped_line_count(content, width.saturating_sub(2));
    (wrapped_lines as u16 + 2).max(min_height)
}

use super::theme::Theme;

#[cfg(test)]
mod tests {
    use super::{SectionBlock, SectionTone, estimate_height};
    use ::iocraft::prelude::*;

    #[test]
    fn section_tone_maps_to_theme_accents() {
        let theme = super::Theme::default();
        assert_eq!(SectionTone::Intent.accent(theme), theme.accent_purple);
        assert_eq!(SectionTone::Actions.accent(theme), theme.accent_yellow);
        assert_eq!(SectionTone::Result.accent(theme), theme.accent_green);
        assert_eq!(SectionTone::Next.accent(theme), theme.accent_cyan);
    }

    #[test]
    fn section_estimate_height_obeys_minimum() {
        assert_eq!(estimate_height("ok", 20, 4), 4);
    }

    #[test]
    fn section_block_renders_left_border_header_and_body() {
        let actual = element! {
            View(width: 22) {
                SectionBlock(
                    tone: SectionTone::Next,
                    icon: "→",
                    title: "Next",
                    body: "Ship the migration.",
                )
            }
        }
        .to_string();

        assert_eq!(
            actual,
            "│ → NEXT              \n│                     \n│ Ship the migration. \n"
        );
    }
}
