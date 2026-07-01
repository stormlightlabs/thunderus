//! Application state, message types, and the `update` function.
//!
//! This follows the Elm architecture (TEA):
//!
//! `update(&mut App, Msg) -> Option<Msg>` is the only mutation path.

#[cfg(test)]
mod tests;

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use serde::{Deserialize, Serialize};

use crate::cli::{Cli, WebSearchMode};
use crate::tools::shell::ProcessRegistry;
use crate::{context, session, tools};

/// Number of UI ticks the user has to press Ctrl+D a second time before the
/// quit confirmation expires and a fresh double-press is needed.
///
/// With the default 100 ms tick rate this is roughly 3 seconds.
const QUIT_CONFIRM_TIMEOUT_TICKS: u64 = 30;

/// Top-level interaction mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum Mode {
    /// Normal prompt entry.
    #[default]
    Prompt,
    /// Slash-command entry, entered with `:`.
    Command,
    /// Help overlay, entered with `?`.
    Help,
}

/// Semantic run state, used for the status line.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub enum RunState {
    /// Nothing in flight.
    #[default]
    Idle,
    /// Agent stream active.
    Working,
    /// A stop has been requested via Escape; stream is winding down.
    Stopping,
    /// A recoverable error occurred; the prompt is editable again.
    Error(String),
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

/// Where input submitted during an active run should be queued.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum QueueTarget {
    /// Inject before the next provider request in the active run.
    Steering,
    /// Submit as a new user turn after the active run finishes.
    #[default]
    FollowUp,
}

impl QueueTarget {
    pub fn toggle(self) -> Self {
        match self {
            QueueTarget::Steering => QueueTarget::FollowUp,
            QueueTarget::FollowUp => QueueTarget::Steering,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            QueueTarget::Steering => "steering",
            QueueTarget::FollowUp => "follow-up",
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
    /// Unicode icon & label used in session-record transcript entries for file writes.
    pub fn icon(&self) -> &'static str {
        match self {
            ToolStatus::Ok => "✓ wrote",
            ToolStatus::Failed => "✕ write failed",
            ToolStatus::Running => "⠋ writing",
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

/// Events from the background agent stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentEvent {
    Started,
    Status(String),
    Usage {
        input_tokens: u64,
        output_tokens: u64,
    },
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
        write_result: Option<tools::WriteResult>,
        /// Structured shell result if this was a `run_shell` tool, else `None`.
        /// Boxed to avoid a large enum variant (ProcessResult carries
        /// multiple Vec<String>s).
        shell_result: Option<Box<tools::shell::ProcessResult>>,
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
    /// A raw mouse event from the terminal.
    Mouse(crossterm::event::MouseEvent),
    /// Periodic tick.
    Tick,
    /// Clear the transcript.
    Clear,
    /// Quit the app.
    Quit,
    /// An agent stream event.
    Agent(AgentEvent),
}

/// The full application state used to draw the screen.
#[derive(Debug)]
pub struct App {
    pub session_id: String,
    pub mode: Mode,
    pub run_state: RunState,
    pub input: String,
    /// Submitted prompt history for Up/Down recall.
    pub input_history: Vec<String>,
    /// Current index into `input_history` while navigating history.
    pub history_cursor: Option<usize>,
    /// Draft input captured before history navigation starts.
    pub history_draft: String,
    pub transcript: Vec<Entry>,
    pub cwd: PathBuf,
    pub model: String,
    pub user_label: String,
    pub websearch: WebSearchMode,
    /// Whether diagnostic provider/log status rows should be shown in transcript.
    pub verbose: bool,
    /// Provider token usage accumulated for this session.
    pub session_tokens_in: u64,
    pub session_tokens_out: u64,
    /// Loaded context sources (e.g. AGENTS.md).
    pub context_sources: Vec<context::ContextSource>,
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
    pub session_writer: Option<session::SessionWriter>,
    /// Monotonic turn counter for session record correlation.
    pub turn_count: u64,
    /// Registry of background processes started via `run_shell`.
    pub process_registry: ProcessRegistry,
    /// The last submitted prompt text, retained so it can be restored on
    /// provider failure. Cleared on successful completion.
    pub last_input: Option<String>,
    /// Current target for input submitted while the agent is running.
    pub queue_target: QueueTarget,
    /// Steering messages waiting to be sent to the active agent thread.
    pub queued_steering: Vec<String>,
    /// Follow-up prompts to submit as new turns after the active run completes.
    pub queued_followups: Vec<String>,
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

        let mut transcript = Vec::new();
        if !context_sources.is_empty() {
            let summaries: Vec<String> = context_sources.iter().map(|s| s.summary()).collect();
            transcript.push(Entry::Status { text: format!("context  {}", summaries.join(", ")) });
        }

        let sessions_dir = session::sessions_dir(&workspace_root);
        let session_id = session::generate_session_id();
        let mut session_writer = session::SessionWriter::create(
            &sessions_dir,
            &session_id,
            &workspace_root.display().to_string(),
            "scratch",
            "umans",
            &cli.model,
            cli.websearch.label(),
            env!("CARGO_PKG_VERSION"),
        )
        .ok();

        if let Some(ref mut writer) = session_writer.as_mut()
            && !context_sources.is_empty()
        {
            let _ = writer.append_context(&context_sources);
        }

        App {
            session_id,
            mode: Mode::default(),
            run_state: RunState::default(),
            input: String::new(),
            input_history: Vec::new(),
            history_cursor: None,
            history_draft: String::new(),
            transcript,
            cwd: cli.cwd.clone(),
            model: cli.model.clone(),
            user_label: default_user_label(),
            websearch: cli.websearch,
            verbose: cli.verbose,
            session_tokens_in: 0,
            session_tokens_out: 0,
            context_sources,
            scroll_offset: 0,
            ui_tick: 0,
            ctrl_d_pending: None,
            session_writer,
            turn_count: 0,
            process_registry: ProcessRegistry::new(),
            last_input: None,
            queue_target: QueueTarget::default(),
            queued_steering: Vec::new(),
            queued_followups: Vec::new(),
            quit: false,
        }
    }

    /// Derive the granular status label for the status line.
    ///
    /// Maps `RunState` plus the last transcript entry into one of
    /// idle, sending, thinking, streaming, running tool, stopping,
    /// cancelled, failed, error, done.
    pub fn status_label(&self) -> &'static str {
        match self.run_state {
            RunState::Working => match self.transcript.last() {
                Some(Entry::Reasoning { streaming: true, .. }) => "thinking",
                Some(Entry::Assistant { streaming: true, .. }) => "streaming",
                Some(Entry::Tool { status: ToolStatus::Running, .. }) => "running tool",
                Some(Entry::User { .. }) | None => "sending",
                _ => "streaming",
            },
            RunState::Stopping => "stopping",
            RunState::Error(_) => "failed",
            RunState::Idle => {
                if matches!(self.transcript.last(), Some(Entry::Status { text }) if text == "cancelled") {
                    return "cancelled";
                }
                match self.last_non_status_entry() {
                    Some(Entry::Error { .. }) => "failed",
                    Some(Entry::Tool { status: ToolStatus::Failed, .. }) => "failed",
                    Some(Entry::Assistant { streaming: false, .. })
                    | Some(Entry::Tool { status: ToolStatus::Ok, .. }) => "done",
                    _ => "idle",
                }
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
            RunState::Stopping => PromptState::Stopped,
            RunState::Error(_) => PromptState::Errored,
            RunState::Idle => {
                if matches!(self.transcript.last(), Some(Entry::Status { text }) if text == "cancelled") {
                    return PromptState::Stopped;
                }
                match self.last_non_status_entry() {
                    Some(Entry::Error { .. }) => PromptState::Errored,
                    _ => PromptState::Editable,
                }
            }
        }
    }

    fn last_non_status_entry(&self) -> Option<&Entry> {
        self.transcript
            .iter()
            .rev()
            .find(|entry| !matches!(entry, Entry::Status { .. }))
            .or_else(|| self.transcript.last())
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
        Msg::Mouse(mouse) => handle_mouse(app, *mouse),
        Msg::Quit => {
            app.process_registry.cancel_all();
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
/// - Up/Down recall prompt history.
/// - PageUp/PageDown and Ctrl+Alt+U/D/Y/E scroll the transcript (available even while the
///   agent is running, so cancel/quit stay usable).
///     - Page up: jump by 10 lines.
///     - Page down: jump by 10 lines, or reset to newest.
///     - Ctrl+Alt+U/D: jump by 10 lines without dedicated paging keys.
///     - Ctrl+Alt+Y/E: scroll by one line.
fn handle_key(app: &mut App, key: KeyEvent) -> Option<Msg> {
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        app.quit = true;
        return Some(Msg::Quit);
    }

    if key.code == KeyCode::Char('d')
        && key.modifiers.contains(KeyModifiers::CONTROL)
        && !key.modifiers.contains(KeyModifiers::ALT)
    {
        if let Some(deadline) = app.ctrl_d_pending
            && !now_or_after_deadline(app.ui_tick, deadline)
        {
            app.ctrl_d_pending = None;
            app.quit = true;
            return Some(Msg::Quit);
        } else {
            let deadline = app.ui_tick.wrapping_add(QUIT_CONFIRM_TIMEOUT_TICKS);
            app.ctrl_d_pending = Some(deadline);
            app.transcript
                .push(Entry::Status { text: String::from("Press CTRL+D again to quit.") });
            maybe_pin_to_bottom(app);
            return None;
        }
    }

    if key.code == KeyCode::Char('t') && key.modifiers.contains(KeyModifiers::CONTROL) {
        if app.run_state == RunState::Working {
            app.queue_target = app.queue_target.toggle();
            app.transcript
                .push(Entry::Status { text: format!("queue target: {}", app.queue_target.label()) });
            maybe_pin_to_bottom(app);
        }
        return None;
    }

    app.ctrl_d_pending = None;

    match app.mode {
        Mode::Help => handle_help_key(app, key),
        Mode::Command => handle_command_key(app, key),
        Mode::Prompt => handle_prompt_key(app, key),
    }
}

/// Handle keys in Help overlay mode: Esc or `?` returns to the previous mode.
fn handle_help_key(app: &mut App, key: KeyEvent) -> Option<Msg> {
    match key.code {
        KeyCode::Esc | KeyCode::Char('?') | KeyCode::Enter => {
            app.mode = Mode::Prompt;
            None
        }
        _ => None,
    }
}

fn handle_mouse(app: &mut App, mouse: MouseEvent) -> Option<Msg> {
    const MOUSE_SCROLL_LINES: usize = 3;

    match mouse.kind {
        MouseEventKind::ScrollUp => {
            app.scroll_offset = app.scroll_offset.saturating_add(MOUSE_SCROLL_LINES);
            None
        }
        MouseEventKind::ScrollDown => {
            app.scroll_offset = app.scroll_offset.saturating_sub(MOUSE_SCROLL_LINES);
            None
        }
        _ => None,
    }
}

/// Handle keys in Command mode: typed chars build the command buffer,
/// Enter executes, Esc/Backspace-on-empty returns to Prompt.
fn handle_command_key(app: &mut App, key: KeyEvent) -> Option<Msg> {
    match key.code {
        KeyCode::Esc => {
            app.mode = Mode::Prompt;
            app.input.clear();
            None
        }
        KeyCode::Backspace => {
            if app.input.is_empty() {
                app.mode = Mode::Prompt;
            } else {
                app.input.pop();
            }
            None
        }
        KeyCode::Enter => {
            let text = app.input.trim().to_string();
            app.input.clear();
            app.mode = Mode::Prompt;
            if text.is_empty() { None } else { handle_command(app, &text) }
        }
        KeyCode::Char(ch) => {
            app.input.push(ch);
            None
        }
        _ => None,
    }
}

/// Handle keys in normal Prompt mode.
fn handle_prompt_key(app: &mut App, key: KeyEvent) -> Option<Msg> {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.modifiers.contains(KeyModifiers::ALT) {
        match key.code {
            KeyCode::Char('u') | KeyCode::Char('U') => {
                app.scroll_offset = app.scroll_offset.saturating_add(10);
                return None;
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                app.scroll_offset = app.scroll_offset.saturating_sub(10);
                return None;
            }
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                app.scroll_offset = app.scroll_offset.saturating_add(1);
                return None;
            }
            KeyCode::Char('e') | KeyCode::Char('E') => {
                app.scroll_offset = app.scroll_offset.saturating_sub(1);
                return None;
            }
            _ => {}
        }
    }

    match key.code {
        KeyCode::Char('?') if app.input.is_empty() => {
            app.mode = Mode::Help;
            None
        }
        KeyCode::Char(':') if app.input.is_empty() && matches!(app.run_state, RunState::Idle | RunState::Error(_)) => {
            app.mode = Mode::Command;
            None
        }
        KeyCode::Up => {
            recall_older_input(app);
            None
        }
        KeyCode::Down => {
            recall_newer_input(app);
            None
        }
        KeyCode::PageUp => {
            app.scroll_offset = app.scroll_offset.saturating_add(10);
            None
        }
        KeyCode::PageDown => {
            if app.scroll_offset > 10 {
                app.scroll_offset -= 10;
            } else {
                app.scroll_offset = 0;
            }
            None
        }
        KeyCode::Char(ch) => {
            exit_history_navigation(app);
            app.input.push(ch);
            None
        }
        KeyCode::Backspace => {
            exit_history_navigation(app);
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
/// Returns an optional follow-up [`Msg`].
fn handle_submit(app: &mut App) -> Option<Msg> {
    if app.run_state == RunState::Working {
        queue_running_input(app);
        return None;
    }

    if !matches!(app.run_state, RunState::Idle | RunState::Error(_)) {
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

    submit_user_turn(app, text)
}

fn queue_running_input(app: &mut App) {
    let text = app.input.trim().to_string();
    if text.is_empty() {
        app.input.clear();
        return;
    }

    app.input.clear();
    remember_input(app, &text);
    match app.queue_target {
        QueueTarget::Steering => {
            app.queued_steering.push(text);
            app.transcript
                .push(Entry::Status { text: format!("queued steering ({})", app.queued_steering.len()) });
        }
        QueueTarget::FollowUp => {
            app.queued_followups.push(text);
            app.transcript
                .push(Entry::Status { text: format!("queued follow-up ({})", app.queued_followups.len()) });
        }
    }
    maybe_pin_to_bottom(app);
}

fn submit_user_turn(app: &mut App, text: String) -> Option<Msg> {
    remember_input(app, &text);
    app.transcript.push(Entry::User { text: text.clone() });
    app.input.clear();
    app.history_cursor = None;
    app.history_draft.clear();
    app.last_input = Some(text);
    app.turn_count += 1;
    let turn_id = format!("turn_{}", app.turn_count);
    if let Some(ref mut writer) = app.session_writer {
        let _ = writer.append_entry(app.transcript.last().unwrap(), &turn_id);
    }
    Some(Msg::Agent(AgentEvent::Started))
}

/// Route a slash command (the part after `/` or the text after `:`).
fn handle_command(app: &mut App, command: &str) -> Option<Msg> {
    match command {
        "clear" => {
            app.transcript.clear();
            app.input.clear();
            app.queued_steering.clear();
            app.queued_followups.clear();
            Some(Msg::Clear)
        }
        "quit" | "exit" => {
            app.input.clear();
            app.quit = true;
            Some(Msg::Quit)
        }
        "help" => {
            app.mode = Mode::Help;
            None
        }
        "bg" => {
            list_background_processes(app);
            None
        }
        _ => None,
    }
}

/// List background processes in the transcript.
fn list_background_processes(app: &mut App) {
    let bg_ids: Vec<u64> = app.process_registry.background_ids().collect();
    if bg_ids.is_empty() {
        app.transcript
            .push(Entry::Status { text: String::from("no background processes") });
    } else {
        let lines: Vec<String> = bg_ids
            .iter()
            .filter_map(|id| {
                app.process_registry.get(*id).map(|p| {
                    let elapsed = p.elapsed().as_secs();
                    let cmd = p.command.join(" ");
                    format!("[{id}] {cmd} cwd={} ({elapsed}s)", p.cwd.display())
                })
            })
            .collect();
        app.transcript
            .push(Entry::Status { text: format!("background processes:\n{}", lines.join("\n")) });
    }
    maybe_pin_to_bottom(app);
}

/// Process an [`AgentEvent`] and mutate `app` accordingly.
fn handle_agent_event(app: &mut App, event: AgentEvent) -> Option<Msg> {
    match event {
        AgentEvent::Started => {
            app.run_state = RunState::Working;
            None
        }
        AgentEvent::Status(text) => {
            if app.verbose || !is_verbose_status(&text) {
                app.transcript.push(Entry::Status { text });
                maybe_pin_to_bottom(app);
            }
            None
        }
        AgentEvent::Usage { input_tokens, output_tokens } => {
            app.session_tokens_in = app.session_tokens_in.saturating_add(input_tokens);
            app.session_tokens_out = app.session_tokens_out.saturating_add(output_tokens);
            if let Some(ref mut writer) = app.session_writer {
                let _ = writer.append_usage(input_tokens, output_tokens);
            }
            None
        }
        AgentEvent::AssistantDelta(delta) => {
            if let Some(Entry::Assistant { text, streaming: true }) = app.transcript.last_mut() {
                text.push_str(&delta);
            } else {
                app.transcript.push(Entry::Assistant { text: delta, streaming: true });
            }
            maybe_pin_to_bottom(app);
            None
        }
        AgentEvent::ReasoningDelta(delta) => {
            if let Some(Entry::Reasoning { text, streaming: true }) = app.transcript.last_mut() {
                text.push_str(&delta);
            } else {
                app.transcript.push(Entry::Reasoning { text: delta, streaming: true });
            }
            maybe_pin_to_bottom(app);
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
            maybe_pin_to_bottom(app);
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

            if let Some(result) = shell_result {
                if result.kind == tools::shell::ProcessKind::Background {
                    let cancel = tools::shell::CancelFlag::new();
                    let id =
                        app.process_registry
                            .register(result.command.clone(), result.cwd.clone(), result.kind, cancel);
                    app.transcript.push(Entry::Status {
                        text: format!("background process [{id}] started: {}", result.command.join(" ")),
                    });
                    maybe_pin_to_bottom(app);
                }

                if let Some(ref mut writer) = app.session_writer {
                    let turn_id = format!("turn_{}", app.turn_count);
                    let _ = writer.append_shell_exec(&turn_id, &result);
                }
            }
            None
        }
        AgentEvent::Finished => {
            finalize_streaming(app);
            app.run_state = RunState::Idle;
            app.last_input = None;
            persist_final_response(app);
            if app.queued_followups.is_empty() {
                None
            } else {
                let next = app.queued_followups.remove(0);
                submit_user_turn(app, next)
            }
        }
        AgentEvent::Failed(msg) => {
            finalize_streaming(app);
            app.transcript.push(Entry::Error { text: msg.clone() });
            app.run_state = RunState::Error(msg);
            if let Some(input) = app.last_input.take() {
                app.input = input;
            }
            persist_last_entry(app);
            None
        }
        AgentEvent::Cancelled => {
            finalize_streaming(app);
            if app.run_state == RunState::Working {
                app.transcript.push(Entry::Status { text: String::from("cancelled") });
            }
            app.run_state = RunState::Idle;
            app.last_input = None;
            app.queued_steering.clear();
            persist_last_entry(app);
            None
        }
    }
}

/// Cancel an active stream by marking all streaming entries complete,
/// recording a cancelled status entry, and transitioning to `Stopping`.
///
/// The app loop observes the transition out of `Working` and drops the
/// background receiver, which stops the agent thread on its next failed send.
/// When the `Cancelled` agent event arrives (or the channel disconnects), the
/// state transitions from `Stopping` to `Idle`.
fn cancel_stream(app: &mut App) {
    finalize_streaming(app);
    app.transcript.push(Entry::Status { text: String::from("cancelled") });
    app.run_state = RunState::Stopping;
    persist_last_entry(app);
    maybe_pin_to_bottom(app);
}

/// Reset the scroll offset to pin the transcript to the newest entries.
fn pin_to_bottom(app: &mut App) {
    app.scroll_offset = 0;
}

fn maybe_pin_to_bottom(app: &mut App) {
    if app.scroll_offset == 0 {
        pin_to_bottom(app);
    }
}

fn remember_input(app: &mut App, text: &str) {
    if text.is_empty() || app.input_history.last().is_some_and(|last| last == text) {
        return;
    }
    app.input_history.push(text.to_string());
}

fn recall_older_input(app: &mut App) {
    if app.input_history.is_empty() {
        return;
    }

    let next = match app.history_cursor {
        Some(0) => 0,
        Some(index) => index.saturating_sub(1),
        None => {
            app.history_draft = app.input.clone();
            app.input_history.len() - 1
        }
    };
    app.history_cursor = Some(next);
    app.input = app.input_history[next].clone();
}

fn recall_newer_input(app: &mut App) {
    let Some(index) = app.history_cursor else {
        return;
    };

    if index + 1 < app.input_history.len() {
        let next = index + 1;
        app.history_cursor = Some(next);
        app.input = app.input_history[next].clone();
    } else {
        app.history_cursor = None;
        app.input = app.history_draft.clone();
        app.history_draft.clear();
    }
}

fn exit_history_navigation(app: &mut App) {
    if app.history_cursor.is_some() {
        app.history_cursor = None;
        app.history_draft.clear();
    }
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

/// Persist the final model response even if provider status rows were appended
/// after the last assistant/reasoning delta.
fn persist_final_response(app: &mut App) {
    if let Some(ref mut writer) = app.session_writer
        && let Some(entry) = app.transcript.iter().rev().find(|entry| {
            matches!(
                entry,
                Entry::Assistant { streaming: false, .. } | Entry::Reasoning { streaming: false, .. }
            )
        })
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

fn is_verbose_status(text: &str) -> bool {
    text.starts_with("provider:") || text.starts_with("logs  ") || text.starts_with("tool budget:")
}
