//! Keyboard interaction, prompt accessories, and picker behavior.
//!
//! This module converts terminal key and mouse events into prompt edits,
//! accessory transitions, picker selections, and submitted [`Msg`] values. It
//! owns command mode, file/model/reasoning/skill pickers, detail-pane
//! navigation, input history, and queued input while an agent is running.
use super::*;

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
    Models,
    ReasoningEffort,
    Skills,
    /// Bounded inspection of the current context ledger.
    Context,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FilePickerSource {
    Forced,
    Mention { token_start: usize },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KeyOutcome {
    Unhandled,
    Handled,
    Followup(Msg),
}

impl KeyOutcome {
    fn with(followup: Option<Msg>) -> Self {
        match followup {
            Some(msg) => Self::Followup(msg),
            None => Self::Handled,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PickerItem {
    pub label: String,
    pub detail: String,
}

impl PickerItem {
    pub fn new(label: impl Into<String>, detail: impl Into<String>) -> Self {
        Self { label: label.into(), detail: detail.into() }
    }

    fn searchable(&self) -> String {
        if self.detail.is_empty() { self.label.clone() } else { format!("{} {}", self.label, self.detail) }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PickerState {
    pub query: String,
    pub all_items: Vec<PickerItem>,
    pub matches: Vec<PickerItem>,
    /// Character indices of fuzzy match highlights, parallel to `matches`.
    pub match_indices: Vec<Vec<usize>>,
    pub selected: usize,
    pub scroll: usize,
    limit: usize,
}

impl PickerState {
    pub fn new(all_items: Vec<PickerItem>, limit: usize) -> Self {
        let (matches, match_indices) = split_filter_items(&all_items, "", limit);
        Self { query: String::new(), all_items, matches, match_indices, selected: 0, scroll: 0, limit }
    }

    pub fn refresh_matches(&mut self) {
        let (matches, match_indices) = split_filter_items(&self.all_items, &self.query, self.limit);
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
        self.selected = self.selected.saturating_sub(VISIBLE_ROWS);
        self.ensure_selected_visible();
    }

    fn page_down(&mut self) {
        if self.matches.is_empty() {
            return;
        }
        self.selected = (self.selected + VISIBLE_ROWS).min(self.matches.len().saturating_sub(1));
        self.ensure_selected_visible();
    }

    pub fn selected(&self) -> Option<&PickerItem> {
        self.matches.get(self.selected)
    }

    fn ensure_selected_visible(&mut self) {
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + VISIBLE_ROWS {
            self.scroll = self.selected.saturating_sub(VISIBLE_ROWS - 1);
        }
    }
}

/// - Ctrl+C cancels a running agent stream, otherwise quits.
/// - Ctrl+D requires a double-press: the first press shows a confirmation
///   message; the second press within roughly three seconds quits. Any other
///   key (or timeout) cancels the pending state.
/// - Printable characters append to the input buffer.
/// - Backspace removes the last character.
/// - `Enter` submits: slash commands (`/clear`, `/quit`) are routed, otherwise
///   the input is appended as [`Entry::User`] and cleared.
/// - Escape cancels an active agent stream.
/// - Up/Down recall prompt history.
pub fn handle_key(app: &mut App, key: KeyEvent) -> Option<Msg> {
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        if app.run_state == RunState::Working {
            agent_lifecycle::cancel_stream(app);
            return None;
        }
        app.quit = true;
        return Some(Msg::Quit);
    }

    if key.code == KeyCode::Char('o') && key.modifiers.contains(KeyModifiers::CONTROL) {
        open_detail_surface(app);
        return None;
    }

    if key.code == KeyCode::Char('d')
        && key.modifiers.contains(KeyModifiers::CONTROL)
        && !key.modifiers.contains(KeyModifiers::ALT)
    {
        if let Some(deadline) = app.ctrl_d_pending
            && !agent_lifecycle::now_or_after_deadline(app.ui_tick, deadline)
        {
            app.ctrl_d_pending = None;
            app.quit = true;
            return Some(Msg::Quit);
        } else {
            let deadline = app.ui_tick.wrapping_add(quit_confirm_timeout_ticks(app));
            app.ctrl_d_pending = Some(deadline);
            app.transcript
                .push(Entry::Status { text: String::from("Press CTRL+D again to quit.") });
            return None;
        }
    }

    if key.code == KeyCode::Char('t')
        && key.modifiers.contains(KeyModifiers::CONTROL)
        && app.run_state == RunState::Working
    {
        app.queue_target = app.queue_target.toggle();
        app.transcript
            .push(Entry::Status { text: format!("queue target: {}", app.queue_target.label()) });
        return None;
    }

    app.ctrl_d_pending = None;

    if app.pending_permission.is_some() {
        return agent_lifecycle::handle_permission_key(app, key);
    }

    if app.first_run_recovery.is_some() {
        return handle_first_run_key(app, key);
    }

    if app.detail_pane.open {
        return handle_detail_pane_key(app, key);
    }

    if !matches!(app.prompt_accessory, PromptAccessory::None) {
        match handle_accessory_key(app, key) {
            KeyOutcome::Unhandled => {}
            KeyOutcome::Handled => return None,
            KeyOutcome::Followup(msg) => return Some(msg),
        }
    }

    match app.mode {
        Mode::Command => handle_command_key(app, key),
        Mode::Prompt => handle_prompt_key(app, key),
    }
}

pub fn handle_mouse(app: &mut App, mouse: MouseEvent) -> Option<Msg> {
    if app.first_run_recovery.is_some() {
        return None;
    }

    match app.prompt_accessory {
        PromptAccessory::Files(_)
        | PromptAccessory::Models
        | PromptAccessory::ReasoningEffort
        | PromptAccessory::Skills
        | PromptAccessory::Context => {
            if let Some(picker) = app.picker.as_mut() {
                match mouse.kind {
                    MouseEventKind::ScrollUp => picker.move_up(),
                    MouseEventKind::ScrollDown => picker.move_down(),
                    _ => {}
                }
            }
            None
        }
        _ => None,
    }
}

pub fn handle_accessory_key(app: &mut App, key: KeyEvent) -> KeyOutcome {
    match app.prompt_accessory {
        PromptAccessory::Help => match key.code {
            KeyCode::Esc => {
                close_prompt_accessory(app);
                KeyOutcome::Handled
            }
            _ => KeyOutcome::Unhandled,
        },
        PromptAccessory::Commands { .. } => handle_command_accessory_key(app, key),
        PromptAccessory::Files(_) => handle_file_accessory_key(app, key),
        PromptAccessory::Models => handle_model_accessory_key(app, key),
        PromptAccessory::ReasoningEffort => handle_reasoning_effort_accessory_key(app, key),
        PromptAccessory::Skills => handle_skill_accessory_key(app, key),
        PromptAccessory::Context => match key.code {
            KeyCode::Esc => {
                close_prompt_accessory(app);
                KeyOutcome::Handled
            }
            _ => KeyOutcome::Unhandled,
        },
        PromptAccessory::None => KeyOutcome::Unhandled,
    }
}

pub fn handle_command_accessory_key(app: &mut App, key: KeyEvent) -> KeyOutcome {
    let count = command_suggestions_for_app(app).len();
    match key.code {
        KeyCode::Esc => {
            close_prompt_accessory(app);
            KeyOutcome::Handled
        }
        KeyCode::Up => {
            if let PromptAccessory::Commands { selected } = &mut app.prompt_accessory {
                *selected = selected.saturating_sub(1);
            }
            KeyOutcome::Handled
        }
        KeyCode::Down => {
            if let PromptAccessory::Commands { selected } = &mut app.prompt_accessory {
                *selected = (*selected + 1).min(count.saturating_sub(1));
            }
            KeyOutcome::Handled
        }
        KeyCode::Enter
            if count > 0
                && !command_suggestions_for_app(app)
                    .iter()
                    .any(|(cmd, _)| *cmd == command_query(app)) =>
        {
            KeyOutcome::with(accept_command_suggestion(app))
        }
        _ => KeyOutcome::Unhandled,
    }
}

pub fn handle_file_accessory_key(app: &mut App, key: KeyEvent) -> KeyOutcome {
    let source = match app.prompt_accessory {
        PromptAccessory::Files(source) => source,
        _ => return KeyOutcome::Unhandled,
    };
    let Some(picker) = app.picker.as_mut() else {
        return KeyOutcome::Unhandled;
    };
    match key.code {
        KeyCode::Esc => {
            close_prompt_accessory(app);
            KeyOutcome::Handled
        }
        KeyCode::Enter => {
            accept_file_suggestion(app);
            KeyOutcome::Handled
        }
        KeyCode::Up => {
            picker.move_up();
            KeyOutcome::Handled
        }
        KeyCode::Down => {
            picker.move_down();
            KeyOutcome::Handled
        }
        KeyCode::PageUp => {
            picker.page_up();
            KeyOutcome::Handled
        }
        KeyCode::PageDown => {
            picker.page_down();
            KeyOutcome::Handled
        }
        KeyCode::Backspace if source == FilePickerSource::Forced => {
            picker.query.pop();
            picker.refresh_matches();
            KeyOutcome::Handled
        }
        KeyCode::Char(ch) if source == FilePickerSource::Forced => {
            picker.query.push(ch);
            picker.refresh_matches();
            KeyOutcome::Handled
        }
        _ => KeyOutcome::Unhandled,
    }
}

pub fn handle_model_accessory_key(app: &mut App, key: KeyEvent) -> KeyOutcome {
    let Some(picker) = app.picker.as_mut() else {
        return KeyOutcome::Unhandled;
    };
    match key.code {
        KeyCode::Esc => {
            close_prompt_accessory(app);
            KeyOutcome::Handled
        }
        KeyCode::Enter => {
            accept_model_suggestion(app);
            KeyOutcome::Handled
        }
        KeyCode::Up => {
            picker.move_up();
            KeyOutcome::Handled
        }
        KeyCode::Down => {
            picker.move_down();
            KeyOutcome::Handled
        }
        KeyCode::PageUp => {
            picker.page_up();
            KeyOutcome::Handled
        }
        KeyCode::PageDown => {
            picker.page_down();
            KeyOutcome::Handled
        }
        KeyCode::Backspace => {
            picker.query.pop();
            picker.refresh_matches();
            KeyOutcome::Handled
        }
        KeyCode::Char(ch) => {
            picker.query.push(ch);
            picker.refresh_matches();
            KeyOutcome::Handled
        }
        _ => KeyOutcome::Unhandled,
    }
}

pub fn handle_reasoning_effort_accessory_key(app: &mut App, key: KeyEvent) -> KeyOutcome {
    let Some(picker) = app.picker.as_mut() else {
        return KeyOutcome::Unhandled;
    };
    match key.code {
        KeyCode::Esc => {
            finish_reasoning_effort_picker(app);
            KeyOutcome::Handled
        }
        KeyCode::Enter => {
            accept_reasoning_effort_suggestion(app);
            KeyOutcome::Handled
        }
        KeyCode::Up => {
            picker.move_up();
            KeyOutcome::Handled
        }
        KeyCode::Down => {
            picker.move_down();
            KeyOutcome::Handled
        }
        KeyCode::PageUp => {
            picker.page_up();
            KeyOutcome::Handled
        }
        KeyCode::PageDown => {
            picker.page_down();
            KeyOutcome::Handled
        }
        KeyCode::Backspace => {
            picker.query.pop();
            picker.refresh_matches();
            KeyOutcome::Handled
        }
        KeyCode::Char(ch) => {
            picker.query.push(ch);
            picker.refresh_matches();
            KeyOutcome::Handled
        }
        _ => KeyOutcome::Unhandled,
    }
}

pub fn handle_skill_accessory_key(app: &mut App, key: KeyEvent) -> KeyOutcome {
    let Some(picker) = app.picker.as_mut() else {
        return KeyOutcome::Unhandled;
    };
    match key.code {
        KeyCode::Esc => {
            close_prompt_accessory(app);
            KeyOutcome::Handled
        }
        KeyCode::Enter => {
            accept_skill_suggestion(app);
            KeyOutcome::Handled
        }
        KeyCode::Up => {
            picker.move_up();
            KeyOutcome::Handled
        }
        KeyCode::Down => {
            picker.move_down();
            KeyOutcome::Handled
        }
        KeyCode::PageUp => {
            picker.page_up();
            KeyOutcome::Handled
        }
        KeyCode::PageDown => {
            picker.page_down();
            KeyOutcome::Handled
        }
        KeyCode::Backspace => {
            picker.query.pop();
            picker.refresh_matches();
            KeyOutcome::Handled
        }
        KeyCode::Char(ch) => {
            picker.query.push(ch);
            picker.refresh_matches();
            KeyOutcome::Handled
        }
        _ => KeyOutcome::Unhandled,
    }
}

/// Handle keys in Command mode: typed chars build the command buffer,
/// Enter executes, Esc/Backspace-on-empty returns to Prompt.
pub fn handle_command_key(app: &mut App, key: KeyEvent) -> Option<Msg> {
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
        KeyCode::Tab => {
            accept_prompt_suggestion(app);
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
pub fn handle_prompt_key(app: &mut App, key: KeyEvent) -> Option<Msg> {
    if app.pending_permission.is_some() {
        return None;
    }

    if key.modifiers.contains(KeyModifiers::ALT) {
        let handled = match key.code {
            KeyCode::Left | KeyCode::Char('b') => {
                app.input.cursor_word_left();
                agent_lifecycle::exit_history_navigation(app);
                true
            }
            KeyCode::Right | KeyCode::Char('f') => {
                app.input.cursor_word_right();
                agent_lifecycle::exit_history_navigation(app);
                true
            }
            KeyCode::Backspace => {
                let killed = app.input.kill_word_left();
                if !killed.is_empty() {
                    app.kill_ring.push(killed);
                }
                agent_lifecycle::exit_history_navigation(app);
                sync_prompt_accessory(app);
                true
            }
            KeyCode::Char('d') => {
                let killed = app.input.kill_word_right();
                if !killed.is_empty() {
                    app.kill_ring.push(killed);
                }
                agent_lifecycle::exit_history_navigation(app);
                sync_prompt_accessory(app);
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
                agent_lifecycle::exit_history_navigation(app);
                true
            }
            KeyCode::Right => {
                app.input.cursor_word_right();
                agent_lifecycle::exit_history_navigation(app);
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
                agent_lifecycle::exit_history_navigation(app);
                sync_prompt_accessory(app);
                true
            }
            KeyCode::Char('e') => {
                app.input.cursor_to_end();
                agent_lifecycle::exit_history_navigation(app);
                sync_prompt_accessory(app);
                true
            }
            KeyCode::Char('b') => {
                app.input.cursor_left();
                agent_lifecycle::exit_history_navigation(app);
                sync_prompt_accessory(app);
                true
            }
            KeyCode::Char('f') => {
                app.input.cursor_right();
                agent_lifecycle::exit_history_navigation(app);
                sync_prompt_accessory(app);
                true
            }
            KeyCode::Char('j') => {
                agent_lifecycle::exit_history_navigation(app);
                app.input.insert_char('\n');
                sync_prompt_accessory(app);
                true
            }
            KeyCode::Char('k') => {
                let killed = app.input.kill_to_end_of_line();
                if !killed.is_empty() {
                    app.kill_ring.push(killed);
                }
                agent_lifecycle::exit_history_navigation(app);
                sync_prompt_accessory(app);
                true
            }
            KeyCode::Char('u') => {
                let killed = app.input.kill_to_start_of_line();
                if !killed.is_empty() {
                    app.kill_ring.push(killed);
                }
                agent_lifecycle::exit_history_navigation(app);
                sync_prompt_accessory(app);
                true
            }
            KeyCode::Char('w') => {
                let killed = app.input.kill_word_left();
                if !killed.is_empty() {
                    app.kill_ring.push(killed);
                }
                agent_lifecycle::exit_history_navigation(app);
                sync_prompt_accessory(app);
                true
            }
            KeyCode::Char('y') => {
                if let Some(killed) = app.kill_ring.last() {
                    app.input.yank(killed);
                }
                agent_lifecycle::exit_history_navigation(app);
                sync_prompt_accessory(app);
                true
            }
            KeyCode::Char('t') => {
                app.input.transpose_chars();
                agent_lifecycle::exit_history_navigation(app);
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
            agent_lifecycle::exit_history_navigation(app);
            app.input.insert_char('\n');
            sync_prompt_accessory(app);
            None
        }
        KeyCode::Up => {
            if !app.input.cursor_up() {
                agent_lifecycle::recall_older_input(app);
            } else {
                agent_lifecycle::exit_history_navigation(app);
            }
            sync_prompt_accessory(app);
            None
        }
        KeyCode::Down => {
            if !app.input.cursor_down() {
                agent_lifecycle::recall_newer_input(app);
            } else {
                agent_lifecycle::exit_history_navigation(app);
            }
            sync_prompt_accessory(app);
            None
        }
        KeyCode::Left => {
            app.input.cursor_left();
            agent_lifecycle::exit_history_navigation(app);
            sync_prompt_accessory(app);
            None
        }
        KeyCode::Right => {
            app.input.cursor_right();
            agent_lifecycle::exit_history_navigation(app);
            sync_prompt_accessory(app);
            None
        }
        KeyCode::Home => {
            app.input.cursor_to_start();
            agent_lifecycle::exit_history_navigation(app);
            sync_prompt_accessory(app);
            None
        }
        KeyCode::End => {
            app.input.cursor_to_end();
            agent_lifecycle::exit_history_navigation(app);
            sync_prompt_accessory(app);
            None
        }
        KeyCode::PageUp | KeyCode::PageDown => None,
        KeyCode::Delete => {
            agent_lifecycle::exit_history_navigation(app);
            app.input.delete_forward();
            sync_prompt_accessory(app);
            None
        }
        KeyCode::Char(ch) => {
            agent_lifecycle::exit_history_navigation(app);
            app.input.insert_char(ch);
            sync_prompt_accessory(app);
            None
        }
        KeyCode::Backspace => {
            agent_lifecycle::exit_history_navigation(app);
            app.input.backspace();
            sync_prompt_accessory(app);
            None
        }
        KeyCode::Enter => handle_submit(app),
        KeyCode::Tab => accept_prompt_suggestion(app),
        KeyCode::Esc if app.run_state == RunState::Working => {
            agent_lifecycle::cancel_stream(app);
            None
        }
        _ => None,
    }
}

pub fn command_query(app: &App) -> String {
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

pub fn accept_command_suggestion(app: &mut App) -> Option<Msg> {
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

/// Accept the active prompt suggestion based on current accessory focus.
///
/// The prompt can surface command suggestions (`:` / slash-mode) and file
/// mention suggestions (`@path`). This helper keeps the selection model
/// centralized and safely no-ops when no suggestion is available.
pub fn accept_prompt_suggestion(app: &mut App) -> Option<Msg> {
    match app.prompt_accessory {
        PromptAccessory::Commands { selected: _ } => accept_command_suggestion(app),
        PromptAccessory::Files(_) => {
            accept_file_suggestion(app);
            None
        }
        PromptAccessory::None
        | PromptAccessory::Help
        | PromptAccessory::Models
        | PromptAccessory::ReasoningEffort
        | PromptAccessory::Skills
        | PromptAccessory::Context => {
            if app.mode == Mode::Command || app.input.as_str().starts_with('/') {
                accept_command_suggestion(app)
            } else {
                None
            }
        }
    }
}

pub fn open_file_picker(app: &mut App, source: FilePickerSource) {
    match tools::searchable_file_paths(&app.cwd, 2_000) {
        Ok(files) => {
            let items = files.into_iter().map(|path| PickerItem::new(path, "")).collect();
            app.picker = Some(PickerState::new(items, LARGE_PICKER_LIMIT));
            app.prompt_accessory = PromptAccessory::Files(source);
            sync_file_picker_query(app);
        }
        Err(err) => {
            app.transcript
                .push(Entry::Error { text: format!("file picker failed: {err}") });
        }
    }
}

pub fn open_model_picker(app: &mut App) {
    let items = if app.model_picker_items.is_empty() {
        offline_model_picker_items()
    } else {
        app.model_picker_items.clone()
    }
    .into_iter()
    .filter(|item| provider_authenticated(provider_for_model(&item.label), &app.cwd))
    .collect::<Vec<_>>();
    if items.is_empty() {
        app.transcript
            .push(Entry::Status { text: String::from("no authenticated providers; run /login <provider> or /setup") });
        return;
    }
    app.picker = Some(PickerState::new(items, MODEL_PICKER_LIMIT));
    app.prompt_accessory = PromptAccessory::Models;
}

pub fn open_reasoning_effort_picker(app: &mut App) {
    let options = crate::providers::reasoning_options(&app.model);
    let items = options
        .into_iter()
        .map(|effort| {
            PickerItem::new(
                effort.label(),
                format!("{} — {}", effort.display_label(), effort.description()),
            )
        })
        .collect();
    let mut picker = PickerState::new(items, MODEL_PICKER_LIMIT);
    picker.selected = crate::providers::reasoning_options(&app.model)
        .iter()
        .position(|effort| *effort == app.cli.reasoning_effort)
        .unwrap_or_default();
    app.picker = Some(picker);
    app.prompt_accessory = PromptAccessory::ReasoningEffort;
}

pub fn open_skill_picker(app: &mut App) {
    for diagnostic in &app.skill_diagnostics {
        app.transcript.push(Entry::Error { text: diagnostic.summary() });
    }

    if app.skills.is_empty() {
        app.transcript
            .push(Entry::Status { text: String::from("skills  none loaded") });
        return;
    }

    let items = app
        .skills
        .iter()
        .map(|skill| PickerItem::new(skill.name.clone(), skill.description.clone()))
        .collect();
    app.picker = Some(PickerState::new(items, LARGE_PICKER_LIMIT));
    app.prompt_accessory = PromptAccessory::Skills;
}

pub fn offline_model_picker_items() -> Vec<PickerItem> {
    umans::known_models()
        .into_iter()
        .map(|model| PickerItem::new(model.id, model.description))
        .chain(
            opencode::known_models()
                .into_iter()
                .map(|model| PickerItem::new(model.id, model.description)),
        )
        .chain(
            codex::known_models()
                .into_iter()
                .map(|model| PickerItem::new(model.id, model.description)),
        )
        .collect()
}

pub fn load_legacy_project_input_history(sessions_dir: &Path) -> Vec<String> {
    let mut newest_first = Vec::new();

    for path in session::list_session_files(sessions_dir)
        .into_iter()
        .take(PROJECT_INPUT_HISTORY_SESSION_LIMIT)
    {
        for record in session::SessionReader::read_records_from_tail(&path, PROJECT_INPUT_HISTORY_BYTES_PER_SESSION)
            .into_iter()
            .rev()
        {
            let session::SessionRecord::User { text, .. } = record else {
                continue;
            };
            let text = text.trim();
            if text.is_empty() || newest_first.last().is_some_and(|last| last == text) {
                continue;
            }
            newest_first.push(text.to_string());
            if newest_first.len() >= INPUT_HISTORY_LIMIT {
                newest_first.reverse();
                return newest_first;
            }
        }
    }

    newest_first.reverse();
    newest_first
}

pub fn close_prompt_accessory(app: &mut App) {
    if matches!(
        app.prompt_accessory,
        PromptAccessory::Files(_)
            | PromptAccessory::Models
            | PromptAccessory::ReasoningEffort
            | PromptAccessory::Skills
    ) {
        app.picker = None;
    }
    app.prompt_accessory = PromptAccessory::None;
}

/// Open the highest-priority detail surface target.
///
/// Priority:
/// 1. Failed tool output.
/// 2. Tool output that is likely truncated in the live transcript preview.
/// 3. Latest available tool output.
pub fn open_detail_surface(app: &mut App) {
    let Some(index) = next_detail_target(app) else {
        return;
    };
    app.detail_pane = DetailPane { entry_index: index, scroll: 0, open: true };
}

pub fn next_detail_target(app: &App) -> Option<usize> {
    const TOOL_PREVIEW_LINES: usize = 6;

    let mut fallback = None;
    let mut truncated = None;

    for (index, entry) in app.transcript.iter().enumerate().rev() {
        let Entry::Tool { status, output, .. } = entry else {
            continue;
        };

        fallback.get_or_insert(index);

        if matches!(status, ToolStatus::Failed) {
            return Some(index);
        }

        if output.len() > TOOL_PREVIEW_LINES && truncated.is_none() {
            truncated = Some(index);
        }
    }

    truncated.or(fallback)
}

/// Handle keys while the detail pane is open.
///
/// - `Tab`/`Esc` close the pane and return control to the prompt.
/// - `Up`/`PageUp` scroll up.
/// - `Down`/`PageDown` scroll down.
/// - All other keys are swallowed so the prompt is not mutated while the
///   detail pane has focus.
pub fn handle_detail_pane_key(app: &mut App, key: KeyEvent) -> Option<Msg> {
    let total = detail_pane_output_count(app);
    match key.code {
        KeyCode::Tab | KeyCode::Esc => {
            app.detail_pane.open = false;
            None
        }
        KeyCode::Up | KeyCode::PageUp => {
            app.detail_pane.scroll_up();
            None
        }
        KeyCode::Down | KeyCode::PageDown => {
            app.detail_pane.scroll_down(total);
            None
        }
        _ => None,
    }
}

/// Count output lines available for the detail pane's current target entry.
pub fn detail_pane_output_count(app: &App) -> usize {
    let Some(entry) = app.transcript.get(app.detail_pane.entry_index) else {
        return 0;
    };
    match entry {
        Entry::Tool { output, .. } => output.len(),
        _ => 0,
    }
}

/// Run fuzzy filter and split results into parallel item + index vectors.
pub fn split_filter_items(all_items: &[PickerItem], query: &str, limit: usize) -> (Vec<PickerItem>, Vec<Vec<usize>>) {
    if query.trim().is_empty() {
        return (
            all_items.iter().take(limit).cloned().collect(),
            all_items.iter().take(limit).map(|_| Vec::new()).collect(),
        );
    }

    let searchable_items: Vec<String> = all_items.iter().map(PickerItem::searchable).collect();
    let filtered = fuzzy::fuzzy_filter(&searchable_items, query, limit);
    filtered
        .into_iter()
        .filter_map(|(matched, indices)| {
            searchable_items
                .iter()
                .position(|candidate| candidate == &matched)
                .and_then(|idx| all_items.get(idx).cloned().map(|item| (item, indices)))
        })
        .collect::<Vec<_>>()
        .into_iter()
        .unzip()
}

pub fn insert_file_path(app: &mut App, path: &str) {
    if !app.input.is_empty() && !app.input.text_before_cursor().ends_with(char::is_whitespace) {
        app.input.insert_char(' ');
    }
    app.input.insert_str(path);
}

pub fn accept_file_suggestion(app: &mut App) {
    let Some(path) = app
        .picker
        .as_ref()
        .and_then(|picker| picker.selected().map(|item| item.label.clone()))
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

pub fn accept_model_suggestion(app: &mut App) {
    let Some(model) = app
        .picker
        .as_ref()
        .and_then(|picker| picker.selected().map(|item| item.label.clone()))
    else {
        return;
    };

    app.model = model.clone();
    app.cli.model = model.clone();
    match config::write_project_model(&app.cwd, &model) {
        Ok(path) => {
            let display = config::project_config_path_display(&path, &app.cwd);
            app.transcript
                .push(Entry::Status { text: format!("model: {model} (saved to {display})") });
        }
        Err(err) => {
            app.transcript.push(Entry::Status { text: format!("model: {model}") });
            app.transcript
                .push(Entry::Error { text: format!("failed to save selected model to project config: {err}") });
        }
    }
    if crate::providers::reasoning_options(&model).len() > 1 {
        open_reasoning_effort_picker(app);
    } else {
        close_prompt_accessory(app);
    }
}

pub fn accept_reasoning_effort_suggestion(app: &mut App) {
    let Some(effort) = app
        .picker
        .as_ref()
        .and_then(|picker| picker.selected())
        .and_then(|item| ReasoningEffort::parse(&item.label))
    else {
        return;
    };

    if !crate::providers::reasoning_option_is_supported(&app.model, effort) {
        app.transcript.push(Entry::Error {
            text: format!(
                "reasoning control `{}` is not supported by {}",
                effort.label(),
                app.model
            ),
        });
        return;
    }

    app.cli.reasoning_effort = effort;
    let pending_setup = app.pending_setup_reasoning_effort;
    match write_reasoning_effort_config(app, effort, pending_setup.map(|pending| pending.scope)) {
        Ok((_path, display)) => {
            app.transcript
                .push(Entry::Status { text: format!("reasoning effort: {} (saved to {display})", effort.label()) });
        }
        Err(err) => {
            app.transcript
                .push(Entry::Status { text: format!("reasoning effort: {}", effort.label()) });
            app.transcript
                .push(Entry::Error { text: format!("failed to save reasoning effort to project config: {err}") });
        }
    }
    finish_reasoning_effort_picker(app);
}

pub fn write_reasoning_effort_config(
    app: &App, effort: ReasoningEffort, scope: Option<CredentialScope>,
) -> std::io::Result<(PathBuf, String)> {
    let (path, display) = match scope {
        Some(CredentialScope::Global) => {
            let path = config::global_config_path()
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "HOME is not available"))?;
            let display = config::global_config_path_display(&path);
            (path, display)
        }
        Some(CredentialScope::Project) | None => {
            let path = config::project_config_path(&app.cwd);
            let display = config::project_config_path_display(&path, &app.cwd);
            (path, display)
        }
    };
    config::write_reasoning_effort_config(&path, effort)?;
    Ok((path, display))
}

pub fn finish_reasoning_effort_picker(app: &mut App) {
    close_prompt_accessory(app);
    if let Some(pending) = app.pending_setup_reasoning_effort.take() {
        advance_after_setup_model_config(app, pending.provider);
    }
}

pub fn accept_skill_suggestion(app: &mut App) {
    let Some(name) = app
        .picker
        .as_ref()
        .and_then(|picker| picker.selected().map(|item| item.label.clone()))
    else {
        return;
    };
    let Some(skill) = app.skills.iter().find(|skill| skill.name == name).cloned() else {
        close_prompt_accessory(app);
        return;
    };

    match skills::load_skill(&skill) {
        Ok(loaded) => {
            for diagnostic in &loaded.diagnostics {
                app.transcript.push(Entry::Error { text: diagnostic.summary() });
            }
            let text = format!(
                "# Skill: {}\n\n_Source: {}_\n\n{}",
                loaded.activation.name,
                loaded.activation.path.display(),
                loaded.markdown
            );
            app.transcript.push(Entry::Agent { text, streaming: false });
            if let Some(ref mut writer) = app.session_writer {
                let _ = writer.append_skill_activation(&loaded.activation);
            }
        }
        Err(diagnostic) => app.transcript.push(Entry::Error { text: diagnostic.summary() }),
    }
    close_prompt_accessory(app);
}

pub fn sync_prompt_accessory(app: &mut App) {
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

pub fn active_at_token(app: &App) -> Option<(usize, String)> {
    let before = app.input.text_before_cursor();
    let chars: Vec<char> = before.chars().collect();
    let token_start = chars.iter().rposition(|ch| ch.is_whitespace()).map_or(0, |idx| idx + 1);
    if chars.get(token_start) != Some(&'@') {
        return None;
    }
    let query: String = chars[token_start + 1..].iter().collect();
    Some((token_start, query))
}

pub fn sync_file_picker_query(app: &mut App) {
    let query = match app.prompt_accessory {
        PromptAccessory::Files(FilePickerSource::Mention { .. }) => active_at_token(app).map(|(_, query)| query),
        PromptAccessory::Files(FilePickerSource::Forced) => app.picker.as_ref().map(|picker| picker.query.clone()),
        _ => None,
    };
    let Some(query) = query else {
        return;
    };
    if let Some(picker) = app.picker.as_mut()
        && picker.query != query
    {
        picker.query = query;
        picker.refresh_matches();
    }
}

pub fn handle_submit(app: &mut App) -> Option<Msg> {
    if app.pending_permission.is_some() {
        return None;
    }

    if app.run_state == RunState::Working {
        let text = app.input.as_str().trim().to_string();
        if text.is_empty() {
            app.input.clear();
            return None;
        }
        if let Some(literal) = text.strip_prefix("//") {
            queue_running_input(app, &format!("/{literal}"));
            return None;
        }
        if let Some(command) = text.strip_prefix('/') {
            app.input.clear();
            return handle_running_command(app, command);
        }
        queue_running_input(app, &text);
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

pub fn queue_running_input(app: &mut App, text: &str) {
    app.input.clear();
    agent_lifecycle::remember_input(app, text);
    let (kind, count) = match app.queue_target {
        QueueTarget::Steering => {
            app.queued_steering.push(text.to_string());
            ("steering", app.queued_steering.len())
        }
        QueueTarget::FollowUp => {
            app.queued_followups.push(text.to_string());
            ("follow-up", app.queued_followups.len())
        }
    };
    let audit_error = app
        .session_writer
        .as_mut()
        .and_then(|writer| writer.append_queued(kind, text).err());
    app.transcript
        .push(Entry::Status { text: format!("queued {kind} ({count})") });
    if let Some(err) = audit_error {
        app.transcript
            .push(Entry::Error { text: format!("failed to record queued {kind} in session audit log: {err}") });
    }
}

pub fn submit_user_turn(app: &mut App, text: String) -> Option<Msg> {
    start_turn(app, text, true)
}

/// Start an internal provider turn without adding it to the user transcript,
/// input history, or durable user-turn records.
///
/// Compaction uses this for its configured-model summary request. It remains
/// an ordinary agent turn for lifecycle and cancellation purposes, but is not
/// text the user entered and should never be rendered as such.
pub(super) fn submit_internal_turn(app: &mut App, text: String) -> Option<Msg> {
    start_turn(app, text, false)
}

fn start_turn(app: &mut App, text: String, record_user_entry: bool) -> Option<Msg> {
    if app.pending_compaction_review.is_some() {
        app.transcript
            .push(Entry::Error { text: "review the pending compaction before submitting another turn".to_string() });
        app.input.set_text(&text);
        return None;
    }
    if let Some(recovery) = selected_provider_missing(app) {
        app.first_run_recovery = Some(recovery);
        return None;
    }

    let user_entry = record_user_entry.then(|| Entry::User { text: text.clone() });
    if let Some(entry) = user_entry.as_ref() {
        agent_lifecycle::remember_input(app, &text);
        app.transcript.push(entry.clone());
    }
    app.input.clear();
    app.history_cursor = None;
    app.history_draft.clear();
    app.last_input = Some(text);
    app.ttft.start_turn();
    app.turn_count += 1;
    let turn_id = format!("turn_{}", app.turn_count);
    agent_lifecycle::refresh_mcp_config_audit(app, &turn_id);
    if let Some(ref mut writer) = app.session_writer
        && let Some(entry) = user_entry.as_ref()
    {
        let _ = writer.append_entry(entry, &turn_id);
    }
    Some(Msg::Agent(AgentEvent::Started))
}
