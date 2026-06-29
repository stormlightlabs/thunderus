//! Application state, message types, and the `update` function.
//!
//! This follows the Elm architecture (TEA):
//! - `update(&mut App, Msg) -> Option<Msg>` is the only mutation path.

use std::path::PathBuf;

use crate::cli::{Cli, WebSearchMode};

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
/// TODO: Stopping/Error
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
/// TODO: Failed
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
/// TODO: Status variant
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
/// TODO: Failed
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
/// TODO: Submit/Clear
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
/// - Printable chars append to the input buffer.
/// - `Backspace` removes the last char.
/// - `Enter` submits: slash commands (`/clear`, `/quit`) are routed, otherwise
///   the input is appended as [`Entry::User`] and cleared.
/// - `q` quits only when the input is empty (so it stays usable while typing).
/// - `Ctrl+C` and `Ctrl+D` always quit.
///
/// FIXME: Get rid of this clippy warning/address it
#[allow(clippy::needless_pass_by_value)]
pub fn update(app: &mut App, msg: Msg) -> Option<Msg> {
    match msg {
        Msg::Key(key) => handle_key(app, key),
        Msg::Submit => handle_submit(app),
        Msg::Quit => {
            app.quit = true;
            None
        }
        Msg::Tick => None,
        Msg::Clear => {
            app.transcript.clear();
            None
        }
        Msg::Agent(event) => handle_agent_event(app, event),
    }
}

/// - Ctrl+C and Ctrl+D always quit, even mid-input.
/// - `q` quits only when the input buffer is empty, so it doesn't fight typing.
/// - Printable characters append to the input buffer.
/// - Backspace removes the last character.
/// - Enter submits the current input.
/// - Escape cancels an active agent stream.
fn handle_key(app: &mut App, key: crossterm::event::KeyEvent) -> Option<Msg> {
    use crossterm::event::{KeyCode, KeyModifiers};

    match key.code {
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.quit = true;
            Some(Msg::Quit)
        }
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.quit = true;
            Some(Msg::Quit)
        }
        KeyCode::Char('q') if app.input.is_empty() => {
            app.quit = true;
            Some(Msg::Quit)
        }
        KeyCode::Char(ch) => {
            app.input.push(ch);
            None
        }
        KeyCode::Backspace => {
            app.input.pop();
            None
        }
        KeyCode::Enter => handle_submit(app),
        KeyCode::Esc if app.run_state == RunState::Working => {
            cancel_stream(app);
            None
        }
        _ => None,
    }
}

/// Handle an `Enter` submit. Slash commands are routed; otherwise the input is
/// appended as [`Entry::User`] and cleared, and the fake agent stream is started.
///
/// Returns an optional follow-up `Msg`.
fn handle_submit(app: &mut App) -> Option<Msg> {
    if app.run_state != RunState::Idle {
        return None;
    }

    let text = app.input.trim().to_string();
    if text.is_empty() {
        app.input.clear();
        return None;
    }

    if let Some(command) = text.strip_prefix('/') {
        return handle_command(app, command);
    }

    app.transcript.push(Entry::User { text });
    app.input.clear();
    Some(Msg::Agent(AgentEvent::Started))
}

/// Route a slash command (the part after `/`).
fn handle_command(app: &mut App, command: &str) -> Option<Msg> {
    match command {
        "clear" => {
            app.transcript.clear();
            app.input.clear();
            None
        }
        "quit" | "exit" => {
            app.input.clear();
            app.quit = true;
            Some(Msg::Quit)
        }
        _ => None,
    }
}

/// Process an [`AgentEvent`] and mutate `app` accordingly.
fn handle_agent_event(app: &mut App, event: AgentEvent) -> Option<Msg> {
    match event {
        AgentEvent::Started => {
            app.run_state = RunState::Working;
            None
        }
        AgentEvent::AssistantDelta(delta) => {
            if let Some(Entry::Assistant { text, streaming: true }) = app.transcript.last_mut() {
                text.push_str(&delta);
            } else {
                app.transcript.push(Entry::Assistant { text: delta, streaming: true });
            }
            None
        }
        AgentEvent::ReasoningDelta(delta) => {
            if let Some(Entry::Reasoning { text, streaming: true }) = app.transcript.last_mut() {
                text.push_str(&delta);
            } else {
                app.transcript.push(Entry::Reasoning { text: delta, streaming: true });
            }
            None
        }
        AgentEvent::ToolStarted { name } => {
            app.transcript
                .push(Entry::Tool { name, status: ToolStatus::Running, output: Vec::new() });
            None
        }
        AgentEvent::ToolOutput { line } => {
            if let Some(Entry::Tool { output, .. }) = app.transcript.last_mut() {
                output.push(line);
            }
            None
        }
        AgentEvent::ToolFinished => {
            if let Some(Entry::Tool { status, .. }) = app.transcript.last_mut() {
                *status = ToolStatus::Ok;
            }
            None
        }
        AgentEvent::Finished => {
            finalize_streaming(app);
            app.run_state = RunState::Idle;
            None
        }
        AgentEvent::Failed(msg) => {
            finalize_streaming(app);
            app.transcript.push(Entry::Error { text: msg });
            app.run_state = RunState::Idle;
            None
        }
    }
}

/// Cancel an active stream by marking all streaming entries complete and
/// returning to idle.
///
/// The app loop drops the channel receiver after this.
fn cancel_stream(app: &mut App) {
    finalize_streaming(app);
    app.run_state = RunState::Idle;
}

/// Mark all streaming `Assistant` and `Reasoning` entries as complete.
fn finalize_streaming(app: &mut App) {
    for entry in &mut app.transcript {
        match entry {
            Entry::Assistant { streaming, .. } => *streaming = false,
            Entry::Reasoning { streaming, .. } => *streaming = false,
            _ => {}
        }
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

    #[test]
    fn printable_chars_append_to_input() {
        let mut app = fresh_app();
        for ch in "hello".chars() {
            update(&mut app, Msg::Key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE)));
        }
        assert_eq!(app.input, "hello");
        assert!(app.transcript.is_empty());
    }

    #[test]
    fn backspace_removes_last_char() {
        let mut app = fresh_app();
        app.input = String::from("abc");
        update(
            &mut app,
            Msg::Key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)),
        );
        assert_eq!(app.input, "ab");
    }

    #[test]
    fn backspace_on_empty_input_is_noop() {
        let mut app = fresh_app();
        update(
            &mut app,
            Msg::Key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)),
        );
        assert_eq!(app.input, "");
    }

    #[test]
    fn enter_submits_user_entry_and_clears_input() {
        let mut app = fresh_app();
        app.input = String::from("explain this repo");
        update(&mut app, Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
        assert_eq!(app.input, "");
        assert_eq!(app.transcript.len(), 1);
        assert_eq!(
            app.transcript[0],
            Entry::User { text: String::from("explain this repo") }
        );
    }

    #[test]
    fn enter_on_empty_input_does_nothing() {
        let mut app = fresh_app();
        update(&mut app, Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
        assert_eq!(app.input, "");
        assert!(app.transcript.is_empty());
    }

    #[test]
    fn enter_trims_whitespace_before_submit() {
        let mut app = fresh_app();
        app.input = String::from("  hello  ");
        update(&mut app, Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
        assert_eq!(app.input, "");
        assert_eq!(app.transcript.len(), 1);
        assert_eq!(app.transcript[0], Entry::User { text: String::from("hello") });
    }

    #[test]
    fn slash_clear_clears_transcript_and_input() {
        let mut app = fresh_app();
        app.transcript.push(Entry::User { text: String::from("old") });
        app.input = String::from("/clear");
        update(&mut app, Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
        assert!(app.transcript.is_empty());
        assert_eq!(app.input, "");
        assert!(!app.quit);
    }

    #[test]
    fn slash_quit_sets_quit_flag() {
        let mut app = fresh_app();
        app.input = String::from("/quit");
        let follow = update(&mut app, Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
        assert!(app.quit);
        assert_eq!(follow, Some(Msg::Quit));
        assert_eq!(app.input, "");
    }

    #[test]
    fn slash_exit_also_quits() {
        let mut app = fresh_app();
        app.input = String::from("/exit");
        update(&mut app, Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
        assert!(app.quit);
    }

    #[test]
    fn unknown_slash_command_is_ignored() {
        let mut app = fresh_app();
        app.input = String::from("/bogus");
        update(&mut app, Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
        assert!(!app.quit);
        assert!(app.transcript.is_empty());
        assert_eq!(app.input, "/bogus");
    }

    #[test]
    fn msg_clear_clears_transcript() {
        let mut app = fresh_app();
        app.transcript.push(Entry::User { text: String::from("a") });
        app.transcript.push(Entry::User { text: String::from("b") });
        update(&mut app, Msg::Clear);
        assert!(app.transcript.is_empty());
    }

    #[test]
    fn q_does_not_quit_while_typing() {
        let mut app = fresh_app();
        app.input = String::from("query");
        update(
            &mut app,
            Msg::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)),
        );
        assert!(!app.quit);
        assert_eq!(app.input, "queryq");
    }

    #[test]
    fn q_quits_when_input_empty() {
        let mut app = fresh_app();
        let follow = update(
            &mut app,
            Msg::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)),
        );
        assert!(app.quit);
        assert_eq!(follow, Some(Msg::Quit));
    }

    #[test]
    fn agent_started_sets_working() {
        let mut app = fresh_app();
        update(&mut app, Msg::Agent(AgentEvent::Started));
        assert_eq!(app.run_state, RunState::Working);
    }

    #[test]
    fn assistant_delta_creates_streaming_entry() {
        let mut app = fresh_app();
        update(&mut app, Msg::Agent(AgentEvent::AssistantDelta(String::from("Hello"))));
        assert_eq!(app.transcript.len(), 1);
        assert_eq!(
            app.transcript[0],
            Entry::Assistant { text: String::from("Hello"), streaming: true }
        );
    }

    #[test]
    fn assistant_delta_appends_to_existing_streaming_entry() {
        let mut app = fresh_app();
        update(&mut app, Msg::Agent(AgentEvent::AssistantDelta(String::from("Hello "))));
        update(&mut app, Msg::Agent(AgentEvent::AssistantDelta(String::from("world"))));
        assert_eq!(app.transcript.len(), 1);
        assert_eq!(
            app.transcript[0],
            Entry::Assistant { text: String::from("Hello world"), streaming: true }
        );
    }

    #[test]
    fn assistant_delta_creates_new_entry_after_finished() {
        let mut app = fresh_app();
        update(&mut app, Msg::Agent(AgentEvent::AssistantDelta(String::from("first"))));
        update(&mut app, Msg::Agent(AgentEvent::Finished));
        update(&mut app, Msg::Agent(AgentEvent::AssistantDelta(String::from("second"))));
        assert_eq!(app.transcript.len(), 2);
        assert_eq!(
            app.transcript[0],
            Entry::Assistant { text: String::from("first"), streaming: false }
        );
        assert_eq!(
            app.transcript[1],
            Entry::Assistant { text: String::from("second"), streaming: true }
        );
    }

    #[test]
    fn reasoning_delta_creates_streaming_entry() {
        let mut app = fresh_app();
        update(
            &mut app,
            Msg::Agent(AgentEvent::ReasoningDelta(String::from("Thinking..."))),
        );
        assert_eq!(app.transcript.len(), 1);
        assert_eq!(
            app.transcript[0],
            Entry::Reasoning { text: String::from("Thinking..."), streaming: true }
        );
    }

    #[test]
    fn reasoning_delta_appends_to_existing_streaming_entry() {
        let mut app = fresh_app();
        update(
            &mut app,
            Msg::Agent(AgentEvent::ReasoningDelta(String::from("Step 1. "))),
        );
        update(
            &mut app,
            Msg::Agent(AgentEvent::ReasoningDelta(String::from("Step 2."))),
        );
        assert_eq!(app.transcript.len(), 1);
        assert_eq!(
            app.transcript[0],
            Entry::Reasoning { text: String::from("Step 1. Step 2."), streaming: true }
        );
    }

    #[test]
    fn tool_started_creates_running_tool_entry() {
        let mut app = fresh_app();
        update(
            &mut app,
            Msg::Agent(AgentEvent::ToolStarted { name: String::from("read_file") }),
        );
        assert_eq!(app.transcript.len(), 1);
        assert_eq!(
            app.transcript[0],
            Entry::Tool { name: String::from("read_file"), status: ToolStatus::Running, output: Vec::new() }
        );
    }

    #[test]
    fn tool_output_appends_to_last_tool_entry() {
        let mut app = fresh_app();
        update(
            &mut app,
            Msg::Agent(AgentEvent::ToolStarted { name: String::from("read_file") }),
        );
        update(
            &mut app,
            Msg::Agent(AgentEvent::ToolOutput { line: String::from("line 1") }),
        );
        update(
            &mut app,
            Msg::Agent(AgentEvent::ToolOutput { line: String::from("line 2") }),
        );
        assert_eq!(app.transcript.len(), 1);

        match &app.transcript[0] {
            Entry::Tool { output, .. } => assert_eq!(*output, vec!["line 1", "line 2"]),
            _ => panic!("expected Tool entry"),
        }
    }

    #[test]
    fn tool_finished_sets_status_ok() {
        let mut app = fresh_app();
        update(
            &mut app,
            Msg::Agent(AgentEvent::ToolStarted { name: String::from("read_file") }),
        );
        update(&mut app, Msg::Agent(AgentEvent::ToolFinished));
        match &app.transcript[0] {
            Entry::Tool { status, .. } => assert_eq!(*status, ToolStatus::Ok),
            _ => panic!("expected Tool entry"),
        }
    }

    #[test]
    fn finished_marks_streaming_false_and_returns_to_idle() {
        let mut app = fresh_app();
        update(&mut app, Msg::Agent(AgentEvent::Started));
        update(&mut app, Msg::Agent(AgentEvent::AssistantDelta(String::from("text"))));
        update(
            &mut app,
            Msg::Agent(AgentEvent::ReasoningDelta(String::from("thoughts"))),
        );
        assert_eq!(app.run_state, RunState::Working);

        update(&mut app, Msg::Agent(AgentEvent::Finished));
        assert_eq!(app.run_state, RunState::Idle);

        if let Entry::Assistant { streaming, .. } = &app.transcript[0] {
            assert!(!*streaming);
        } else {
            panic!("expected Assistant entry");
        }

        match &app.transcript[1] {
            Entry::Reasoning { streaming, .. } => assert!(!*streaming),
            _ => panic!("expected Reasoning entry"),
        }
    }

    #[test]
    fn failed_adds_error_entry_and_returns_to_idle() {
        let mut app = fresh_app();
        update(&mut app, Msg::Agent(AgentEvent::Started));
        update(
            &mut app,
            Msg::Agent(AgentEvent::AssistantDelta(String::from("partial"))),
        );
        assert_eq!(app.run_state, RunState::Working);

        update(
            &mut app,
            Msg::Agent(AgentEvent::Failed(String::from("connection lost"))),
        );
        assert_eq!(app.run_state, RunState::Idle);
        assert!(matches!(app.transcript.last(), Some(Entry::Error { text }) if text == "connection lost"));

        match &app.transcript[0] {
            Entry::Assistant { streaming, .. } => assert!(!*streaming),
            _ => panic!("expected Assistant entry"),
        }
    }

    #[test]
    fn escape_cancels_working_stream() {
        let mut app = fresh_app();
        update(&mut app, Msg::Agent(AgentEvent::Started));
        update(
            &mut app,
            Msg::Agent(AgentEvent::AssistantDelta(String::from("partial"))),
        );
        assert_eq!(app.run_state, RunState::Working);

        update(&mut app, Msg::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
        assert_eq!(app.run_state, RunState::Idle);

        match &app.transcript[0] {
            Entry::Assistant { streaming, .. } => assert!(!*streaming),
            _ => panic!("expected Assistant entry"),
        }
    }

    #[test]
    fn escape_does_nothing_when_idle() {
        let mut app = fresh_app();
        update(&mut app, Msg::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
        assert_eq!(app.run_state, RunState::Idle);
        assert!(app.transcript.is_empty());
    }

    #[test]
    fn submit_while_working_is_ignored() {
        let mut app = fresh_app();
        app.run_state = RunState::Working;
        app.input = String::from("queued message");
        update(&mut app, Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
        assert_eq!(app.input, "queued message");
        assert!(app.transcript.is_empty());
    }

    #[test]
    fn submit_kicks_off_agent_via_followup() {
        let mut app = fresh_app();
        app.input = String::from("explain this repo");
        let follow = update(&mut app, Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
        assert_eq!(app.input, "");
        assert_eq!(app.transcript.len(), 1);
        assert_eq!(follow, Some(Msg::Agent(AgentEvent::Started)));
    }
}
