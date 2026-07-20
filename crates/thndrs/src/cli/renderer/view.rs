//! Pure renderer view projection.
//!
//! [`RendererView`] is a data-only staging area built from [`App`] plus
//! terminal dimensions. It contains no crossterm types and performs no terminal
//! writes. The view separates semantic row construction from viewport policy so
//! that [`super::alternate::AlternateViewport`] can focus on viewport policy,
//! projection caching, and frame composition.

#[cfg(test)]
mod tests;

use crate::app::{
    App, CONTEXT_INSPECTION_MAX_ITEMS, ChatGptOAuthMethod, Entry, FilePickerSource, FirstRunRecovery, Mode,
    PromptAccessory, RecoveryStage, RunState, ToolStatus,
};
use crate::cli::commands::setup::SetupProviderArg;
use crate::renderer::row::{CursorCoord, Row};
use crate::renderer::transcript::TranscriptRowContext;
use crate::tools::shell::redact_secrets;
use crate::utils;

/// Renderer adapter for iocraft-backed or other bounded surfaces.
pub trait SurfaceRenderer {
    fn render_surface(&mut self, input: SurfaceRenderInput<'_>) -> Vec<Row>;
}

/// A backend-neutral theme role.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThemeRole {
    Text,
    Muted,
    Selected,
    Warning,
    Error,
    DiffAdded,
    DiffRemoved,
}

/// A transcript row family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TranscriptRowKind {
    User,
    Assistant,
    Reasoning,
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
        TranscriptRowView { kind: self, stable, primary, tool: None, edit: None, diff: None }
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
    TranscriptLens {
        selected_entry: Option<usize>,
        scroll: usize,
    },
    SetupForm(SetupFormView),
    StructuredTable(TableView),
}

impl From<&App> for FocusedSurfaceView {
    fn from(app: &App) -> Self {
        if let Some(permission) = app.pending_permission.as_ref() {
            return FocusedSurfaceView::Permission(PermissionView {
                title: permission.title.clone(),
                scope: "local user · active tool only · no TUI sandbox".to_string(),
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
        if app.detail_pane.open
            && let Some(Entry::Tool { name, status, output, .. }) = app.transcript.get(app.detail_pane.entry_index)
        {
            return FocusedSurfaceView::ToolDetail(ToolDetailView {
                entry_index: app.detail_pane.entry_index,
                title: name.clone(),
                status: *status,
                scroll: app.detail_pane.scroll,
                output: output.clone(),
            });
        }
        match app.prompt_accessory {
            PromptAccessory::Help => {
                FocusedSurfaceView::Help(HelpView { queue_target_toggle: matches!(app.run_state, RunState::Working) })
            }
            PromptAccessory::Commands { selected } => {
                let items = crate::app::command_suggestions_for_app(app)
                    .into_iter()
                    .map(|suggestion| PickerItemView { label: suggestion.name, detail: suggestion.detail })
                    .collect();
                FocusedSurfaceView::CommandPicker(PickerView {
                    title: "commands".to_string(),
                    query: app.input.text(),
                    selected,
                    items,
                })
            }
            PromptAccessory::Files(_) => app
                .render_picker_surface("files")
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
            PromptAccessory::Context => FocusedSurfaceView::StructuredTable(app.render_context_table()),
            _ => FocusedSurfaceView::None,
        }
    }
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
    pub queue_target_toggle: bool,
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

/// Transcript portion of the view: committed banner rows, stable rows, and rows
/// that are still mutable (streaming or running).
#[derive(Clone)]
pub struct TranscriptView {
    /// Every transcript row in chronological application order.
    ///
    /// Unlike the stable/live partitions below, this projection never moves a
    /// settled entry ahead of an earlier mutable entry. Full-screen renderers
    /// should use this sequence as their source of truth.
    pub rows: Vec<Row>,
    /// Banner rows shown before the first transcript entry is committed.
    pub banner_rows: Vec<Row>,
    /// Settled rows retained for live-tail classification and compatibility
    /// snapshots.
    pub stable_rows: Vec<Row>,
    /// Rows that remain mutable until their entry settles.
    pub live_rows: Vec<Row>,
}

/// Project one transcript entry at a specific width.
///
/// Alternate-screen caching uses this boundary to invalidate a changing entry
/// without rebuilding settled entries above it.
pub fn project_transcript_entry(app: &App, entry_index: usize, width: usize) -> (Vec<Row>, Vec<Row>) {
    let Some(entry) = app.transcript.get(entry_index) else {
        return (Vec::new(), Vec::new());
    };
    let previous_was_tool = entry_index
        .checked_sub(1)
        .and_then(|index| app.transcript.get(index))
        .is_some_and(|entry| matches!(entry, Entry::Tool { .. }));
    TranscriptRowContext {
        user_label: &app.user_label,
        cwd: &app.cwd,
        width,
        entry_index: Some(entry_index),
        tool_group_start: !previous_was_tool,
    }
    .rows_for_entry_stable_and_live_rows(entry)
}

impl TranscriptView {
    fn build(app: &App, width: usize) -> Self {
        let banner_rows = app.render_banner_rows(width);

        if app.transcript.is_empty() {
            return Self { rows: banner_rows.clone(), banner_rows, stable_rows: Vec::new(), live_rows: Vec::new() };
        }

        let mut rows = banner_rows.clone();
        let mut stable_rows = Vec::new();
        let mut live_rows = Vec::new();
        stable_rows.extend(banner_rows);

        let ctx = TranscriptRowContext {
            user_label: &app.user_label,
            cwd: &app.cwd,
            width,
            entry_index: None,
            tool_group_start: true,
        };

        let mut previous_was_tool = false;
        for (index, entry) in app.transcript.iter().enumerate() {
            let mut entry_ctx = ctx.clone();
            entry_ctx.entry_index = Some(index);
            entry_ctx.tool_group_start = !previous_was_tool;
            let (entry_stable, entry_live) = entry_ctx.rows_for_entry_stable_and_live_rows(entry);
            rows.extend(entry_stable.iter().cloned());
            rows.extend(entry_live.iter().cloned());
            if entry_stable.is_empty() {
                live_rows.extend(entry_live);
            } else {
                stable_rows.extend(entry_stable);
                live_rows.extend(entry_live);
            }
            previous_was_tool = matches!(entry, Entry::Tool { .. });
        }

        Self { rows, banner_rows: Vec::new(), stable_rows, live_rows }
    }
}

/// Renderer-owned semantic view data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticUiView {
    pub transcript: SemanticTranscriptView,
    pub prompt: PromptSurfaceView,
    pub orientation: OrientationBandView,
    pub focused_surface: FocusedSurfaceView,
}

impl From<&App> for SemanticUiView {
    fn from(app: &App) -> Self {
        Self {
            transcript: SemanticTranscriptView { rows: app.transcript.iter().map(TranscriptRowView::from).collect() },
            prompt: PromptSurfaceView::from(app),
            orientation: OrientationBandView::from(app),
            focused_surface: FocusedSurfaceView::from(app),
        }
    }
}

/// Semantic transcript records before terminal wrapping and styling.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SemanticTranscriptView {
    pub rows: Vec<TranscriptRowView>,
}

/// A semantic transcript row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TranscriptRowView {
    pub kind: TranscriptRowKind,
    pub stable: bool,
    pub primary: String,
    pub tool: Option<ToolStateView>,
    pub edit: Option<EditSummaryView>,
    pub diff: Option<DiffSummaryView>,
}

impl From<&Entry> for TranscriptRowView {
    fn from(entry: &Entry) -> Self {
        match entry {
            Entry::User { text } => TranscriptRowKind::User.build_row(true, text.clone()),
            Entry::Agent { text, streaming } => TranscriptRowKind::Assistant.build_row(!streaming, text.clone()),
            Entry::Reasoning { text, streaming } => TranscriptRowKind::Reasoning.build_row(!streaming, text.clone()),
            Entry::Status { text } if text == "cancelled" => TranscriptRowKind::Cancelled.build_row(true, text.clone()),
            Entry::Status { text } => TranscriptRowKind::Status.build_row(true, text.clone()),
            Entry::Error { text } => TranscriptRowKind::Error.build_row(true, text.clone()),
            Entry::Tool { name, arguments, status, output } => {
                let diff = DiffSummaryView::build(output);
                let edit = EditSummaryView::build(name, output, *status);
                let kind = if diff.is_some() {
                    TranscriptRowKind::Diff
                } else if edit.is_some() {
                    TranscriptRowKind::Edit
                } else if *status == ToolStatus::Cancelled {
                    TranscriptRowKind::Cancelled
                } else {
                    TranscriptRowKind::Tool
                };
                TranscriptRowView {
                    kind,
                    stable: *status != ToolStatus::Running,
                    primary: name.clone(),
                    tool: Some(ToolStateView {
                        name: name.clone(),
                        arguments: arguments.clone(),
                        status: *status,
                        output_lines: output.len(),
                        truncated_preview: output.len() > 6,
                    }),
                    edit,
                    diff,
                }
            }
        }
    }
}

/// Tool execution state represented in renderer data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolStateView {
    pub name: String,
    pub arguments: String,
    pub status: ToolStatus,
    pub output_lines: usize,
    pub truncated_preview: bool,
}

/// File edit summary inferred from write-capable tool entries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditSummaryView {
    pub path: Option<String>,
    pub operation: Option<String>,
    pub status: ToolStatus,
}

impl EditSummaryView {
    fn build(name: &str, output: &[String], status: ToolStatus) -> Option<EditSummaryView> {
        let is_write_tool = ["create_file", "replace_range", "write_patch"]
            .iter()
            .any(|tool| name.starts_with(tool));
        if !is_write_tool
            && !output
                .iter()
                .any(|line| line.contains("wrote") || line.contains("replaced"))
        {
            return None;
        }
        Some(EditSummaryView {
            path: output.iter().find_map(|line| {
                line.rsplit_once(": ").map(|(_, path)| path.to_string()).or_else(|| {
                    line.split_whitespace()
                        .last()
                        .filter(|part| part.contains('/'))
                        .map(str::to_string)
                })
            }),
            operation: name.split('#').next().map(str::to_string),
            status,
        })
    }
}

/// Diff summary inferred from tool output when unified-style diff lines exist.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffSummaryView {
    pub files: Vec<String>,
    pub added: usize,
    pub removed: usize,
}

impl DiffSummaryView {
    fn build(output: &[String]) -> Option<Self> {
        let mut added = 0usize;
        let mut removed = 0usize;
        let mut files = Vec::new();
        for line in output {
            if let Some(path) = line.strip_prefix("+++ ") {
                files.push(path.trim_start_matches("b/").to_string());
            } else if line.starts_with('+') && !line.starts_with("+++") {
                added += 1;
            } else if line.starts_with('-') && !line.starts_with("---") {
                removed += 1;
            }
        }
        if added == 0 && removed == 0 && files.is_empty() {
            None
        } else {
            files.sort();
            files.dedup();
            Some(Self { files, added, removed })
        }
    }
}

/// Prompt surface semantic data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PromptSurfaceView {
    pub draft: String,
    pub mode: PromptModeView,
    pub status: PromptStatusView,
    pub queued: Option<QueuedSummaryView>,
    pub suggestions: Vec<PromptSuggestionView>,
}

impl From<&App> for PromptSurfaceView {
    fn from(app: &App) -> Self {
        let queued = app.render_queued_summary_view();
        let suggestions = app.render_prompt_suggestions();
        let has_draft = !app.input.is_empty();
        let status = match (&app.run_state, queued.is_some(), suggestions.is_empty(), has_draft) {
            (RunState::Error(_), _, _, true) => PromptStatusView::Retryable,
            (RunState::Error(_), _, _, false) => PromptStatusView::Failed,
            (RunState::Working, true, _, _) => PromptStatusView::Queued,
            (RunState::Working, false, _, _) | (RunState::Stopping, _, _, _) => PromptStatusView::Running,
            (_, _, false, _) => PromptStatusView::Suggesting,
            (_, _, _, true) => PromptStatusView::Drafting,
            _ => match app.transcript.last() {
                Some(Entry::Status { text }) if text == "cancelled" => PromptStatusView::Cancelled,
                _ => PromptStatusView::Idle,
            },
        };
        PromptSurfaceView {
            draft: app.input.text(),
            mode: match app.mode {
                Mode::Command => PromptModeView::Command,
                _ => PromptModeView::Prompt,
            },
            status,
            queued,
            suggestions,
        }
    }
}

/// Queued steering/follow-up summary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueuedSummaryView {
    pub steering_count: usize,
    pub followup_count: usize,
    pub target: String,
}

/// Prompt suggestion projected from command and file mention state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PromptSuggestionView {
    pub label: String,
    pub detail: String,
    pub selected: bool,
    pub kind: PromptSuggestionKind,
}

/// Compact orientation/status band semantic data.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OrientationBandView {
    pub fields: Vec<OrientationFieldView>,
}

impl From<&App> for OrientationBandView {
    fn from(app: &App) -> Self {
        let mut fields = vec![
            OrientationFieldView {
                label: "workspace".to_string(),
                value: app.cwd.display().to_string(),
                priority: 20,
                truncate: TruncationPolicy::EllipsizeMiddle,
            },
            OrientationFieldView {
                label: "model".to_string(),
                value: app.model.clone(),
                priority: 10,
                truncate: TruncationPolicy::EllipsizeEnd,
            },
            OrientationFieldView {
                label: "run".to_string(),
                value: app.status_label().to_string(),
                priority: 0,
                truncate: TruncationPolicy::Hide,
            },
            OrientationFieldView {
                label: "session".to_string(),
                value: if app.session_id.is_empty() { "thndrs".to_string() } else { app.session_id.clone() },
                priority: 15,
                truncate: TruncationPolicy::EllipsizeEnd,
            },
            OrientationFieldView {
                label: "trust".to_string(),
                value: "local user · workspace-contained tools · no TUI sandbox".to_string(),
                priority: 40,
                truncate: TruncationPolicy::Hide,
            },
        ];
        if app.ttft.is_pending() {
            fields.push(OrientationFieldView {
                label: "ttft".to_string(),
                value: "pending".to_string(),
                priority: 30,
                truncate: TruncationPolicy::Hide,
            });
        }
        OrientationBandView { fields }
    }
}

/// A truncatable orientation field.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrientationFieldView {
    pub label: String,
    pub value: String,
    pub priority: u8,
    pub truncate: TruncationPolicy,
}

/// Semantic picker/list surface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PickerView {
    pub title: String,
    pub query: String,
    pub selected: usize,
    pub items: Vec<PickerItemView>,
}

/// Semantic picker row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PickerItemView {
    pub label: String,
    pub detail: String,
}

/// Full tool-output detail surface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolDetailView {
    pub entry_index: usize,
    pub title: String,
    pub status: ToolStatus,
    pub scroll: usize,
    pub output: Vec<String>,
}

/// Unified-diff detail surface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffDetailView {
    pub summary: DiffSummaryView,
    pub lines: Vec<String>,
}

/// Setup/recovery form semantic state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupFormView {
    pub title: String,
    pub stage: String,
    pub status: String,
    pub details: Vec<String>,
    pub fields: Vec<SetupFieldView>,
    pub focus_index: usize,
    pub actions: Vec<PickerItemView>,
    pub selected: usize,
    pub validation_errors: Vec<String>,
    pub submit_label: String,
    pub cancel_label: String,
    pub complete: bool,
}

/// A setup/recovery form field.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupFieldView {
    pub label: String,
    pub value: String,
    pub focused: bool,
    pub secret: bool,
    pub multiline: bool,
    pub error: Option<String>,
}

/// Structured table semantic state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TableView {
    pub header: Vec<TableCellView>,
    pub rows: Vec<Vec<TableCellView>>,
    pub selected_row: Option<usize>,
    pub narrow_fallback: Vec<String>,
}

/// A table cell with alignment and width policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TableCellView {
    pub text: String,
    pub alignment: ColumnAlignment,
    pub width: ColumnWidthPolicy,
}

/// Adapter input for a bounded focused surface renderer.
pub struct SurfaceRenderInput<'a> {
    pub surface: &'a FocusedSurfaceView,
    pub theme: &'a SurfaceThemeView,
    pub width: usize,
    pub height: usize,
}

impl<'a> SurfaceRenderInput<'a> {
    pub fn new(surface: &'a FocusedSurfaceView, theme: &'a SurfaceThemeView, width: usize, height: usize) -> Self {
        Self { surface, theme, width, height }
    }
}

/// Semantic theme roles available to bounded surface adapters.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SurfaceThemeView {
    pub text: ThemeRole,
    pub muted: ThemeRole,
    pub selected: ThemeRole,
    pub warning: ThemeRole,
    pub error: ThemeRole,
    pub diff_added: ThemeRole,
    pub diff_removed: ThemeRole,
}

impl Default for SurfaceThemeView {
    fn default() -> Self {
        Self::new()
    }
}

impl SurfaceThemeView {
    pub fn new() -> Self {
        SurfaceThemeView {
            text: ThemeRole::Text,
            muted: ThemeRole::Muted,
            selected: ThemeRole::Selected,
            warning: ThemeRole::Warning,
            error: ThemeRole::Error,
            diff_added: ThemeRole::DiffAdded,
            diff_removed: ThemeRole::DiffRemoved,
        }
    }
}

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
        app: &App, width: usize, _height: usize, transcript: &TranscriptView, semantic: &SemanticUiView,
    ) -> LiveView {
        let live_tail = transcript.live_rows.clone();
        let (prompt_rows, prompt_cursor) = super::live::prompt_rows_for(app, width);
        let prompt_body_budget = super::live::MAX_PROMPT_ROWS.saturating_sub(super::live::composer_frame_height(width));
        let (prompt_rows, prompt_cursor) =
            clip_prompt_rows_around_cursor(prompt_rows, prompt_cursor, prompt_body_budget);
        let (prompt_rows, prompt_cursor) = super::live::frame_prompt_rows(app, width, prompt_rows, prompt_cursor);

        let accessory_rows = match &semantic.focused_surface {
            FocusedSurfaceView::ToolDetail(_)
            | FocusedSurfaceView::DiffDetail(_)
            | FocusedSurfaceView::TranscriptLens { .. } => Vec::new(),
            _ => super::live::accessory_rows(app, width, super::live::MAX_ACCESSORY_ROWS),
        };
        let detail_pane = match &semantic.focused_surface {
            FocusedSurfaceView::ToolDetail(_)
            | FocusedSurfaceView::DiffDetail(_)
            | FocusedSurfaceView::TranscriptLens { .. } => super::adapter::render_surface(&SurfaceRenderInput::new(
                &semantic.focused_surface,
                &SurfaceThemeView::new(),
                width,
                super::live::MAX_ACCESSORY_ROWS,
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
            static_status: super::live::static_status_row(app, width),
        }
    }
}

impl App {
    fn render_queued_summary_view(&self) -> Option<QueuedSummaryView> {
        let steering_count = self.queued_steering.len();
        let followup_count = self.queued_followups.len();
        if steering_count == 0 && followup_count == 0 {
            None
        } else {
            Some(QueuedSummaryView { steering_count, followup_count, target: self.queue_target.label().to_string() })
        }
    }

    fn render_prompt_suggestions(&self) -> Vec<PromptSuggestionView> {
        match self.prompt_accessory {
            PromptAccessory::Commands { selected } => crate::app::command_suggestions_for_app(self)
                .into_iter()
                .enumerate()
                .map(|(index, suggestion)| PromptSuggestionView {
                    label: suggestion.name,
                    detail: suggestion.detail,
                    selected: index == selected,
                    kind: PromptSuggestionKind::Command,
                })
                .collect(),
            PromptAccessory::Files(FilePickerSource::Mention { .. }) => {
                self.render_picker_suggestions(PromptSuggestionKind::FileMention)
            }
            _ => Vec::new(),
        }
    }

    fn render_picker_suggestions(&self, kind: PromptSuggestionKind) -> Vec<PromptSuggestionView> {
        self.picker
            .as_ref()
            .map(|picker| {
                picker
                    .matches
                    .iter()
                    .enumerate()
                    .map(|(index, item)| PromptSuggestionView {
                        label: item.label.clone(),
                        detail: item.detail.clone(),
                        selected: index == picker.selected,
                        kind,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn render_picker_surface(&self, title: &str) -> Option<PickerView> {
        let picker = self.picker.as_ref()?;
        Some(PickerView {
            title: title.to_string(),
            query: picker.query.clone(),
            selected: picker.selected,
            items: picker
                .matches
                .iter()
                .map(|item| PickerItemView { label: item.label.clone(), detail: item.detail.clone() })
                .collect(),
        })
    }

    fn render_setup_form_view(&self) -> Option<SetupFormView> {
        let recovery = self.first_run_recovery.as_ref()?;
        let (label, value, secret) = setup_field(recovery);
        let provider = recovery
            .provider
            .map(|provider| provider.label().to_string())
            .unwrap_or_else(|| "advanced / ACP".to_string());
        let stage = recovery.stage.label().to_string();
        let status = format!("{provider} · {stage}");
        let details = setup_details(recovery);
        Some(SetupFormView {
            title: "setup".to_string(),
            stage,
            status,
            details,
            fields: vec![SetupFieldView { label, value, focused: true, secret, multiline: false, error: None }],
            focus_index: 0,
            actions: setup_actions(recovery),
            selected: recovery.selected,
            validation_errors: Vec::new(),
            submit_label: if recovery.stage == RecoveryStage::EnterKey {
                "submit".to_string()
            } else {
                "continue".to_string()
            },
            cancel_label: "cancel".to_string(),
            complete: false,
        })
    }

    /// Project the context ledger into bounded table data owned by the renderer.
    pub fn render_context_table(&self) -> TableView {
        let Some(ledger) = &self.context_ledger else {
            return TableView {
                header: vec![TableCellView {
                    text: "context".to_string(),
                    alignment: ColumnAlignment::Left,
                    width: ColumnWidthPolicy::Flexible,
                }],
                rows: vec![vec![TableCellView {
                    text: "no ledger".to_string(),
                    alignment: ColumnAlignment::Left,
                    width: ColumnWidthPolicy::Flexible,
                }]],
                selected_row: None,
                narrow_fallback: vec!["context unavailable".to_string()],
            };
        };

        let review = self.last_compaction_review.map(|a| a.label()).unwrap_or("none");
        let counts = ledger.counts();
        let mut rows = vec![
            context_table_row(
                "budget",
                &format!("{} / {}", ledger.budget.used, ledger.budget.target),
                "tokens",
                "target",
            ),
            context_table_row(
                "source",
                ledger.budget.limits.source.label(),
                ledger.budget.limits.confidence.label(),
                "limits",
            ),
            context_table_row(
                "compaction",
                &format!("{} / {}", self.effective_compaction_policy().mode.label(), review),
                &counts.visible.to_string(),
                "review",
            ),
        ];
        rows.extend(
            ledger
                .items
                .iter()
                .take(crate::app::CONTEXT_INSPECTION_MAX_ITEMS)
                .map(|item| {
                    let details = crate::context::export::export_item(item);
                    vec![
                        TableCellView {
                            text: redact_context_display(&item.id),
                            alignment: ColumnAlignment::Left,
                            width: ColumnWidthPolicy::Percent(34),
                        },
                        TableCellView {
                            text: format!(
                                "{} / {} lifecycle:{} reason:{} prot:{} [{}] rec:{} repl:{} verify:{}",
                                item.kind.label(),
                                item.visibility.label(),
                                details.lifecycle.label(),
                                details.reason_code,
                                yes_no(details.protected),
                                context_protection_label(&details),
                                yes_no(details.recovery_available),
                                details.replacement.as_deref().unwrap_or("none"),
                                details.verification.as_deref().unwrap_or("none")
                            ),
                            alignment: ColumnAlignment::Left,
                            width: ColumnWidthPolicy::Percent(26),
                        },
                        TableCellView {
                            text: item.token_estimate.to_string(),
                            alignment: ColumnAlignment::Right,
                            width: ColumnWidthPolicy::Fixed(9),
                        },
                        TableCellView {
                            text: redact_context_display(&item.label),
                            alignment: ColumnAlignment::Left,
                            width: ColumnWidthPolicy::Flexible,
                        },
                    ]
                }),
        );

        let mut narrow_fallback = vec![
            format!("budget {} / {} tokens", ledger.budget.used, ledger.budget.target),
            format!(
                "limits {} ({})",
                ledger.budget.limits.source.label(),
                ledger.budget.limits.confidence.label()
            ),
            format!(
                "compaction {} review {}",
                self.effective_compaction_policy().mode.label(),
                review
            ),
            format!(
                "items {} visible {} pinned {} dropped {} archived {} blocked {}",
                ledger.items.len(),
                counts.visible,
                counts.pinned,
                counts.dropped,
                counts.archived,
                counts.blocked
            ),
        ];
        narrow_fallback.extend(ledger.items.iter().take(CONTEXT_INSPECTION_MAX_ITEMS).map(|item| {
            let details = crate::context::export::export_item(item);
            format!(
                "{} visibility {} lifecycle {} reason {} protected {} [{}] recovery {} replacement {} relations {}",
                redact_context_display(&item.id),
                item.visibility.label(),
                details.lifecycle.label(),
                details.reason_code,
                yes_no(details.protected),
                context_protection_label(&details),
                yes_no(details.recovery_available),
                details.replacement.as_deref().unwrap_or("none"),
                details
                    .relations
                    .iter()
                    .map(|relation| {
                        format!(
                            "{}:{}->{}:{}",
                            relation.kind.label(),
                            redact_context_display(&relation.id),
                            redact_context_display(&relation.target_id),
                            relation.status.label()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }));
        TableView {
            header: vec![
                TableCellView {
                    text: "context".to_string(),
                    alignment: ColumnAlignment::Left,
                    width: ColumnWidthPolicy::Percent(34),
                },
                TableCellView {
                    text: "visibility / lifecycle / protection / relations".to_string(),
                    alignment: ColumnAlignment::Left,
                    width: ColumnWidthPolicy::Percent(26),
                },
                TableCellView {
                    text: "tokens".to_string(),
                    alignment: ColumnAlignment::Right,
                    width: ColumnWidthPolicy::Fixed(9),
                },
                TableCellView {
                    text: "label".to_string(),
                    alignment: ColumnAlignment::Left,
                    width: ColumnWidthPolicy::Flexible,
                },
            ],
            rows,
            selected_row: None,
            narrow_fallback,
        }
    }
}

fn setup_details(recovery: &FirstRunRecovery) -> Vec<String> {
    let mut details = Vec::new();
    match recovery.stage {
        RecoveryStage::ChooseProvider => {
            details.push("Choose a provider before a model; no provider or model is assumed by setup.".to_string())
        }
        RecoveryStage::ModelSelection => details.push("Choose the model available for this provider.".to_string()),
        RecoveryStage::ModelConfigScope => {
            details.push("Optionally save the selected model to project or global config.".to_string())
        }
        RecoveryStage::MissingCredential => match recovery.provider {
            Some(SetupProviderArg::ChatgptCodex) => details.push(
                "Browser PKCE is the default. Device code is an explicit headless route; neither asks for an API key."
                    .to_string(),
            ),
            Some(SetupProviderArg::Umans) => details.push(
                "Create a Umans Code API key at app.umans.ai, enter it hidden, then choose global or project storage."
                    .to_string(),
            ),
            _ => details
                .push("The credential stays hidden and is written only after an explicit scope choice.".to_string()),
        },
        RecoveryStage::EnterKey => match recovery.provider {
            Some(SetupProviderArg::Umans) => details.push(
                "Input is hidden. Enter stores the Umans Code key at the chosen scope; Esc preserves the draft."
                    .to_string(),
            ),
            _ => details.push("Input is hidden. Enter continues; Esc preserves the draft.".to_string()),
        },
        RecoveryStage::ConfirmStore => details.push("Choose where the credential may be stored.".to_string()),
        RecoveryStage::Instructions => details.push(setup_instruction(recovery).to_string()),
        RecoveryStage::ChatGptOAuthRequesting => {
            details.push("Starting the selected ChatGPT OAuth method.".to_string())
        }
        RecoveryStage::ChatGptOAuthPolling => match recovery.chatgpt_oauth.as_ref() {
            Some(oauth) => {
                match oauth.method {
                    ChatGptOAuthMethod::Browser => {
                        details.push("Open or copy this authorization URL:".to_string());
                        if let Some(url) = oauth.authorization_url.as_deref() {
                            details.push(url.to_string());
                        }
                    }
                    _ => {
                        if let Some(code) = oauth.code.as_ref() {
                            let uri = code
                                .verification_uri
                                .as_deref()
                                .unwrap_or("https://auth.openai.com/codex/device");
                            details.push(format!("Open {uri} and enter code {}.", code.user_code));
                        }
                    }
                };
                details.push(oauth.status.clone());
            }
            None => details.push("Waiting for ChatGPT OAuth.".to_string()),
        },
        RecoveryStage::ChatGptOAuthPasteRedirect => {
            details.push("Paste the full browser redirect URL. Input is hidden.".to_string())
        }
        RecoveryStage::ChatGptOAuthFailed => details.push(
            recovery
                .chatgpt_oauth
                .as_ref()
                .map(|oauth| oauth.status.clone())
                .unwrap_or_else(|| "ChatGPT OAuth failed.".to_string()),
        ),
        RecoveryStage::LogoutConfirm => details.push("Remove the credential from the selected store.".to_string()),
        RecoveryStage::AcpMissing => {
            details.push("ACP models use ACP agent config, not provider API keys.".to_string())
        }
    }
    details
}

fn setup_actions(recovery: &FirstRunRecovery) -> Vec<PickerItemView> {
    let labels: Vec<String> = match recovery.stage {
        RecoveryStage::ChooseProvider => vec![
            "ChatGPT Codex".to_string(),
            "Umans".to_string(),
            "show setup instructions".to_string(),
        ],
        RecoveryStage::ModelSelection => recovery
            .provider
            .map(crate::app::setup_model_options)
            .unwrap_or_default()
            .into_iter()
            .map(|item| item.label)
            .collect(),
        RecoveryStage::ModelConfigScope => vec![
            "project config".to_string(),
            "global config".to_string(),
            "skip model config".to_string(),
            "cancel setup".to_string(),
        ],
        RecoveryStage::MissingCredential => {
            if recovery.provider == Some(SetupProviderArg::ChatgptCodex) {
                vec![
                    "start browser PKCE login".to_string(),
                    "use headless device code".to_string(),
                    "switch model/provider".to_string(),
                    "show setup instructions".to_string(),
                    "continue without setup".to_string(),
                    "quit".to_string(),
                ]
            } else {
                vec![
                    match recovery.provider {
                        Some(SetupProviderArg::Umans) => "enter Umans API key".to_string(),
                        _ => "enter API key".to_string(),
                    },
                    "switch model/provider".to_string(),
                    "show setup instructions".to_string(),
                    "continue without setup".to_string(),
                    "quit".to_string(),
                ]
            }
        }
        RecoveryStage::EnterKey => Vec::new(),
        RecoveryStage::ConfirmStore | RecoveryStage::LogoutConfirm => vec![
            "global credentials".to_string(),
            "project credentials".to_string(),
            "cancel".to_string(),
        ],
        RecoveryStage::Instructions => vec!["back".to_string(), "close".to_string()],
        RecoveryStage::ChatGptOAuthRequesting | RecoveryStage::ChatGptOAuthPasteRedirect => vec!["cancel".to_string()],
        RecoveryStage::ChatGptOAuthPolling => {
            if recovery
                .chatgpt_oauth
                .as_ref()
                .is_some_and(|oauth| oauth.method == ChatGptOAuthMethod::Browser)
            {
                vec!["cancel".to_string(), "paste full redirect URL".to_string()]
            } else {
                vec!["cancel".to_string()]
            }
        }
        RecoveryStage::ChatGptOAuthFailed => vec![
            "retry browser PKCE".to_string(),
            "use headless device code".to_string(),
            "back".to_string(),
        ],
        RecoveryStage::AcpMissing => vec![
            "switch model/provider".to_string(),
            "show ACP setup".to_string(),
            "continue without setup".to_string(),
            "quit".to_string(),
        ],
    };
    labels
        .into_iter()
        .map(|label| PickerItemView { detail: String::new(), label })
        .collect()
}

fn setup_instruction(recovery: &FirstRunRecovery) -> &'static str {
    match recovery.provider {
        Some(arg) => match arg {
            SetupProviderArg::ChatgptCodex => {
                "Run `thndrs setup --provider chatgpt-codex` or `thndrs login chatgpt-codex` outside the TUI."
            }
            SetupProviderArg::Umans => {
                "Create a key at app.umans.ai, then run `thndrs login umans`; thndrs stores it in credentials.env, never TOML."
            }
            _ => "Run `thndrs setup` or `thndrs login <provider>` outside the TUI.",
        },
        None => "Advanced providers remain available through `thndrs setup` or ACP configuration.",
    }
}

fn setup_field(recovery: &FirstRunRecovery) -> (String, String, bool) {
    match recovery.stage {
        RecoveryStage::ChooseProvider => ("provider".to_string(), "choose provider".to_string(), false),
        RecoveryStage::ModelSelection => (
            "model".to_string(),
            recovery
                .provider
                .map(crate::app::setup_model_options)
                .and_then(|options| options.get(recovery.selected).map(|item| item.label.clone()))
                .unwrap_or_else(|| "choose model".to_string()),
            false,
        ),
        RecoveryStage::ModelConfigScope => (
            "config".to_string(),
            match recovery.selected {
                0 => "project config".to_string(),
                1 => "global config".to_string(),
                2 => "skip model config".to_string(),
                _ => "cancel setup".to_string(),
            },
            false,
        ),
        RecoveryStage::EnterKey => (
            recovery
                .provider
                .map(|provider| format!("{} API key", provider.label()))
                .unwrap_or_else(|| "API key".to_string()),
            if recovery.secret_input.is_empty() { String::new() } else { "[hidden]".to_string() },
            true,
        ),
        RecoveryStage::MissingCredential => (
            "provider".to_string(),
            recovery
                .provider
                .map_or_else(|| "advanced / ACP".to_string(), |provider| provider.label().to_string()),
            false,
        ),
        RecoveryStage::ConfirmStore => (
            "credential scope".to_string(),
            match recovery.selected {
                0 => "global credentials".to_string(),
                1 => "project credentials".to_string(),
                _ => "cancel".to_string(),
            },
            false,
        ),
        RecoveryStage::Instructions => ("next".to_string(), "follow setup instructions".to_string(), false),
        RecoveryStage::ChatGptOAuthRequesting | RecoveryStage::ChatGptOAuthPolling => {
            ("provider".to_string(), "ChatGPT OAuth".to_string(), false)
        }
        RecoveryStage::ChatGptOAuthPasteRedirect => (
            "redirect URL".to_string(),
            if recovery.secret_input.is_empty() { String::new() } else { "[hidden]".to_string() },
            true,
        ),
        RecoveryStage::ChatGptOAuthFailed => ("provider".to_string(), "ChatGPT OAuth failed".to_string(), false),
        RecoveryStage::LogoutConfirm => (
            "credential scope".to_string(),
            match recovery.selected {
                0 => "global credentials",
                1 => "project credentials",
                _ => "cancel",
            }
            .to_string(),
            false,
        ),
        RecoveryStage::AcpMissing => ("provider".to_string(), "ACP agent config".to_string(), false),
    }
}

fn context_table_row(name: &str, state: &str, tokens: &str, label: &str) -> Vec<TableCellView> {
    vec![
        TableCellView {
            text: name.to_string(),
            alignment: ColumnAlignment::Left,
            width: ColumnWidthPolicy::Percent(34),
        },
        TableCellView {
            text: state.to_string(),
            alignment: ColumnAlignment::Left,
            width: ColumnWidthPolicy::Percent(26),
        },
        TableCellView {
            text: tokens.to_string(),
            alignment: ColumnAlignment::Right,
            width: ColumnWidthPolicy::Fixed(9),
        },
        TableCellView { text: label.to_string(), alignment: ColumnAlignment::Left, width: ColumnWidthPolicy::Flexible },
    ]
}

fn redact_context_display(value: &str) -> String {
    utils::truncate_ellipsis(&redact_secrets(value), 160)
}

fn context_protection_label(item: &crate::context::export::ExportContextItem) -> String {
    if item.protection_released {
        return "released".to_string();
    }
    let labels = item.protection.labels();
    if labels.is_empty() { "none".to_string() } else { labels.join(",") }
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
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
