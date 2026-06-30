//! Application state, message types, and the `update` function.
//!
//! This follows the Elm architecture (TEA):
//!
//! `update(&mut App, Msg) -> Option<Msg>` is the only mutation path.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::cli::{Cli, WebSearchMode};
use crate::{context, session};

/// Number of UI ticks the user has to press Ctrl+D a second time before the
/// quit confirmation expires and a fresh double-press is needed.
///
/// With the default 100 ms tick rate this is roughly 3 seconds.
const QUIT_CONFIRM_TIMEOUT_TICKS: u64 = 30;

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
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub enum RunState {
    /// Nothing in flight.
    #[default]
    Idle,
    /// Agent stream active.
    Working,
    /// A stop has been requested; stream is winding down.
    #[allow(dead_code)]
    Stopping,
    /// A recoverable error occurred.
    #[allow(dead_code)]
    Error(String),
}

impl RunState {
    #[allow(dead_code)]
    pub fn label(&self) -> &'static str {
        match self {
            RunState::Idle => "idle",
            RunState::Working => "working",
            RunState::Stopping => "stopping",
            RunState::Error(_) => "error",
        }
    }
}

/// The user-facing prompt state, derived from [`RunState`] and the transcript.
///
/// This drives prompt-line styling (color, hint text) to show editable,
/// submitted, streaming, stopped, errored.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum PromptState {
    /// Idle — user can type and submit.
    #[default]
    Editable,
    /// Prompt submitted; waiting for the first agent event.
    Submitted,
    /// Agent is actively streaming reasoning or assistant text.
    Streaming,
    /// Agent is executing a tool call.
    RunningTool,
    /// Run was cancelled; prompt is editable again.
    Stopped,
    /// Run failed with an error; prompt is editable again.
    Errored,
}

impl PromptState {
    /// A short hint shown at the prompt when not editable.
    pub fn hint(&self) -> &'static str {
        match self {
            PromptState::Editable => "",
            PromptState::Submitted => "(sending…)",
            PromptState::Streaming => "(streaming… esc to cancel)",
            PromptState::RunningTool => "(running tool… esc to cancel)",
            PromptState::Stopped => "(stopped)",
            PromptState::Errored => "(error)",
        }
    }
}

/// Status of a tool entry in the transcript.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default, Serialize, Deserialize)]
pub enum ToolStatus {
    /// Tool started, not yet finished.
    #[default]
    Running,
    /// Tool finished successfully.
    Ok,
    /// Tool failed.
    Failed,
}

impl ToolStatus {
    /// TODO: use unicode symbols
    pub fn icon(&self) -> &'static str {
        match self {
            ToolStatus::Ok => "wrote",
            ToolStatus::Failed => "write failed",
            ToolStatus::Running => "writing",
        }
    }
}

/// One transcript row.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
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
        arguments: String,
        status: ToolStatus,
        output: Vec<String>,
    },
    /// A status row (e.g. context sources loaded).
    Status { text: String },
    /// An error row.
    Error { text: String },
}

impl Entry {
    /// Single-line rendering, kept for backwards-compatible callers.
    #[allow(dead_code)]
    pub fn to_line(&self) -> ratatui::text::Line<'_> {
        crate::ui::entry_lines(self, 0, "You")
            .into_iter()
            .next()
            .unwrap_or_default()
    }
}

/// Events from the background agent stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentEvent {
    Started,
    AssistantDelta(String),
    ReasoningDelta(String),
    ToolStarted {
        id: String,
        name: String,
        arguments: String,
    },
    ToolFinished {
        id: String,
        output: Vec<String>,
        status: ToolStatus,
        /// Structured write result if this was a file-write tool, else `None`.
        write_result: Option<crate::tools::WriteResult>,
        /// Structured shell result if this was a `run_shell` tool, else `None`.
        /// Boxed to avoid a large enum variant (ProcessResult carries
        /// multiple Vec<String>s).
        shell_result: Option<Box<crate::tools::shell::ProcessResult>>,
    },
    Finished,
    Failed(String),
    Cancelled,
}

/// The single message type fed into `update`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Msg {
    /// A raw key event from the terminal.
    Key(crossterm::event::KeyEvent),
    /// Periodic tick.
    Tick,
    /// Submit the current input.
    #[allow(dead_code)]
    Submit,
    /// Clear the transcript.
    #[allow(dead_code)]
    Clear,
    /// Quit the app.
    Quit,
    /// An agent stream event.
    Agent(AgentEvent),
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
    pub user_label: String,
    pub websearch: WebSearchMode,
    /// Loaded context sources (e.g. AGENTS.md).
    pub context_sources: Vec<crate::context::ContextSource>,
    /// Scroll offset in transcript lines from the bottom. 0 = pinned to newest.
    /// Positive values scroll up (toward older entries).
    pub scroll_offset: usize,
    /// Monotonic UI tick used for lightweight animated affordances.
    pub ui_tick: u64,
    /// When `Some`, the user pressed Ctrl+D once and we are waiting for a
    /// second press within [`QUIT_CONFIRM_TIMEOUT_TICKS`] ticks to actually
    /// quit. The value is the tick deadline at which the pending confirmation
    /// expires.
    pub ctrl_d_pending: Option<u64>,
    /// Append-only session writer. `None` when persistence is disabled
    /// (e.g. the sessions directory is not writable).
    pub session_writer: Option<crate::session::SessionWriter>,
    /// Monotonic turn counter for session record correlation.
    pub turn_count: u64,
    /// When true the loop should stop and the app exit.
    pub quit: bool,
}

impl App {
    /// Build the initial app from parsed CLI args.
    ///
    /// Discovers the workspace root from `--cwd` (preferring the git root),
    /// loads root `AGENTS.md` if present, and adds a transcript status entry
    /// showing loaded context sources.
    pub fn from_cli(cli: &Cli) -> Self {
        let workspace_root = context::discover_workspace_root(&cli.cwd);
        let context_sources = match context::load_agents_md(&workspace_root) {
            Some(source) => vec![source],
            None => Vec::new(),
        };

        let sessions_dir = session::sessions_dir(&workspace_root);
        let session_titles = session::list_session_titles(&sessions_dir);
        let sidebar = if session_titles.is_empty() {
            Sidebar::placeholder()
        } else {
            Sidebar { sessions: session_titles, active: Some(0) }
        };

        let resumed_transcript = session::latest_session_file(&sessions_dir)
            .map(|p| session::SessionReader::read_transcript(&p))
            .unwrap_or_default();

        let mut transcript = Vec::new();
        if !context_sources.is_empty() {
            let summaries: Vec<String> = context_sources.iter().map(|s| s.summary()).collect();
            transcript.push(Entry::Status { text: format!("context  {}", summaries.join(", ")) });
        }

        transcript.extend(resumed_transcript);

        let mut session_writer = session::SessionWriter::create(
            &sessions_dir,
            &session::generate_session_id(),
            &workspace_root.display().to_string(),
            "scratch",
            "umans",
            &cli.model,
            &format!("{:?}", cli.websearch).to_lowercase(),
            env!("CARGO_PKG_VERSION"),
        )
        .ok();

        if let Some(ref mut writer) = session_writer.as_mut()
            && !context_sources.is_empty()
        {
            let _ = writer.append_context(&context_sources);
        }

        App {
            mode: Mode::default(),
            run_state: RunState::default(),
            input: String::new(),
            transcript,
            sidebar,
            view: crate::ui::ViewState::default(),
            cwd: cli.cwd.clone(),
            model: cli.model.clone(),
            user_label: default_user_label(),
            websearch: cli.websearch,
            context_sources,
            scroll_offset: 0,
            ui_tick: 0,
            ctrl_d_pending: None,
            session_writer,
            turn_count: 0,
            quit: false,
        }
    }

    /// Derive the granular status label for the sidebar/status line.
    ///
    /// Maps `RunState` plus the last transcript entry into one of
    /// idle, sending, thinking, streaming, running tool, cancelled,
    /// failed, done.
    pub fn status_label(&self) -> &'static str {
        if self.run_state == RunState::Working {
            match self.transcript.last() {
                Some(Entry::Reasoning { streaming: true, .. }) => "thinking",
                Some(Entry::Assistant { streaming: true, .. }) => "streaming",
                Some(Entry::Tool { status: ToolStatus::Running, .. }) => "running tool",
                Some(Entry::User { .. }) | None => "sending",
                _ => "streaming",
            }
        } else {
            match self.transcript.last() {
                Some(Entry::Status { text }) if text == "cancelled" => "cancelled",
                Some(Entry::Error { .. }) => "failed",
                Some(Entry::Assistant { streaming: false, .. })
                | Some(Entry::Tool { status: ToolStatus::Ok | ToolStatus::Failed, .. }) => "done",
                _ => "idle",
            }
        }
    }

    /// Derive the prompt UI state from `run_state` and the transcript.
    pub fn prompt_state(&self) -> PromptState {
        match self.run_state {
            RunState::Working => match self.transcript.last() {
                Some(Entry::Reasoning { streaming: true, .. }) | Some(Entry::Assistant { streaming: true, .. }) => {
                    PromptState::Streaming
                }
                Some(Entry::Tool { status: ToolStatus::Running, .. }) => PromptState::RunningTool,
                _ => PromptState::Submitted,
            },
            _ => match self.transcript.last() {
                Some(Entry::Status { text }) if text == "cancelled" => PromptState::Stopped,
                Some(Entry::Error { .. }) => PromptState::Errored,
                _ => PromptState::Editable,
            },
        }
    }
}

/// The only mutation path. Returns an optional follow-up message.
///
/// - Printable chars append to the input buffer.
/// - `Backspace` removes the last char.
/// - `Enter` submits: slash commands (`/clear`, `/quit`) are routed, otherwise
///   the input is appended as [`Entry::User`] and cleared.
/// - `Ctrl+C` always quits immediately.
/// - `Ctrl+D` requires a double-press: the first press shows a confirmation
///   message, the second press within [`QUIT_CONFIRM_TIMEOUT_TICKS`] ticks
///   quits. The pending state is cleared on timeout or any other key.
/// - `q` is a normal input character (no longer quits).
pub fn update(app: &mut App, msg: &Msg) -> Option<Msg> {
    match msg {
        Msg::Key(key) => handle_key(app, *key),
        Msg::Submit => handle_submit(app),
        Msg::Quit => {
            app.quit = true;
            None
        }
        Msg::Tick => {
            app.ui_tick = app.ui_tick.wrapping_add(1);
            if let Some(deadline) = app.ctrl_d_pending
                && now_or_after_deadline(app.ui_tick, deadline)
            {
                app.ctrl_d_pending = None;
            }
            None
        }
        Msg::Clear => {
            app.transcript.clear();
            None
        }
        Msg::Agent(event) => handle_agent_event(app, event.clone()),
    }
}

/// - Ctrl+C always quits immediately, even mid-input.
/// - Ctrl+D requires a double-press: the first press shows a confirmation
///   message; the second press within [`QUIT_CONFIRM_TIMEOUT_TICKS`] ticks
///   quits. Any other key (or timeout) cancels the pending state.
/// - Printable characters append to the input buffer.
/// - Backspace removes the last character.
/// - Enter submits the current input.
/// - Escape cancels an active agent stream.
/// - Up/Down/PageUp/PageDown scroll the transcript (available even while the
///   agent is running, so cancel/quit stay usable).
///     - Scroll up (toward older entries). Works while the agent is running.
///     - Scroll down (toward newer entries).
///     - Page up: jump by 10 lines.
///     - Page down: jump by 10 lines, or reset to newest.
fn handle_key(app: &mut App, key: crossterm::event::KeyEvent) -> Option<Msg> {
    use crossterm::event::{KeyCode, KeyModifiers};

    match key.code {
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.quit = true;
            Some(Msg::Quit)
        }
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            if let Some(deadline) = app.ctrl_d_pending
                && !now_or_after_deadline(app.ui_tick, deadline)
            {
                app.ctrl_d_pending = None;
                app.quit = true;
                Some(Msg::Quit)
            } else {
                let deadline = app.ui_tick.wrapping_add(QUIT_CONFIRM_TIMEOUT_TICKS);
                app.ctrl_d_pending = Some(deadline);
                app.transcript
                    .push(Entry::Status { text: String::from("Press CTRL+D again to quit.") });
                pin_to_bottom(app);
                None
            }
        }

        _ => {
            app.ctrl_d_pending = None;

            match key.code {
                KeyCode::Up | KeyCode::Char('k') if app.input.is_empty() => {
                    app.scroll_offset = app.scroll_offset.saturating_add(1);
                }
                KeyCode::Down | KeyCode::Char('j') if app.input.is_empty() => {
                    app.scroll_offset = app.scroll_offset.saturating_sub(1);
                }
                KeyCode::PageUp => app.scroll_offset = app.scroll_offset.saturating_add(10),
                KeyCode::PageDown => match app.scroll_offset > 10 {
                    true => app.scroll_offset -= 10,
                    false => app.scroll_offset = 0,
                },
                KeyCode::Char(ch) => app.input.push(ch),
                KeyCode::Backspace => {
                    app.input.pop();
                }
                KeyCode::Enter => return handle_submit(app),
                KeyCode::Esc if app.run_state == RunState::Working => cancel_stream(app),
                _ => {}
            }
            None
        }
    }
}

/// Handle an `Enter` submit. Slash commands are routed; otherwise the input is
/// appended as [`Entry::User`] and cleared, and the fake agent stream is started.
///
/// Returns an optional follow-up [`Msg`].
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
    app.turn_count += 1;
    let turn_id = format!("turn_{}", app.turn_count);
    if let Some(ref mut writer) = app.session_writer {
        let _ = writer.append_entry(app.transcript.last().unwrap(), &turn_id);
    }
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
            pin_to_bottom(app);
            None
        }
        AgentEvent::ReasoningDelta(delta) => {
            if let Some(Entry::Reasoning { text, streaming: true }) = app.transcript.last_mut() {
                text.push_str(&delta);
            } else {
                app.transcript.push(Entry::Reasoning { text: delta, streaming: true });
            }
            pin_to_bottom(app);
            None
        }
        AgentEvent::ToolStarted { id, name, arguments } => {
            app.transcript.push(Entry::Tool {
                name: format!("{name}#{id}"),
                arguments: arguments.clone(),
                status: ToolStatus::Running,
                output: Vec::new(),
            });
            if let Some(ref mut writer) = app.session_writer {
                let turn_id = format!("turn_{}", app.turn_count);
                let _ = writer.append_tool_started(&turn_id, &id, &name, &arguments);
            }
            pin_to_bottom(app);
            None
        }
        AgentEvent::ToolFinished { id, output, status, write_result, shell_result } => {
            for entry in app.transcript.iter_mut().rev() {
                if let Entry::Tool { name, output: out, status: s, .. } = entry
                    && name.ends_with(&format!("#{id}"))
                {
                    *out = output;
                    *s = status;
                    break;
                }
            }

            persist_last_entry(app);

            if let Some(result) = write_result
                && let Some(ref mut writer) = app.session_writer
            {
                let turn_id = format!("turn_{}", app.turn_count);
                let _ = writer.append_file_write(&turn_id, &result, status);
            }

            if let Some(result) = shell_result
                && let Some(ref mut writer) = app.session_writer
            {
                let turn_id = format!("turn_{}", app.turn_count);
                let _ = writer.append_shell_exec(&turn_id, &result);
            }
            None
        }
        AgentEvent::Finished => {
            finalize_streaming(app);
            app.run_state = RunState::Idle;
            persist_last_entry(app);
            None
        }
        AgentEvent::Failed(msg) => {
            finalize_streaming(app);
            app.transcript.push(Entry::Error { text: msg });
            app.run_state = RunState::Idle;
            persist_last_entry(app);
            None
        }
        AgentEvent::Cancelled => {
            finalize_streaming(app);
            app.transcript.push(Entry::Status { text: String::from("cancelled") });
            app.run_state = RunState::Idle;
            persist_last_entry(app);
            None
        }
    }
}

/// Cancel an active stream by marking all streaming entries complete,
/// recording a cancelled status entry, and returning to idle.
///
/// The app loop observes the transition out of `Working` and drops the
/// background receiver, which stops the agent thread on its next failed send.
fn cancel_stream(app: &mut App) {
    finalize_streaming(app);
    app.transcript.push(Entry::Status { text: String::from("cancelled") });
    app.run_state = RunState::Idle;
    persist_last_entry(app);
    pin_to_bottom(app);
}

/// Reset the scroll offset to pin the transcript to the newest entries.
fn pin_to_bottom(app: &mut App) {
    app.scroll_offset = 0;
}

/// Persist the last transcript entry to the session file, if a writer exists.
///
/// Only finalized entries are written — streaming/running entries are skipped
/// by `SessionRecord::from_entry`.
fn persist_last_entry(app: &mut App) {
    if let Some(ref mut writer) = app.session_writer
        && let Some(entry) = app.transcript.last()
    {
        let turn_id = format!("turn_{}", app.turn_count);
        let _ = writer.append_entry(entry, &turn_id);
    }
}

/// Whether `ui_tick` is at or past `deadline`, accounting for wrap-around.
///
/// If `deadline` has wrapped (e.g. `ui_tick` is small and `deadline` is near
/// `u64::MAX`), we treat the deadline as already passed — a wrap is so rare
/// that expiring early is the safe choice.
fn now_or_after_deadline(ui_tick: u64, deadline: u64) -> bool {
    if deadline >= ui_tick { deadline.wrapping_sub(ui_tick) > u64::MAX / 2 } else { true }
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

fn default_user_label() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .map(|name| format!("User ({name})"))
        .unwrap_or_else(|_| String::from("You"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::io::Write;

    fn fresh_app() -> App {
        let dir = tempfile::tempdir().expect("create temp dir");
        let cli = Cli { cwd: dir.path().to_path_buf(), ..Cli::default() };
        let mut app = App::from_cli(&cli);
        app.session_writer = None;
        app
    }

    #[test]
    fn q_appends_to_input_and_does_not_quit() {
        let mut app = fresh_app();
        let follow = update(
            &mut app,
            &Msg::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)),
        );
        assert!(!app.quit, "q should not quit");
        assert_eq!(app.input, "q", "q should append to input");
        assert_eq!(follow, None);
    }

    #[test]
    fn ctrl_c_sets_quit_flag() {
        let mut app = fresh_app();
        update(
            &mut app,
            &Msg::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
        );
        assert!(app.quit);
    }

    #[test]
    fn ctrl_d_first_press_shows_confirmation() {
        let mut app = fresh_app();
        update(
            &mut app,
            &Msg::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)),
        );
        assert!(!app.quit, "first Ctrl+D should not quit");
        assert!(app.ctrl_d_pending.is_some(), "should arm pending confirmation");
        assert!(
            app.transcript.iter().any(|e| matches!(
                e,
                Entry::Status { text } if text.contains("Press CTRL+D again to quit")
            )),
            "should show confirmation message"
        );
    }

    #[test]
    fn ctrl_d_second_press_quits() {
        let mut app = fresh_app();
        update(
            &mut app,
            &Msg::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)),
        );
        assert!(!app.quit);
        assert!(app.ctrl_d_pending.is_some());

        let follow = update(
            &mut app,
            &Msg::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)),
        );
        assert!(app.quit, "second Ctrl+D should quit");
        assert!(app.ctrl_d_pending.is_none(), "pending should be cleared on quit");
        assert_eq!(follow, Some(Msg::Quit));
    }

    #[test]
    fn ctrl_d_timeout_expires_and_requires_double_press_again() {
        let mut app = fresh_app();
        update(
            &mut app,
            &Msg::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)),
        );
        assert!(app.ctrl_d_pending.is_some());

        for _ in 0..QUIT_CONFIRM_TIMEOUT_TICKS + 1 {
            update(&mut app, &Msg::Tick);
        }
        assert!(app.ctrl_d_pending.is_none(), "pending should expire after timeout");

        update(
            &mut app,
            &Msg::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)),
        );
        assert!(!app.quit, "expired second press should not quit");
        assert!(app.ctrl_d_pending.is_some(), "should arm a fresh confirmation");
    }

    #[test]
    fn ctrl_d_cancelled_by_other_key() {
        let mut app = fresh_app();
        update(
            &mut app,
            &Msg::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)),
        );
        assert!(app.ctrl_d_pending.is_some());

        update(
            &mut app,
            &Msg::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)),
        );
        assert!(app.ctrl_d_pending.is_none(), "other key should cancel pending Ctrl+D");

        update(
            &mut app,
            &Msg::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)),
        );
        assert!(!app.quit, "should not quit after cancellation");
        assert!(app.ctrl_d_pending.is_some());
    }

    #[test]
    fn ctrl_d_works_even_with_input() {
        let mut app = fresh_app();
        app.input = String::from("some text");
        update(
            &mut app,
            &Msg::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)),
        );
        assert!(!app.quit);
        assert!(app.ctrl_d_pending.is_some());

        update(
            &mut app,
            &Msg::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)),
        );
        assert!(app.quit, "Ctrl+D should quit even with input present");
    }

    #[test]
    fn other_keys_do_not_quit() {
        let mut app = fresh_app();
        update(
            &mut app,
            &Msg::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)),
        );
        assert!(!app.quit);
        update(&mut app, &Msg::Tick);
        assert!(!app.quit);
    }

    #[test]
    fn tick_increments_ui_tick() {
        let mut app = fresh_app();
        assert_eq!(app.ui_tick, 0);
        update(&mut app, &Msg::Tick);
        assert_eq!(app.ui_tick, 1);
    }

    #[test]
    fn quit_message_sets_quit_flag() {
        let mut app = fresh_app();
        update(&mut app, &Msg::Quit);
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
            update(
                &mut app,
                &Msg::Key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE)),
            );
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
            &Msg::Key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)),
        );
        assert_eq!(app.input, "ab");
    }

    #[test]
    fn backspace_on_empty_input_is_noop() {
        let mut app = fresh_app();
        update(
            &mut app,
            &Msg::Key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)),
        );
        assert_eq!(app.input, "");
    }

    #[test]
    fn enter_submits_user_entry_and_clears_input() {
        let mut app = fresh_app();
        app.input = String::from("explain this repo");
        update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
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
        update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
        assert_eq!(app.input, "");
        assert!(app.transcript.is_empty());
    }

    #[test]
    fn enter_trims_whitespace_before_submit() {
        let mut app = fresh_app();
        app.input = String::from("  hello  ");
        update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
        assert_eq!(app.input, "");
        assert_eq!(app.transcript.len(), 1);
        assert_eq!(app.transcript[0], Entry::User { text: String::from("hello") });
    }

    #[test]
    fn slash_clear_clears_transcript_and_input() {
        let mut app = fresh_app();
        app.transcript.push(Entry::User { text: String::from("old") });
        app.input = String::from("/clear");
        update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
        assert!(app.transcript.is_empty());
        assert_eq!(app.input, "");
        assert!(!app.quit);
    }

    #[test]
    fn slash_quit_sets_quit_flag() {
        let mut app = fresh_app();
        app.input = String::from("/quit");
        let follow = update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
        assert!(app.quit);
        assert_eq!(follow, Some(Msg::Quit));
        assert_eq!(app.input, "");
    }

    #[test]
    fn slash_exit_also_quits() {
        let mut app = fresh_app();
        app.input = String::from("/exit");
        update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
        assert!(app.quit);
    }

    #[test]
    fn unknown_slash_command_is_ignored() {
        let mut app = fresh_app();
        app.input = String::from("/bogus");
        update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
        assert!(!app.quit);
        assert!(app.transcript.is_empty());
        assert_eq!(app.input, "/bogus");
    }

    #[test]
    fn msg_clear_clears_transcript() {
        let mut app = fresh_app();
        app.transcript.push(Entry::User { text: String::from("a") });
        app.transcript.push(Entry::User { text: String::from("b") });
        update(&mut app, &Msg::Clear);
        assert!(app.transcript.is_empty());
    }

    #[test]
    fn q_does_not_quit_even_when_input_empty() {
        let mut app = fresh_app();
        let follow = update(
            &mut app,
            &Msg::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)),
        );
        assert!(!app.quit, "q should never quit");
        assert_eq!(follow, None);
        assert_eq!(app.input, "q");
    }

    #[test]
    fn agent_started_sets_working() {
        let mut app = fresh_app();
        update(&mut app, &Msg::Agent(AgentEvent::Started));
        assert_eq!(app.run_state, RunState::Working);
    }

    #[test]
    fn assistant_delta_creates_streaming_entry() {
        let mut app = fresh_app();
        update(&mut app, &Msg::Agent(AgentEvent::AssistantDelta(String::from("Hello"))));
        assert_eq!(app.transcript.len(), 1);
        assert_eq!(
            app.transcript[0],
            Entry::Assistant { text: String::from("Hello"), streaming: true }
        );
    }

    #[test]
    fn assistant_delta_appends_to_existing_streaming_entry() {
        let mut app = fresh_app();
        update(
            &mut app,
            &Msg::Agent(AgentEvent::AssistantDelta(String::from("Hello "))),
        );
        update(&mut app, &Msg::Agent(AgentEvent::AssistantDelta(String::from("world"))));
        assert_eq!(app.transcript.len(), 1);
        assert_eq!(
            app.transcript[0],
            Entry::Assistant { text: String::from("Hello world"), streaming: true }
        );
    }

    #[test]
    fn assistant_delta_creates_new_entry_after_finished() {
        let mut app = fresh_app();
        update(&mut app, &Msg::Agent(AgentEvent::AssistantDelta(String::from("first"))));
        update(&mut app, &Msg::Agent(AgentEvent::Finished));
        update(
            &mut app,
            &Msg::Agent(AgentEvent::AssistantDelta(String::from("second"))),
        );
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
            &Msg::Agent(AgentEvent::ReasoningDelta(String::from("Thinking..."))),
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
            &Msg::Agent(AgentEvent::ReasoningDelta(String::from("Step 1. "))),
        );
        update(
            &mut app,
            &Msg::Agent(AgentEvent::ReasoningDelta(String::from("Step 2."))),
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
            &Msg::Agent(AgentEvent::ToolStarted {
                id: String::from("0"),
                name: String::from("read_file"),
                arguments: String::from("{}"),
            }),
        );
        assert_eq!(app.transcript.len(), 1);
        match &app.transcript[0] {
            Entry::Tool { name, arguments, status, output } => {
                assert_eq!(name, "read_file#0");
                assert_eq!(arguments, "{}");
                assert_eq!(*status, ToolStatus::Running);
                assert!(output.is_empty());
            }
            _ => panic!("expected Tool entry"),
        }
    }

    #[test]
    fn tool_finished_sets_output_and_status() {
        let mut app = fresh_app();
        update(
            &mut app,
            &Msg::Agent(AgentEvent::ToolStarted {
                id: String::from("0"),
                name: String::from("read_file"),
                arguments: String::from("{}"),
            }),
        );
        update(
            &mut app,
            &Msg::Agent(AgentEvent::ToolFinished {
                id: String::from("0"),
                output: vec![String::from("line 1"), String::from("line 2")],
                status: ToolStatus::Ok,
                write_result: None,
                shell_result: None,
            }),
        );
        match &app.transcript[0] {
            Entry::Tool { status, output, .. } => {
                assert_eq!(*status, ToolStatus::Ok);
                assert_eq!(*output, vec!["line 1", "line 2"]);
            }
            _ => panic!("expected Tool entry"),
        }
    }

    #[test]
    fn tool_finished_marks_failed_status() {
        let mut app = fresh_app();
        update(
            &mut app,
            &Msg::Agent(AgentEvent::ToolStarted {
                id: String::from("0"),
                name: String::from("read_file"),
                arguments: String::from("{}"),
            }),
        );
        update(
            &mut app,
            &Msg::Agent(AgentEvent::ToolFinished {
                id: String::from("0"),
                output: Vec::new(),
                status: ToolStatus::Failed,
                write_result: None,
                shell_result: None,
            }),
        );
        match &app.transcript[0] {
            Entry::Tool { status, .. } => assert_eq!(*status, ToolStatus::Failed),
            _ => panic!("expected Tool entry"),
        }
    }

    #[test]
    fn cancelled_event_adds_status_and_returns_to_idle() {
        let mut app = fresh_app();
        update(&mut app, &Msg::Agent(AgentEvent::Started));
        update(
            &mut app,
            &Msg::Agent(AgentEvent::AssistantDelta(String::from("partial"))),
        );
        assert_eq!(app.run_state, RunState::Working);

        update(&mut app, &Msg::Agent(AgentEvent::Cancelled));
        assert_eq!(app.run_state, RunState::Idle);
        assert!(matches!(app.transcript.last(), Some(Entry::Status { text }) if text == "cancelled"));

        match &app.transcript[0] {
            Entry::Assistant { streaming, .. } => assert!(!*streaming),
            _ => panic!("expected Assistant entry"),
        }
    }

    #[test]
    fn finished_marks_streaming_false_and_returns_to_idle() {
        let mut app = fresh_app();
        update(&mut app, &Msg::Agent(AgentEvent::Started));
        update(&mut app, &Msg::Agent(AgentEvent::AssistantDelta(String::from("text"))));
        update(
            &mut app,
            &Msg::Agent(AgentEvent::ReasoningDelta(String::from("thoughts"))),
        );

        assert_eq!(app.run_state, RunState::Working);
        update(&mut app, &Msg::Agent(AgentEvent::Finished));
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
        update(&mut app, &Msg::Agent(AgentEvent::Started));
        update(
            &mut app,
            &Msg::Agent(AgentEvent::AssistantDelta(String::from("partial"))),
        );
        assert_eq!(app.run_state, RunState::Working);

        update(
            &mut app,
            &Msg::Agent(AgentEvent::Failed(String::from("connection lost"))),
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
        update(&mut app, &Msg::Agent(AgentEvent::Started));
        update(
            &mut app,
            &Msg::Agent(AgentEvent::AssistantDelta(String::from("partial"))),
        );
        assert_eq!(app.run_state, RunState::Working);

        update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
        assert_eq!(app.run_state, RunState::Idle);

        match &app.transcript[0] {
            Entry::Assistant { streaming, .. } => assert!(!*streaming),
            _ => panic!("expected Assistant entry"),
        }
    }

    #[test]
    fn escape_does_nothing_when_idle() {
        let mut app = fresh_app();
        update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
        assert_eq!(app.run_state, RunState::Idle);
        assert!(app.transcript.is_empty());
    }

    #[test]
    fn submit_while_working_is_ignored() {
        let mut app = fresh_app();
        app.run_state = RunState::Working;
        app.input = String::from("queued message");
        update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
        assert_eq!(app.input, "queued message");
        assert!(app.transcript.is_empty());
    }

    #[test]
    fn submit_kicks_off_agent_via_followup() {
        let mut app = fresh_app();
        app.input = String::from("explain this repo");
        let follow = update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
        assert_eq!(app.input, "");
        assert_eq!(app.transcript.len(), 1);
        assert_eq!(follow, Some(Msg::Agent(AgentEvent::Started)));
    }

    #[test]
    fn app_without_agents_md_has_no_context_sources() {
        let app = fresh_app();
        assert!(app.context_sources.is_empty());
        assert!(app.transcript.is_empty());
    }

    #[test]
    fn app_with_agents_md_loads_context_and_adds_status() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let agents_path = dir.path().join("AGENTS.md");
        let mut f = std::fs::File::create(&agents_path).expect("create AGENTS.md");
        f.write_all(b"# Project\n\nBuild with cargo.\n")
            .expect("write AGENTS.md");

        let cli = Cli { cwd: dir.path().to_path_buf(), ..Cli::default() };
        let app = App::from_cli(&cli);

        assert_eq!(app.context_sources.len(), 1);
        let source = &app.context_sources[0];
        assert_eq!(source.path, agents_path);
        assert_eq!(source.scope, ".");
        assert!(!source.truncated);
        assert!(source.content.contains("# Project"));

        assert_eq!(app.transcript.len(), 1);
        match &app.transcript[0] {
            Entry::Status { text } => assert!(text.contains("loaded")),
            _ => panic!("expected Status entry for context source"),
        }
    }

    #[test]
    fn app_with_oversized_agents_md_marks_truncation() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let big_content = "x".repeat(crate::context::AGENTS_MD_SIZE_CAP + 1000);
        let agents_path = dir.path().join("AGENTS.md");
        let mut f = std::fs::File::create(&agents_path).expect("create AGENTS.md");
        f.write_all(big_content.as_bytes()).expect("write AGENTS.md");

        let cli = Cli { cwd: dir.path().to_path_buf(), ..Cli::default() };
        let app = App::from_cli(&cli);

        assert_eq!(app.context_sources.len(), 1);
        let source = &app.context_sources[0];
        assert!(source.truncated);
        assert!(source.content.len() <= crate::context::AGENTS_MD_SIZE_CAP);

        match &app.transcript[0] {
            Entry::Status { text } => assert!(text.contains("truncated")),
            _ => panic!("expected Status entry"),
        }
    }

    #[test]
    fn context_sources_are_guidance_not_permission() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let content = "# Instructions\n\nModel: gpt-4\nAllow: rm -rf\n";
        let mut f = std::fs::File::create(dir.path().join("AGENTS.md")).expect("create");
        f.write_all(content.as_bytes()).expect("write");

        let cli = Cli { cwd: dir.path().to_path_buf(), ..Cli::default() };
        let app = App::from_cli(&cli);

        assert_eq!(app.model, "umans-coder");
        assert!(app.context_sources[0].content.contains("Model: gpt-4"));
    }

    #[test]
    fn status_label_idle_when_no_transcript() {
        let app = fresh_app();
        assert_eq!(app.status_label(), "idle");
    }

    #[test]
    fn status_label_sending_after_user_submit() {
        let mut app = fresh_app();
        update(&mut app, &Msg::Agent(AgentEvent::Started));
        app.transcript.push(Entry::User { text: String::from("hi") });
        assert_eq!(app.status_label(), "sending");
    }

    #[test]
    fn status_label_thinking_during_reasoning_stream() {
        let mut app = fresh_app();
        update(&mut app, &Msg::Agent(AgentEvent::Started));
        update(&mut app, &Msg::Agent(AgentEvent::ReasoningDelta(String::from("hmm"))));
        assert_eq!(app.status_label(), "thinking");
    }

    #[test]
    fn status_label_streaming_during_assistant_stream() {
        let mut app = fresh_app();
        update(&mut app, &Msg::Agent(AgentEvent::Started));
        update(&mut app, &Msg::Agent(AgentEvent::AssistantDelta(String::from("hi"))));
        assert_eq!(app.status_label(), "streaming");
    }

    #[test]
    fn status_label_running_tool_when_tool_active() {
        let mut app = fresh_app();
        update(&mut app, &Msg::Agent(AgentEvent::Started));
        update(
            &mut app,
            &Msg::Agent(AgentEvent::ToolStarted {
                id: String::from("0"),
                name: String::from("read_file"),
                arguments: String::from("{}"),
            }),
        );
        assert_eq!(app.status_label(), "running tool");
    }

    #[test]
    fn status_label_done_after_finished() {
        let mut app = fresh_app();
        update(&mut app, &Msg::Agent(AgentEvent::Started));
        update(&mut app, &Msg::Agent(AgentEvent::AssistantDelta(String::from("done"))));
        update(&mut app, &Msg::Agent(AgentEvent::Finished));
        assert_eq!(app.status_label(), "done");
    }

    #[test]
    fn status_label_failed_after_error() {
        let mut app = fresh_app();
        update(&mut app, &Msg::Agent(AgentEvent::Started));
        update(&mut app, &Msg::Agent(AgentEvent::Failed(String::from("boom"))));
        assert_eq!(app.status_label(), "failed");
    }

    #[test]
    fn status_label_cancelled_after_cancel() {
        let mut app = fresh_app();
        update(&mut app, &Msg::Agent(AgentEvent::Started));
        update(&mut app, &Msg::Agent(AgentEvent::Cancelled));
        assert_eq!(app.status_label(), "cancelled");
    }

    #[test]
    fn prompt_state_editable_when_idle() {
        let app = fresh_app();
        assert_eq!(app.prompt_state(), PromptState::Editable);
    }

    #[test]
    fn prompt_state_streaming_during_assistant_delta() {
        let mut app = fresh_app();
        update(&mut app, &Msg::Agent(AgentEvent::Started));
        update(&mut app, &Msg::Agent(AgentEvent::AssistantDelta(String::from("hi"))));
        assert_eq!(app.prompt_state(), PromptState::Streaming);
    }

    #[test]
    fn prompt_state_running_tool_when_tool_active() {
        let mut app = fresh_app();
        update(&mut app, &Msg::Agent(AgentEvent::Started));
        update(
            &mut app,
            &Msg::Agent(AgentEvent::ToolStarted {
                id: String::from("0"),
                name: String::from("read_file"),
                arguments: String::from("{}"),
            }),
        );
        assert_eq!(app.prompt_state(), PromptState::RunningTool);
    }

    #[test]
    fn prompt_state_stopped_after_cancel() {
        let mut app = fresh_app();
        update(&mut app, &Msg::Agent(AgentEvent::Started));
        update(&mut app, &Msg::Agent(AgentEvent::Cancelled));
        assert_eq!(app.prompt_state(), PromptState::Stopped);
    }

    #[test]
    fn prompt_state_errored_after_failure() {
        let mut app = fresh_app();
        update(&mut app, &Msg::Agent(AgentEvent::Started));
        update(&mut app, &Msg::Agent(AgentEvent::Failed(String::from("boom"))));
        assert_eq!(app.prompt_state(), PromptState::Errored);
    }

    #[test]
    fn scroll_offset_starts_at_zero() {
        let app = fresh_app();
        assert_eq!(app.scroll_offset, 0);
    }

    #[test]
    fn up_arrow_increases_scroll_offset() {
        let mut app = fresh_app();
        app.transcript.push(Entry::User { text: String::from("line 1") });
        app.transcript.push(Entry::User { text: String::from("line 2") });
        update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)));
        assert_eq!(app.scroll_offset, 1);
    }

    #[test]
    fn down_arrow_decreases_scroll_offset() {
        let mut app = fresh_app();
        app.scroll_offset = 3;
        update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)));
        assert_eq!(app.scroll_offset, 2);
    }

    #[test]
    fn page_up_jumps_by_ten() {
        let mut app = fresh_app();
        update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE)));
        assert_eq!(app.scroll_offset, 10);
    }

    #[test]
    fn page_down_resets_to_zero_when_small() {
        let mut app = fresh_app();
        app.scroll_offset = 5;
        update(
            &mut app,
            &Msg::Key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE)),
        );
        assert_eq!(app.scroll_offset, 0);
    }

    #[test]
    fn page_down_subtracts_ten_when_large() {
        let mut app = fresh_app();
        app.scroll_offset = 15;
        update(
            &mut app,
            &Msg::Key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE)),
        );
        assert_eq!(app.scroll_offset, 5);
    }

    #[test]
    fn assistant_delta_resets_scroll_to_bottom() {
        let mut app = fresh_app();
        app.scroll_offset = 5;
        update(&mut app, &Msg::Agent(AgentEvent::Started));
        update(&mut app, &Msg::Agent(AgentEvent::AssistantDelta(String::from("hi"))));
        assert_eq!(app.scroll_offset, 0);
    }

    #[test]
    fn scroll_does_not_interfere_with_typing() {
        let mut app = fresh_app();
        app.input = String::from("typing");
        update(
            &mut app,
            &Msg::Key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE)),
        );
        assert_eq!(app.input, "typingk");
        assert_eq!(app.scroll_offset, 0);
    }

    #[test]
    fn vim_j_scroll_works_when_input_empty() {
        let mut app = fresh_app();
        app.scroll_offset = 2;
        update(
            &mut app,
            &Msg::Key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE)),
        );
        assert_eq!(app.scroll_offset, 1);
    }
}
