//! Keyboard interaction, prompt accessories, and picker behavior.
//!
//! This module converts terminal key and mouse events into prompt edits,
//! accessory transitions, picker selections, and submitted [`Msg`] values. It
//! owns command mode, file/model/reasoning/skill pickers, detail-pane
//! navigation, input history, and queued input while an agent is running.
use super::*;
use crate::input::MouseInput;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Top-level interaction mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum Mode {
    /// Normal prompt entry.
    #[default]
    Prompt,
    /// Slash-command entry, entered with `:`.
    Command,
}

/// The semantic focus selected before keyboard translation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputFocus {
    Prompt,
    Command,
    Help,
    Context,
    Picker,
    Detail,
    TranscriptSearch,
    Queue,
    Setup,
    Permission,
}

/// A key binding that is independent of terminal event kind and state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KeyBinding {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

impl KeyBinding {
    pub const fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }
}

/// Small semantic actions understood by the application domains.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Action {
    InsertText(String),
    Backspace,
    DeleteForward,
    CursorLeft,
    CursorRight,
    CursorUp,
    CursorDown,
    CursorWordLeft,
    CursorWordRight,
    CursorStart,
    CursorEnd,
    KillToLineStart,
    KillToLineEnd,
    KillWordLeft,
    KillWordRight,
    Yank,
    Transpose,
    Newline,
    Submit,
    AcceptSuggestion,
    EnterCommand,
    OpenHelp,
    OpenDetail,
    OpenTranscriptSearch,
    OpenQueue,
    ToggleQueueTarget,
    QuitConfirm,
    Interrupt,
    Cancel,
    CloseOverlay,
    Confirm,
    SelectPrevious,
    SelectNext,
    PagePrevious,
    PageNext,
    ScrollOverlayUp,
    ScrollOverlayDown,
    ScrollTranscriptUp,
    ScrollTranscriptDown,
    ScrollTranscriptHalfUp,
    ScrollTranscriptHalfDown,
    TranscriptTop,
    TranscriptFollowTail,
    ExtendTranscriptSelectionUp,
    ExtendTranscriptSelectionDown,
    CopyTranscriptSelection,
    ClearTranscriptSelection,
    SearchNext,
    SearchPrevious,
    QueueEdit,
    QueueRetarget,
    QueueDelete,
    QueueReorderUp,
    QueueReorderDown,
    QueueSendNow,
    QueueSendAfterStep,
    Resize { width: u16, height: u16 },
    Suspend,
    FocusGained,
    FocusLost,
}

impl Action {
    fn description(&self) -> &'static str {
        match self {
            Self::OpenDetail => "open output, diff, warning, or error detail",
            Self::Submit => "submit prompt",
            Self::CloseOverlay | Self::Cancel => "close help, files, or commands",
            Self::CursorUp | Self::CursorDown => "move cursor or recall history",
            Self::OpenHelp => "show help",
            Self::OpenTranscriptSearch => "search transcript",
            Self::OpenQueue => "inspect queued input",
            Self::ToggleQueueTarget => "toggle queue target",
            Self::Transpose => "transpose characters",
            Self::Newline => "insert newline",
            Self::CursorStart | Self::CursorEnd => "move to start/end of line",
            Self::CursorLeft | Self::CursorRight => "move cursor left/right",
            Self::KillWordLeft => "delete previous word",
            Self::KillToLineEnd => "delete to end of line",
            Self::KillToLineStart => "delete to start of line",
            Self::Yank => "yank (paste) last kill",
            Self::CursorWordLeft | Self::CursorWordRight => "move word left/right",
            Self::KillWordRight => "delete next word",
            Self::Interrupt => "stop a running turn",
            Self::AcceptSuggestion => "accept a command or file suggestion",
            Self::QuitConfirm => "quit after double-press",
            _ => "",
        }
    }
}

/// User-overridable semantic keymap. Unbound keys use the built-in map.
#[derive(Clone, Debug, Default)]
pub struct Keymap {
    overrides: Vec<(InputFocus, KeyBinding, Action)>,
}

impl Keymap {
    /// Bind one key for one focused component.
    pub fn bind(&mut self, focus: InputFocus, binding: KeyBinding, action: Action) {
        if let Some(existing) = self
            .overrides
            .iter_mut()
            .find(|(current_focus, current, _)| *current_focus == focus && *current == binding)
        {
            existing.2 = action;
        } else {
            self.overrides.push((focus, binding, action));
        }
    }

    /// Return a configured binding, if one exists.
    pub fn binding(&self, focus: InputFocus, key: KeyEvent) -> Option<Action> {
        self.overrides
            .iter()
            .find(|(current_focus, binding, _)| {
                *current_focus == focus && binding.code == key.code && binding.modifiers == key.modifiers
            })
            .map(|(_, _, action)| action.clone())
    }

    /// Project help from the same semantic actions used for dispatch.
    pub fn help_bindings(&self, working: bool) -> Vec<KeyHelp> {
        let mut bindings = DEFAULT_HELP
            .iter()
            .map(|(key, description)| KeyHelp { key: (*key).to_string(), description: (*description).to_string() })
            .collect::<Vec<_>>();
        if working {
            if let Some(binding) = bindings.iter_mut().find(|binding| binding.key == "Ctrl+T") {
                binding.description = Action::ToggleQueueTarget.description().to_string();
            }
        }
        for (_, binding, action) in &self.overrides {
            let key = key_label(*binding);
            if !bindings.iter().any(|entry| entry.key == key) {
                let description = action.description();
                if !description.is_empty() {
                    bindings.push(KeyHelp { key, description: description.to_string() });
                }
            }
        }
        bindings
    }
}

fn key_label(binding: KeyBinding) -> String {
    let code = match binding.code {
        KeyCode::Char(ch) => ch.to_ascii_uppercase().to_string(),
        KeyCode::Enter => "Enter".to_string(),
        KeyCode::Esc => "Escape".to_string(),
        KeyCode::Backspace => "Backspace".to_string(),
        KeyCode::Tab => "Tab".to_string(),
        KeyCode::Left => "Left".to_string(),
        KeyCode::Right => "Right".to_string(),
        KeyCode::Up => "Up".to_string(),
        KeyCode::Down => "Down".to_string(),
        _ => format!("{:?}", binding.code),
    };
    let mut prefix = String::new();
    if binding.modifiers.contains(KeyModifiers::CONTROL) {
        prefix.push_str("Ctrl+");
    }
    if binding.modifiers.contains(KeyModifiers::ALT) {
        prefix.push_str("Alt+");
    }
    format!("{prefix}{code}")
}

/// One help row projected from the active keymap.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyHelp {
    pub key: String,
    pub description: String,
}

const DEFAULT_HELP: &[(&str, &str)] = &[
    ("── Navigation ──", ""),
    ("Ctrl+O", "open output, diff, warning, or error detail"),
    ("Enter", "accept highlighted item"),
    ("Escape", "close help, files, or commands"),
    ("Up/Down", "move cursor or recall history"),
    ("Ctrl+P", "open workspace file picker"),
    ("Ctrl+Shift+F", "search transcript"),
    ("Ctrl+Q", "inspect queued input"),
    ("Alt+Shift+Up/Down", "extend transcript selection"),
    ("Ctrl+Shift+C", "copy transcript selection"),
    ("Ctrl+Shift+X", "clear transcript selection"),
    ("Ctrl+T", "transpose characters"),
    ("── Editing ──", ""),
    ("Shift+Enter", "insert newline"),
    ("Ctrl+A/E", "move to start/end of line"),
    ("Ctrl+B/F", "move cursor left/right"),
    ("Ctrl+W", "delete previous word"),
    ("Ctrl+K", "delete to end of line"),
    ("Ctrl+U", "delete to start of line"),
    ("Ctrl+Y", "yank (paste) last kill"),
    ("Alt+B/F", "move word left/right"),
    ("Alt+D", "delete next word"),
    ("Alt+Bksp", "delete previous word"),
    ("── Files ──", ""),
    ("@path", "mention a file from fuzzy search"),
    ("── App ──", ""),
    ("Ctrl+C", "stop a running turn"),
    ("Tab", "accept a command or file suggestion"),
    ("Ctrl+D", "quit after double-press"),
];

/// Resolve the active application focus without exposing overlay internals.
pub fn input_focus(app: &App) -> InputFocus {
    if app.overlay.permission().is_some() {
        InputFocus::Permission
    } else if app.overlay.setup().is_some() {
        InputFocus::Setup
    } else if app.overlay.is_detail() {
        InputFocus::Detail
    } else if app.overlay.transcript_search().is_some() {
        InputFocus::TranscriptSearch
    } else if app.overlay.queue().is_some() {
        InputFocus::Queue
    } else if !matches!(app.overlay.accessory(), PromptAccessory::None) {
        match app.overlay.accessory() {
            PromptAccessory::Help => InputFocus::Help,
            PromptAccessory::Context => InputFocus::Context,
            PromptAccessory::Commands { .. }
            | PromptAccessory::Files(_)
            | PromptAccessory::Models
            | PromptAccessory::ReasoningEffort
            | PromptAccessory::Skills
            | PromptAccessory::Sessions => InputFocus::Picker,
            PromptAccessory::None => InputFocus::Prompt,
        }
    } else if app.composer.mode == Mode::Command {
        InputFocus::Command
    } else {
        InputFocus::Prompt
    }
}

/// Translate one normalized terminal input through focus, mode, and keymap.
pub fn translate_input(app: &App, input: TerminalInput) -> Vec<Action> {
    let keymap = app.runtime.keymap.clone();
    translate_input_with_keymap(app, input, &keymap)
}

/// Translation entry point used by tests and embedders with a custom keymap.
pub fn translate_input_with_keymap(app: &App, input: TerminalInput, keymap: &Keymap) -> Vec<Action> {
    match input {
        TerminalInput::Key(mut key) => {
            if key.kind == crossterm::event::KeyEventKind::Release {
                return Vec::new();
            }
            key.kind = crossterm::event::KeyEventKind::Press;
            let focus = input_focus(app);
            keymap
                .binding(focus, key)
                .or_else(|| default_key_action(app, focus, key))
                .into_iter()
                .collect()
        }
        TerminalInput::Paste(text) => vec![Action::InsertText(text)],
        TerminalInput::Mouse(mouse) => match (input_focus(app), mouse) {
            (InputFocus::Prompt | InputFocus::Command, MouseInput::ScrollUp) => {
                vec![Action::ScrollTranscriptUp]
            }
            (InputFocus::Prompt | InputFocus::Command, MouseInput::ScrollDown) => {
                vec![Action::ScrollTranscriptDown]
            }
            (_, MouseInput::ScrollUp) => vec![Action::ScrollOverlayUp],
            (_, MouseInput::ScrollDown) => vec![Action::ScrollOverlayDown],
            (_, MouseInput::Other) => Vec::new(),
        },
        TerminalInput::Resize { width, height } => vec![Action::Resize { width, height }],
        TerminalInput::FocusGained => vec![Action::FocusGained],
        TerminalInput::FocusLost => vec![Action::FocusLost],
    }
}

fn default_key_action(app: &App, focus: InputFocus, key: KeyEvent) -> Option<Action> {
    let modifiers = key.modifiers;
    let control = modifiers.contains(KeyModifiers::CONTROL);
    let alt = modifiers.contains(KeyModifiers::ALT);
    let shift = modifiers.contains(KeyModifiers::SHIFT);
    if matches!(focus, InputFocus::TranscriptSearch | InputFocus::Queue) {
        return focused_surface_key_action(focus, key);
    }
    if matches!(focus, InputFocus::Prompt) && matches!(app.overlay.accessory(), PromptAccessory::None) {
        match (key.code, control, alt, shift) {
            (KeyCode::Char('c'), true, false, true) => return Some(Action::CopyTranscriptSelection),
            (KeyCode::Char('x'), true, false, true) => return Some(Action::ClearTranscriptSelection),
            (KeyCode::Char('f'), true, false, true) => return Some(Action::OpenTranscriptSearch),
            (KeyCode::Up, false, true, true) => return Some(Action::ExtendTranscriptSelectionUp),
            (KeyCode::Down, false, true, true) => return Some(Action::ExtendTranscriptSelectionDown),
            _ => {}
        }
    }
    if control && !alt {
        let action = match key.code {
            KeyCode::Char('c') => Some(Action::Interrupt),
            KeyCode::Char('d') => Some(Action::QuitConfirm),
            KeyCode::Char('o') => Some(Action::OpenDetail),
            KeyCode::Char('q') => Some(Action::OpenQueue),
            KeyCode::Char('t') if app.runtime.run_state == RunState::Working => Some(Action::ToggleQueueTarget),
            KeyCode::Char('z') => Some(Action::Suspend),
            _ => None,
        };
        if action.is_some() {
            return action;
        }
    }

    if matches!(focus, InputFocus::Prompt) && matches!(app.overlay.accessory(), PromptAccessory::None) {
        match (key.code, modifiers) {
            (KeyCode::PageUp, modifiers) if modifiers.contains(KeyModifiers::CONTROL) => {
                return Some(Action::ScrollTranscriptHalfUp);
            }
            (KeyCode::PageDown, modifiers) if modifiers.contains(KeyModifiers::CONTROL) => {
                return Some(Action::ScrollTranscriptHalfDown);
            }
            (KeyCode::PageUp, _) => return Some(Action::ScrollTranscriptUp),
            (KeyCode::PageDown, _) => return Some(Action::ScrollTranscriptDown),
            (KeyCode::Up, modifiers) if modifiers.contains(KeyModifiers::ALT) => {
                return Some(Action::ScrollTranscriptUp);
            }
            (KeyCode::Down, modifiers) if modifiers.contains(KeyModifiers::ALT) => {
                return Some(Action::ScrollTranscriptDown);
            }
            (KeyCode::Home, modifiers) if modifiers.contains(KeyModifiers::CONTROL) => {
                return Some(Action::TranscriptTop);
            }
            (KeyCode::End, modifiers) if modifiers.contains(KeyModifiers::CONTROL) => {
                return Some(Action::TranscriptFollowTail);
            }
            _ => {}
        }
    }

    match focus {
        InputFocus::Permission => match key.code {
            KeyCode::Up => Some(Action::SelectPrevious),
            KeyCode::Down => Some(Action::SelectNext),
            KeyCode::Enter => Some(Action::Confirm),
            KeyCode::Esc => Some(Action::Cancel),
            _ => None,
        },
        InputFocus::Setup => match key.code {
            KeyCode::Esc => Some(Action::Cancel),
            KeyCode::Up => Some(Action::SelectPrevious),
            KeyCode::Down => Some(Action::SelectNext),
            KeyCode::Enter => Some(Action::Confirm),
            KeyCode::Backspace if !control && !alt => Some(Action::Backspace),
            KeyCode::Char(ch) if !control && !alt => Some(Action::InsertText(ch.to_string())),
            _ => None,
        },
        InputFocus::Detail => match key.code {
            KeyCode::Tab | KeyCode::Esc => Some(Action::CloseOverlay),
            KeyCode::Up | KeyCode::PageUp => Some(Action::ScrollOverlayUp),
            KeyCode::Down | KeyCode::PageDown => Some(Action::ScrollOverlayDown),
            _ => None,
        },
        InputFocus::TranscriptSearch | InputFocus::Queue => unreachable!("focused surfaces return above"),
        InputFocus::Help => (key.code == KeyCode::Esc).then_some(Action::CloseOverlay),
        InputFocus::Context => match app.composer.mode {
            Mode::Command => default_key_action(app, InputFocus::Command, key),
            Mode::Prompt => prompt_key_action(app, key),
        },
        InputFocus::Picker => match key.code {
            KeyCode::Esc => Some(Action::CloseOverlay),
            KeyCode::Enter => Some(Action::Confirm),
            KeyCode::Tab => Some(Action::AcceptSuggestion),
            KeyCode::Up => Some(Action::SelectPrevious),
            KeyCode::Down => Some(Action::SelectNext),
            KeyCode::PageUp => Some(Action::PagePrevious),
            KeyCode::PageDown => Some(Action::PageNext),
            KeyCode::Backspace if !control && !alt => Some(Action::Backspace),
            KeyCode::Char(ch) if !control && !alt => Some(Action::InsertText(ch.to_string())),
            _ => None,
        },
        InputFocus::Command => match key.code {
            KeyCode::Esc => Some(Action::Cancel),
            KeyCode::Backspace => Some(Action::Backspace),
            KeyCode::Tab => Some(Action::AcceptSuggestion),
            KeyCode::Enter => Some(Action::Submit),
            KeyCode::Char(ch) => Some(Action::InsertText(ch.to_string())),
            _ => None,
        },
        InputFocus::Prompt => prompt_key_action(app, key),
    }
}

fn focused_surface_key_action(focus: InputFocus, key: KeyEvent) -> Option<Action> {
    let control = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    match focus {
        InputFocus::TranscriptSearch => match key.code {
            KeyCode::Esc => Some(Action::CloseOverlay),
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => Some(Action::SearchPrevious),
            KeyCode::Enter | KeyCode::Down => Some(Action::SearchNext),
            KeyCode::Up => Some(Action::SearchPrevious),
            KeyCode::Backspace if !control && !alt => Some(Action::Backspace),
            KeyCode::Char(ch) if !control && !alt => Some(Action::InsertText(ch.to_string())),
            _ => None,
        },
        InputFocus::Queue => match key.code {
            KeyCode::Esc => Some(Action::CloseOverlay),
            KeyCode::Enter => Some(Action::Confirm),
            KeyCode::Up if control => Some(Action::QueueReorderUp),
            KeyCode::Down if control => Some(Action::QueueReorderDown),
            KeyCode::Up => Some(Action::SelectPrevious),
            KeyCode::Down => Some(Action::SelectNext),
            KeyCode::Backspace => Some(Action::Backspace),
            KeyCode::Char('e') if !control && !alt => Some(Action::QueueEdit),
            KeyCode::Char('t') if !control && !alt => Some(Action::QueueRetarget),
            KeyCode::Char('d') if !control && !alt => Some(Action::QueueDelete),
            KeyCode::Char('s') if !control && !alt => Some(Action::QueueSendNow),
            KeyCode::Char('a') if !control && !alt => Some(Action::QueueSendAfterStep),
            KeyCode::Char(ch) if !control && !alt => Some(Action::InsertText(ch.to_string())),
            _ => None,
        },
        _ => unreachable!("only focused surfaces are routed here"),
    }
}

fn prompt_key_action(app: &App, key: KeyEvent) -> Option<Action> {
    let modifiers = key.modifiers;
    let control = modifiers.contains(KeyModifiers::CONTROL);
    let alt = modifiers.contains(KeyModifiers::ALT);
    if alt {
        return match key.code {
            KeyCode::Left | KeyCode::Char('b') => Some(Action::CursorWordLeft),
            KeyCode::Right | KeyCode::Char('f') => Some(Action::CursorWordRight),
            KeyCode::Backspace => Some(Action::KillWordLeft),
            KeyCode::Char('d') => Some(Action::KillWordRight),
            _ => None,
        };
    }
    if control {
        return match key.code {
            KeyCode::Left => Some(Action::CursorWordLeft),
            KeyCode::Right => Some(Action::CursorWordRight),
            KeyCode::Char('a') => Some(Action::CursorStart),
            KeyCode::Char('e') => Some(Action::CursorEnd),
            KeyCode::Char('b') => Some(Action::CursorLeft),
            KeyCode::Char('f') => Some(Action::CursorRight),
            KeyCode::Char('j') => Some(Action::Newline),
            KeyCode::Char('k') => Some(Action::KillToLineEnd),
            KeyCode::Char('u') => Some(Action::KillToLineStart),
            KeyCode::Char('w') => Some(Action::KillWordLeft),
            KeyCode::Char('y') => Some(Action::Yank),
            KeyCode::Char('t') => Some(Action::Transpose),
            _ => None,
        };
    }
    match key.code {
        KeyCode::Char('?') if app.composer.input.is_empty() => Some(Action::OpenHelp),
        KeyCode::Char(':')
            if app.composer.input.is_empty()
                && matches!(app.runtime.run_state, RunState::Idle | RunState::Error(_)) =>
        {
            Some(Action::EnterCommand)
        }
        KeyCode::Enter if modifiers.contains(KeyModifiers::SHIFT) => Some(Action::Newline),
        KeyCode::Up => Some(Action::CursorUp),
        KeyCode::Down => Some(Action::CursorDown),
        KeyCode::Left => Some(Action::CursorLeft),
        KeyCode::Right => Some(Action::CursorRight),
        KeyCode::Home => Some(Action::CursorStart),
        KeyCode::End => Some(Action::CursorEnd),
        KeyCode::Delete => Some(Action::DeleteForward),
        KeyCode::Backspace => Some(Action::Backspace),
        KeyCode::Enter => Some(Action::Submit),
        KeyCode::Tab => Some(Action::AcceptSuggestion),
        KeyCode::Esc if app.runtime.run_state == RunState::Working => Some(Action::Cancel),
        KeyCode::Char(ch) => Some(Action::InsertText(ch.to_string())),
        _ => None,
    }
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
    Sessions,
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
    Followup(Box<Msg>),
}

impl KeyOutcome {
    fn with(followup: Option<Msg>) -> Self {
        match followup {
            Some(msg) => Self::Followup(Box::new(msg)),
            None => Self::Handled,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PickerItem {
    pub label: String,
    pub detail: String,
    value: String,
}

impl PickerItem {
    pub fn new(label: impl Into<String>, detail: impl Into<String>) -> Self {
        let label = label.into();
        Self { value: label.clone(), label, detail: detail.into() }
    }

    fn with_value(label: impl Into<String>, detail: impl Into<String>, value: impl Into<String>) -> Self {
        Self { label: label.into(), detail: detail.into(), value: value.into() }
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

/// Route one semantic action through the focused component.
pub fn handle_action(app: &mut App, action: Action) -> Option<Msg> {
    match action {
        Action::Interrupt => {
            if app.runtime.run_state == RunState::Working {
                agent_lifecycle::cancel_stream(app);
                None
            } else {
                app.runtime.quit = true;
                Some(Msg::Quit)
            }
        }
        Action::OpenDetail => {
            open_detail_surface(app);
            None
        }
        Action::OpenTranscriptSearch => {
            app.overlay.show_transcript_search();
            None
        }
        Action::OpenQueue => {
            app.overlay.show_queue();
            None
        }
        Action::QuitConfirm => {
            if let Some(deadline) = app.runtime.ctrl_d_pending
                && !agent_lifecycle::now_or_after_deadline(app.runtime.ui_tick, deadline)
            {
                app.runtime.ctrl_d_pending = None;
                app.runtime.quit = true;
                Some(Msg::Quit)
            } else {
                let deadline = app.runtime.ui_tick.wrapping_add(quit_confirm_timeout_ticks(app));
                app.runtime.ctrl_d_pending = Some(deadline);
                app.transcript
                    .entries
                    .push(Entry::Status { text: String::from("Press CTRL+D again to quit.") });
                None
            }
        }
        Action::ToggleQueueTarget if app.runtime.run_state == RunState::Working => {
            app.composer.queue_target = app.composer.queue_target.toggle();
            app.transcript
                .entries
                .push(Entry::Status { text: format!("queue target: {}", app.composer.queue_target.label()) });
            None
        }
        Action::Resize { .. }
        | Action::Suspend
        | Action::FocusGained
        | Action::FocusLost
        | Action::ScrollTranscriptUp
        | Action::ScrollTranscriptDown
        | Action::ScrollTranscriptHalfUp
        | Action::ScrollTranscriptHalfDown
        | Action::TranscriptTop
        | Action::TranscriptFollowTail
        | Action::ExtendTranscriptSelectionUp
        | Action::ExtendTranscriptSelectionDown
        | Action::CopyTranscriptSelection
        | Action::ClearTranscriptSelection => None,
        action => {
            app.runtime.ctrl_d_pending = None;
            if app.overlay.permission().is_some() {
                return agent_lifecycle::handle_permission_action(app, &action);
            }
            if app.overlay.setup().is_some() {
                return handle_first_run_action(app, action);
            }
            if app.overlay.is_detail() {
                return handle_detail_action(app, &action);
            }
            if app.overlay.transcript_search().is_some() {
                handle_transcript_search_action(app, action);
                return None;
            }
            if app.overlay.queue().is_some() {
                return handle_queue_action(app, action);
            }
            if !matches!(app.overlay.accessory(), PromptAccessory::None) {
                match handle_accessory_action(app, action) {
                    KeyOutcome::Followup(msg) => return Some(*msg),
                    KeyOutcome::Handled | KeyOutcome::Unhandled => return None,
                }
            }
            match app.composer.mode {
                Mode::Command => handle_command_action(app, action),
                Mode::Prompt => handle_prompt_action(app, action),
            }
        }
    }
}

fn handle_accessory_action(app: &mut App, action: Action) -> KeyOutcome {
    match app.overlay.accessory() {
        PromptAccessory::Help => match action {
            Action::CloseOverlay | Action::Cancel => {
                close_prompt_accessory(app);
                KeyOutcome::Handled
            }
            _ => KeyOutcome::Unhandled,
        },
        PromptAccessory::Context => match action {
            Action::CloseOverlay | Action::Cancel => {
                close_prompt_accessory(app);
                KeyOutcome::Handled
            }
            action => KeyOutcome::with(match app.composer.mode {
                Mode::Command => handle_command_action(app, action),
                Mode::Prompt => handle_prompt_action(app, action),
            }),
        },
        PromptAccessory::Commands { .. } => {
            let count = command_suggestions_for_app(app).len();
            match action {
                Action::CloseOverlay | Action::Cancel => {
                    close_prompt_accessory(app);
                    KeyOutcome::Handled
                }
                Action::SelectPrevious => {
                    if let Some(selected) = app.overlay.command_selected_mut() {
                        *selected = selected.saturating_sub(1);
                    }
                    KeyOutcome::Handled
                }
                Action::SelectNext => {
                    if let Some(selected) = app.overlay.command_selected_mut() {
                        *selected = (*selected + 1).min(count.saturating_sub(1));
                    }
                    KeyOutcome::Handled
                }
                Action::Confirm
                    if count > 0
                        && !command_suggestions_for_app(app)
                            .iter()
                            .any(|suggestion| suggestion.name == command_query(app)) =>
                {
                    KeyOutcome::with(accept_command_suggestion(app))
                }
                Action::AcceptSuggestion => KeyOutcome::with(accept_prompt_suggestion(app)),
                Action::Confirm => {
                    let result = match app.composer.mode {
                        Mode::Command => handle_command_action(app, Action::Submit),
                        Mode::Prompt => handle_prompt_action(app, Action::Submit),
                    };
                    match result {
                        Some(msg) => KeyOutcome::Followup(Box::new(msg)),
                        None => KeyOutcome::Handled,
                    }
                }
                Action::Backspace | Action::InsertText(_) => KeyOutcome::with(match action {
                    Action::Backspace => handle_command_action(app, Action::Backspace),
                    Action::InsertText(text) => handle_command_action(app, Action::InsertText(text)),
                    _ => None,
                }),
                _ => KeyOutcome::Unhandled,
            }
        }
        PromptAccessory::Files(source) => handle_picker_action(app, source, action, PickerActionKind::Files),
        PromptAccessory::Models => {
            handle_picker_action(app, FilePickerSource::Forced, action, PickerActionKind::Models)
        }
        PromptAccessory::ReasoningEffort => {
            handle_picker_action(app, FilePickerSource::Forced, action, PickerActionKind::Reasoning)
        }
        PromptAccessory::Skills => {
            handle_picker_action(app, FilePickerSource::Forced, action, PickerActionKind::Skills)
        }
        PromptAccessory::Sessions => {
            handle_picker_action(app, FilePickerSource::Forced, action, PickerActionKind::Sessions)
        }
        PromptAccessory::None => KeyOutcome::Unhandled,
    }
}

#[derive(Clone, Copy)]
enum PickerActionKind {
    Files,
    Models,
    Reasoning,
    Skills,
    Sessions,
}

fn handle_picker_action(app: &mut App, source: FilePickerSource, action: Action, kind: PickerActionKind) -> KeyOutcome {
    if matches!(source, FilePickerSource::Mention { .. }) {
        return match action {
            Action::Backspace => KeyOutcome::with(handle_prompt_action(app, Action::Backspace)),
            Action::InsertText(text) => KeyOutcome::with(handle_prompt_action(app, Action::InsertText(text))),
            _ => handle_picker_action(app, FilePickerSource::Forced, action, kind),
        };
    }

    match action {
        Action::CloseOverlay | Action::Cancel => {
            if matches!(kind, PickerActionKind::Reasoning) {
                finish_reasoning_effort_picker(app);
            } else {
                close_prompt_accessory(app);
            }
            KeyOutcome::Handled
        }
        Action::Confirm => {
            match kind {
                PickerActionKind::Files => accept_file_suggestion(app),
                PickerActionKind::Models => accept_model_suggestion(app),
                PickerActionKind::Reasoning => accept_reasoning_effort_suggestion(app),
                PickerActionKind::Skills => accept_skill_suggestion(app),
                PickerActionKind::Sessions => accept_session_suggestion(app),
            }
            KeyOutcome::Handled
        }
        Action::AcceptSuggestion => {
            if matches!(kind, PickerActionKind::Files) {
                accept_file_suggestion(app);
            }
            KeyOutcome::Handled
        }
        Action::SelectPrevious => {
            if let Some(picker) = app.overlay.picker_mut() {
                picker.move_up();
            }
            KeyOutcome::Handled
        }
        Action::SelectNext => {
            if let Some(picker) = app.overlay.picker_mut() {
                picker.move_down();
            }
            KeyOutcome::Handled
        }
        Action::PagePrevious => {
            if let Some(picker) = app.overlay.picker_mut() {
                picker.page_up();
            }
            KeyOutcome::Handled
        }
        Action::PageNext => {
            if let Some(picker) = app.overlay.picker_mut() {
                picker.page_down();
            }
            KeyOutcome::Handled
        }
        Action::Backspace => {
            if let Some(picker) = app.overlay.picker_mut() {
                picker.query.pop();
                picker.refresh_matches();
            }
            KeyOutcome::Handled
        }
        Action::InsertText(text) => {
            if let Some(picker) = app.overlay.picker_mut() {
                picker.query.push_str(&text);
                picker.refresh_matches();
            }
            KeyOutcome::Handled
        }
        _ => KeyOutcome::Unhandled,
    }
}

fn handle_detail_action(app: &mut App, action: &Action) -> Option<Msg> {
    let total = detail_pane_output_count(app);
    match action {
        Action::CloseOverlay | Action::Cancel => app.overlay.close(),
        Action::ScrollOverlayUp => {
            if let Some(detail) = app.overlay.detail_mut() {
                detail.scroll_up();
            }
        }
        Action::ScrollOverlayDown => {
            if let Some(detail) = app.overlay.detail_mut() {
                detail.scroll_down(total);
            }
        }
        _ => {}
    }
    None
}

fn handle_transcript_search_action(app: &mut App, action: Action) {
    match action {
        Action::CloseOverlay | Action::Cancel => app.overlay.close(),
        Action::Backspace => {
            if let Some(search) = app.overlay.transcript_search_mut() {
                search.query.backspace();
                search.refresh(&app.transcript.entries);
            }
        }
        Action::InsertText(text) => {
            if let Some(search) = app.overlay.transcript_search_mut() {
                search.query.insert_str(&text);
                search.refresh(&app.transcript.entries);
            }
        }
        Action::SearchNext => {
            if let Some(search) = app.overlay.transcript_search_mut() {
                search.next();
            }
        }
        Action::SearchPrevious => {
            if let Some(search) = app.overlay.transcript_search_mut() {
                search.previous();
            }
        }
        _ => {}
    }
}

fn handle_queue_action(app: &mut App, action: Action) -> Option<Msg> {
    let count = app.composer.queue.items.len();
    if matches!(action, Action::CloseOverlay | Action::Cancel) {
        if app
            .overlay
            .queue_mut()
            .is_some_and(|pane| pane.editing.take().is_some())
        {
            return None;
        }
        app.overlay.close();
        return None;
    }
    if let Some(pane) = app.overlay.queue_mut() {
        match action {
            Action::SelectPrevious if pane.editing.is_none() => pane.selected = pane.selected.saturating_sub(1),
            Action::SelectNext if pane.editing.is_none() => {
                pane.selected = (pane.selected + 1).min(count.saturating_sub(1));
            }
            Action::Backspace if pane.editing.is_some() => {
                pane.editing.as_mut().expect("editing checked above").backspace();
                return None;
            }
            Action::InsertText(text) if pane.editing.is_some() => {
                pane.editing.as_mut().expect("editing checked above").insert_str(&text);
                return None;
            }
            _ => {}
        }
    }

    let selected = app.overlay.queue().map(|pane| pane.selected).unwrap_or_default();
    let id = app.composer.queue.items.get(selected).map(|item| item.id)?;
    let pending = app
        .composer
        .queue
        .item(id)
        .is_some_and(|item| item.settlement == QueueSettlement::Pending);
    match action {
        Action::QueueEdit if pending => {
            let text = app
                .composer
                .queue
                .item(id)
                .map(|item| item.text.clone())
                .unwrap_or_default();
            if let Some(pane) = app.overlay.queue_mut() {
                pane.editing = Some(PromptInput::from(text));
            }
        }
        Action::Confirm => {
            let edited = app.overlay.queue_mut().and_then(|pane| pane.editing.take());
            if let Some(edited) = edited
                && let Some(item) = app.composer.queue.item_mut(id)
            {
                item.text = edited.text();
                audit_queue_transition(app, id, "edit");
            }
        }
        Action::QueueRetarget if pending => {
            if let Some(item) = app.composer.queue.item_mut(id) {
                item.target = item.target.toggle();
            }
            audit_queue_transition(app, id, "retarget");
        }
        Action::QueueDelete if pending => {
            app.composer.queue.settle(id, QueueSettlement::Deleted);
            audit_queue_transition(app, id, "deleted");
        }
        Action::QueueReorderUp if pending && selected > 0 => {
            app.composer.queue.items.swap(selected, selected - 1);
            if let Some(pane) = app.overlay.queue_mut() {
                pane.selected -= 1;
            }
            audit_queue_transition(app, id, "reorder-up");
        }
        Action::QueueReorderDown if pending && selected + 1 < count => {
            app.composer.queue.items.swap(selected, selected + 1);
            if let Some(pane) = app.overlay.queue_mut() {
                pane.selected += 1;
            }
            audit_queue_transition(app, id, "reorder-down");
        }
        Action::QueueSendAfterStep if pending => {
            if let Some(item) = app.composer.queue.item_mut(id) {
                item.target = QueueTarget::FollowUp;
            }
            audit_queue_transition(app, id, "send-after-step");
        }
        Action::QueueSendNow if pending => {
            let text = app
                .composer
                .queue
                .item(id)
                .map(|item| item.text.clone())
                .unwrap_or_default();
            app.overlay.close();
            if app.runtime.run_state == RunState::Working {
                if let Some(item) = app.composer.queue.item_mut(id) {
                    item.target = QueueTarget::Steering;
                }
                audit_queue_transition(app, id, "send-now");
                return None;
            } else {
                app.composer.queue.settle(id, QueueSettlement::Sent);
                audit_queue_transition(app, id, "sent");
                let draft = app.composer.input.clone();
                let followup = submit_user_turn(app, text);
                app.composer.input = draft;
                return followup;
            }
        }
        _ => {}
    }
    None
}

pub(crate) fn audit_queue_transition(app: &mut App, id: QueueItemId, action: &str) {
    let Some(item) = app.composer.queue.item(id) else {
        return;
    };
    let kind = item.target.label().to_string();
    let text = item.text.clone();
    let result = app
        .session
        .writer
        .as_mut()
        .and_then(|writer| writer.append_queued(id.0, &kind, action, &text).err());
    if let Some(err) = result {
        if let Some(item) = app.composer.queue.item_mut(id) {
            item.audit = QueueAuditState::Failed(err.to_string());
        }
        app.transcript
            .entries
            .push(Entry::Error { text: format!("queue audit failed for {id}; queued content was retained") });
    }
}

fn handle_command_action(app: &mut App, action: Action) -> Option<Msg> {
    match action {
        Action::Cancel => {
            app.composer.mode = Mode::Prompt;
            app.composer.input.clear();
            close_prompt_accessory(app);
        }
        Action::Backspace => {
            if app.composer.input.is_empty() {
                app.composer.mode = Mode::Prompt;
                close_prompt_accessory(app);
            } else {
                app.composer.input.backspace();
                sync_prompt_accessory(app);
            }
        }
        Action::AcceptSuggestion => {
            accept_prompt_suggestion(app);
        }
        Action::Submit => {
            let text = app.composer.input.as_str().trim().to_string();
            app.composer.input.clear();
            app.composer.mode = Mode::Prompt;
            close_prompt_accessory(app);
            if !text.is_empty() {
                return handle_command(app, &text);
            }
        }
        Action::InsertText(text) => {
            app.composer.input.insert_str(&text);
            sync_prompt_accessory(app);
        }
        _ => {}
    }
    None
}

fn handle_prompt_action(app: &mut App, action: Action) -> Option<Msg> {
    match action {
        Action::CursorWordLeft => app.composer.input.cursor_word_left(),
        Action::CursorWordRight => app.composer.input.cursor_word_right(),
        Action::KillWordLeft => {
            let killed = app.composer.input.kill_word_left();
            if !killed.is_empty() {
                app.composer.kill_ring.push(killed);
            }
        }
        Action::KillWordRight => {
            let killed = app.composer.input.kill_word_right();
            if !killed.is_empty() {
                app.composer.kill_ring.push(killed);
            }
        }
        Action::CursorStart => app.composer.input.cursor_to_start(),
        Action::CursorEnd => app.composer.input.cursor_to_end(),
        Action::CursorLeft => app.composer.input.cursor_left(),
        Action::CursorRight => app.composer.input.cursor_right(),
        Action::Newline => app.composer.input.insert_char('\n'),
        Action::KillToLineEnd => {
            let killed = app.composer.input.kill_to_end_of_line();
            if !killed.is_empty() {
                app.composer.kill_ring.push(killed);
            }
        }
        Action::KillToLineStart => {
            let killed = app.composer.input.kill_to_start_of_line();
            if !killed.is_empty() {
                app.composer.kill_ring.push(killed);
            }
        }
        Action::Yank => {
            if let Some(killed) = app.composer.kill_ring.last() {
                app.composer.input.yank(killed);
            }
        }
        Action::Transpose => {
            app.composer.input.transpose_chars();
        }
        Action::CursorUp => {
            if !app.composer.input.cursor_up() {
                agent_lifecycle::recall_older_input(app);
            }
        }
        Action::CursorDown => {
            if !app.composer.input.cursor_down() {
                agent_lifecycle::recall_newer_input(app);
            }
        }
        Action::DeleteForward => {
            app.composer.input.delete_forward();
        }
        Action::Backspace => {
            app.composer.input.backspace();
        }
        Action::InsertText(text) => app.composer.input.insert_str(&text),
        Action::OpenHelp => app.overlay.show_help(),
        Action::EnterCommand => {
            app.composer.mode = Mode::Command;
            app.overlay.show_commands();
        }
        Action::Submit => return handle_submit(app),
        Action::AcceptSuggestion => return accept_prompt_suggestion(app),
        Action::Cancel => {
            if app.runtime.run_state == RunState::Working {
                agent_lifecycle::cancel_stream(app);
            }
        }
        _ => {}
    }
    agent_lifecycle::exit_history_navigation(app);
    sync_prompt_accessory(app);
    None
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
pub fn command_query(app: &App) -> String {
    if app.composer.mode == Mode::Command {
        app.composer.input.as_str().trim_start().to_string()
    } else {
        app.composer
            .input
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
    let selected = match app.overlay.accessory() {
        PromptAccessory::Commands { selected } => selected.min(suggestions.len() - 1),
        _ => 0,
    };
    let command = &suggestions[selected].name;
    let replacement = if app.composer.mode == Mode::Command { format!("{command} ") } else { format!("/{command} ") };
    app.composer.input.set_text(&replacement);
    app.overlay.close();
    None
}

/// Accept the active prompt suggestion based on current accessory focus.
///
/// The prompt can surface command suggestions (`:` / slash-mode) and file
/// mention suggestions (`@path`). This helper keeps the selection model
/// centralized and safely no-ops when no suggestion is available.
pub fn accept_prompt_suggestion(app: &mut App) -> Option<Msg> {
    match app.overlay.accessory() {
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
        | PromptAccessory::Sessions
        | PromptAccessory::Context => {
            if app.composer.mode == Mode::Command || app.composer.input.as_str().starts_with('/') {
                accept_command_suggestion(app)
            } else {
                None
            }
        }
    }
}

pub fn open_file_picker(app: &mut App, source: FilePickerSource) {
    match tools::searchable_file_paths(&app.runtime.cwd, 2_000) {
        Ok(files) => {
            let items = files.into_iter().map(|path| PickerItem::new(path, "")).collect();
            let _ = app.overlay.show_picker(
                PromptAccessory::Files(source),
                PickerState::new(items, LARGE_PICKER_LIMIT),
            );
            sync_file_picker_query(app);
        }
        Err(err) => {
            app.transcript
                .entries
                .push(Entry::Error { text: format!("file picker failed: {err}") });
        }
    }
}

pub fn open_model_picker(app: &mut App) {
    let items = if app.runtime.model_picker_items.is_empty() {
        offline_model_picker_items()
    } else {
        app.runtime.model_picker_items.clone()
    }
    .into_iter()
    .filter(|item| provider_authenticated(provider_for_model(&item.label), &app.runtime.cwd))
    .collect::<Vec<_>>();
    if items.is_empty() {
        app.transcript
            .entries
            .push(Entry::Status { text: String::from("no authenticated providers; run /login <provider> or /setup") });
        return;
    }
    let _ = app
        .overlay
        .show_picker(PromptAccessory::Models, PickerState::new(items, MODEL_PICKER_LIMIT));
}

pub fn open_reasoning_effort_picker(app: &mut App) {
    let options = crate::providers::reasoning_options(&app.runtime.model);
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
    picker.selected = crate::providers::reasoning_options(&app.runtime.model)
        .iter()
        .position(|effort| *effort == app.runtime.cli.reasoning_effort)
        .unwrap_or_default();
    let _ = app.overlay.show_picker(PromptAccessory::ReasoningEffort, picker);
}

pub fn open_skill_picker(app: &mut App) {
    for diagnostic in &app.transcript.skill_diagnostics {
        app.transcript.entries.push(Entry::Error { text: diagnostic.summary() });
    }

    if app.transcript.skills.is_empty() {
        app.transcript
            .entries
            .push(Entry::Status { text: String::from("skills  none loaded") });
        return;
    }

    let items = app
        .transcript
        .skills
        .iter()
        .map(|skill| PickerItem::new(skill.name.clone(), skill.description.clone()))
        .collect();
    let _ = app
        .overlay
        .show_picker(PromptAccessory::Skills, PickerState::new(items, LARGE_PICKER_LIMIT));
}

pub fn open_session_picker(app: &mut App) {
    if app.is_ephemeral() {
        app.transcript
            .entries
            .push(Entry::Error { text: String::from("cannot resume a session in ephemeral mode") });
        return;
    }

    let items = session::list_session_files(&app.session_directory())
        .into_iter()
        .filter_map(|path| {
            let id = path.file_stem()?.to_str()?.to_string();
            if id == app.session.id {
                return None;
            }
            let summary = session::SessionReader::read_summary(&path);
            Some(PickerItem::with_value(
                summary.title,
                format!(
                    "{id} · {} · {} in / {} out",
                    summary.model, summary.input_tokens, summary.output_tokens
                ),
                id,
            ))
        })
        .collect::<Vec<_>>();
    if items.is_empty() {
        app.transcript
            .entries
            .push(Entry::Status { text: String::from("no other sessions found") });
        return;
    }
    let _ = app
        .overlay
        .show_picker(PromptAccessory::Sessions, PickerState::new(items, LARGE_PICKER_LIMIT));
}

fn accept_session_suggestion(app: &mut App) {
    let session_id = app
        .overlay
        .picker()
        .and_then(PickerState::selected)
        .map(|item| item.value.clone());
    close_prompt_accessory(app);
    if let Some(session_id) = session_id
        && let Err(error) = app.resume_session(&session_id)
    {
        app.transcript.entries.push(Entry::Error { text: error.to_string() });
    }
}

pub fn offline_model_picker_items() -> Vec<PickerItem> {
    opencode::known_models()
        .into_iter()
        .map(|model| PickerItem::new(model.id, model.description))
        .chain(
            codex::known_models()
                .into_iter()
                .map(|model| PickerItem::new(model.id, model.description)),
        )
        .collect()
}

pub fn close_prompt_accessory(app: &mut App) {
    app.overlay.close();
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
    app.overlay.show_detail(index);
}

pub fn next_detail_target(app: &App) -> Option<usize> {
    const TOOL_PREVIEW_LINES: usize = 6;

    let mut fallback = None;
    let mut truncated = None;

    for (index, entry) in app.transcript.entries.iter().enumerate().rev() {
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

/// Count output lines available for the detail pane's current target entry.
pub fn detail_pane_output_count(app: &App) -> usize {
    let Some(detail) = app.overlay.detail() else {
        return 0;
    };
    let Some(entry) = app.transcript.entries.get(detail.entry_index) else {
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
    if !app.composer.input.is_empty() && !app.composer.input.text_before_cursor().ends_with(char::is_whitespace) {
        app.composer.input.insert_char(' ');
    }
    app.composer.input.insert_str(path);
}

pub fn accept_file_suggestion(app: &mut App) {
    let Some(path) = app
        .overlay
        .picker()
        .as_ref()
        .and_then(|picker| picker.selected().map(|item| item.label.clone()))
    else {
        return;
    };

    match app.overlay.accessory() {
        PromptAccessory::Files(FilePickerSource::Mention { token_start }) => {
            let end = app.composer.input.cursor();
            let replacement = format!("@{path} ");
            app.composer.input.replace_range(token_start, end, &replacement);
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
        .overlay
        .picker()
        .as_ref()
        .and_then(|picker| picker.selected().map(|item| item.label.clone()))
    else {
        return;
    };

    app.runtime.model = model.clone();
    app.runtime.cli.model = model.clone();
    app.composer.input.clear();
    app.runtime.codex_usage = None;
    match config::write_project_model(&app.runtime.cwd, &model) {
        Ok(path) => {
            let display = config::project_config_path_display(&path, &app.runtime.cwd);
            app.transcript
                .entries
                .push(Entry::Status { text: format!("model: {model} (saved to {display})") });
        }
        Err(err) => {
            app.transcript
                .entries
                .push(Entry::Status { text: format!("model: {model}") });
            app.transcript
                .entries
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
        .overlay
        .picker()
        .as_ref()
        .and_then(|picker| picker.selected())
        .and_then(|item| ReasoningEffort::parse(&item.label))
    else {
        return;
    };

    if !crate::providers::reasoning_option_is_supported(&app.runtime.model, effort) {
        app.transcript.entries.push(Entry::Error {
            text: format!(
                "reasoning control `{}` is not supported by {}",
                effort.label(),
                app.runtime.model
            ),
        });
        return;
    }

    app.runtime.cli.reasoning_effort = effort;
    let pending_setup = app.overlay.pending_setup_reasoning_effort();
    match write_reasoning_effort_config(app, effort, pending_setup.map(|pending| pending.scope)) {
        Ok((_path, display)) => {
            app.transcript
                .entries
                .push(Entry::Status { text: format!("reasoning effort: {} (saved to {display})", effort.label()) });
        }
        Err(err) => {
            app.transcript
                .entries
                .push(Entry::Status { text: format!("reasoning effort: {}", effort.label()) });
            app.transcript
                .entries
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
            let path = config::project_config_path(&app.runtime.cwd);
            let display = config::project_config_path_display(&path, &app.runtime.cwd);
            (path, display)
        }
    };
    config::write_reasoning_effort_config(&path, effort)?;
    Ok((path, display))
}

pub fn finish_reasoning_effort_picker(app: &mut App) {
    let pending_setup = app.overlay.take_pending_setup_reasoning_effort();
    close_prompt_accessory(app);
    if let Some(pending) = pending_setup {
        advance_after_setup_model_config(app, pending.provider);
    }
}

pub fn accept_skill_suggestion(app: &mut App) {
    let Some(name) = app
        .overlay
        .picker()
        .as_ref()
        .and_then(|picker| picker.selected().map(|item| item.label.clone()))
    else {
        return;
    };
    let Some(skill) = app.transcript.skills.iter().find(|skill| skill.name == name).cloned() else {
        close_prompt_accessory(app);
        return;
    };

    match skills::load_skill(&skill) {
        Ok(loaded) => {
            for diagnostic in &loaded.diagnostics {
                app.transcript.entries.push(Entry::Error { text: diagnostic.summary() });
            }
            let text = format!(
                "# Skill: {}\n\n_Source: {}_\n\n{}",
                loaded.activation.name,
                loaded.activation.path.display(),
                loaded.markdown
            );
            app.transcript.entries.push(Entry::Agent { text, streaming: false });
            if let Some(ref mut writer) = app.session.writer {
                let _ = writer.append_skill_activation(&loaded.activation);
            }
        }
        Err(diagnostic) => app.transcript.entries.push(Entry::Error { text: diagnostic.summary() }),
    }
    close_prompt_accessory(app);
}

pub fn sync_prompt_accessory(app: &mut App) {
    if app.composer.mode == Mode::Command {
        app.overlay.show_commands();
        return;
    }

    if app.composer.input.as_str().starts_with('/') {
        app.overlay.show_commands();
        return;
    }

    if let Some((token_start, _query)) = active_at_token(app) {
        if !matches!(app.overlay.accessory(), PromptAccessory::Files(FilePickerSource::Mention { token_start: existing }) if existing == token_start)
        {
            open_file_picker(app, FilePickerSource::Mention { token_start });
        } else {
            sync_file_picker_query(app);
        }
        return;
    }

    if !matches!(app.overlay.accessory(), PromptAccessory::Help) {
        close_prompt_accessory(app);
    }
}

pub fn active_at_token(app: &App) -> Option<(usize, String)> {
    let before = app.composer.input.text_before_cursor();
    let chars: Vec<char> = before.chars().collect();
    let token_start = chars.iter().rposition(|ch| ch.is_whitespace()).map_or(0, |idx| idx + 1);
    if chars.get(token_start) != Some(&'@') {
        return None;
    }
    let query: String = chars[token_start + 1..].iter().collect();
    Some((token_start, query))
}

pub fn sync_file_picker_query(app: &mut App) {
    let query = match app.overlay.accessory() {
        PromptAccessory::Files(FilePickerSource::Mention { .. }) => active_at_token(app).map(|(_, query)| query),
        PromptAccessory::Files(FilePickerSource::Forced) => app.overlay.picker().map(|picker| picker.query.clone()),
        _ => None,
    };
    let Some(query) = query else {
        return;
    };
    if let Some(picker) = app.overlay.picker_mut()
        && picker.query != query
    {
        picker.query = query;
        picker.refresh_matches();
    }
}

pub fn handle_submit(app: &mut App) -> Option<Msg> {
    if app.overlay.permission().is_some() {
        return None;
    }

    if app.runtime.run_state == RunState::Working {
        let text = app.composer.input.as_str().trim().to_string();
        if text.is_empty() {
            app.composer.input.clear();
            return None;
        }
        if let Some(literal) = text.strip_prefix("//") {
            queue_running_input(app, &format!("/{literal}"));
            return None;
        }
        if let Some(command) = text.strip_prefix('/') {
            app.composer.input.clear();
            return handle_running_command(app, command);
        }
        queue_running_input(app, &text);
        return None;
    }

    if !matches!(app.runtime.run_state, RunState::Idle | RunState::Error(_)) {
        return None;
    }

    let text = app.composer.input.as_str().trim().to_string();
    if text.is_empty() {
        app.composer.input.clear();
        return None;
    }

    if let Some(command) = text.strip_prefix('/') {
        return handle_command(app, command);
    }

    submit_user_turn(app, text)
}

pub fn queue_running_input(app: &mut App, text: &str) {
    app.composer.input.clear();
    agent_lifecycle::remember_input(app, text);
    let target = app.composer.queue_target;
    let kind = target.label();
    let id = app
        .composer
        .queue
        .push(target, text.to_string(), crate::datetime::now_iso8601());
    let count = app.composer.queue.pending_count(target);
    let audit_error = app
        .session
        .writer
        .as_mut()
        .and_then(|writer| writer.append_queued(id.0, kind, "add", text).err());
    if let Some(err) = audit_error.as_ref()
        && let Some(item) = app.composer.queue.item_mut(id)
    {
        item.audit = QueueAuditState::Failed(err.to_string());
    }
    app.transcript
        .entries
        .push(Entry::Status { text: format!("queued {kind} {id} ({count})") });
    if let Some(err) = audit_error {
        app.transcript
            .entries
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
pub fn submit_internal_turn(app: &mut App, text: String) -> Option<Msg> {
    start_turn(app, text, false)
}

fn start_turn(app: &mut App, text: String, record_user_entry: bool) -> Option<Msg> {
    if app.transcript.pending_compaction_review.is_some() {
        app.transcript
            .entries
            .push(Entry::Error { text: "review the pending compaction before submitting another turn".to_string() });
        app.composer.input.set_text(&text);
        return None;
    }
    if let Some(recovery) = selected_provider_missing(app) {
        app.overlay.show_setup(recovery);
        return None;
    }

    let user_entry = record_user_entry.then(|| Entry::User { text: text.clone() });
    if let Some(entry) = user_entry.as_ref() {
        agent_lifecycle::remember_input(app, &text);
        app.transcript.entries.push(entry.clone());
    }
    app.composer.input.clear();
    app.composer.history_cursor = None;
    app.composer.history_draft.clear();
    app.composer.last_input = Some(text);
    app.runtime.ttft.start_turn();
    app.session.turn_count += 1;
    let turn_id = format!("turn_{}", app.session.turn_count);
    agent_lifecycle::refresh_mcp_config_audit(app, &turn_id);
    if let Some(ref mut writer) = app.session.writer
        && let Some(entry) = user_entry.as_ref()
    {
        let _ = writer.append_entry(entry, &turn_id);
    }
    Some(Msg::Agent(AgentEvent::Started))
}
