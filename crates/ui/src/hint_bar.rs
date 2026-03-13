use super::theme::resolve_theme;
use ::iocraft::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HintToken {
    Text(String),
    Key(String),
}

#[derive(Default, Props)]
pub struct HintBarProps {
    pub tokens: Vec<HintToken>,
}

#[component]
pub fn HintBar(hooks: Hooks, props: &HintBarProps) -> impl Into<AnyElement<'static>> {
    let theme = resolve_theme(&hooks);
    let contents = props
        .tokens
        .iter()
        .map(|token| match token {
            HintToken::Text(text) => MixedTextContent::new(text).color(theme.text_muted),
            HintToken::Key(key) => MixedTextContent::new(key).color(theme.accent_cyan),
        })
        .collect::<Vec<_>>();

    element! {
        View(
            width: 100pct,
            height: 1,
            justify_content: JustifyContent::Center,
            background_color: theme.bg_secondary,
        ) {
            MixedText(align: TextAlign::Center, contents: contents)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{HintBar, HintToken};
    use ::iocraft::prelude::*;

    #[test]
    fn hint_bar_centers_key_tokens() {
        let actual = element! {
            View(width: 22) {
                HintBar(tokens: vec![
                    HintToken::Key("↑/↓".to_string()),
                    HintToken::Text(" move ".to_string()),
                    HintToken::Key("Enter".to_string()),
                    HintToken::Text(" submit".to_string()),
                ])
            }
        }
        .to_string();

        assert_eq!(actual, " ↑/↓ move Enter submit\n");
    }
}
