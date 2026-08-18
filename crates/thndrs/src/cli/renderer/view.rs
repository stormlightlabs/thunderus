//! Pure renderer view projection.
//!
//! [`RendererView`] is a data-only staging area built from [`App`] plus
//! terminal dimensions. It contains no crossterm types and performs no terminal
//! writes. The view separates semantic row construction from viewport policy so
//! that the inline terminal coordinator can keep terminal policy separate,
//! projection caching, and frame composition.

#[cfg(test)]
mod tests;

use crate::app::{
    App, BlockContentState, CONTEXT_INSPECTION_MAX_ITEMS, ChatGptOAuthMethod, Entry, FilePickerSource,
    FirstRunRecovery, McpTrustAction, McpTrustSurface, Mode, PromptAccessory, QueueAuditState, QueueTarget,
    RecoveryStage, RunState, ToolLifecycleState, ToolStatus, TranscriptBlock, TranscriptBlockId, TranscriptBlockKind,
};
use crate::cli::commands::setup::SetupProviderArg;
use crate::renderer::row::{CursorCoord, Row};
pub use crate::renderer::style::ThemeRole;
use crate::renderer::transcript::{
    ActivityImportance, ActivityKind, ActivityProjection, ActivitySummary, TranscriptRowContext, edit_path_from_args,
    summarize_tool_invocation,
};
use crate::tools::shell::redact_secrets;
use crate::utils;

/// A transcript row family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TranscriptRowKind {
    User,
    Assistant,
    Reasoning,
    Skill,
    Tool,
    Edit,
    Diff,
    Status,
    Error,
    Notice,
    Cancelled,
}

impl TranscriptRowKind {
    fn build_row(self, stable: bool, primary: String) -> TranscriptRowView {
        TranscriptRowView {
            block_id: None,
            block_kind: None,
            kind: self,
            stable,
            primary,
            tool: None,
            edit: None,
            diff: None,
        }
    }
}

/// Prompt suggestion family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PromptSuggestionKind {
    Command,
    FileMention,
}

/// Prompt input mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PromptModeView {
    Prompt,
    Command,
}

/// Exact prompt states from the UI contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PromptStatusView {
    Idle,
    Drafting,
    Suggesting,
    Running,
    Queued,
    Failed,
    Retryable,
    Cancelled,
}

/// Width fallback policy for semantic status/orientation fields.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TruncationPolicy {
    Hide,
    EllipsizeMiddle,
    EllipsizeEnd,
}

/// Focused bounded surface semantic state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FocusedSurfaceView {
    None,
    Permission(PermissionView),
    CommandPicker(PickerView),
    FilePicker(PickerView),
    Help(HelpView),
    ToolDetail(ToolDetailView),
    DiffDetail(DiffDetailView),
    TranscriptSearch(TranscriptSearchView),
    Queue(QueueView),
    TranscriptLens {
        selected_entry: Option<usize>,
        scroll: usize,
    },
    SetupForm(SetupFormView),
    McpTrust(McpTrustView),
    StructuredTable(TableView),
}

impl From<&App> for FocusedSurfaceView {
    fn from(app: &App) -> Self {
        if let Some(permission) = app.overlay.permission() {
            return FocusedSurfaceView::Permission(PermissionView {
                title: permission.title.clone(),
                scope: "ACP client · active tool request".to_string(),
                selected: permission.selected,
                options: permission
                    .options
                    .iter()
                    .map(|option| PermissionOptionView {
                        label: option.name.clone(),
                        kind: option.kind.label().to_string(),
                    })
                    .collect(),
            });
        }
        if let Some(form) = app.render_setup_form_view() {
            return FocusedSurfaceView::SetupForm(form);
        }
        if let Some(surface) = app.overlay.mcp_trust() {
            return FocusedSurfaceView::McpTrust(McpTrustView::from(surface));
        }
        if let Some(search) = app.overlay.transcript_search() {
            return FocusedSurfaceView::TranscriptSearch(TranscriptSearchView {
                query: search.query.text(),
                current: (!search.matches.is_empty()).then_some(search.selected + 1),
                total: search.matches.len(),
                truncated: search.truncated,
            });
        }
        if let Some(pane) = app.overlay.queue() {
            let items = app
                .composer
                .queue
                .items
                .iter()
                .map(|item| QueueItemView {
                    id: item.id.to_string(),
                    target: item.target.label().to_string(),
                    preview: item.preview(72),
                    created_at: item.created_at.clone(),
                    audit: match &item.audit {
                        QueueAuditState::Recorded => "recorded".to_string(),
                        QueueAuditState::Failed(_) => "audit failed".to_string(),
                    },
                    settlement: item.settlement.label().to_string(),
                })
                .collect();
            return FocusedSurfaceView::Queue(QueueView {
                items,
                selected: pane.selected,
                editing: pane.editing.as_ref().map(|input| input.text()),
            });
        }
        if let Some(detail) = app.overlay.detail()
            && let Some(Entry::Tool { name, status, output, .. }) = app.transcript.entries.get(detail.entry_index)
        {
            return FocusedSurfaceView::ToolDetail(ToolDetailView {
                entry_index: detail.entry_index,
                title: name.clone(),
                status: *status,
                scroll: detail.scroll,
                output: output
                    .iter()
                    .map(|line| super::tool_output::sanitize_terminal_text(line))
                    .collect(),
            });
        }
        match app.overlay.accessory() {
            PromptAccessory::Help => FocusedSurfaceView::Help(HelpView {
                scroll: app.overlay.help_scroll().unwrap_or_default(),
                bindings: app
                    .runtime
                    .keymap
                    .help_bindings(matches!(app.runtime.run_state, RunState::Working)),
            }),
            PromptAccessory::Commands { selected } => {
                let items = crate::app::command_suggestions_for_app(app)
                    .into_iter()
                    .map(|suggestion| PickerItemView { label: suggestion.name, detail: suggestion.detail })
                    .collect();
                FocusedSurfaceView::CommandPicker(PickerView {
                    title: "commands".to_string(),
                    query: app.composer.input.text(),
                    selected,
                    items,
                })
            }
            PromptAccessory::Files(_) => app
                .render_picker_surface("paths")
                .map_or(FocusedSurfaceView::None, FocusedSurfaceView::FilePicker),
            PromptAccessory::Models => app
                .render_picker_surface("models")
                .map_or(FocusedSurfaceView::None, FocusedSurfaceView::CommandPicker),
            PromptAccessory::ReasoningEffort => app
                .render_picker_surface("reasoning effort")
                .map_or(FocusedSurfaceView::None, FocusedSurfaceView::CommandPicker),
            PromptAccessory::Skills => app
                .render_picker_surface("skills")
                .map_or(FocusedSurfaceView::None, FocusedSurfaceView::CommandPicker),
            PromptAccessory::Sessions => app
                .render_picker_surface("sessions")
                .map_or(FocusedSurfaceView::None, FocusedSurfaceView::CommandPicker),
            PromptAccessory::Context => FocusedSurfaceView::StructuredTable(app.render_context_table()),
            _ => FocusedSurfaceView::None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TranscriptSearchView {
    pub query: String,
    pub current: Option<usize>,
    pub total: usize,
    pub truncated: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueueItemView {
    pub id: String,
    pub target: String,
    pub preview: String,
    pub created_at: String,
    pub audit: String,
    pub settlement: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueueView {
    pub items: Vec<QueueItemView>,
    pub selected: usize,
    pub editing: Option<String>,
}

/// A semantic ACP permission decision surface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PermissionView {
    pub title: String,
    pub scope: String,
    pub options: Vec<PermissionOptionView>,
    pub selected: usize,
}

/// A single ACP permission choice after provider-specific kinds are lowered to
/// display-safe semantic text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PermissionOptionView {
    pub label: String,
    pub kind: String,
}

/// Semantic help state for the one context-sensitive keyboard binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HelpView {
    pub scroll: usize,
    pub bindings: Vec<crate::app::KeyHelp>,
}

/// Table column alignment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColumnAlignment {
    Left,
    Right,
    Center,
}

/// Table column width policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColumnWidthPolicy {
    Fixed(usize),
    Percent(u8),
    Flexible,
}

/// Complete view of what the renderer should draw this tick.
pub struct RendererView {
    /// Semantic, backend-independent view records for renderer-owned UI state.
    pub semantic: SemanticUiView,
    pub transcript: TranscriptView,
    pub live: LiveView,
    pub width: usize,
    #[allow(dead_code)]
    pub height: usize,
}

impl RendererView {
    /// Build a pure data view from app state and terminal dimensions.
    pub fn build(app: &App, width: usize, height: usize) -> Self {
        let semantic = SemanticUiView::from(app);
        let transcript = TranscriptView::build(app, width);
        let live = LiveView::build(app, width, height, &transcript, &semantic);
        Self { semantic, transcript, live, width, height }
    }
}

mod app_projection;
mod semantic;
mod transcript;

pub use semantic::*;
pub use transcript::*;

/// Live chrome portion of the view: prompt, status, and accessory rows.
pub struct LiveView {
    /// Clipped mutable transcript tail rows.
    pub live_tail: Vec<Row>,
    /// Prompt input rows.
    pub prompt_rows: Vec<Row>,
    /// Cursor coordinate relative to the first prompt row.
    pub prompt_cursor: Option<CursorCoord>,
    /// Optional accessory rows (help, commands, file picker, etc.).
    pub accessory_rows: Vec<Row>,
    /// Summary row for queued steering/follow-up prompts, shown when non-empty.
    pub queued_summary: Option<Row>,
    /// Scrollable detail pane rows for the expanded tool entry.
    pub detail_pane: Vec<Row>,
    /// Static status row below the prompt.
    pub static_status: Row,
}

impl LiveView {
    pub fn build(
        app: &App, width: usize, height: usize, transcript: &TranscriptView, semantic: &SemanticUiView,
    ) -> LiveView {
        let live_tail = transcript.live_rows.clone();
        let (prompt_rows, prompt_cursor) = super::live::prompt_rows_for(app, width);
        let prompt_body_budget = super::live::MAX_PROMPT_ROWS.saturating_sub(super::live::composer_frame_height(width));
        let (prompt_rows, prompt_cursor) =
            clip_prompt_rows_around_cursor(prompt_rows, prompt_cursor, prompt_body_budget);
        let (prompt_rows, prompt_cursor) = super::live::frame_prompt_rows(app, width, prompt_rows, prompt_cursor);
        let min_prompt_chrome = prompt_rows.len() + 1;
        let keep_prompt_gutters = !transcript.live_rows.is_empty()
            && matches!(&semantic.focused_surface, FocusedSurfaceView::None)
            && height >= min_prompt_chrome + 3;
        let reserved_chrome = prompt_rows.len() + if keep_prompt_gutters { 3 } else { 1 };
        let accessory_limit = super::live::MAX_ACCESSORY_ROWS;
        let accessory_height = accessory_limit.min(height.saturating_sub(reserved_chrome));

        let accessory_rows = match &semantic.focused_surface {
            FocusedSurfaceView::ToolDetail(_) => Vec::new(),
            FocusedSurfaceView::DiffDetail(_)
            | FocusedSurfaceView::TranscriptSearch(_)
            | FocusedSurfaceView::Queue(_)
            | FocusedSurfaceView::TranscriptLens { .. } => Vec::new(),
            _ => super::live::accessory_rows(app, width, accessory_height),
        };
        let detail_pane = match &semantic.focused_surface {
            FocusedSurfaceView::ToolDetail(_) => Vec::new(),
            FocusedSurfaceView::DiffDetail(_)
            | FocusedSurfaceView::TranscriptSearch(_)
            | FocusedSurfaceView::Queue(_)
            | FocusedSurfaceView::TranscriptLens { .. } => super::surface::render_surface(&SurfaceRenderInput::new(
                &semantic.focused_surface,
                &SurfaceThemeView::new(),
                width,
                accessory_height,
            )),
            _ => Vec::new(),
        };

        LiveView {
            live_tail,
            prompt_rows,
            prompt_cursor,
            accessory_rows,
            queued_summary: super::live::queued_summary_row(app, width),
            detail_pane,
            static_status: super::status::status_row(app, width),
        }
    }
}

fn clip_prompt_rows_around_cursor(
    rows: Vec<Row>, cursor: Option<CursorCoord>, max_rows: usize,
) -> (Vec<Row>, Option<CursorCoord>) {
    if rows.len() <= max_rows || max_rows == 0 {
        return (rows, cursor);
    }

    let cursor_row = cursor.map_or_else(
        || rows.len().saturating_sub(1),
        |cursor| cursor.row.min(rows.len().saturating_sub(1)),
    );
    let start = cursor_row.saturating_add(1).saturating_sub(max_rows);
    let clipped_rows = rows.into_iter().skip(start).take(max_rows).collect();
    let clipped_cursor = cursor.map(|mut cursor| {
        cursor.row = cursor.row.saturating_sub(start);
        cursor
    });

    (clipped_rows, clipped_cursor)
}
