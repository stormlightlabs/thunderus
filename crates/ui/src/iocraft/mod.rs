pub mod card;
pub mod chat;
pub mod files;
pub mod hint_bar;
pub mod input_field;
pub mod section_block;
pub mod shell;
pub mod status_bar;
pub mod theme;
pub mod welcome;

pub use card::{SuggestionCard, SuggestionCardProps, required_height};
pub use chat::{ChatScreen, ChatScreenProps};
pub use files::{FileBrowser, FileBrowserProps};
pub use hint_bar::{HintBar, HintBarProps, HintToken};
pub use input_field::{InputField, InputFieldProps};
pub use section_block::{SectionBlock, SectionBlockProps, SectionTone, estimate_height};
pub use shell::{AppShell, AppShellProps};
pub use status_bar::{StatusBar, StatusBarProps, status_parts};
pub use theme::{Theme, ThemeProvider, ThemeProviderProps, resolve_theme};
pub use welcome::{WelcomeScreen, WelcomeScreenProps};

pub(crate) fn wrapped_line_count(content: &str, width: u16) -> usize {
    if width == 0 {
        return 1;
    }

    let width = width as usize;
    let mut total = 0usize;

    for line in content.lines() {
        let chars = line.chars().count();
        total += chars.div_ceil(width).max(1);
    }

    if content.is_empty() { 1 } else { total.max(1) }
}

#[cfg(test)]
mod tests {
    use super::wrapped_line_count;

    #[test]
    fn wrapped_line_count_returns_one_for_empty_content() {
        assert_eq!(wrapped_line_count("", 40), 1);
    }

    #[test]
    fn wrapped_line_count_respects_newlines_and_width() {
        assert_eq!(wrapped_line_count("abcd\nefgh", 2), 4);
    }
}
