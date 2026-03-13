use super::theme::resolve_theme;
use ::iocraft::prelude::*;

#[derive(Default, Props)]
pub struct AppShellProps<'a> {
    pub header: Option<AnyElement<'a>>,
    pub body: Option<AnyElement<'a>>,
    pub footer: Option<AnyElement<'a>>,
}

#[component]
pub fn AppShell<'a>(hooks: Hooks, props: &mut AppShellProps<'a>) -> impl Into<AnyElement<'a>> {
    let theme = resolve_theme(&hooks);

    element! {
        View(
            flex_direction: FlexDirection::Column,
            flex_grow: 1.0,
            background_color: theme.bg_primary,
        ) {
            View(height: 1, padding_left: 1, padding_right: 1, background_color: theme.bg_secondary) {
                #(props.header.iter_mut())
            }
            View(
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                padding_left: 1,
                padding_right: 1,
                background_color: theme.bg_terminal,
            ) {
                #(props.body.iter_mut())
            }
            View(height: 1, padding_left: 1, padding_right: 1, background_color: theme.bg_secondary) {
                #(props.footer.iter_mut())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AppShell;
    use ::iocraft::prelude::*;

    #[test]
    fn shell_stacks_header_body_and_footer() {
        let actual = element! {
            View(width: 14, height: 4) {
                AppShell(
                    header: Some(element!(Text(content: "HEAD")).into_any()),
                    body: Some(element!(Text(content: "BODY")).into_any()),
                    footer: Some(element!(Text(content: "FOOT")).into_any()),
                )
            }
        }
        .to_string();

        assert_eq!(
            actual,
            " HEAD         \n BODY         \n              \n FOOT         \n"
        );
    }

    #[test]
    fn shell_renders_blank_regions_when_sections_are_missing() {
        let actual = element! {
            View(width: 8, height: 3) {
                AppShell
            }
        }
        .to_string();

        assert_eq!(actual, "        \n        \n        \n");
    }
}
