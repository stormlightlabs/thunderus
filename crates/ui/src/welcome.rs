use crate::app::ScreenAction;
use crate::theme::resolve_theme;
use crate::{HintBar, HintToken, InputField, SuggestionCard};
use ::iocraft::prelude::*;
use std::path::Path;

const ASCII_LOGO: &str = r#"
▗▄▄▄▖▗▖ ▗▖▗▖ ▗▖▗▖  ▗▖▗▄▄▄ ▗▄▄▄▖▗▄▄▖ ▗▖ ▗▖ ▗▄▄▖
  █  ▐▌ ▐▌▐▌ ▐▌▐▛▚▖▐▌▐▌  █▐▌   ▐▌ ▐▌▐▌ ▐▌▐▌
  █  ▐▛▀▜▌▐▌ ▐▌▐▌ ▝▜▌▐▌  █▐▛▀▀▘▐▛▀▚▖▐▌ ▐▌ ▝▀▚▖
  █  ▐▌ ▐▌▝▚▄▞▘▐▌  ▐▌▐▙▄▄▀▐▙▄▄▖▐▌ ▐▌▝▚▄▞▘▗▄▄▞▘
"#;
const CONTINUATION_PREFIX: &str = "  ";
const DEFAULT_WELCOME_SUGGESTION: &str = "What is your name?";
const INIT_PROMPT: &str = include_str!("../../../meta/INIT.txt");
const PROMPT_PREFIX: &str = "❯ ";
const README_IMPROVEMENT_SUGGESTION: &str = "Read README.md and suggest improvements.";
const WELCOME_GREETING: &str = "Thunderus - What can I help you build?";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WelcomeState {
    pub input_buffer: String,
    pub cursor_position: usize,
    pub selected_suggestion: Option<usize>,
    pub suggestions: Vec<String>,
}

impl Default for WelcomeState {
    fn default() -> Self {
        let workspace_root = std::env::current_dir().unwrap_or_default();
        Self {
            input_buffer: String::new(),
            cursor_position: 0,
            selected_suggestion: None,
            suggestions: build_welcome_suggestions(&workspace_root),
        }
    }
}

impl WelcomeState {
    pub(crate) fn clear_input(&mut self) {
        self.input_buffer.clear();
        self.cursor_position = 0;
        self.selected_suggestion = None;
    }

    pub(crate) fn insert_char(&mut self, ch: char) {
        self.selected_suggestion = None;
        self.input_buffer.insert(self.cursor_position, ch);
        self.cursor_position += 1;
    }

    pub(crate) fn insert_newline(&mut self) {
        self.selected_suggestion = None;
        self.input_buffer.insert(self.cursor_position, '\n');
        self.cursor_position += 1;
    }

    pub(crate) fn move_left(&mut self) {
        self.selected_suggestion = None;
        self.cursor_position = self.cursor_position.saturating_sub(1);
    }

    pub(crate) fn move_right(&mut self) {
        self.selected_suggestion = None;
        self.cursor_position = self.cursor_position.saturating_add(1).min(self.input_buffer.len());
    }

    pub(crate) fn move_cursor_to_end(&mut self) {
        self.selected_suggestion = None;
        self.cursor_position = self.input_buffer.len();
    }

    pub(crate) fn backspace(&mut self) {
        self.selected_suggestion = None;
        if self.cursor_position > 0 {
            self.cursor_position -= 1;
            self.input_buffer.remove(self.cursor_position);
        }
    }

    pub(crate) fn select_previous_suggestion(&mut self) {
        if self.suggestions.is_empty() {
            self.selected_suggestion = None;
            return;
        }

        self.selected_suggestion = match self.selected_suggestion {
            None => Some(0),
            Some(0) => Some(self.suggestions.len() - 1),
            Some(idx) => Some(idx - 1),
        };
    }

    pub(crate) fn select_next_suggestion(&mut self) {
        if self.suggestions.is_empty() {
            self.selected_suggestion = None;
            return;
        }

        self.selected_suggestion = match self.selected_suggestion {
            None => Some(0),
            Some(idx) if idx + 1 >= self.suggestions.len() => Some(0),
            Some(idx) => Some(idx + 1),
        };
    }

    pub(crate) fn submit(&mut self) -> Option<Submission> {
        if self.input_buffer.is_empty() && self.selected_suggestion.is_none() {
            return None;
        }

        let content = if self.input_buffer.is_empty() {
            let idx = self.selected_suggestion.unwrap_or(0);
            self.suggestions
                .get(idx)
                .cloned()
                .or_else(|| self.suggestions.first().cloned())
                .unwrap_or_default()
        } else {
            self.input_buffer.clone()
        };

        self.clear_input();
        if content.starts_with('/') {
            Some(Submission::Command(content))
        } else {
            Some(Submission::Prompt(content))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Submission {
    Prompt(String),
    Command(String),
}

#[derive(Default, Props)]
pub(crate) struct WelcomeScreenProps<'a> {
    pub(crate) model: WelcomeState,
    pub(crate) overlay: Option<AnyElement<'a>>,
}

#[derive(Clone)]
struct RenderedInputLine {
    prefix: &'static str,
    prefix_color: RenderedPrefixColor,
    text: String,
}

#[derive(Clone, Copy)]
enum RenderedPrefixColor {
    Prompt,
    Continuation,
}

#[component]
pub(crate) fn WelcomeScreen<'a>(hooks: Hooks, props: &mut WelcomeScreenProps<'a>) -> impl Into<AnyElement<'a>> {
    let theme = resolve_theme(&hooks);
    let input_lines = rendered_input_lines(&props.model.input_buffer, props.model.cursor_position);

    element! {
        View(
            width: 100pct,
            height: 100pct,
            flex_direction: FlexDirection::Column,
            background_color: theme.bg_terminal,
            position: Position::Relative,
        ) {
            View(
                flex_grow: 1.0,
                justify_content: JustifyContent::Center,
                padding_left: 2,
                padding_right: 2,
            ) {
                View(
                    width: 64,
                    max_width: 100pct,
                    flex_direction: FlexDirection::Column,
                    gap: 1,
                ) {
                    Text(
                        content: logo_text(),
                        color: theme.accent_cyan,
                        wrap: TextWrap::NoWrap,
                        align: TextAlign::Center,
                    )
                    Text(
                        content: WELCOME_GREETING,
                        color: theme.text_primary,
                        weight: Weight::Bold,
                        align: TextAlign::Center,
                    )
                    Text(content: "TRY ASKING", color: theme.text_muted, align: TextAlign::Center)
                    View(width: 100pct, flex_direction: FlexDirection::Column, gap: 1) {
                        #(props.model.suggestions.iter().enumerate().map(|(idx, suggestion)| {
                            element! {
                                SuggestionCard(
                                    icon: "›",
                                    label: suggestion.clone(),
                                    selected: props.model.selected_suggestion == Some(idx),
                                )
                            }
                        }))
                    }
                }
            }
            HintBar(tokens: default_hint_tokens())
            InputField(prompt: "".to_string(), value: "".to_string(), has_focus: false, multiline: true, on_change: |_| {}) {
                #(input_lines.into_iter().map(|line| {
                    let prefix_color = match line.prefix_color {
                        RenderedPrefixColor::Prompt => theme.accent_cyan,
                        RenderedPrefixColor::Continuation => theme.text_muted,
                    };

                    element! {
                        MixedText(contents: vec![
                            MixedTextContent::new(line.prefix).color(prefix_color),
                            MixedTextContent::new(line.text).color(theme.text_primary),
                        ])
                    }
                }))
            }
            #(if props.overlay.is_some() {
                Some(element! {
                    View(position: Position::Absolute, top: 0, left: 0, width: 100pct, height: 100pct) {
                        #(props.overlay.iter_mut())
                    }
                }.into_any())
            } else {
                None
            })
        }
    }
}

pub(crate) fn handle_key(state: &mut WelcomeState, key: &KeyEvent) -> WelcomeOutcome {
    if key.kind != KeyEventKind::Press {
        return WelcomeOutcome::None;
    }

    match key.code {
        KeyCode::Char('c') | KeyCode::Char('d') | KeyCode::Char('q')
            if key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            WelcomeOutcome::Action(ScreenAction::Quit)
        }
        KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.clear_input();
            WelcomeOutcome::None
        }
        KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.move_cursor_to_end();
            WelcomeOutcome::None
        }
        KeyCode::Char('@') => WelcomeOutcome::ActivateFileFinder,
        KeyCode::Char(ch)
            if !key.modifiers.contains(KeyModifiers::CONTROL)
                && !key.modifiers.contains(KeyModifiers::ALT)
                && !key.modifiers.contains(KeyModifiers::SUPER) =>
        {
            state.insert_char(ch);
            WelcomeOutcome::None
        }
        KeyCode::Backspace => {
            state.backspace();
            WelcomeOutcome::None
        }
        KeyCode::Left => {
            state.move_left();
            WelcomeOutcome::None
        }
        KeyCode::Right => {
            state.move_right();
            WelcomeOutcome::None
        }
        KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
            state.insert_newline();
            WelcomeOutcome::None
        }
        KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.insert_newline();
            WelcomeOutcome::None
        }
        KeyCode::Enter => match state.submit() {
            Some(Submission::Prompt(content)) => WelcomeOutcome::Prompt(content),
            Some(Submission::Command(command)) => WelcomeOutcome::Command(command),
            None => WelcomeOutcome::None,
        },
        KeyCode::Up => {
            state.select_previous_suggestion();
            WelcomeOutcome::None
        }
        KeyCode::Down => {
            state.select_next_suggestion();
            WelcomeOutcome::None
        }
        _ => WelcomeOutcome::None,
    }
}

pub(crate) enum WelcomeOutcome {
    None,
    Prompt(String),
    Command(String),
    ActivateFileFinder,
    Action(ScreenAction),
}

fn rendered_input_lines(input: &str, cursor_position: usize) -> Vec<RenderedInputLine> {
    input_with_cursor(input, cursor_position)
        .split('\n')
        .enumerate()
        .map(|(idx, segment)| RenderedInputLine {
            prefix: if idx == 0 { PROMPT_PREFIX } else { CONTINUATION_PREFIX },
            prefix_color: if idx == 0 { RenderedPrefixColor::Prompt } else { RenderedPrefixColor::Continuation },
            text: segment.to_string(),
        })
        .collect()
}

fn input_with_cursor(input: &str, cursor_position: usize) -> String {
    let mut output = input.to_string();
    let cursor = cursor_position.min(output.len());
    output.insert(cursor, '\u{2588}');
    output
}

fn logo_text() -> String {
    ASCII_LOGO.trim_matches('\n').to_string()
}

fn default_hint_tokens() -> Vec<HintToken> {
    vec![
        HintToken::Text("Type ".to_string()),
        HintToken::Key("/help".to_string()),
        HintToken::Text(" for help, ".to_string()),
        HintToken::Key("ctrl+,".to_string()),
        HintToken::Text(" settings, ".to_string()),
        HintToken::Key("ctrl+n".to_string()),
        HintToken::Text(" new chat, ".to_string()),
        HintToken::Key("ctrl+o".to_string()),
        HintToken::Text(" files, ".to_string()),
        HintToken::Key("shift+enter".to_string()),
        HintToken::Text("/".to_string()),
        HintToken::Key("ctrl+j".to_string()),
        HintToken::Text(" newline, ".to_string()),
        HintToken::Key("@".to_string()),
        HintToken::Text(" pin files, ".to_string()),
        HintToken::Key("ctrl+d".to_string()),
        HintToken::Text(" quit".to_string()),
    ]
}

fn build_welcome_suggestions(workspace_root: &Path) -> Vec<String> {
    let mut suggestions = vec![DEFAULT_WELCOME_SUGGESTION.to_string()];
    if has_agents_md(workspace_root) {
        suggestions.push(README_IMPROVEMENT_SUGGESTION.to_string());
    } else {
        suggestions.push(init_prompt_suggestion());
    }
    suggestions
}

fn has_agents_md(workspace_root: &Path) -> bool {
    workspace_root.join("AGENTS.md").exists() || workspace_root.join("agents.md").exists()
}

fn init_prompt_suggestion() -> String {
    let mut first_candidate: Option<String> = None;

    for line in INIT_PROMPT.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.ends_with(':')
            || trimmed.starts_with('-')
            || trimmed.starts_with('`')
            || trimmed.starts_with('"')
            || trimmed.chars().next().is_some_and(|ch| ch.is_ascii_digit())
        {
            continue;
        }

        if first_candidate.is_none() {
            first_candidate = Some(trimmed.to_string());
        }

        if trimmed.to_ascii_lowercase().contains("analyze this codebase") {
            return trimmed.to_string();
        }
    }

    first_candidate.unwrap_or_else(|| README_IMPROVEMENT_SUGGESTION.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rendered_input_lines_prefix_first_and_continuation_rows() {
        let lines = rendered_input_lines("hi\nthere", 2);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].prefix, "❯ ");
        assert_eq!(lines[0].text, "hi█");
        assert_eq!(lines[1].prefix, "  ");
        assert_eq!(lines[1].text, "there");
    }

    #[test]
    fn default_hints_include_file_finder_shortcut() {
        assert!(
            default_hint_tokens()
                .iter()
                .any(|token| matches!(token, HintToken::Key(value) if value == "@"))
        );
    }

    #[test]
    fn build_welcome_suggestions_hides_agents_prompt_when_agents_exists() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time should be valid")
            .as_nanos();
        let workspace = std::env::temp_dir().join(format!("thndrs-ui-suggestions-{unique}"));
        std::fs::create_dir_all(&workspace).expect("workspace should be created");

        let without_agents = build_welcome_suggestions(&workspace);
        assert!(without_agents[1].to_ascii_lowercase().contains("analyze this codebase"));

        std::fs::write(workspace.join("AGENTS.md"), "# rules\n").expect("agents file should be created");
        let with_agents = build_welcome_suggestions(&workspace);
        assert_eq!(with_agents[1], README_IMPROVEMENT_SUGGESTION);

        std::fs::remove_dir_all(workspace).expect("workspace should be removed");
    }
}
