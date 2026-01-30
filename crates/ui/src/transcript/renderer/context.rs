use crate::theme::ThemePalette;
use crate::transcript::entry::CardDetailLevel;
use ratatui::text::Line;
use thunderus_core::ApprovalDecision;

/// Context for rendering tool call cards
pub(super) struct ToolCallContext<'a> {
    pub(super) tool: &'a str,
    pub(super) arguments: &'a str,
    pub(super) risk: &'a str,
    /// WHAT: Plain language description
    pub(super) description: Option<&'a str>,
    /// WHY: Task context
    pub(super) task_context: Option<&'a str>,
    /// SCOPE: Files/paths affected
    pub(super) scope: Option<&'a str>,
    /// RISK: Classification reasoning
    pub(super) classification_reasoning: Option<&'a str>,
    pub(super) rendering: RenderContext<'a>,
}

/// Context for rendering tool result cards
pub(super) struct ToolResultContext<'a> {
    pub(super) tool: &'a str,
    pub(super) result: &'a str,
    pub(super) success: bool,
    pub(super) error: Option<&'a str>,
    /// RESULT: Exit code
    pub(super) exit_code: Option<i32>,
    /// RESULT: Next steps
    pub(super) next_steps: Option<&'a Vec<String>>,
    pub(super) rendering: RenderContext<'a>,
}

/// Context for rendering approval prompt cards
pub(super) struct ApprovalPromptContext<'a> {
    pub(super) action: &'a str,
    pub(super) risk: &'a str,
    /// WHAT: Plain language description
    pub(super) description: Option<&'a str>,
    /// WHY: Task context
    pub(super) task_context: Option<&'a str>,
    /// SCOPE: Files/paths affected
    pub(super) scope: Option<&'a str>,
    /// RISK: Risk reasoning
    pub(super) risk_reasoning: Option<&'a str>,
    pub(super) decision: Option<ApprovalDecision>,
    pub(super) rendering: RenderContext<'a>,
}

/// Context for rendering patch display with hunk labels
pub(super) struct PatchDisplayContext<'a> {
    pub(super) patch_name: &'a str,
    pub(super) file_path: &'a str,
    pub(super) diff_content: &'a str,
    pub(super) hunk_labels: &'a [Option<String>],
    pub(super) rendering: RenderContext<'a>,
}

pub(super) struct RenderContext<'a> {
    pub(super) width: usize,
    pub(super) detail_level: CardDetailLevel,
    pub(super) lines: &'a mut Vec<Line<'static>>,
    pub(super) compact_mode: bool,
    pub(super) theme: ThemePalette,
    pub(super) animation_frame: u8,
}

impl<'a> RenderContext<'a> {
    pub(super) fn new(
        width: usize, detail_level: CardDetailLevel, lines: &'a mut Vec<Line<'static>>, theme: ThemePalette,
        animation_frame: u8,
    ) -> Self {
        let compact_mode = width < 80;
        Self { width, detail_level, lines, compact_mode, theme, animation_frame }
    }

    /// Check if we should render compact (single-line) cards
    pub(super) fn is_compact(&self) -> bool {
        self.compact_mode
    }
}
