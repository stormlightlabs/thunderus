use iced::Color;
use iced::widget::text_editor;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;
use thndrs_core::Conversation;

const INPUT_MIN_LINES: usize = 2;
const INPUT_MAX_LINES: usize = 10;
const INPUT_LINE_HEIGHT: f32 = 20.0;

#[derive(Debug, Clone, Default)]
pub struct BootstrapState {
    pub workspace_root: Option<PathBuf>,
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
    RequestWorkspacePicker,
    WorkspacePicked(Option<PathBuf>),
    BackendEvent(BackendEvent),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    DispatchPrompt(String),
    OpenWorkspacePicker,
    ActivateWorkspace(PathBuf),
    PersistState,
}

#[derive(Debug, Clone)]
pub struct AppModel {
    pub conversation: Conversation,
    pub turns: Vec<ConversationTurn>,
    pub composer: text_editor::Content,
    pub composer_text: String,
    pub active_turn: Option<usize>,
    pub tool_call_lookup: HashMap<String, usize>,
    pub streaming: bool,
    pub status_text: Option<String>,
    pub error_text: Option<String>,
    pub last_model: Option<String>,
    pub workspace_root: Option<PathBuf>,
}

impl AppModel {
    pub fn new(bootstrap: BootstrapState) -> Self {
        let status_text = match (bootstrap.warning, &bootstrap.workspace_root) {
            (Some(warning), _) => Some(warning),
            (None, Some(_)) => Some("Ready".to_string()),
            (None, None) => Some("Select a workspace folder to start".to_string()),
        };

        Self {
            conversation: Conversation::with_default_system_prompt(),
            turns: Vec::new(),
            composer: text_editor::Content::with_text(&bootstrap.composer_text),
            composer_text: bootstrap.composer_text,
            active_turn: None,
            tool_call_lookup: HashMap::new(),
            streaming: false,
            status_text,
            error_text: None,
            last_model: bootstrap.last_model,
            workspace_root: bootstrap.workspace_root,
        }
    }

    fn current_turn_mut(&mut self) -> Option<&mut ConversationTurn> {
        self.active_turn.and_then(|index| self.turns.get_mut(index))
    }

    pub fn input_line_count(&self) -> usize {
        let count = self
            .composer_text
            .lines()
            .filter(|line| !line.is_empty())
            .count()
            .max(1);
        count.clamp(INPUT_MIN_LINES, INPUT_MAX_LINES)
    }

    pub fn input_height(&self) -> f32 {
        self.input_line_count() as f32 * INPUT_LINE_HEIGHT + 16.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationTurn {
    pub prompt: String,
    pub intent: String,
    pub actions: Vec<ToolAction>,
    pub result: String,
    pub next: String,
    pub state: TurnState,
}

impl ConversationTurn {
    fn new(prompt: String) -> Self {
        Self {
            intent: derive_intent(&prompt),
            prompt,
            actions: Vec::new(),
            result: String::new(),
            next: "Waiting for completion".to_string(),
            state: TurnState::Running,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolAction {
    pub id: String,
    pub name: String,
    pub arguments: String,
    pub result: String,
    pub status: ToolActionStatus,
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
            vec![Effect::PersistState]
        }
        ModelMessage::SubmitPrompt => submit_prompt(model, model.composer_text.clone().as_str()),
        ModelMessage::RequestWorkspacePicker => vec![Effect::OpenWorkspacePicker],
        ModelMessage::WorkspacePicked(path) => set_workspace(model, path),
        ModelMessage::BackendEvent(event) => apply_backend_event(model, event),
    }
}

fn set_workspace(model: &mut AppModel, workspace_root: Option<PathBuf>) -> Vec<Effect> {
    let Some(workspace_root) = workspace_root else {
        model.status_text = Some("Workspace selection canceled".to_string());
        return Vec::new();
    };

    model.workspace_root = Some(workspace_root.clone());
    model.status_text = Some("Workspace ready".to_string());
    model.error_text = None;

    vec![Effect::ActivateWorkspace(workspace_root), Effect::PersistState]
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
    model.active_turn = Some(model.turns.len() - 1);
    model.tool_call_lookup.clear();
    model.composer = text_editor::Content::new();
    model.composer_text.clear();
    model.streaming = true;
    model.status_text = Some("Waiting for provider response".to_string());
    model.error_text = None;

    vec![Effect::DispatchPrompt(prompt), Effect::PersistState]
}

fn apply_backend_event(model: &mut AppModel, event: BackendEvent) -> Vec<Effect> {
    match event {
        BackendEvent::Thinking(thinking) => {
            model.status_text = Some(thinking);
        }
        BackendEvent::ToolCalling { id, name, arguments } => {
            if let Some(turn) = model.current_turn_mut() {
                turn.actions.push(ToolAction {
                    id: id.clone(),
                    name,
                    arguments,
                    result: String::new(),
                    status: ToolActionStatus::Running,
                });
                let index = turn.actions.len() - 1;
                model.tool_call_lookup.insert(id, index);
                model.status_text = Some("Executing tools".to_string());
            }
        }
        BackendEvent::ToolCompleted { id, name: _name, result, is_error } => {
            let lookup_index = model.tool_call_lookup.get(&id).copied();
            if let Some(turn) = model.current_turn_mut() {
                if let Some(index) = lookup_index
                    && let Some(action) = turn.actions.get_mut(index)
                {
                    action.result = result;
                    action.status = if is_error { ToolActionStatus::Error } else { ToolActionStatus::Success };
                }
                model.status_text = Some("Tool execution completed".to_string());
            }
        }
        BackendEvent::ContentDelta(delta) => {
            if let Some(turn) = model.current_turn_mut() {
                turn.result.push_str(&delta);
                model.status_text = Some("Streaming response".to_string());
            }
        }
        BackendEvent::ContentDone { model: backend_model } => {
            let mut assistant_content = None;
            if let Some(turn) = model.current_turn_mut() {
                turn.state = TurnState::Completed;
                turn.next = "Continue with a follow-up or refine the request.".to_string();
                assistant_content = Some(turn.result.clone());
            }
            if let Some(content) = assistant_content {
                model.conversation.add_assistant_message(content);
            }
            model.active_turn = None;
            model.streaming = false;
            model.status_text = Some("Ready".to_string());
            model.last_model = Some(backend_model);
            return vec![Effect::PersistState];
        }
        BackendEvent::Error(error) => {
            if let Some(turn) = model.current_turn_mut() {
                turn.state = TurnState::Failed;
                turn.next = "Inspect the error details and retry with a narrower prompt.".to_string();
            }
            model.active_turn = None;
            model.streaming = false;
            model.status_text = Some("Backend error".to_string());
            model.error_text = Some(error);
            return vec![Effect::PersistState];
        }
    }

    Vec::new()
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

    fn ready_model() -> AppModel {
        AppModel::new(BootstrapState {
            workspace_root: Some(PathBuf::from("/tmp/workspace")),
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
            ModelMessage::BackendEvent(BackendEvent::ContentDone { model: "kimi-k2.5".to_string() }),
        );

        assert_eq!(effects, vec![Effect::PersistState]);
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
        let path = PathBuf::from("/tmp/new-workspace");

        let effects = update_model(&mut model, ModelMessage::WorkspacePicked(Some(path.clone())));

        assert_eq!(
            effects,
            vec![Effect::ActivateWorkspace(path.clone()), Effect::PersistState]
        );
        assert_eq!(model.workspace_root, Some(path));
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
}
