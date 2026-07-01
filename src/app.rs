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

use crate::cli::{Cli, Theme, WebSearchMode};
use crate::fuzzy;
use crate::input::PromptInput;
use crate::tools::shell::ProcessRegistry;
use crate::{context, session, tools};

/// Number of UI ticks the user has to press Ctrl+D a second time before the
/// quit confirmation expires and a fresh double-press is needed.
///
/// With the default 100 ms tick rate this is roughly 3 seconds.
const QUIT_CONFIRM_TIMEOUT_TICKS: u64 = 30;

pub const FILE_PICKER_VISIBLE_ROWS: usize = 8;

const FILE_PICKER_LIMIT: usize = 200;

/// Top-level interaction mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum Mode {
    /// Normal prompt entry.
    #[default]
    Prompt,
    /// Slash-command entry, entered with `:`.
    Command,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum PromptAccessory {
    #[default]
    None,
    Help,
    Commands {
        selected: usize,
    },
    Files(FilePickerSource),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FilePickerSource {
    Forced,
    Mention { token_start: usize },
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilePickerState {
    pub query: String,
    pub all_files: Vec<String>,
    pub matches: Vec<String>,
    /// Character indices of fuzzy match highlights, parallel to `matches`.
    pub match_indices: Vec<Vec<usize>>,
    pub selected: usize,
    pub scroll: usize,
}

impl FilePickerState {
    fn new(all_files: Vec<String>) -> Self {
        let (matches, match_indices) = split_filter(&all_files, "", FILE_PICKER_LIMIT);
        Self { query: String::new(), all_files, matches, match_indices, selected: 0, scroll: 0 }
    }

    fn refresh_matches(&mut self) {
        let (matches, match_indices) = split_filter(&self.all_files, &self.query, FILE_PICKER_LIMIT);
        self.matches = matches;
        self.match_indices = match_indices;
        self.selected = self.selected.min(self.matches.len().saturating_sub(1));
        self.ensure_selected_visible();
    }

    fn move_up(&mut self) {
        if self.matches.is_empty() {
            return;
        }
        self.selected = self.selected.saturating_sub(1);
        self.ensure_selected_visible();
    }

    fn move_down(&mut self) {
        if self.matches.is_empty() {
            return;
        }
        self.selected = (self.selected + 1).min(self.matches.len().saturating_sub(1));
        self.ensure_selected_visible();
    }

    fn page_up(&mut self) {
        self.selected = self.selected.saturating_sub(FILE_PICKER_VISIBLE_ROWS);
        self.ensure_selected_visible();
    }

    fn page_down(&mut self) {
        if self.matches.is_empty() {
            return;
        }
        self.selected = (self.selected + FILE_PICKER_VISIBLE_ROWS).min(self.matches.len().saturating_sub(1));
        self.ensure_selected_visible();
    }

    fn selected_path(&self) -> Option<&str> {
        self.matches.get(self.selected).map(String::as_str)
    }

    fn ensure_selected_visible(&mut self) {
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + FILE_PICKER_VISIBLE_ROWS {
            self.scroll = self.selected.saturating_sub(FILE_PICKER_VISIBLE_ROWS - 1);
        }
    }
}

/// The full application state used to draw the screen.
#[derive(Debug)]
pub struct App {
    pub session_id: String,
    pub mode: Mode,
    pub run_state: RunState,
    pub input: PromptInput,
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
    /// UI color theme.
    pub theme: Theme,
    /// Whether diagnostic provider/log status rows should be shown in transcript.
    pub verbose: bool,
    /// Provider token usage accumulated for this session.
    pub session_tokens_in: u64,
    pub session_tokens_out: u64,
    /// Loaded context sources (e.g. AGENTS.md).
    pub context_sources: Vec<context::ContextSource>,
    /// Legacy fullscreen transcript offset. Inline scrollback mode does not use it.
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
    /// Active file picker state.
    pub file_picker: Option<FilePickerState>,
    /// Inline prompt accessory rendered above the input.
    pub prompt_accessory: PromptAccessory,
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
            input: PromptInput::new(),
            input_history: Vec::new(),
            history_cursor: None,
            history_draft: String::new(),
            transcript,
            cwd: workspace_root,
            model: cli.model.clone(),
            user_label: default_user_label(),
            websearch: cli.websearch,
            theme: cli.theme,
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
            file_picker: None,
            prompt_accessory: PromptAccessory::None,
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

    if !matches!(app.prompt_accessory, PromptAccessory::None)
        && let Some(msg) = handle_accessory_key(app, key)
    {
        return msg;
    }

    match app.mode {
        Mode::Command => handle_command_key(app, key),
        Mode::Prompt => handle_prompt_key(app, key),
    }
}

fn handle_mouse(app: &mut App, mouse: MouseEvent) -> Option<Msg> {
    if matches!(app.prompt_accessory, PromptAccessory::Files(_)) {
        if let Some(picker) = app.file_picker.as_mut() {
            match mouse.kind {
                MouseEventKind::ScrollUp => picker.move_up(),
                MouseEventKind::ScrollDown => picker.move_down(),
                _ => {}
            }
        }
        return None;
    }

    None
}

fn handle_accessory_key(app: &mut App, key: KeyEvent) -> Option<Option<Msg>> {
    match app.prompt_accessory {
        PromptAccessory::Help => match key.code {
            KeyCode::Esc => {
                close_prompt_accessory(app);
                Some(None)
            }
            _ => None,
        },
        PromptAccessory::Commands { .. } => handle_command_accessory_key(app, key),
        PromptAccessory::Files(_) => handle_file_accessory_key(app, key),
        PromptAccessory::None => None,
    }
}

fn handle_command_accessory_key(app: &mut App, key: KeyEvent) -> Option<Option<Msg>> {
    let count = command_suggestions_for_app(app).len();
    match key.code {
        KeyCode::Esc => {
            close_prompt_accessory(app);
            Some(None)
        }
        KeyCode::Up => {
            if let PromptAccessory::Commands { selected } = &mut app.prompt_accessory {
                *selected = selected.saturating_sub(1);
            }
            Some(None)
        }
        KeyCode::Down => {
            if let PromptAccessory::Commands { selected } = &mut app.prompt_accessory {
                *selected = (*selected + 1).min(count.saturating_sub(1));
            }
            Some(None)
        }
        KeyCode::Enter
            if count > 0
                && !command_suggestions_for_app(app)
                    .iter()
                    .any(|(cmd, _)| *cmd == command_query(app)) =>
        {
            Some(accept_command_suggestion(app))
        }
        _ => None,
    }
}

fn handle_file_accessory_key(app: &mut App, key: KeyEvent) -> Option<Option<Msg>> {
    let source = match app.prompt_accessory {
        PromptAccessory::Files(source) => source,
        _ => return None,
    };
    let picker = app.file_picker.as_mut()?;
    match key.code {
        KeyCode::Esc => {
            close_prompt_accessory(app);
            Some(None)
        }
        KeyCode::Enter => {
            accept_file_suggestion(app);
            Some(None)
        }
        KeyCode::Up => {
            picker.move_up();
            Some(None)
        }
        KeyCode::Down => {
            picker.move_down();
            Some(None)
        }
        KeyCode::PageUp => {
            picker.page_up();
            Some(None)
        }
        KeyCode::PageDown => {
            picker.page_down();
            Some(None)
        }
        KeyCode::Backspace if source == FilePickerSource::Forced => {
            picker.query.pop();
            picker.refresh_matches();
            Some(None)
        }
        KeyCode::Char(ch) if source == FilePickerSource::Forced => {
            picker.query.push(ch);
            picker.refresh_matches();
            Some(None)
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
            close_prompt_accessory(app);
            None
        }
        KeyCode::Backspace => {
            if app.input.is_empty() {
                app.mode = Mode::Prompt;
                close_prompt_accessory(app);
            } else {
                app.input.backspace();
                sync_prompt_accessory(app);
            }
            None
        }
        KeyCode::Enter => {
            let text = app.input.as_str().trim().to_string();
            app.input.clear();
            app.mode = Mode::Prompt;
            close_prompt_accessory(app);
            if text.is_empty() { None } else { handle_command(app, &text) }
        }
        KeyCode::Char(ch) => {
            app.input.insert_char(ch);
            sync_prompt_accessory(app);
            None
        }
        _ => None,
    }
}

/// Handle keys in normal Prompt mode.
///
/// Cursor keybinds:
/// - `left` / `ctrl+b`: move cursor left
/// - `right` / `ctrl+f`: move cursor right
/// - `alt+left` / `ctrl+left` / `alt+b`: move cursor word left
/// - `alt+right` / `ctrl+right` / `alt+f`: move cursor word right
/// - `home` / `ctrl+a`: move to line start
/// - `end` / `ctrl+e`: move to line end
/// - `shift+enter` / `ctrl+j`: insert newline
/// - `backspace`: delete char before cursor
/// - `delete`: delete char after cursor (forward delete)
fn handle_prompt_key(app: &mut App, key: KeyEvent) -> Option<Msg> {
    if key.code == KeyCode::Char('p') && key.modifiers.contains(KeyModifiers::CONTROL) {
        open_file_picker(app, FilePickerSource::Forced);
        return None;
    }

    if key.modifiers.contains(KeyModifiers::ALT) {
        let handled = match key.code {
            KeyCode::Left | KeyCode::Char('b') => {
                app.input.cursor_word_left();
                exit_history_navigation(app);
                true
            }
            KeyCode::Right | KeyCode::Char('f') => {
                app.input.cursor_word_right();
                exit_history_navigation(app);
                true
            }
            _ => false,
        };
        if handled {
            return None;
        }
    }

    if key.modifiers.contains(KeyModifiers::CONTROL) && !key.modifiers.contains(KeyModifiers::ALT) {
        let handled = match key.code {
            KeyCode::Left => {
                app.input.cursor_word_left();
                exit_history_navigation(app);
                true
            }
            KeyCode::Right => {
                app.input.cursor_word_right();
                exit_history_navigation(app);
                true
            }
            _ => false,
        };
        if handled {
            return None;
        }
    }

    if key.modifiers.contains(KeyModifiers::CONTROL) && !key.modifiers.contains(KeyModifiers::ALT) {
        let handled = match key.code {
            KeyCode::Char('a') => {
                app.input.cursor_to_start();
                exit_history_navigation(app);
                sync_prompt_accessory(app);
                true
            }
            KeyCode::Char('e') => {
                app.input.cursor_to_end();
                exit_history_navigation(app);
                sync_prompt_accessory(app);
                true
            }
            KeyCode::Char('b') => {
                app.input.cursor_left();
                exit_history_navigation(app);
                sync_prompt_accessory(app);
                true
            }
            KeyCode::Char('f') => {
                app.input.cursor_right();
                exit_history_navigation(app);
                sync_prompt_accessory(app);
                true
            }
            KeyCode::Char('j') => {
                exit_history_navigation(app);
                app.input.insert_char('\n');
                sync_prompt_accessory(app);
                true
            }
            _ => false,
        };
        if handled {
            return None;
        }
    }

    match key.code {
        KeyCode::Char('?') if app.input.is_empty() => {
            app.prompt_accessory = PromptAccessory::Help;
            None
        }
        KeyCode::Char(':') if app.input.is_empty() && matches!(app.run_state, RunState::Idle | RunState::Error(_)) => {
            app.mode = Mode::Command;
            app.prompt_accessory = PromptAccessory::Commands { selected: 0 };
            None
        }
        KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
            exit_history_navigation(app);
            app.input.insert_char('\n');
            sync_prompt_accessory(app);
            None
        }
        KeyCode::Up => {
            if !app.input.cursor_up() {
                recall_older_input(app);
            } else {
                exit_history_navigation(app);
            }
            sync_prompt_accessory(app);
            None
        }
        KeyCode::Down => {
            if !app.input.cursor_down() {
                recall_newer_input(app);
            } else {
                exit_history_navigation(app);
            }
            sync_prompt_accessory(app);
            None
        }
        KeyCode::Left => {
            app.input.cursor_left();
            exit_history_navigation(app);
            sync_prompt_accessory(app);
            None
        }
        KeyCode::Right => {
            app.input.cursor_right();
            exit_history_navigation(app);
            sync_prompt_accessory(app);
            None
        }
        KeyCode::Home => {
            app.input.cursor_to_start();
            exit_history_navigation(app);
            sync_prompt_accessory(app);
            None
        }
        KeyCode::End => {
            app.input.cursor_to_end();
            exit_history_navigation(app);
            sync_prompt_accessory(app);
            None
        }
        KeyCode::PageUp | KeyCode::PageDown => None,
        KeyCode::Delete => {
            exit_history_navigation(app);
            app.input.delete_forward();
            sync_prompt_accessory(app);
            None
        }
        KeyCode::Char(ch) => {
            exit_history_navigation(app);
            app.input.insert_char(ch);
            sync_prompt_accessory(app);
            None
        }
        KeyCode::Backspace => {
            exit_history_navigation(app);
            app.input.backspace();
            sync_prompt_accessory(app);
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

pub fn command_suggestions_for_app(app: &App) -> Vec<(&'static str, &'static str)> {
    let query = command_query(app);
    let commands = [
        ("clear", "clear transcript"),
        ("quit", "exit app"),
        ("exit", "exit app"),
        ("help", "show help"),
        ("bg", "list background processes"),
    ];
    commands
        .into_iter()
        .filter(|(cmd, _)| cmd.starts_with(&query))
        .collect()
}

fn command_query(app: &App) -> String {
    if app.mode == Mode::Command {
        app.input.as_str().trim_start().to_string()
    } else {
        app.input
            .as_str()
            .strip_prefix('/')
            .unwrap_or("")
            .trim_start()
            .to_string()
    }
}

fn accept_command_suggestion(app: &mut App) -> Option<Msg> {
    let suggestions = command_suggestions_for_app(app);
    if suggestions.is_empty() {
        return None;
    }
    let selected = match app.prompt_accessory {
        PromptAccessory::Commands { selected } => selected.min(suggestions.len() - 1),
        _ => 0,
    };
    let command = suggestions[selected].0;
    let replacement = if app.mode == Mode::Command { format!("{command} ") } else { format!("/{command} ") };
    app.input.set_text(&replacement);
    app.prompt_accessory = PromptAccessory::None;
    None
}

fn open_file_picker(app: &mut App, source: FilePickerSource) {
    match tools::searchable_file_paths(&app.cwd, 2_000) {
        Ok(files) => {
            app.file_picker = Some(FilePickerState::new(files));
            app.prompt_accessory = PromptAccessory::Files(source);
            sync_file_picker_query(app);
        }
        Err(err) => {
            app.transcript
                .push(Entry::Error { text: format!("file picker failed: {err}") });
            maybe_pin_to_bottom(app);
        }
    }
}

fn close_prompt_accessory(app: &mut App) {
    if matches!(app.prompt_accessory, PromptAccessory::Files(_)) {
        app.file_picker = None;
    }
    app.prompt_accessory = PromptAccessory::None;
}

/// Run fuzzy filter and split results into parallel matches + indices vectors.
fn split_filter(all_files: &[String], query: &str, limit: usize) -> (Vec<String>, Vec<Vec<usize>>) {
    let filtered = fuzzy::fuzzy_filter_with_indices(all_files, query, limit);
    filtered.into_iter().unzip()
}

fn insert_file_path(app: &mut App, path: &str) {
    if !app.input.is_empty() && !app.input.text_before_cursor().ends_with(char::is_whitespace) {
        app.input.insert_char(' ');
    }
    app.input.insert_str(path);
}

fn accept_file_suggestion(app: &mut App) {
    let Some(path) = app
        .file_picker
        .as_ref()
        .and_then(|picker| picker.selected_path().map(str::to_string))
    else {
        return;
    };

    match app.prompt_accessory {
        PromptAccessory::Files(FilePickerSource::Mention { token_start }) => {
            let end = app.input.cursor();
            let replacement = format!("@{path} ");
            app.input.replace_range(token_start, end, &replacement);
        }
        PromptAccessory::Files(FilePickerSource::Forced) => {
            insert_file_path(app, &path);
        }
        _ => {}
    }

    close_prompt_accessory(app);
}

fn sync_prompt_accessory(app: &mut App) {
    if app.mode == Mode::Command {
        app.prompt_accessory = PromptAccessory::Commands { selected: 0 };
        return;
    }

    if app.input.as_str().starts_with('/') {
        app.prompt_accessory = PromptAccessory::Commands { selected: 0 };
        return;
    }

    if let Some((token_start, _query)) = active_at_token(app) {
        if !matches!(app.prompt_accessory, PromptAccessory::Files(FilePickerSource::Mention { token_start: existing }) if existing == token_start)
        {
            open_file_picker(app, FilePickerSource::Mention { token_start });
        } else {
            sync_file_picker_query(app);
        }
        return;
    }

    if !matches!(app.prompt_accessory, PromptAccessory::Help) {
        close_prompt_accessory(app);
    }
}

fn active_at_token(app: &App) -> Option<(usize, String)> {
    let before = app.input.text_before_cursor();
    let chars: Vec<char> = before.chars().collect();
    let token_start = chars.iter().rposition(|ch| ch.is_whitespace()).map_or(0, |idx| idx + 1);
    if chars.get(token_start) != Some(&'@') {
        return None;
    }
    let query: String = chars[token_start + 1..].iter().collect();
    Some((token_start, query))
}

fn sync_file_picker_query(app: &mut App) {
    let query = match app.prompt_accessory {
        PromptAccessory::Files(FilePickerSource::Mention { .. }) => active_at_token(app).map(|(_, query)| query),
        PromptAccessory::Files(FilePickerSource::Forced) => app.file_picker.as_ref().map(|picker| picker.query.clone()),
        _ => None,
    };
    let Some(query) = query else {
        return;
    };
    if let Some(picker) = app.file_picker.as_mut()
        && picker.query != query
    {
        picker.query = query;
        picker.refresh_matches();
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

    let text = app.input.as_str().trim().to_string();
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
    let text = app.input.as_str().trim().to_string();
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
            app.prompt_accessory = PromptAccessory::Help;
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
            finalize_reasoning(app);
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
            finalize_streaming(app);
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
            finalize_streaming(app);
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
                app.input.set_text(&input);
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
            app.history_draft = app.input.text();
            app.input_history.len() - 1
        }
    };
    app.history_cursor = Some(next);
    app.input.set_text(&app.input_history[next]);
}

fn recall_newer_input(app: &mut App) {
    let Some(index) = app.history_cursor else {
        return;
    };

    if index + 1 < app.input_history.len() {
        let next = index + 1;
        app.history_cursor = Some(next);
        app.input.set_text(&app.input_history[next]);
    } else {
        app.history_cursor = None;
        app.input.set_text(&app.history_draft);
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

/// Mark active reasoning entries complete when the model moves on to visible
/// assistant text or a tool call.
fn finalize_reasoning(app: &mut App) {
    for entry in &mut app.transcript {
        if let Entry::Reasoning { streaming, .. } = entry {
            *streaming = false;
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
