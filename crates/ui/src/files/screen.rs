use crate::app::ScreenAction;
use crate::files::{FileBrowserAction, FileBrowserApp, FileTreeRow, HighlightSegment, HighlightedLine};
use crate::hint_bar::HintToken;
use crate::theme::{Theme, resolve_theme};
use crate::{HintBar, InputField};
use ::iocraft::prelude::*;
use std::cmp;

const DEFAULT_VIEWPORT_WIDTH: u16 = 100;
const DEFAULT_VIEWPORT_HEIGHT: u16 = 28;
const HINT_ROW_HEIGHT: u16 = 1;
const STATUS_ROW_HEIGHT: u16 = 2;
const FINDER_HEIGHT: u16 = 12;

#[derive(Props)]
pub(crate) struct FileBrowserProps {
    pub(crate) initial_browser: Option<FileBrowserApp>,
    pub(crate) revision: u64,
    pub(crate) active: bool,
    pub(crate) handle_events: bool,
    pub(crate) viewport_width: u16,
    pub(crate) viewport_height: u16,
    pub(crate) on_action: HandlerMut<'static, ScreenAction>,
}

impl Default for FileBrowserProps {
    fn default() -> Self {
        Self {
            initial_browser: None,
            revision: 0,
            active: true,
            handle_events: true,
            viewport_width: 0,
            viewport_height: 0,
            on_action: HandlerMut::default(),
        }
    }
}

struct FileBrowserCallbacks {
    on_action: HandlerMut<'static, ScreenAction>,
}

#[component]
pub(crate) fn FileBrowser(mut hooks: Hooks, props: &mut FileBrowserProps) -> impl Into<AnyElement<'static>> {
    let theme = resolve_theme(&hooks);
    let (terminal_width, terminal_height) = hooks.use_terminal_size();
    let viewport_width = resolve_dimension(props.viewport_width, terminal_width, DEFAULT_VIEWPORT_WIDTH);
    let viewport_height = resolve_dimension(props.viewport_height, terminal_height, DEFAULT_VIEWPORT_HEIGHT);
    let mut model = hooks.use_state({
        let initial_browser = props.initial_browser.clone();
        move || initial_browser.unwrap_or_default()
    });
    let callbacks = hooks.use_ref(|| FileBrowserCallbacks { on_action: props.on_action.take() });

    hooks.use_effect(
        {
            let mut model = model;
            let initial_browser = props.initial_browser.clone();
            move || {
                model.set(initial_browser.clone().unwrap_or_default());
            }
        },
        [props.revision],
    );

    hooks.use_terminal_events({
        let mut model = model;
        let mut callbacks = callbacks;
        let active = props.active;
        let handle_events = props.handle_events;
        move |event| {
            if !active || !handle_events {
                return;
            }
            if let TerminalEvent::Key(key) = event
                && key.kind == KeyEventKind::Press
            {
                dispatch_file_browser_key(&mut model, &mut callbacks, &key);
            }
        }
    });

    let main_height = viewport_height
        .saturating_sub(HINT_ROW_HEIGHT)
        .saturating_sub(STATUS_ROW_HEIGHT)
        .max(1);
    let page_size = main_height.saturating_sub(3).max(1) as usize;
    let mut snapshot = model.read().clone();
    if snapshot.sync_viewports(page_size, page_size) {
        model.set(snapshot.clone());
    }

    let tree_rows = snapshot.tree_rows();
    let content_rows = snapshot.content_rows();
    let content_line_number_width = snapshot.content_line_number_width();
    let status_line = truncate_text(snapshot.status_line(), viewport_width.saturating_sub(4) as usize);
    let hint_tokens = hint_tokens(snapshot.is_finder_active());

    element! {
        View(
            width: viewport_width,
            height: viewport_height,
            flex_direction: FlexDirection::Column,
            background_color: theme.bg_terminal,
            position: Position::Relative,
        ) {
            View(
                height: main_height,
                width: 100pct,
                flex_direction: FlexDirection::Row,
                gap: 1,
                padding_left: 1,
                padding_right: 1,
                padding_top: 1,
            ) {
                #(tree_pane(&snapshot, &tree_rows, theme))
                #(content_pane(&snapshot, &content_rows, content_line_number_width, theme))
            }
            HintBar(tokens: hint_tokens)
            View(height: STATUS_ROW_HEIGHT, width: 100pct) {
                InputField(prompt: "", value: "", has_focus: false, multiline: false, on_change: |_| {}) {
                    Text(content: status_line, color: theme.text_muted, wrap: TextWrap::NoWrap)
                }
            }
            #(if snapshot.is_finder_active() {
                Some(finder_overlay(&snapshot, viewport_width, viewport_height, theme))
            } else {
                None
            })
        }
    }
}

fn resolve_dimension(explicit: u16, measured: u16, fallback: u16) -> u16 {
    if explicit > 0 {
        explicit
    } else if measured > 0 {
        measured
    } else {
        fallback
    }
}

fn dispatch_file_browser_key(
    model: &mut State<FileBrowserApp>, callbacks: &mut Ref<FileBrowserCallbacks>, key: &KeyEvent,
) {
    let mut next = model.read().clone();
    let action = next.handle_input(crossterm_key_event(key));

    if let Some(screen_action) = map_file_browser_action(action) {
        let mut callbacks = callbacks.write();
        (callbacks.on_action)(screen_action);
    }

    model.set(next);
}

fn crossterm_key_event(key: &KeyEvent) -> crossterm::event::KeyEvent {
    crossterm::event::KeyEvent {
        code: key.code,
        modifiers: key.modifiers,
        kind: match key.kind {
            KeyEventKind::Press => crossterm::event::KeyEventKind::Press,
            KeyEventKind::Repeat => crossterm::event::KeyEventKind::Repeat,
            KeyEventKind::Release => crossterm::event::KeyEventKind::Release,
        },
        state: crossterm::event::KeyEventState::NONE,
    }
}

fn map_file_browser_action(action: FileBrowserAction) -> Option<ScreenAction> {
    match action {
        FileBrowserAction::None => None,
        FileBrowserAction::Quit => Some(ScreenAction::Quit),
        FileBrowserAction::ExitToChat => Some(ScreenAction::ReturnToPrevious),
    }
}

fn tree_pane(snapshot: &FileBrowserApp, rows: &[FileTreeRow], theme: Theme) -> AnyElement<'static> {
    element! {
        View(
            width: 30pct,
            min_width: 24,
            max_width: 38,
            flex_grow: 1.0,
            flex_direction: FlexDirection::Column,
            border_style: BorderStyle::Round,
            border_color: theme.border_color,
            padding_left: 1,
            padding_right: 1,
        ) {
            Text(content: snapshot.workspace_title(), color: theme.text_muted, weight: Weight::Bold)
            #(rows.iter().map(|row| tree_row(row, theme)))
            #(if rows.is_empty() {
                Some(element! {
                    Text(content: "(empty workspace)", color: theme.text_muted)
                }.into_any())
            } else {
                None
            })
        }
    }
    .into_any()
}

fn tree_row(row: &FileTreeRow, theme: Theme) -> AnyElement<'static> {
    let icon = if row.is_dir { if row.expanded { "v" } else { ">" } } else { "-" };
    let icon_color = if row.is_dir { theme.accent_yellow } else { theme.text_muted };
    let name_color = if row.active {
        theme.accent_cyan
    } else if row.selected {
        theme.text_primary
    } else {
        theme.text_secondary
    };
    let name_weight = if row.active || row.selected { Weight::Bold } else { Weight::Normal };

    element! {
        MixedText(contents: vec![
            MixedTextContent::new(if row.selected { "› " } else { "  " }).color(theme.accent_cyan),
            MixedTextContent::new("  ".repeat(row.depth as usize)),
            MixedTextContent::new(icon).color(icon_color),
            MixedTextContent::new(" "),
            MixedTextContent::new(&row.name).color(name_color).weight(name_weight),
        ])
    }
    .into_any()
}

fn content_pane(
    snapshot: &FileBrowserApp, rows: &[HighlightedLine], line_number_width: usize, theme: Theme,
) -> AnyElement<'static> {
    element! {
        View(width: 70pct, flex_grow: 1.0, flex_direction: FlexDirection::Column, gap: 1) {
            Text(content: snapshot.breadcrumb(), color: theme.accent_cyan, wrap: TextWrap::NoWrap)
            View(
                flex_grow: 1.0,
                width: 100pct,
                flex_direction: FlexDirection::Column,
                border_style: BorderStyle::Round,
                border_color: theme.border_color,
                padding_left: 1,
                padding_right: 1,
            ) {
                #(if rows.is_empty() {
                    Some(element! {
                        Text(
                            content: "Select a file from the tree or open fuzzy finder with @",
                            color: theme.text_muted,
                            wrap: TextWrap::Wrap,
                        )
                    }.into_any())
                } else {
                    None
                })
                #(rows.iter().map(|line| content_row(line, line_number_width, theme)))
            }
        }
    }
    .into_any()
}

fn content_row(line: &HighlightedLine, line_number_width: usize, theme: Theme) -> AnyElement<'static> {
    let mut contents =
        vec![MixedTextContent::new(format!("{:>line_number_width$} ", line.line_number)).color(theme.text_muted)];

    if line.segments.is_empty() {
        contents.push(MixedTextContent::new(" "));
    } else {
        for segment in &line.segments {
            contents.push(highlight_segment_content(segment, theme));
        }
    }

    element! {
        MixedText(contents: contents)
    }
    .into_any()
}

fn highlight_segment_content(segment: &HighlightSegment, _theme: Theme) -> MixedTextContent {
    let content = MixedTextContent::new(segment.text.clone()).color(segment.fg);
    if segment.bold { content.weight(Weight::Bold) } else { content }
}

fn hint_tokens(finder_active: bool) -> Vec<HintToken> {
    if finder_active {
        return vec![
            HintToken::Text("Type to filter, ".to_string()),
            HintToken::Key("Enter".to_string()),
            HintToken::Text(" open, ".to_string()),
            HintToken::Key("Esc".to_string()),
            HintToken::Text(" close".to_string()),
        ];
    }

    vec![
        HintToken::Text("Use ".to_string()),
        HintToken::Key("↑/↓".to_string()),
        HintToken::Text(" move, ".to_string()),
        HintToken::Key("←/→".to_string()),
        HintToken::Text(" collapse/open, ".to_string()),
        HintToken::Key("@".to_string()),
        HintToken::Text(" finder, ".to_string()),
        HintToken::Key("PgUp/PgDn".to_string()),
        HintToken::Text(" scroll, ".to_string()),
        HintToken::Key("Esc".to_string()),
        HintToken::Text(" return".to_string()),
    ]
}

fn finder_overlay(
    snapshot: &FileBrowserApp, viewport_width: u16, viewport_height: u16, theme: Theme,
) -> AnyElement<'static> {
    let overlay_width = cmp::min(72, viewport_width.saturating_sub(4)).max(24);
    let overlay_height = cmp::min(FINDER_HEIGHT, viewport_height.saturating_sub(2)).max(6);
    let left = viewport_width.saturating_sub(overlay_width) / 2;
    let top = viewport_height.saturating_sub(overlay_height) / 2;

    element! {
        View(
            position: Position::Absolute,
            left: left,
            top: top,
            width: overlay_width,
            height: overlay_height,
            flex_direction: FlexDirection::Column,
            border_style: BorderStyle::Round,
            border_color: theme.accent_cyan,
            background_color: theme.bg_terminal,
            padding_left: 1,
            padding_right: 1,
        ) {
            Text(content: "open file", color: theme.accent_cyan, weight: Weight::Bold)
            Text(
                content: format!("@{}", snapshot.finder_query()),
                color: theme.text_primary,
                wrap: TextWrap::NoWrap,
            )
            #(snapshot.finder_rows().into_iter().map(|(selected, path)| {
                let prefix = if selected { "> " } else { "  " };
                let color = if selected { theme.accent_cyan } else { theme.text_secondary };

                element! {
                    Text(content: format!("{prefix}{path}"), color: color, wrap: TextWrap::NoWrap)
                }
            }))
        }
    }
    .into_any()
}

fn truncate_text(value: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }

    let chars = value.chars().collect::<Vec<_>>();
    if chars.len() <= max_chars {
        return value.to_string();
    }

    let keep = max_chars.saturating_sub(1);
    let mut truncated = chars.into_iter().take(keep).collect::<String>();
    truncated.push('…');
    truncated
}

#[cfg(test)]
mod tests {
    use super::FileBrowser;
    use crate::app::ScreenAction;
    use crate::files::FileBrowserApp;
    use ::iocraft::prelude::*;
    use futures::stream::{self, StreamExt};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[derive(Default, Props)]
    struct FileBrowserHarnessProps {
        initial_browser: Option<FileBrowserApp>,
        mode: HarnessMode,
    }

    #[derive(Clone, Copy, Default, PartialEq, Eq)]
    enum HarnessMode {
        #[default]
        TimedExit,
        Quit,
    }

    #[component]
    fn FileBrowserHarness(props: &FileBrowserHarnessProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
        let mut system = hooks.use_context_mut::<SystemContext>();
        let mut quit = hooks.use_state(|| false);
        let mut timed_exit = hooks.use_state(|| false);

        if props.mode == HarnessMode::TimedExit {
            hooks.use_future(async move {
                smol::Timer::after(Duration::from_millis(40)).await;
                timed_exit.set(true);
            });
        }

        if quit.get() || timed_exit.get() {
            system.exit();
            return element! {
                Text(content: if quit.get() { "quit" } else { "timed" })
            }
            .into_any();
        }

        let mode = props.mode;

        element! {
            FileBrowser(
                initial_browser: props.initial_browser.clone(),
                viewport_width: 90u16,
                viewport_height: 24u16,
                on_action: move |action| {
                    if mode == HarnessMode::Quit && action == ScreenAction::Quit {
                        quit.set(true);
                    }
                },
            )
        }
        .into_any()
    }

    #[test]
    fn file_browser_renders_tree_and_preview_content() {
        let workspace = test_workspace();
        let app = FileBrowserApp::new(workspace);
        let actual = element! {
            FileBrowser(initial_browser: Some(app), viewport_width: 90u16, viewport_height: 24u16)
        }
        .to_string();

        assert!(actual.contains("src"));
        assert!(actual.contains("pub fn alpha()"));
        assert!(actual.contains("alpha.rs"));
    }

    #[test]
    fn file_browser_opens_selected_file_from_tree() {
        let workspace = test_workspace();
        let mut app = FileBrowserApp::new(workspace);
        app.handle_input(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Down,
            crossterm::event::KeyModifiers::NONE,
        ));
        app.handle_input(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Down,
            crossterm::event::KeyModifiers::NONE,
        ));
        app.handle_input(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));

        let actual = element! {
            FileBrowser(initial_browser: Some(app), viewport_width: 90u16, viewport_height: 24u16)
        }
        .to_string();

        assert!(actual.contains("pub fn beta()"));
    }

    #[test]
    fn file_browser_opens_fuzzy_finder_overlay() {
        smol::block_on(async {
            let workspace = test_workspace();
            let app = FileBrowserApp::new(workspace);
            let canvases = element! {
                FileBrowserHarness(initial_browser: Some(app))
            }
            .mock_terminal_render_loop(MockTerminalConfig::with_events(stream::iter(vec![TerminalEvent::Key(
                KeyEvent::new(KeyEventKind::Press, KeyCode::Char('@')),
            )])))
            .map(|canvas| canvas.to_string())
            .collect::<Vec<_>>()
            .await;

            assert!(canvases.iter().any(|canvas| canvas.contains("open file")));
            assert!(canvases.iter().any(|canvas| canvas.contains("beta.rs")));
        });
    }

    #[test]
    fn file_browser_emits_quit_action() {
        smol::block_on(async {
            let workspace = test_workspace();
            let app = FileBrowserApp::new(workspace);
            let canvases = element! {
                FileBrowserHarness(initial_browser: Some(app), mode: HarnessMode::Quit)
            }
            .mock_terminal_render_loop(MockTerminalConfig::with_events(stream::iter(vec![TerminalEvent::Key(
                {
                    let mut key = KeyEvent::new(KeyEventKind::Press, KeyCode::Char('d'));
                    key.modifiers = KeyModifiers::CONTROL;
                    key
                },
            )])))
            .map(|canvas| canvas.to_string())
            .collect::<Vec<_>>()
            .await;

            assert!(canvases.iter().any(|canvas| canvas.contains("quit")));
        });
    }

    fn test_workspace() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be valid")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("thndrs-ui-iocraft-files-{unique}"));
        fs::create_dir_all(root.join("src")).expect("test workspace should be created");
        fs::write(
            root.join("src/alpha.rs"),
            "pub fn alpha() {\n    println!(\"alpha\");\n}\n",
        )
        .expect("alpha source should be written");
        fs::write(
            root.join("src/beta.rs"),
            "pub fn beta() {\n    println!(\"beta\");\n}\n",
        )
        .expect("beta source should be written");
        root
    }
}
