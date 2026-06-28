//! Application state, message types, and the `update` function.
//!
//! This follows the Elm architecture (TEA):
//! - `update(&mut App, Msg) -> Option<Msg>` is the only mutation path.

use std::path::PathBuf;

use crate::cli::{Cli, WebSearchMode};

use crossterm::event::KeyCode;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

/// Top-level interaction mode.
///
/// TODO: Command/Help
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum Mode {
    /// Normal prompt entry.
    #[default]
    Prompt,
    /// Slash-command entry.
    Command,
    /// Help overlay.
    Help,
}

/// Semantic run state, used for the sidebar/status line.
///
/// TODO: Working/Stopping/Error
#[derive(Clone, Debug, Eq, PartialEq, Default)]
#[allow(dead_code)]
pub enum RunState {
    /// Nothing in flight.
    #[default]
    Idle,
    /// Agent stream active.
    Working,
    /// A stop has been requested; stream is winding down.
    Stopping,
    /// A recoverable error occurred.
    Error(String),
}

impl RunState {
    pub fn label(&self) -> &'static str {
        match self {
            RunState::Idle => "idle",
            RunState::Working => "working",
            RunState::Stopping => "stopping",
            RunState::Error(_) => "error",
        }
    }
}

/// Status of a tool entry in the transcript.
///
/// TODO: Ok/Failed
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
#[allow(dead_code)]
pub enum ToolStatus {
    /// Tool started, not yet finished.
    #[default]
    Running,
    /// Tool finished successfully.
    Ok,
    /// Tool failed.
    Failed,
}

/// One transcript row.
///
/// TODO: Entry variants populated
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub enum Entry {
    /// User-submitted text.
    User { text: String },
    /// Assistant text, possibly still streaming.
    Assistant { text: String, streaming: bool },
    /// Reasoning/thinking text, kept separate from final assistant text.
    Reasoning { text: String, streaming: bool },
    /// A tool call block.
    Tool {
        name: String,
        status: ToolStatus,
        output: Vec<String>,
    },
    /// A status row (e.g. context sources loaded).
    Status { text: String },
    /// An error row.
    Error { text: String },
}

impl Entry {
    pub fn to_line(&self) -> Line<'_> {
        match self {
            Entry::User { text } => Line::from(vec![
                Span::styled("user     ", Style::default().fg(Color::Blue)),
                Span::raw(text.as_str()),
            ]),
            Entry::Assistant { text, streaming: _ } => Line::from(vec![
                Span::styled("assistant ", Style::default().fg(Color::Green)),
                Span::raw(text.as_str()),
            ]),
            Entry::Reasoning { text, streaming: _ } => Line::from(vec![
                Span::styled("reasoning ", Style::default().fg(Color::Magenta)),
                Span::raw(text.as_str()),
            ]),
            Entry::Tool { name, status, .. } => {
                let status_label = match status {
                    crate::app::ToolStatus::Running => "running",
                    crate::app::ToolStatus::Ok => "ok",
                    crate::app::ToolStatus::Failed => "failed",
                };
                Line::from(vec![
                    Span::styled("tool     ", Style::default().fg(Color::Yellow)),
                    Span::raw(format!("{name} [{status_label}]")),
                ])
            }
            Entry::Status { text } => Line::from(vec![
                Span::styled("status   ", Style::default().fg(Color::DarkGray)),
                Span::raw(text.as_str()),
            ]),
            Entry::Error { text } => Line::from(vec![
                Span::styled("error    ", Style::default().fg(Color::Red)),
                Span::raw(text.as_str()),
            ]),
        }
    }
}

/// Sidebar model
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct Sidebar {
    /// Display names of known sessions, newest first.
    pub sessions: Vec<String>,
    /// Index of the active session, if any.
    pub active: Option<usize>,
}

impl Sidebar {
    pub fn placeholder() -> Self {
        Sidebar { sessions: vec![String::from("scratch")], active: Some(0) }
    }
}

/// The full application state used to draw the screen.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct App {
    pub mode: Mode,
    pub run_state: RunState,
    pub input: String,
    pub transcript: Vec<Entry>,
    pub sidebar: Sidebar,
    pub view: crate::ui::ViewState,
    pub cwd: PathBuf,
    pub model: String,
    pub websearch: WebSearchMode,
    /// When true the loop should stop and the app exit.
    pub quit: bool,
}

impl App {
    /// Build the initial app from parsed CLI args.
    ///
    /// TODO: this could be a from/into impl
    pub fn from_cli(cli: &Cli) -> Self {
        App {
            mode: Mode::default(),
            run_state: RunState::default(),
            input: String::new(),
            transcript: Vec::new(),
            sidebar: Sidebar::placeholder(),
            view: crate::ui::ViewState::default(),
            cwd: cli.cwd.clone(),
            model: cli.model.clone(),
            websearch: cli.websearch,
            quit: false,
        }
    }
}

/// Events from the background agent stream.
///
/// TODO: Agent events
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub enum AgentEvent {
    Started,
    AssistantDelta(String),
    ReasoningDelta(String),
    ToolStarted { name: String },
    ToolOutput { line: String },
    ToolFinished,
    Finished,
    Failed(String),
}

/// The single message type fed into `update`.
///
/// TODO: Submit/Clear/Agent
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub enum Msg {
    /// A raw key event from the terminal.
    Key(crossterm::event::KeyEvent),
    /// Periodic tick.
    Tick,
    /// Submit the current input.
    Submit,
    /// Clear the transcript.
    Clear,
    /// Quit the app.
    Quit,
    /// An agent stream event.
    Agent(AgentEvent),
}

/// The only mutation path. Returns an optional follow-up message.
///
/// `q`, `Ctrl+D`, and `Ctrl+C` quit
///
/// TODO: All other input, submit, clear, and agent behavior
/// FIXME: don't allow this
#[allow(clippy::needless_pass_by_value)]
pub fn update(app: &mut App, msg: Msg) -> Option<Msg> {
    match msg {
        Msg::Key(key) => match key.code {
            KeyCode::Char('q') => {
                app.quit = true;
                Some(Msg::Quit)
            }
            KeyCode::Char('c') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                app.quit = true;
                Some(Msg::Quit)
            }
            KeyCode::Char('d') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                app.quit = true;
                Some(Msg::Quit)
            }
            _ => None,
        },
        Msg::Quit => {
            app.quit = true;
            None
        }
        Msg::Tick => None,
        Msg::Submit | Msg::Clear | Msg::Agent(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn fresh_app() -> App {
        App::from_cli(&Cli::default())
    }

    #[test]
    fn quit_key_sets_quit_flag() {
        let mut app = fresh_app();
        let follow = update(
            &mut app,
            Msg::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)),
        );
        assert!(app.quit);
        assert_eq!(follow, Some(Msg::Quit));
    }

    #[test]
    fn ctrl_c_sets_quit_flag() {
        let mut app = fresh_app();
        update(
            &mut app,
            Msg::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
        );
        assert!(app.quit);
    }

    #[test]
    fn ctrl_d_sets_quit_flag() {
        let mut app = fresh_app();
        update(
            &mut app,
            Msg::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)),
        );
        assert!(app.quit);
    }

    #[test]
    fn other_keys_do_not_quit() {
        let mut app = fresh_app();
        update(
            &mut app,
            Msg::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)),
        );
        assert!(!app.quit);
        update(&mut app, Msg::Tick);
        assert!(!app.quit);
    }

    #[test]
    fn quit_message_sets_quit_flag() {
        let mut app = fresh_app();
        update(&mut app, Msg::Quit);
        assert!(app.quit);
    }

    #[test]
    fn placeholder_sidebar_has_one_active_session() {
        let sidebar = Sidebar::placeholder();
        assert_eq!(sidebar.sessions, vec!["scratch"]);
        assert_eq!(sidebar.active, Some(0));
    }
}
