//! Backend-independent semantic records for transcript and focused surfaces.

use super::*;
use crate::renderer::tool_output;

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
            transcript: SemanticTranscriptView {
                rows: app.transcript.entries.blocks().map(TranscriptRowView::from).collect(),
            },
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
    pub block_id: Option<TranscriptBlockId>,
    pub block_kind: Option<TranscriptBlockKind>,
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
            Entry::Skill { name, path, token_estimate, context_percent, .. } => TranscriptRowKind::Skill.build_row(
                true,
                skill_activation_summary(name, path, *token_estimate, *context_percent),
            ),
            Entry::Status { text } if text == "cancelled" => TranscriptRowKind::Cancelled.build_row(true, text.clone()),
            Entry::Status { text } => TranscriptRowKind::Status.build_row(true, text.clone()),
            Entry::Error { text } => TranscriptRowKind::Error.build_row(true, text.clone()),
            Entry::Tool { name, arguments, status, output } => {
                let diff = DiffSummaryView::build(name, output);
                let edit = EditSummaryView::build(name, arguments, output, *status);
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
                    block_id: None,
                    block_kind: None,
                    kind,
                    stable: *status != ToolStatus::Running,
                    primary: name.clone(),
                    tool: Some(ToolStateView {
                        name: name.clone(),
                        arguments: arguments.clone(),
                        status: *status,
                        output_lines: output.len(),
                        truncated_preview: output.len() > 6,
                        action: None,
                        target: None,
                        target_state: None,
                        lifecycle: None,
                        result_state: None,
                    }),
                    edit,
                    diff,
                }
            }
        }
    }
}

pub(crate) fn skill_activation_summary(
    name: &str, path: &str, token_estimate: usize, context_percent: Option<u8>,
) -> String {
    let context = match context_percent {
        Some(0) => "<1% context".to_string(),
        Some(percent) => format!("{percent}% context"),
        None => "context unknown".to_string(),
    };
    format!(
        "{name} · ~{} tokens · {context} · {path}",
        compact_count(token_estimate)
    )
}

fn compact_count(count: usize) -> String {
    if count < 1_000 {
        return count.to_string();
    }
    if count < 10_000 {
        return format!("{:.1}k", count as f64 / 1_000.0);
    }
    format!("{}k", count.div_ceil(1_000))
}

impl From<TranscriptBlock<'_>> for TranscriptRowView {
    fn from(block: TranscriptBlock<'_>) -> Self {
        let mut row = TranscriptRowView::from(block.entry);
        row.block_id = Some(block.id.clone());
        row.block_kind = Some(block.kind);
        if let Some(tool) = row.tool.as_mut() {
            tool.action = block.action().map(str::to_string);
            tool.target = block.target().map(str::to_string);
            tool.target_state = block.target_state();
            tool.lifecycle = block.lifecycle();
            tool.result_state = block.result_state();
        }
        row
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
    pub action: Option<String>,
    pub target: Option<String>,
    pub target_state: Option<BlockContentState>,
    pub lifecycle: Option<ToolLifecycleState>,
    pub result_state: Option<BlockContentState>,
}

/// File edit summary inferred from write-capable tool entries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditSummaryView {
    pub path: Option<String>,
    pub operation: Option<String>,
    pub status: ToolStatus,
}

impl EditSummaryView {
    fn build(name: &str, arguments: &str, output: &[String], status: ToolStatus) -> Option<EditSummaryView> {
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
            path: super::super::transcript::edit_path_from_args(arguments).or_else(|| {
                output.iter().find_map(|line| {
                    line.rsplit_once(": ").map(|(_, path)| path.to_string()).or_else(|| {
                        line.split_whitespace()
                            .last()
                            .filter(|part| part.contains('/'))
                            .map(str::to_string)
                    })
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
    fn build(name: &str, output: &[String]) -> Option<Self> {
        let diff = tool_output::projected_diff(name, output)?;
        let (files, added, removed) = diff.summary();
        if added == 0 && removed == 0 && files.is_empty() {
            None
        } else {
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
        let has_draft = !app.composer.input.is_empty();
        let status = match (
            &app.runtime.run_state,
            queued.is_some(),
            suggestions.is_empty(),
            has_draft,
        ) {
            (RunState::Error(_), _, _, true) => PromptStatusView::Retryable,
            (RunState::Error(_), _, _, false) => PromptStatusView::Failed,
            (RunState::Working, true, _, _) => PromptStatusView::Queued,
            (RunState::Working, false, _, _) | (RunState::Stopping, _, _, _) => PromptStatusView::Running,
            (_, _, false, _) => PromptStatusView::Suggesting,
            (_, _, _, true) => PromptStatusView::Drafting,
            _ => match app.transcript.entries.last() {
                Some(Entry::Status { text }) if text == "cancelled" => PromptStatusView::Cancelled,
                _ => PromptStatusView::Idle,
            },
        };
        PromptSurfaceView {
            draft: app.composer.input.text(),
            mode: match app.composer.mode {
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
                value: app.runtime.cwd.display().to_string(),
                priority: 20,
                truncate: TruncationPolicy::EllipsizeMiddle,
            },
            OrientationFieldView {
                label: "model".to_string(),
                value: app.runtime.model.clone(),
                priority: 10,
                truncate: TruncationPolicy::EllipsizeEnd,
            },
            OrientationFieldView {
                label: "run".to_string(),
                value: app.status_label(),
                priority: 0,
                truncate: TruncationPolicy::Hide,
            },
            OrientationFieldView {
                label: "session".to_string(),
                value: app.run_label().to_string(),
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
        if app.runtime.ttft.is_pending() {
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
    pub attention: bool,
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
    pub primary: ThemeRole,
    pub secondary: ThemeRole,
    pub selection: ThemeRole,
    pub warning: ThemeRole,
    pub failure: ThemeRole,
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
            primary: ThemeRole::Primary,
            secondary: ThemeRole::Secondary,
            selection: ThemeRole::Selection,
            warning: ThemeRole::Warning,
            failure: ThemeRole::Failure,
            diff_added: ThemeRole::DiffAdded,
            diff_removed: ThemeRole::DiffRemoved,
        }
    }
}
