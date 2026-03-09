use iced::Color;
use iced::widget::markdown;
use iced::widget::text_editor;
use serde_json::Value;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;
use thndrs_core::{Conversation, Role};

const INPUT_MIN_LINES: usize = 2;
const INPUT_MAX_LINES: usize = 10;
const INPUT_LINE_HEIGHT: f32 = 20.0;
const MAX_FILE_TREE_ENTRIES: usize = 280;
const MAX_FILE_TREE_DEPTH: usize = 3;
const MAX_TOOL_PREVIEW_CHARS: usize = 2_400;
const MAX_TOOL_PREVIEW_LINES: usize = 48;
const MAX_RECENT_WORKSPACES: usize = 12;

// TODO: This could be shared/moved to core between TUI and GUI
const BUILTIN_SLASH_COMMANDS: [(&str, &str); 7] = [
    ("/help", "Show help, shortcuts, and command list"),
    ("/clear", "Clear the current session transcript"),
    ("/model", "Switch or inspect active model"),
    ("/tokens", "Show token usage details"),
    ("/mcp", "List active MCP servers and tools"),
    ("/skills", "List available skills"),
    ("/debug log", "Show recent session logs"),
];

#[derive(Debug, Clone, Default)]
pub struct BootstrapState {
    pub workspace_root: Option<PathBuf>,
    pub recent_workspaces: Vec<PathBuf>,
    pub composer_text: String,
    pub last_model: Option<String>,
    pub warning: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Model(ModelMessage),
    PollBackend(Instant),
}

#[derive(Debug, Clone)]
pub enum ModelMessage {
    ComposerEdited(text_editor::Action),
    SubmitPrompt,
    GoHome,
    SwitchWorkspace(PathBuf),
    RequestWorkspacePicker,
    WorkspacePicked(Option<PathBuf>),
    WorkspaceFilesLoaded(Vec<FileTreeEntry>),
    ToggleToolAction {
        turn_index: usize,
        action_index: usize,
    },
    ApplyComposerSuggestion(ComposerSuggestion),
    OpenMarkdownLink(String),
    SessionsLoaded(Vec<SessionSummary>),
    SessionActivated(String),
    SessionResumed {
        session_id: String,
        title: String,
        messages: Vec<thndrs_core::Message>,
    },
    SessionDeleted(String),
    ResumeSession(String),
    DeleteSession(String),
    ExportSession(String),
    SessionExported(String),
    SessionSearchEdited(String),
    PersistenceFailed(String),
    Tick,
    BackendEvent(BackendEvent),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    DispatchPrompt(String),
    OpenWorkspacePicker,
    DeactivateWorkspace,
    ActivateWorkspace(PathBuf),
    LoadWorkspaceFiles(PathBuf),
    LoadSessions,
    PersistUserPrompt {
        prompt: String,
        title: String,
    },
    PersistAssistantMessage {
        content: String,
    },
    PersistToolCall {
        name: String,
        arguments: String,
        result: Option<String>,
        status: String,
    },
    PersistSessionMetadata {
        session_id: String,
        model: String,
        usage: TokenUsageSummary,
    },
    RefineSessionTitle {
        session_id: String,
        prompt: String,
        assistant_content: String,
    },
    ExtractSessionMemories {
        session_id: String,
        prompt: String,
        assistant_content: String,
    },
    SearchSessions(String),
    ExportSession(String),
    ResumeSession(String),
    DeleteSession(String),
    PersistState,
}

#[derive(Debug, Clone)]
pub struct AppModel {
    pub conversation: Conversation,
    pub turns: Vec<ConversationTurn>,
    pub sessions: Vec<SessionSummary>,
    pub selected_turn: Option<usize>,
    pub workspace_files: Vec<FileTreeEntry>,
    pub composer_suggestions: Vec<ComposerSuggestion>,
    pub composer: text_editor::Content,
    pub composer_text: String,
    pub active_turn: Option<usize>,
    pub tool_call_lookup: HashMap<String, usize>,
    pub streaming: bool,
    pub activity: ActivityState,
    pub animation_tick: u64,
    pub status_text: Option<String>,
    pub error_text: Option<String>,
    pub last_model: Option<String>,
    pub workspace_root: Option<PathBuf>,
    pub recent_workspaces: Vec<PathBuf>,
    pub active_session_id: Option<String>,
    pub session_search_query: String,
}

impl AppModel {
    pub fn new(bootstrap: BootstrapState) -> Self {
        let workspace_root = bootstrap.workspace_root.filter(|path| path.is_dir());
        let recent_workspaces = normalize_recent_workspaces(workspace_root.as_ref(), bootstrap.recent_workspaces);
        let status_text = match (bootstrap.warning, &workspace_root) {
            (Some(warning), _) => Some(warning),
            (None, Some(_)) => Some("Ready".to_string()),
            (None, None) => Some("Select a workspace folder to start".to_string()),
        };

        let mut model = Self {
            conversation: Conversation::with_default_system_prompt(),
            turns: Vec::new(),
            sessions: Vec::new(),
            selected_turn: None,
            workspace_files: Vec::new(),
            composer_suggestions: Vec::new(),
            composer: text_editor::Content::with_text(&bootstrap.composer_text),
            composer_text: bootstrap.composer_text,
            active_turn: None,
            tool_call_lookup: HashMap::new(),
            streaming: false,
            activity: ActivityState::Idle,
            animation_tick: 0,
            status_text,
            error_text: None,
            last_model: bootstrap.last_model,
            workspace_root,
            recent_workspaces,
            active_session_id: None,
            session_search_query: String::new(),
        };

        refresh_composer_suggestions(&mut model);
        model
    }

    fn current_turn_mut(&mut self) -> Option<&mut ConversationTurn> {
        self.active_turn.and_then(|index| self.turns.get_mut(index))
    }

    pub fn input_line_count(&self) -> usize {
        let line_count = if self.composer_text.is_empty() {
            1
        } else {
            self.composer_text.chars().filter(|ch| *ch == '\n').count() + 1
        };

        line_count.clamp(INPUT_MIN_LINES, INPUT_MAX_LINES)
    }

    pub fn input_height(&self) -> f32 {
        self.input_line_count() as f32 * INPUT_LINE_HEIGHT + 16.0
    }

    pub fn spinner_frame(&self) -> &'static str {
        const FRAMES: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];
        FRAMES[(self.animation_tick as usize) % FRAMES.len()]
    }
}

#[derive(Debug, Clone)]
pub struct ConversationTurn {
    pub prompt: String,
    pub intent: String,
    pub thinking: Vec<String>,
    pub actions: Vec<ToolAction>,
    pub result: String,
    pub result_markdown: Vec<markdown::Item>,
    pub error: Option<String>,
    pub next: String,
    pub state: TurnState,
}

impl ConversationTurn {
    fn new(prompt: String) -> Self {
        Self {
            intent: derive_intent(&prompt),
            prompt,
            thinking: Vec::new(),
            actions: Vec::new(),
            result: String::new(),
            result_markdown: Vec::new(),
            error: None,
            next: "Agent is analyzing your request.".to_string(),
            state: TurnState::Running,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ToolAction {
    pub id: String,
    pub name: String,
    pub arguments: String,
    pub result: String,
    pub status: ToolActionStatus,
    pub expanded: bool,
    pub diff_preview: Option<DiffPreview>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffPreview {
    pub title: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineKind {
    Added,
    Removed,
    Context,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolActionStatus {
    Running,
    Success,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnState {
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityState {
    Idle,
    Dispatching,
    Thinking,
    RunningTools,
    Streaming,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TokenUsageSummary {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

impl From<thndrs_providers::Usage> for TokenUsageSummary {
    fn from(value: thndrs_providers::Usage) -> Self {
        Self {
            prompt_tokens: value.prompt_tokens,
            completion_tokens: value.completion_tokens,
            total_tokens: value.total_tokens,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendEvent {
    Thinking(String),
    ToolCalling {
        id: String,
        name: String,
        arguments: String,
    },
    ToolCompleted {
        id: String,
        name: String,
        result: String,
        is_error: bool,
    },
    ContentDelta(String),
    ContentDone {
        model: String,
        usage: TokenUsageSummary,
    },
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionKind {
    Intent,
    Actions,
    Result,
    Next,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaletteContract {
    pub accent_cyan: &'static str,
    pub accent_purple: &'static str,
    pub accent_green: &'static str,
    pub accent_yellow: &'static str,
    pub accent_red: &'static str,
    pub bg_terminal: &'static str,
    pub text_primary: &'static str,
    pub text_secondary: &'static str,
    pub border: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutContract {
    pub sections: [SectionKind; 4],
    pub prompt_symbol: &'static str,
    pub title: &'static str,
    pub terminal_header_height_px: u16,
    pub input_border_top: bool,
    pub input_top_padding_px: u16,
    pub palette: PaletteContract,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSummary {
    pub id: String,
    pub title: String,
    pub message_count: i64,
    pub updated_at_label: String,
    pub model: Option<String>,
    pub total_tokens: Option<u32>,
    pub workspace: Option<String>,
    pub age_group: SessionAgeGroup,
    pub is_active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionAgeGroup {
    Today,
    Yesterday,
    Older,
}

impl SessionAgeGroup {
    pub fn label(self) -> &'static str {
        match self {
            SessionAgeGroup::Today => "Today",
            SessionAgeGroup::Yesterday => "Yesterday",
            SessionAgeGroup::Older => "Older",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileTreeEntry {
    pub relative_path: String,
    pub depth: usize,
    pub is_dir: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposerSuggestion {
    pub label: String,
    pub detail: String,
    pub insert_text: String,
    pub mode: ComposerSuggestionMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposerSuggestionMode {
    SlashCommand,
    FileMention,
}

pub const DESIGN_LAYOUT_CONTRACT: LayoutContract = LayoutContract {
    sections: [
        SectionKind::Intent,
        SectionKind::Actions,
        SectionKind::Result,
        SectionKind::Next,
    ],
    prompt_symbol: "❯",
    title: "Thunderus",
    terminal_header_height_px: 40,
    input_border_top: true,
    input_top_padding_px: 24,
    palette: PaletteContract {
        accent_cyan: "#33b1ff",
        accent_purple: "#be95ff",
        accent_green: "#42be65",
        accent_yellow: "#f1c21b",
        accent_red: "#fa4d56",
        bg_terminal: "#0c0c0c",
        text_primary: "#f4f4f4",
        text_secondary: "#c6c6c6",
        border: "#393939",
    },
};

pub fn design_layout_contract() -> &'static LayoutContract {
    &DESIGN_LAYOUT_CONTRACT
}

pub fn derive_intent(prompt: &str) -> String {
    let trimmed = prompt.trim();
    if trimmed.is_empty() {
        "No intent provided.".to_string()
    } else if trimmed.ends_with('.') {
        trimmed.to_string()
    } else {
        format!("{trimmed}.")
    }
}

pub fn update_model(model: &mut AppModel, message: ModelMessage) -> Vec<Effect> {
    match message {
        ModelMessage::ComposerEdited(action) => {
            model.composer.perform(action);
            model.composer_text = model.composer.text();
            refresh_composer_suggestions(model);
            vec![Effect::PersistState]
        }
        ModelMessage::SubmitPrompt => {
            let prompt = model.composer_text.clone();
            submit_prompt(model, &prompt)
        }
        ModelMessage::GoHome => go_home(model),
        ModelMessage::SwitchWorkspace(path) => set_workspace(model, Some(path)),
        ModelMessage::RequestWorkspacePicker => vec![Effect::OpenWorkspacePicker],
        ModelMessage::WorkspacePicked(path) => set_workspace(model, path),
        ModelMessage::WorkspaceFilesLoaded(files) => {
            model.workspace_files = files;
            refresh_composer_suggestions(model);
            Vec::new()
        }
        ModelMessage::ToggleToolAction { turn_index, action_index } => {
            if let Some(turn) = model.turns.get_mut(turn_index)
                && let Some(action) = turn.actions.get_mut(action_index)
            {
                action.expanded = !action.expanded;
            }
            Vec::new()
        }
        ModelMessage::ApplyComposerSuggestion(suggestion) => {
            apply_composer_suggestion(model, &suggestion);
            vec![Effect::PersistState]
        }
        ModelMessage::OpenMarkdownLink(uri) => {
            model.status_text = Some(format!("Link clicked: {uri}"));
            Vec::new()
        }
        ModelMessage::SessionsLoaded(summaries) => {
            model.sessions = summaries;
            update_active_session_marker(model);
            Vec::new()
        }
        ModelMessage::SessionActivated(session_id) => {
            model.active_session_id = Some(session_id);
            update_active_session_marker(model);
            Vec::new()
        }
        ModelMessage::SessionResumed { session_id, title, messages } => {
            apply_resumed_session(model, &session_id, &title, messages);
            Vec::new()
        }
        ModelMessage::SessionDeleted(session_id) => {
            model.sessions.retain(|summary| summary.id != session_id);
            if model.active_session_id.as_deref() == Some(session_id.as_str()) {
                model.active_session_id = None;
                model.turns.clear();
                model.selected_turn = None;
                model.active_turn = None;
                model.conversation = Conversation::with_default_system_prompt();
            }
            update_active_session_marker(model);
            Vec::new()
        }
        ModelMessage::SessionSearchEdited(query) => {
            model.session_search_query = query.clone();
            if query.trim().is_empty() {
                vec![Effect::LoadSessions]
            } else {
                vec![Effect::SearchSessions(query)]
            }
        }
        ModelMessage::SessionExported(path) => {
            model.status_text = Some(format!("Session exported to {path}"));
            model.error_text = None;
            Vec::new()
        }
        ModelMessage::ExportSession(session_id) => vec![Effect::ExportSession(session_id)],
        ModelMessage::ResumeSession(session_id) => vec![Effect::ResumeSession(session_id)],
        ModelMessage::DeleteSession(session_id) => vec![Effect::DeleteSession(session_id)],
        ModelMessage::PersistenceFailed(error) => {
            model.status_text = Some("Persistence error".to_string());
            model.error_text = Some(error);
            Vec::new()
        }
        ModelMessage::Tick => {
            model.animation_tick = model.animation_tick.wrapping_add(1);
            Vec::new()
        }
        ModelMessage::BackendEvent(event) => apply_backend_event(model, event),
    }
}

fn set_workspace(model: &mut AppModel, workspace_root: Option<PathBuf>) -> Vec<Effect> {
    let Some(workspace_root) = workspace_root else {
        model.status_text = Some("Workspace selection canceled".to_string());
        return Vec::new();
    };
    if !workspace_root.is_dir() {
        model.status_text = Some("Workspace unavailable".to_string());
        model.error_text = Some(format!("Workspace does not exist: {}", workspace_root.display()));
        model.recent_workspaces.retain(|path| path != &workspace_root);
        return vec![Effect::PersistState];
    }

    model.workspace_root = Some(workspace_root.clone());
    remember_recent_workspace(&mut model.recent_workspaces, &workspace_root);
    model.conversation = Conversation::with_default_system_prompt();
    model.turns.clear();
    model.sessions.clear();
    model.active_session_id = None;
    model.selected_turn = None;
    model.workspace_files.clear();
    model.active_turn = None;
    model.tool_call_lookup.clear();
    model.composer = text_editor::Content::new();
    model.composer_text.clear();
    model.session_search_query.clear();
    model.composer_suggestions.clear();
    model.streaming = false;
    model.activity = ActivityState::Idle;
    model.status_text = Some("Workspace ready".to_string());
    model.error_text = None;

    vec![
        Effect::ActivateWorkspace(workspace_root.clone()),
        Effect::LoadWorkspaceFiles(workspace_root),
        Effect::LoadSessions,
        Effect::PersistState,
    ]
}

fn go_home(model: &mut AppModel) -> Vec<Effect> {
    model.workspace_root = None;
    model.conversation = Conversation::with_default_system_prompt();
    model.turns.clear();
    model.sessions.clear();
    model.active_session_id = None;
    model.selected_turn = None;
    model.workspace_files.clear();
    model.active_turn = None;
    model.tool_call_lookup.clear();
    model.composer = text_editor::Content::new();
    model.composer_text.clear();
    model.session_search_query.clear();
    model.composer_suggestions.clear();
    model.streaming = false;
    model.activity = ActivityState::Idle;
    model.error_text = None;
    model.status_text = Some("Select a workspace folder to start".to_string());

    vec![Effect::DeactivateWorkspace, Effect::PersistState]
}

fn submit_prompt(model: &mut AppModel, raw_prompt: &str) -> Vec<Effect> {
    if model.workspace_root.is_none() {
        model.error_text = Some("Select a workspace folder before sending prompts".to_string());
        model.status_text = Some("Workspace required".to_string());
        return Vec::new();
    }

    if model.streaming {
        return Vec::new();
    }

    let prompt = raw_prompt.trim().to_string();
    if prompt.is_empty() {
        return Vec::new();
    }

    model.conversation.add_user_message(&prompt);
    model.turns.push(ConversationTurn::new(prompt.clone()));
    let turn_index = model.turns.len() - 1;
    model.selected_turn = Some(turn_index);
    model.active_turn = Some(turn_index);
    model.tool_call_lookup.clear();
    model.composer = text_editor::Content::new();
    model.composer_text.clear();
    model.composer_suggestions.clear();
    model.streaming = true;
    model.activity = ActivityState::Dispatching;
    model.status_text = Some("Dispatching prompt to provider".to_string());
    model.error_text = None;

    vec![
        Effect::PersistUserPrompt { prompt: prompt.clone(), title: compose_session_title(&prompt) },
        Effect::DispatchPrompt(prompt.clone()),
        Effect::PersistState,
    ]
}

fn apply_backend_event(model: &mut AppModel, event: BackendEvent) -> Vec<Effect> {
    let mut effects = Vec::new();

    match event {
        BackendEvent::Thinking(thinking) => {
            if let Some(turn) = model.current_turn_mut()
                && !thinking.trim().is_empty()
                && turn.thinking.last() != Some(&thinking)
            {
                turn.thinking.push(thinking);
            }
            model.activity = ActivityState::Thinking;
            model.status_text = Some("Agent is reasoning".to_string());
        }
        BackendEvent::ToolCalling { id, name, arguments } => {
            let diff_preview = infer_diff_preview(&name, &arguments, "");
            if let Some(turn) = model.current_turn_mut() {
                turn.actions.push(ToolAction {
                    id: id.clone(),
                    name: name.clone(),
                    arguments: arguments.clone(),
                    result: String::new(),
                    status: ToolActionStatus::Running,
                    expanded: true,
                    diff_preview,
                });
                let index = turn.actions.len() - 1;
                model.tool_call_lookup.insert(id, index);
                model.activity = ActivityState::RunningTools;
                model.status_text = Some("Executing tools".to_string());
            }
            effects.push(Effect::PersistToolCall { name, arguments, result: None, status: "running".to_string() });
        }
        BackendEvent::ToolCompleted { id, name, result, is_error } => {
            let lookup_index = model.tool_call_lookup.get(&id).copied();
            let mut tool_arguments = String::new();
            if let Some(turn) = model.current_turn_mut() {
                if let Some(index) = lookup_index
                    && let Some(action) = turn.actions.get_mut(index)
                {
                    tool_arguments = action.arguments.clone();
                    action.result = result.clone();
                    action.status = if is_error { ToolActionStatus::Error } else { ToolActionStatus::Success };
                    if action.diff_preview.is_none() {
                        action.diff_preview = infer_diff_preview(&action.name, &action.arguments, &action.result);
                    }
                    if is_error {
                        action.expanded = true;
                    }
                }
                model.status_text = Some("Tool execution completed".to_string());
            }
            effects.push(Effect::PersistToolCall {
                name,
                arguments: tool_arguments,
                result: Some(result),
                status: if is_error { "error".to_string() } else { "success".to_string() },
            });
        }
        BackendEvent::ContentDelta(delta) => {
            if let Some(turn) = model.current_turn_mut() {
                turn.result.push_str(&delta);
                turn.result_markdown = parse_markdown_items(&turn.result);
            }
            model.activity = ActivityState::Streaming;
            model.status_text = Some("Streaming response".to_string());
        }
        BackendEvent::ContentDone { model: backend_model, usage } => {
            let mut assistant_content = None;
            let mut prompt = None;
            if let Some(turn) = model.current_turn_mut() {
                turn.state = TurnState::Completed;
                turn.next = "Continue with a follow-up or refine the request.".to_string();
                turn.error = None;
                turn.result_markdown = parse_markdown_items(&turn.result);
                assistant_content = Some(turn.result.clone());
                prompt = Some(turn.prompt.clone());
            }
            if let (Some(content), Some(prompt)) = (assistant_content, prompt) {
                model.conversation.add_assistant_message(content.clone());
                effects.push(Effect::PersistAssistantMessage { content: content.clone() });

                if let Some(session_id) = model.active_session_id.clone() {
                    effects.push(Effect::PersistSessionMetadata {
                        session_id: session_id.clone(),
                        model: backend_model.clone(),
                        usage,
                    });
                    effects.push(Effect::RefineSessionTitle {
                        session_id: session_id.clone(),
                        prompt: prompt.clone(),
                        assistant_content: content.clone(),
                    });
                    effects.push(Effect::ExtractSessionMemories { session_id, prompt, assistant_content: content });
                }
            }
            model.active_turn = None;
            model.streaming = false;
            model.activity = ActivityState::Idle;
            model.status_text = Some("Ready".to_string());
            model.error_text = None;
            model.last_model = Some(backend_model);
            effects.push(Effect::PersistState);
            return effects;
        }
        BackendEvent::Error(error) => {
            if let Some(turn) = model.current_turn_mut() {
                for action in &mut turn.actions {
                    if action.status == ToolActionStatus::Running {
                        action.status = ToolActionStatus::Error;
                        if action.result.is_empty() {
                            action.result = "Interrupted after backend error".to_string();
                        }
                        action.expanded = true;
                    }
                }

                turn.state = TurnState::Failed;
                turn.error = Some(error.clone());
                turn.next = "Review the error, tighten scope, or retry with fewer tool steps.".to_string();
            }
            model.active_turn = None;
            model.streaming = false;
            model.activity = ActivityState::Failed;
            model.status_text = Some("Backend error".to_string());
            model.error_text = Some(error);
            effects.push(Effect::PersistState);
            return effects;
        }
    }

    effects
}

fn apply_composer_suggestion(model: &mut AppModel, suggestion: &ComposerSuggestion) {
    match suggestion.mode {
        ComposerSuggestionMode::SlashCommand => {
            model.composer_text = format!("{} ", suggestion.insert_text.trim());
        }
        ComposerSuggestionMode::FileMention => {
            let target = format!("@{}", suggestion.insert_text.trim());
            model.composer_text = replace_last_mention(&model.composer_text, &target);
        }
    }

    model.composer = text_editor::Content::with_text(&model.composer_text);
    refresh_composer_suggestions(model);
}

fn refresh_composer_suggestions(model: &mut AppModel) {
    model.composer_suggestions = compute_suggestions(&model.composer_text, &model.workspace_files);
}

fn update_active_session_marker(model: &mut AppModel) {
    let active = model.active_session_id.as_deref();
    for summary in &mut model.sessions {
        summary.is_active = active == Some(summary.id.as_str());
    }
}

fn apply_resumed_session(model: &mut AppModel, session_id: &str, title: &str, messages: Vec<thndrs_core::Message>) {
    model.active_session_id = Some(session_id.to_string());
    model.conversation = Conversation::with_default_system_prompt();
    model.turns = reconstruct_turns_from_messages(&messages);
    model.active_turn = None;
    model.selected_turn = None;
    model.streaming = false;
    model.activity = ActivityState::Idle;
    model.error_text = None;
    model.status_text = Some(format!("Resumed session {title}"));

    for message in messages {
        if message.role == Role::System {
            continue;
        }
        model.conversation.add_message(message);
    }

    if let Some(summary) = model.sessions.iter_mut().find(|summary| summary.id == session_id) {
        summary.is_active = true;
    }
    update_active_session_marker(model);
}

fn reconstruct_turns_from_messages(messages: &[thndrs_core::Message]) -> Vec<ConversationTurn> {
    let mut turns = Vec::new();

    for message in messages {
        match message.role {
            Role::User => turns.push(ConversationTurn {
                intent: derive_intent(&message.content),
                prompt: message.content.clone(),
                thinking: Vec::new(),
                actions: Vec::new(),
                result: String::new(),
                result_markdown: Vec::new(),
                error: None,
                next: "Continue with a follow-up or refine the request.".to_string(),
                state: TurnState::Completed,
            }),
            Role::Assistant => {
                if let Some(turn) = turns.last_mut() {
                    turn.result = message.content.clone();
                    turn.result_markdown = parse_markdown_items(&turn.result);
                    turn.state = TurnState::Completed;
                }
            }
            Role::System | Role::Tool => {}
        }
    }

    turns
}

fn compose_session_title(prompt: &str) -> String {
    let line = prompt.lines().next().unwrap_or_default().trim();
    if line.is_empty() {
        return "Untitled prompt".to_string();
    }

    truncate_with_ellipsis(line, 58)
}

fn normalize_recent_workspaces(current: Option<&PathBuf>, recent: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut normalized = Vec::new();
    if let Some(path) = current {
        remember_recent_workspace(&mut normalized, path);
    }
    for path in recent {
        remember_recent_workspace(&mut normalized, &path);
    }
    normalized
}

fn remember_recent_workspace(recent_workspaces: &mut Vec<PathBuf>, workspace_root: &Path) {
    if !workspace_root.is_dir() {
        return;
    }
    recent_workspaces.retain(|path| path != workspace_root);
    recent_workspaces.insert(0, workspace_root.to_path_buf());
    if recent_workspaces.len() > MAX_RECENT_WORKSPACES {
        recent_workspaces.truncate(MAX_RECENT_WORKSPACES);
    }
}

fn truncate_with_ellipsis(input: &str, max_chars: usize) -> String {
    if input.chars().count() <= max_chars {
        return input.to_string();
    }

    let mut out = String::new();
    for ch in input.chars().take(max_chars.saturating_sub(1)) {
        out.push(ch);
    }
    out.push('…');
    out
}

fn compute_suggestions(composer_text: &str, workspace_files: &[FileTreeEntry]) -> Vec<ComposerSuggestion> {
    let trimmed = composer_text.trim_end();

    if let Some(command_prefix) = slash_prefix(trimmed) {
        let mut suggestions = BUILTIN_SLASH_COMMANDS
            .iter()
            .filter(|(command, _description)| command.starts_with(command_prefix))
            .map(|(command, description)| ComposerSuggestion {
                label: (*command).to_string(),
                detail: (*description).to_string(),
                insert_text: (*command).to_string(),
                mode: ComposerSuggestionMode::SlashCommand,
            })
            .collect::<Vec<_>>();

        suggestions.truncate(6);
        return suggestions;
    }

    if let Some(file_prefix) = file_mention_prefix(trimmed) {
        let normalized = file_prefix.to_lowercase();
        let mut matches =
            workspace_files
                .iter()
                .filter(|entry| !entry.is_dir)
                .filter(|entry| {
                    if normalized.is_empty() { true } else { entry.relative_path.to_lowercase().contains(&normalized) }
                })
                .map(|entry| ComposerSuggestion {
                    label: format!("@{}", entry.relative_path),
                    detail: "Mention workspace file".to_string(),
                    insert_text: entry.relative_path.clone(),
                    mode: ComposerSuggestionMode::FileMention,
                })
                .collect::<Vec<_>>();

        matches.sort_by(|a, b| a.insert_text.cmp(&b.insert_text));
        matches.truncate(8);
        return matches;
    }

    Vec::new()
}

fn slash_prefix(composer_text: &str) -> Option<&str> {
    let trimmed_start = composer_text.trim_start();
    if !trimmed_start.starts_with('/') {
        return None;
    }

    let token = trimmed_start.split_whitespace().next().unwrap_or(trimmed_start);
    if token.contains(' ') {
        return None;
    }

    Some(token)
}

fn file_mention_prefix(composer_text: &str) -> Option<&str> {
    let token = composer_text
        .split(|ch: char| ch.is_whitespace() || ch == '\n')
        .next_back()
        .unwrap_or_default();
    let prefix = token.strip_prefix('@')?;
    if prefix.contains('@') {
        return None;
    }

    Some(prefix)
}

fn replace_last_mention(input: &str, replacement: &str) -> String {
    let mut split_index = input.len();
    for (index, ch) in input.char_indices().rev() {
        if ch.is_whitespace() || ch == '\n' {
            split_index = index + ch.len_utf8();
            break;
        }
        split_index = index;
    }

    let prefix = &input[..split_index];
    format!("{prefix}{replacement} ")
}

pub fn collect_workspace_files(workspace_root: &Path) -> Vec<FileTreeEntry> {
    let mut entries = Vec::new();
    collect_workspace_files_inner(workspace_root, workspace_root, 0, &mut entries);
    entries
}

fn collect_workspace_files_inner(root: &Path, current: &Path, depth: usize, output: &mut Vec<FileTreeEntry>) {
    if depth > MAX_FILE_TREE_DEPTH || output.len() >= MAX_FILE_TREE_ENTRIES {
        return;
    }

    let read_dir = match std::fs::read_dir(current) {
        Ok(read_dir) => read_dir,
        Err(_) => return,
    };

    let mut children = Vec::new();
    for entry in read_dir.flatten() {
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().to_string();
        if should_skip_workspace_entry(&file_name) {
            continue;
        }

        let is_dir = path.is_dir();
        children.push((file_name, path, is_dir));
    }

    children.sort_by(|left, right| match (left.2, right.2) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        _ => left.0.cmp(&right.0),
    });

    for (_name, path, is_dir) in children {
        if output.len() >= MAX_FILE_TREE_ENTRIES {
            return;
        }

        let relative = path
            .strip_prefix(root)
            .ok()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string());

        output.push(FileTreeEntry { relative_path: relative, depth, is_dir });

        if is_dir {
            collect_workspace_files_inner(root, &path, depth + 1, output);
        }
    }
}

fn should_skip_workspace_entry(name: &str) -> bool {
    matches!(name, ".git" | ".jj" | "target" | "node_modules" | ".sandbox")
}

pub fn parse_markdown_items(content: &str) -> Vec<markdown::Item> {
    if content.trim().is_empty() { Vec::new() } else { markdown::parse(content).collect() }
}

pub fn normalize_tool_text(raw: &str) -> String {
    let pretty_json = serde_json::from_str::<Value>(raw)
        .ok()
        .and_then(|value| serde_json::to_string_pretty(&value).ok())
        .unwrap_or_else(|| raw.to_string());

    let mut out = String::new();
    for (line_count, line) in pretty_json.lines().enumerate() {
        if line_count >= MAX_TOOL_PREVIEW_LINES {
            out.push_str("\n… truncated …");
            break;
        }

        if !out.is_empty() {
            out.push('\n');
        }

        let shortened = truncate_with_ellipsis(line, MAX_TOOL_PREVIEW_CHARS / MAX_TOOL_PREVIEW_LINES);
        out.push_str(&shortened);

        if out.chars().count() >= MAX_TOOL_PREVIEW_CHARS {
            out.push_str("\n… truncated …");
            break;
        }
    }

    if out.is_empty() { truncate_with_ellipsis(&pretty_json, MAX_TOOL_PREVIEW_CHARS) } else { out }
}

fn infer_diff_preview(tool_name: &str, arguments: &str, result: &str) -> Option<DiffPreview> {
    let from_result = extract_unified_diff(result);
    if !from_result.is_empty() {
        return Some(DiffPreview { title: "Tool diff output".to_string(), lines: from_result });
    }

    let normalized = tool_name.to_ascii_lowercase();
    if !matches!(normalized.as_str(), "write" | "edit" | "patch" | "apply_patch") {
        return None;
    }

    let value = serde_json::from_str::<Value>(arguments).ok()?;
    let path = value.get("path").and_then(Value::as_str).unwrap_or("<unknown>");

    if let (Some(old), Some(new)) = (
        value.get("old").and_then(Value::as_str),
        value.get("new").and_then(Value::as_str),
    ) {
        return Some(diff_from_before_after(path, old, new));
    }

    if let (Some(old), Some(new)) = (
        value.get("old_str").and_then(Value::as_str),
        value.get("new_str").and_then(Value::as_str),
    ) {
        return Some(diff_from_before_after(path, old, new));
    }

    if let Some(content) = value
        .get("content")
        .and_then(Value::as_str)
        .or_else(|| value.get("text").and_then(Value::as_str))
    {
        let mut lines = vec![DiffLine { kind: DiffLineKind::Context, text: format!("+++ {path}") }];

        for line in content.lines().take(12) {
            lines.push(DiffLine { kind: DiffLineKind::Added, text: format!("+ {line}") });
        }

        if content.lines().count() > 12 {
            lines.push(DiffLine { kind: DiffLineKind::Context, text: "… more lines omitted …".to_string() });
        }

        return Some(DiffPreview { title: format!("Planned write: {path}"), lines });
    }

    None
}

fn extract_unified_diff(raw: &str) -> Vec<DiffLine> {
    let mut lines = Vec::new();

    for line in raw.lines().take(80) {
        if line.starts_with("+++") {
            lines.push(DiffLine { kind: DiffLineKind::Added, text: line.to_string() });
            continue;
        }

        if line.starts_with("---") {
            lines.push(DiffLine { kind: DiffLineKind::Removed, text: line.to_string() });
            continue;
        }

        if line.starts_with("+") {
            lines.push(DiffLine { kind: DiffLineKind::Added, text: line.to_string() });
            continue;
        }

        if line.starts_with("-") {
            lines.push(DiffLine { kind: DiffLineKind::Removed, text: line.to_string() });
            continue;
        }

        if line.starts_with("@@") || line.starts_with("diff --git") {
            lines.push(DiffLine { kind: DiffLineKind::Context, text: line.to_string() });
        }
    }

    lines
}

fn diff_from_before_after(path: &str, before: &str, after: &str) -> DiffPreview {
    let before_lines = before.lines().collect::<Vec<_>>();
    let after_lines = after.lines().collect::<Vec<_>>();
    let max = before_lines.len().max(after_lines.len());

    let mut lines = vec![
        DiffLine { kind: DiffLineKind::Removed, text: format!("--- {path}") },
        DiffLine { kind: DiffLineKind::Added, text: format!("+++ {path}") },
    ];

    for index in 0..max.min(20) {
        let before_line = before_lines.get(index).copied();
        let after_line = after_lines.get(index).copied();

        match (before_line, after_line) {
            (Some(left), Some(right)) if left == right => {
                lines.push(DiffLine { kind: DiffLineKind::Context, text: format!("  {left}") })
            }
            (Some(left), Some(right)) => {
                lines.push(DiffLine { kind: DiffLineKind::Removed, text: format!("- {left}") });
                lines.push(DiffLine { kind: DiffLineKind::Added, text: format!("+ {right}") });
            }
            (Some(left), None) => lines.push(DiffLine { kind: DiffLineKind::Removed, text: format!("- {left}") }),
            (None, Some(right)) => lines.push(DiffLine { kind: DiffLineKind::Added, text: format!("+ {right}") }),
            (None, None) => break,
        }
    }

    if max > 20 {
        lines.push(DiffLine { kind: DiffLineKind::Context, text: "… more lines omitted …".to_string() });
    }

    DiffPreview { title: format!("Diff preview: {path}"), lines }
}

pub fn color_hex(hex: &str) -> Color {
    let value = hex.trim_start_matches('#');
    if value.len() != 6 {
        return Color::BLACK;
    }

    let parsed = u32::from_str_radix(value, 16).unwrap_or(0);
    let r = ((parsed >> 16) & 0xFF) as f32 / 255.0;
    let g = ((parsed >> 8) & 0xFF) as f32 / 255.0;
    let b = (parsed & 0xFF) as f32 / 255.0;
    Color::from_rgb(r, g, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_workspace(name: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!("thndrs-gui-{name}-{}-{nonce}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn ready_model() -> AppModel {
        let workspace = temp_workspace("ready");
        AppModel::new(BootstrapState {
            workspace_root: Some(workspace.clone()),
            recent_workspaces: vec![workspace],
            composer_text: String::new(),
            last_model: None,
            warning: None,
        })
    }

    #[test]
    fn mvu_submit_starts_running_turn() {
        let mut model = ready_model();
        model.composer = text_editor::Content::with_text("Refactor the auth module to use middleware pattern");
        model.composer_text = "Refactor the auth module to use middleware pattern".to_string();
        let effects = update_model(&mut model, ModelMessage::SubmitPrompt);

        assert_eq!(
            effects,
            vec![
                Effect::PersistUserPrompt {
                    prompt: "Refactor the auth module to use middleware pattern".to_string(),
                    title: "Refactor the auth module to use middleware pattern".to_string(),
                },
                Effect::DispatchPrompt("Refactor the auth module to use middleware pattern".to_string()),
                Effect::PersistState
            ]
        );
        assert_eq!(model.turns.len(), 1);
        assert!(model.streaming);
        assert_eq!(model.turns[0].state, TurnState::Running);
        assert_eq!(
            model.turns[0].intent,
            "Refactor the auth module to use middleware pattern."
        );
    }

    #[test]
    fn mvu_tool_events_update_actions() {
        let mut model = ready_model();
        model.composer = text_editor::Content::with_text("Inspect auth module");
        model.composer_text = "Inspect auth module".to_string();
        let _ = update_model(&mut model, ModelMessage::SubmitPrompt);

        let _ = update_model(
            &mut model,
            ModelMessage::BackendEvent(BackendEvent::ToolCalling {
                id: "tool_1".to_string(),
                name: "read".to_string(),
                arguments: "src/auth.rs".to_string(),
            }),
        );
        let _ = update_model(
            &mut model,
            ModelMessage::BackendEvent(BackendEvent::ToolCompleted {
                id: "tool_1".to_string(),
                name: "read".to_string(),
                result: "Loaded file".to_string(),
                is_error: false,
            }),
        );

        assert_eq!(model.turns[0].actions.len(), 1);
        assert_eq!(model.turns[0].actions[0].name, "read");
        assert_eq!(model.turns[0].actions[0].result, "Loaded file");
        assert_eq!(model.turns[0].actions[0].status, ToolActionStatus::Success);
    }

    #[test]
    fn mvu_streaming_deltas_complete_turn_and_store_assistant_message() {
        let mut model = ready_model();
        model.composer = text_editor::Content::with_text("Summarize changes");
        model.composer_text = "Summarize changes".to_string();
        let _ = update_model(&mut model, ModelMessage::SubmitPrompt);

        let _ = update_model(
            &mut model,
            ModelMessage::BackendEvent(BackendEvent::ContentDelta("All ".to_string())),
        );
        let _ = update_model(
            &mut model,
            ModelMessage::BackendEvent(BackendEvent::ContentDelta("tests passing.".to_string())),
        );
        let effects = update_model(
            &mut model,
            ModelMessage::BackendEvent(BackendEvent::ContentDone {
                model: "kimi-k2.5".to_string(),
                usage: TokenUsageSummary::default(),
            }),
        );

        assert_eq!(
            effects,
            vec![
                Effect::PersistAssistantMessage { content: "All tests passing.".to_string() },
                Effect::PersistState
            ]
        );
        assert!(!model.streaming);
        assert_eq!(model.turns[0].state, TurnState::Completed);
        assert_eq!(model.turns[0].result, "All tests passing.");
        assert_eq!(model.conversation.last_assistant_message(), Some("All tests passing."));
        assert_eq!(model.last_model.as_deref(), Some("kimi-k2.5"));
    }

    #[test]
    fn layout_contract_matches_design_reference() {
        let contract = design_layout_contract();

        assert_eq!(
            contract.sections,
            [
                SectionKind::Intent,
                SectionKind::Actions,
                SectionKind::Result,
                SectionKind::Next
            ]
        );
        assert_eq!(contract.prompt_symbol, "❯");
        assert_eq!(contract.title, "Thunderus");
        assert_eq!(contract.terminal_header_height_px, 40);
        assert!(contract.input_border_top);
        assert_eq!(contract.input_top_padding_px, 24);
        assert_eq!(contract.palette.accent_cyan, "#33b1ff");
        assert_eq!(contract.palette.accent_purple, "#be95ff");
        assert_eq!(contract.palette.accent_green, "#42be65");
        assert_eq!(contract.palette.accent_yellow, "#f1c21b");
        assert_eq!(contract.palette.accent_red, "#fa4d56");
        assert_eq!(contract.palette.bg_terminal, "#0c0c0c");
        assert_eq!(contract.palette.border, "#393939");
    }

    #[test]
    fn workspace_selection_emits_activation_and_persistence_effects() {
        let mut model = AppModel::new(BootstrapState::default());
        let path = temp_workspace("selection");

        let effects = update_model(&mut model, ModelMessage::WorkspacePicked(Some(path.clone())));

        assert_eq!(
            effects,
            vec![
                Effect::ActivateWorkspace(path.clone()),
                Effect::LoadWorkspaceFiles(path.clone()),
                Effect::LoadSessions,
                Effect::PersistState
            ]
        );
        assert_eq!(model.workspace_root, Some(path));
    }

    #[test]
    fn go_home_clears_workspace_and_deactivates_backend() {
        let mut model = ready_model();

        let effects = update_model(&mut model, ModelMessage::GoHome);

        assert_eq!(effects, vec![Effect::DeactivateWorkspace, Effect::PersistState]);
        assert_eq!(model.workspace_root, None);
        assert!(model.turns.is_empty());
        assert!(model.workspace_files.is_empty());
    }

    #[test]
    fn workspace_switch_reuses_recent_workspace() {
        let workspace = temp_workspace("switch");
        let other_workspace = temp_workspace("switch-other");
        let mut model = AppModel::new(BootstrapState {
            workspace_root: None,
            recent_workspaces: vec![workspace.clone(), other_workspace],
            composer_text: String::new(),
            last_model: None,
            warning: None,
        });
        let path = workspace;

        let effects = update_model(&mut model, ModelMessage::SwitchWorkspace(path.clone()));

        assert_eq!(
            effects,
            vec![
                Effect::ActivateWorkspace(path.clone()),
                Effect::LoadWorkspaceFiles(path.clone()),
                Effect::LoadSessions,
                Effect::PersistState
            ]
        );
        assert_eq!(model.workspace_root, Some(path.clone()));
        assert_eq!(model.recent_workspaces.first(), Some(&path));
    }

    #[test]
    fn submit_without_workspace_is_blocked() {
        let mut model = AppModel::new(BootstrapState::default());
        model.composer = text_editor::Content::with_text("hello");
        model.composer_text = "hello".to_string();

        let effects = update_model(&mut model, ModelMessage::SubmitPrompt);

        assert!(effects.is_empty());
        assert_eq!(
            model.error_text.as_deref(),
            Some("Select a workspace folder before sending prompts")
        );
    }

    #[test]
    fn suggestion_engine_recommends_slash_commands() {
        let suggestions = compute_suggestions("/he", &[]);
        assert!(!suggestions.is_empty());
        assert_eq!(suggestions[0].mode, ComposerSuggestionMode::SlashCommand);
    }

    #[test]
    fn normalize_tool_text_pretty_prints_json() {
        let normalized = normalize_tool_text(r#"{"path":"src/main.rs","flag":true}"#);
        assert!(normalized.contains("\"path\""));
        assert!(normalized.contains("src/main.rs"));
    }

    #[test]
    fn error_event_marks_turn_failed() {
        let mut model = ready_model();
        model.composer = text_editor::Content::with_text("Trigger error");
        model.composer_text = "Trigger error".to_string();
        let _ = update_model(&mut model, ModelMessage::SubmitPrompt);

        let _ = update_model(
            &mut model,
            ModelMessage::BackendEvent(BackendEvent::Error("boom".to_string())),
        );

        assert_eq!(model.turns[0].state, TurnState::Failed);
        assert_eq!(model.turns[0].error.as_deref(), Some("boom"));
        assert!(!model.streaming);
    }
}
