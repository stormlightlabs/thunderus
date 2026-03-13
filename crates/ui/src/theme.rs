use ::iocraft::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub accent_cyan: Color,
    pub accent_pink: Color,
    pub accent_purple: Color,
    pub accent_green: Color,
    pub accent_yellow: Color,
    pub accent_red: Color,
    pub bg_primary: Color,
    pub bg_secondary: Color,
    pub bg_tertiary: Color,
    pub bg_terminal: Color,
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_muted: Color,
    pub border_color: Color,
}

impl Theme {
    const fn rgb(r: u8, g: u8, b: u8) -> Color {
        Color::Rgb { r, g, b }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            accent_cyan: Self::rgb(0x33, 0xb1, 0xff),
            accent_pink: Self::rgb(0xff, 0x7e, 0xb6),
            accent_purple: Self::rgb(0xbe, 0x95, 0xff),
            accent_green: Self::rgb(0x42, 0xbe, 0x65),
            accent_yellow: Self::rgb(0xf1, 0xc2, 0x1b),
            accent_red: Self::rgb(0xfa, 0x4d, 0x56),
            bg_primary: Self::rgb(0x16, 0x16, 0x16),
            bg_secondary: Self::rgb(0x1c, 0x1c, 0x1c),
            bg_tertiary: Self::rgb(0x26, 0x26, 0x26),
            bg_terminal: Self::rgb(0x0c, 0x0c, 0x0c),
            text_primary: Self::rgb(0xf4, 0xf4, 0xf4),
            text_secondary: Self::rgb(0xc6, 0xc6, 0xc6),
            text_muted: Self::rgb(0x8d, 0x8d, 0x8d),
            border_color: Self::rgb(0x39, 0x39, 0x39),
        }
    }
}

#[derive(Default, Props)]
pub struct ThemeProviderProps<'a> {
    pub children: Vec<AnyElement<'a>>,
    pub theme: Theme,
}

#[component]
pub fn ThemeProvider<'a>(props: &mut ThemeProviderProps<'a>) -> impl Into<AnyElement<'a>> {
    let theme = props.theme;

    element! {
        ContextProvider(value: Context::owned(theme)) {
            #(props.children.drain(..))
        }
    }
}

pub fn resolve_theme(hooks: &Hooks<'_, '_>) -> Theme {
    hooks.try_use_context::<Theme>().map(|theme| *theme).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{Theme, ThemeProvider, resolve_theme};
    use ::iocraft::prelude::*;

    #[component]
    fn ThemeEcho(hooks: Hooks) -> impl Into<AnyElement<'static>> {
        let theme = resolve_theme(&hooks);
        let label = if theme == Theme::default() { "default" } else { "custom" };

        element! {
            Text(content: label)
        }
    }

    #[test]
    fn default_theme_matches_oxocarbon_palette() {
        let theme = Theme::default();
        assert_eq!(theme.accent_cyan, Color::Rgb { r: 0x33, g: 0xb1, b: 0xff });
        assert_eq!(theme.bg_primary, Color::Rgb { r: 0x16, g: 0x16, b: 0x16 });
        assert_eq!(theme.text_primary, Color::Rgb { r: 0xf4, g: 0xf4, b: 0xf4 });
    }

    #[test]
    fn theme_provider_makes_theme_available_via_context() {
        let custom_theme = Theme { accent_cyan: Color::Red, ..Theme::default() };

        let actual = element! {
            ThemeProvider(theme: custom_theme) {
                ThemeEcho
            }
        }
        .to_string();

        assert_eq!(actual, "custom\n");
    }
}
