use crate::TranscriptEntry;
use ratatui::text::Line;

use super::context::{ApprovalPromptContext, PatchDisplayContext, RenderContext, ToolCallContext, ToolResultContext};

impl<'a> super::TranscriptRenderer<'a> {
    /// Render a single transcript entry
    pub(super) fn render_entry(
        &self, entry: &TranscriptEntry, width: usize, ellipsis: &str, lines: &mut Vec<Line<'static>>,
    ) {
        match entry {
            TranscriptEntry::UserMessage { content } => self.render_user_message(content, width, lines),
            TranscriptEntry::ModelResponse { content, streaming } => {
                self.render_model_response(content, *streaming, ellipsis, width, lines)
            }
            TranscriptEntry::ToolCall {
                tool,
                arguments,
                risk,
                description,
                task_context,
                scope,
                classification_reasoning,
                detail_level,
            } => self.render_tool_call(ToolCallContext {
                tool,
                arguments,
                risk,
                description: description.as_deref(),
                task_context: task_context.as_deref(),
                scope: scope.as_deref(),
                classification_reasoning: classification_reasoning.as_deref(),
                rendering: RenderContext::new(width, *detail_level, lines, self.theme, self.options.animation_frame),
            }),
            TranscriptEntry::ToolResult { tool, result, success, error, exit_code, next_steps, detail_level } => self
                .render_tool_result(ToolResultContext {
                    tool,
                    result,
                    success: *success,
                    error: error.as_deref(),
                    exit_code: *exit_code,
                    next_steps: next_steps.as_ref(),
                    rendering: RenderContext::new(
                        width,
                        *detail_level,
                        lines,
                        self.theme,
                        self.options.animation_frame,
                    ),
                }),
            TranscriptEntry::PatchDisplay { patch_name, file_path, diff_content, hunk_labels, detail_level } => {
                self.render_patch_display(PatchDisplayContext {
                    patch_name,
                    file_path,
                    diff_content,
                    hunk_labels,
                    rendering: RenderContext::new(
                        width,
                        *detail_level,
                        lines,
                        self.theme,
                        self.options.animation_frame,
                    ),
                });
            }
            TranscriptEntry::ApprovalPrompt {
                action,
                risk,
                description,
                task_context,
                scope,
                risk_reasoning,
                decision,
                detail_level,
            } => self.render_approval_prompt(ApprovalPromptContext {
                action,
                risk,
                description: description.as_deref(),
                task_context: task_context.as_deref(),
                scope: scope.as_deref(),
                risk_reasoning: risk_reasoning.as_deref(),
                decision: *decision,
                rendering: RenderContext::new(width, *detail_level, lines, self.theme, self.options.animation_frame),
            }),
            TranscriptEntry::SystemMessage { content } => self.render_system_message(content, width, lines),
            TranscriptEntry::ErrorEntry { message, error_type, can_retry, context } => {
                self.render_error_entry(message, *error_type, *can_retry, context.as_deref(), width, lines)
            }
            TranscriptEntry::ThinkingIndicator { duration_secs } => {
                self.render_thinking_indicator(*duration_secs, lines)
            }
            TranscriptEntry::StatusLine { message, status_type } => {
                self.render_status_line(message, *status_type, lines)
            }
        }
    }
}
