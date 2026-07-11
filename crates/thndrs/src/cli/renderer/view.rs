//! Pure renderer view projection.
//!
//! [`RendererView`] is a data-only staging area built from [`App`] plus
//! terminal dimensions. It contains no crossterm types and performs no terminal
//! writes. The view separates semantic row construction from viewport policy so
//! that [`super::region::LiveRegion`] can focus on scrollback commits, width
//! epochs, and frame composition.

use crate::app::{App, Entry, FilePickerSource, PromptAccessory, RunState, ToolStatus};
use crate::renderer::row::{CursorCoord, Row};

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

/// Transcript portion of the view: committed banner rows, stable rows, and rows
/// that are still mutable (streaming or running).
pub struct TranscriptView {
    /// Banner rows shown before the first transcript entry is committed.
    pub banner_rows: Vec<Row>,
    /// Rows that can be safely committed to native scrollback.
    pub stable_rows: Vec<Row>,
    /// Rows that must remain in the live viewport until the entry settles.
    pub live_rows: Vec<Row>,
}

/// Renderer-owned semantic view data.
///
/// These records intentionally avoid terminal backend types, crossterm style
/// types, and concrete row formatting. Direct row rendering and future bounded
/// surface adapters consume this projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticUiView {
    pub transcript: SemanticTranscriptView,
    pub prompt: PromptSurfaceView,
    pub orientation: OrientationBandView,
    pub focused_surface: FocusedSurfaceView,
}

impl SemanticUiView {
    pub fn new(app: &App) -> Self {
        SemanticUiView {
            transcript: SemanticTranscriptView { rows: app.transcript.iter().map(semantic_transcript_row).collect() },
            prompt: prompt_surface_view(app),
            orientation: orientation_band_view(app),
            focused_surface: focused_surface_view(app),
        }
    }
}

/// Semantic transcript records before terminal wrapping and styling.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SemanticTranscriptView {
    pub rows: Vec<TranscriptRowView>,
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

/// Tool execution state represented in renderer data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolStateView {
    pub name: String,
    pub arguments: String,
    pub status: ToolRunState,
    pub output_lines: usize,
    pub truncated_preview: bool,
}

/// Stable tool status labels for semantic UI records.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolRunState {
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

/// File edit summary inferred from write-capable tool entries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditSummaryView {
    pub path: Option<String>,
    pub operation: Option<String>,
    pub status: ToolRunState,
}

/// Diff summary inferred from tool output when unified-style diff lines exist.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffSummaryView {
    pub files: Vec<String>,
    pub added: usize,
    pub removed: usize,
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

/// Prompt suggestion family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PromptSuggestionKind {
    Command,
    FileMention,
}

/// Compact orientation/status band semantic data.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OrientationBandView {
    pub fields: Vec<OrientationFieldView>,
}

/// A truncatable orientation field.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrientationFieldView {
    pub label: String,
    pub value: String,
    pub priority: u8,
    pub truncate: TruncationPolicy,
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
    CommandPicker(PickerSurfaceView),
    FilePicker(PickerSurfaceView),
    Help,
    ToolDetail(ToolDetailView),
    DiffDetail(DiffDetailView),
    TranscriptLens {
        selected_entry: Option<usize>,
        scroll: usize,
    },
    SetupForm(SetupFormView),
    StructuredTable(TableView),
}

/// Semantic picker/list surface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PickerSurfaceView {
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
    pub status: ToolRunState,
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
    pub fields: Vec<SetupFieldView>,
    pub focus_index: usize,
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

/// Renderer adapter boundary for iocraft-backed or other bounded surfaces.
pub trait SurfaceRenderer {
    fn render_surface(&mut self, input: SurfaceRenderInput<'_>) -> Vec<Row>;
}

/// Live chrome portion of the view: prompt, status, and accessory rows.
pub struct LiveView {
    /// Clipped mutable transcript tail rows.
    pub live_tail: Vec<Row>,
    /// Dynamic status row above the prompt.
    pub dynamic_status: Row,
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

/// Build a pure data view from app state and terminal dimensions.
///
/// No crossterm types or terminal writes appear here. The returned view is the
/// input to [`super::region::LiveRegion::build_frame`].
pub fn build_view(app: &App, width: usize, height: usize) -> RendererView {
    let semantic = SemanticUiView::new(app);
    let transcript = build_transcript_view(app, width);
    let live = build_live_view(app, width, height, &transcript, &semantic);
    RendererView { semantic, transcript, live, width, height }
}

fn semantic_transcript_row(entry: &Entry) -> TranscriptRowView {
    match entry {
        Entry::User { text } => transcript_row(TranscriptRowKind::User, true, text.clone()),
        Entry::Agent { text, streaming } => transcript_row(TranscriptRowKind::Assistant, !streaming, text.clone()),
        Entry::Reasoning { text, streaming } => transcript_row(TranscriptRowKind::Reasoning, !streaming, text.clone()),
        Entry::Status { text } if text == "cancelled" => {
            transcript_row(TranscriptRowKind::Cancelled, true, text.clone())
        }
        Entry::Status { text } => transcript_row(TranscriptRowKind::Status, true, text.clone()),
        Entry::Error { text } => transcript_row(TranscriptRowKind::Error, true, text.clone()),
        Entry::Tool { name, arguments, status, output } => {
            let status = tool_run_state(*status);
            let diff = diff_summary(output);
            let edit = edit_summary(name, output, status);
            let kind = if diff.is_some() {
                TranscriptRowKind::Diff
            } else if edit.is_some() {
                TranscriptRowKind::Edit
            } else if status == ToolRunState::Cancelled {
                TranscriptRowKind::Cancelled
            } else {
                TranscriptRowKind::Tool
            };
            TranscriptRowView {
                kind,
                stable: status != ToolRunState::Running,
                primary: name.clone(),
                tool: Some(ToolStateView {
                    name: name.clone(),
                    arguments: arguments.clone(),
                    status,
                    output_lines: output.len(),
                    truncated_preview: output.len() > 6,
                }),
                edit,
                diff,
            }
        }
    }
}

fn transcript_row(kind: TranscriptRowKind, stable: bool, primary: String) -> TranscriptRowView {
    TranscriptRowView { kind, stable, primary, tool: None, edit: None, diff: None }
}

fn tool_run_state(status: ToolStatus) -> ToolRunState {
    match status {
        ToolStatus::Running => ToolRunState::Running,
        ToolStatus::Ok => ToolRunState::Succeeded,
        ToolStatus::Failed => ToolRunState::Failed,
        ToolStatus::Cancelled => ToolRunState::Cancelled,
    }
}

fn edit_summary(name: &str, output: &[String], status: ToolRunState) -> Option<EditSummaryView> {
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
        path: output.iter().find_map(|line| path_like_suffix(line)),
        operation: name.split('#').next().map(str::to_string),
        status,
    })
}

fn path_like_suffix(line: &str) -> Option<String> {
    line.rsplit_once(": ").map(|(_, path)| path.to_string()).or_else(|| {
        line.split_whitespace()
            .last()
            .filter(|part| part.contains('/'))
            .map(str::to_string)
    })
}

fn diff_summary(output: &[String]) -> Option<DiffSummaryView> {
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
        Some(DiffSummaryView { files, added, removed })
    }
}

fn prompt_surface_view(app: &App) -> PromptSurfaceView {
    let queued = queued_summary_view(app);
    let suggestions = prompt_suggestions(app);
    let has_draft = !app.input.is_empty();
    let status = match (&app.run_state, queued.is_some(), suggestions.is_empty(), has_draft) {
        (RunState::Error(_), _, _, true) => PromptStatusView::Retryable,
        (RunState::Error(_), _, _, false) => PromptStatusView::Failed,
        (RunState::Working, true, _, _) => PromptStatusView::Queued,
        (RunState::Working, false, _, _) | (RunState::Stopping, _, _, _) => PromptStatusView::Running,
        (_, _, false, _) => PromptStatusView::Suggesting,
        (_, _, _, true) => PromptStatusView::Drafting,
        _ => {
            if matches!(app.transcript.last(), Some(Entry::Status { text }) if text == "cancelled") {
                PromptStatusView::Cancelled
            } else {
                PromptStatusView::Idle
            }
        }
    };
    PromptSurfaceView {
        draft: app.input.text(),
        mode: if app.mode == crate::app::Mode::Command { PromptModeView::Command } else { PromptModeView::Prompt },
        status,
        queued,
        suggestions,
    }
}

fn queued_summary_view(app: &App) -> Option<QueuedSummaryView> {
    let steering_count = app.queued_steering.len();
    let followup_count = app.queued_followups.len();
    if steering_count == 0 && followup_count == 0 {
        None
    } else {
        Some(QueuedSummaryView { steering_count, followup_count, target: app.queue_target.label().to_string() })
    }
}

fn prompt_suggestions(app: &App) -> Vec<PromptSuggestionView> {
    match app.prompt_accessory {
        PromptAccessory::Commands { selected } => crate::app::command_suggestions_for_app(app)
            .into_iter()
            .enumerate()
            .map(|(index, (label, detail))| PromptSuggestionView {
                label: label.to_string(),
                detail: detail.to_string(),
                selected: index == selected,
                kind: PromptSuggestionKind::Command,
            })
            .collect(),
        PromptAccessory::Files(FilePickerSource::Mention { .. }) => {
            picker_suggestions(app, PromptSuggestionKind::FileMention)
        }
        _ => Vec::new(),
    }
}

fn picker_suggestions(app: &App, kind: PromptSuggestionKind) -> Vec<PromptSuggestionView> {
    app.picker
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

fn orientation_band_view(app: &App) -> OrientationBandView {
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

fn focused_surface_view(app: &App) -> FocusedSurfaceView {
    if let Some(form) = setup_form_view(app) {
        return FocusedSurfaceView::SetupForm(form);
    }
    if app.detail_pane.open
        && let Some(Entry::Tool { name, status, output, .. }) = app.transcript.get(app.detail_pane.entry_index)
    {
        return FocusedSurfaceView::ToolDetail(ToolDetailView {
            entry_index: app.detail_pane.entry_index,
            title: name.clone(),
            status: tool_run_state(*status),
            scroll: app.detail_pane.scroll,
            output: output.clone(),
        });
    }
    match app.prompt_accessory {
        PromptAccessory::Help => FocusedSurfaceView::Help,
        PromptAccessory::Commands { selected } => {
            let items = crate::app::command_suggestions_for_app(app)
                .into_iter()
                .map(|(label, detail)| PickerItemView { label: label.to_string(), detail: detail.to_string() })
                .collect();
            FocusedSurfaceView::CommandPicker(PickerSurfaceView {
                title: "commands".to_string(),
                query: app.input.text(),
                selected,
                items,
            })
        }
        PromptAccessory::Files(_) => {
            picker_surface(app, "files").map_or(FocusedSurfaceView::None, FocusedSurfaceView::FilePicker)
        }
        _ => FocusedSurfaceView::None,
    }
}

fn picker_surface(app: &App, title: &str) -> Option<PickerSurfaceView> {
    let picker = app.picker.as_ref()?;
    Some(PickerSurfaceView {
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

fn setup_form_view(app: &App) -> Option<SetupFormView> {
    let recovery = app.first_run_recovery.as_ref()?;
    let secret = !recovery.secret_input.is_empty();
    Some(SetupFormView {
        fields: vec![SetupFieldView {
            label: "credential".to_string(),
            value: if secret { "[hidden]".to_string() } else { String::new() },
            focused: true,
            secret: true,
            multiline: false,
            error: None,
        }],
        focus_index: 0,
        validation_errors: Vec::new(),
        submit_label: "submit".to_string(),
        cancel_label: "cancel".to_string(),
        complete: false,
    })
}

fn build_transcript_view(app: &App, width: usize) -> TranscriptView {
    let banner_rows = super::transcript::banner_rows(app, width);

    if app.transcript.is_empty() {
        return TranscriptView { banner_rows, stable_rows: Vec::new(), live_rows: Vec::new() };
    }

    let mut stable_rows = Vec::new();
    let mut live_rows = Vec::new();

    stable_rows.extend(banner_rows);

    let ctx = super::transcript::TranscriptRowContext {
        user_label: &app.user_label,
        cwd: &app.cwd,
        width,
        entry_index: None,
    };

    for (index, entry) in app.transcript.iter().enumerate() {
        // Submitted prompts remain in the transcript model for provider
        // context, session persistence, retry, and Up/Down recall. The live
        // transcript is an agent activity view, so do not echo the input a
        // second time after it leaves the prompt surface.
        if matches!(entry, Entry::User { .. }) {
            continue;
        }
        let mut entry_ctx = ctx.clone();
        entry_ctx.entry_index = Some(index);
        let (entry_stable, entry_live) = entry_stable_and_live_rows(entry, &entry_ctx);
        if entry_stable.is_empty() {
            live_rows.extend(entry_live);
        } else {
            stable_rows.extend(entry_stable);
            live_rows.extend(entry_live);
        }
    }

    TranscriptView { banner_rows: Vec::new(), stable_rows, live_rows }
}

fn build_live_view(
    app: &App, width: usize, _height: usize, transcript: &TranscriptView, semantic: &SemanticUiView,
) -> LiveView {
    let live_tail = transcript.live_rows.clone();
    let dynamic_status = super::live::dynamic_status_row(app, width);
    let (prompt_rows, prompt_cursor) = super::live::prompt_rows_for(app, width);
    let (prompt_rows, prompt_cursor) =
        clip_prompt_rows_around_cursor(prompt_rows, prompt_cursor, super::live::MAX_PROMPT_ROWS);

    let accessory_rows = if app.pending_permission.is_some() || app.first_run_recovery.is_some() {
        super::live::accessory_rows(app, width, super::live::MAX_ACCESSORY_ROWS)
    } else {
        match &semantic.focused_surface {
            FocusedSurfaceView::CommandPicker(_) | FocusedSurfaceView::FilePicker(_) | FocusedSurfaceView::Help => {
                super::adapter::render_surface(&SurfaceRenderInput::new(
                    &semantic.focused_surface,
                    &SurfaceThemeView::new(),
                    width,
                    super::live::MAX_ACCESSORY_ROWS,
                ))
            }
            FocusedSurfaceView::None
            | FocusedSurfaceView::ToolDetail(_)
            | FocusedSurfaceView::DiffDetail(_)
            | FocusedSurfaceView::TranscriptLens { .. }
            | FocusedSurfaceView::SetupForm(_)
            | FocusedSurfaceView::StructuredTable(_) => {
                super::live::accessory_rows(app, width, super::live::MAX_ACCESSORY_ROWS)
            }
        }
    };
    let queued_summary = super::live::queued_summary_row(app, width);
    let detail_pane = if app.detail_pane.open {
        super::live::detail_pane_rows(app, width, super::live::MAX_ACCESSORY_ROWS)
    } else {
        Vec::new()
    };
    let static_status = super::live::static_status_row(app, width);

    LiveView {
        live_tail,
        dynamic_status,
        prompt_rows,
        prompt_cursor,
        accessory_rows,
        queued_summary,
        detail_pane,
        static_status,
    }
}

/// Split a single entry into stable and live rows.
///
/// Streaming assistant/reasoning blocks and running tools are entirely live
/// until they finish. All other entries are fully stable.
fn entry_stable_and_live_rows(entry: &Entry, ctx: &super::transcript::TranscriptRowContext) -> (Vec<Row>, Vec<Row>) {
    let rows = super::transcript::entry_rows(entry, ctx);
    match entry {
        Entry::Agent { streaming: true, .. }
        | Entry::Reasoning { streaming: true, .. }
        | Entry::Tool { status: ToolStatus::Running, .. } => (Vec::new(), rows),
        _ => (rows, Vec::new()),
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

#[cfg(test)]
mod tests;
