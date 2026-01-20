use crate::theme::Theme;
use crate::transcript::ErrorType;
use crate::{syntax::SyntaxHighlighter, transcript::entry::CardDetailLevel};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const CODE_BLOCK_START: &str = "```";
const CODE_BLOCK_END: &str = "```";

/// Context for rendering tool call cards
struct ToolCallContext<'a> {
    tool: &'a str,
    arguments: &'a str,
    risk: &'a str,
    /// WHAT: Plain language description
    description: Option<&'a str>,
    /// WHY: Task context
    task_context: Option<&'a str>,
    /// SCOPE: Files/paths affected
    scope: Option<&'a str>,
    /// RISK: Classification reasoning
    classification_reasoning: Option<&'a str>,
    rendering: RenderContext<'a>,
}

/// Context for rendering tool result cards
struct ToolResultContext<'a> {
    tool: &'a str,
    result: &'a str,
    success: bool,
    error: Option<&'a str>,
    /// RESULT: Exit code
    exit_code: Option<i32>,
    /// RESULT: Next steps
    next_steps: Option<&'a Vec<String>>,
    rendering: RenderContext<'a>,
}

/// Context for rendering approval prompt cards
struct ApprovalPromptContext<'a> {
    action: &'a str,
    risk: &'a str,
    /// WHAT: Plain language description
    description: Option<&'a str>,
    /// WHY: Task context
    task_context: Option<&'a str>,
    /// SCOPE: Files/paths affected
    scope: Option<&'a str>,
    /// RISK: Risk reasoning
    risk_reasoning: Option<&'a str>,
    decision: Option<super::ApprovalDecision>,
    rendering: RenderContext<'a>,
}

/// Context for rendering patch display with hunk labels
struct PatchDisplayContext<'a> {
    patch_name: &'a str,
    file_path: &'a str,
    diff_content: &'a str,
    hunk_labels: &'a [Option<String>],
    rendering: RenderContext<'a>,
}

struct RenderContext<'a> {
    width: usize,
    detail_level: CardDetailLevel,
    lines: &'a mut Vec<Line<'static>>,
}

impl<'a> RenderContext<'a> {
    pub fn new(width: usize, detail_level: CardDetailLevel, lines: &'a mut Vec<Line<'static>>) -> Self {
        Self { width, detail_level, lines }
    }
}

/// Renders transcript entries to frame
pub struct TranscriptRenderer<'a> {
    transcript: &'a super::Transcript,
    scroll_vertical: u16,
}

impl<'a> TranscriptRenderer<'a> {
    /// Create a new renderer for given transcript
    pub fn new(transcript: &'a super::Transcript) -> Self {
        Self { transcript, scroll_vertical: 0 }
    }

    /// Create a new renderer with scroll offset
    pub fn with_vertical_scroll(transcript: &'a super::Transcript, scroll: u16) -> Self {
        Self { transcript, scroll_vertical: scroll }
    }

    /// Render transcript to the given area
    pub fn render(&self, frame: &mut Frame<'_>, area: Rect) {
        let entries = self.transcript.render_entries();
        let mut text_lines = Vec::new();
        let content_width = area.width.saturating_sub(4) as usize;

        for entry in entries {
            self.render_entry(entry, content_width, &mut text_lines);
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Theme::BORDER))
            .title(Span::styled("Transcript", Style::default().fg(Theme::BLUE)));

        let paragraph = Paragraph::new(Text::from(text_lines))
            .block(block)
            .wrap(Wrap { trim: true })
            .scroll((0, self.scroll_vertical));

        frame.render_widget(paragraph, area);
    }

    /// Render a single transcript entry
    fn render_entry(&self, entry: &super::TranscriptEntry, width: usize, lines: &mut Vec<Line<'static>>) {
        match entry {
            super::TranscriptEntry::UserMessage { content } => {
                self.render_user_message(content, width, lines);
            }
            super::TranscriptEntry::ModelResponse { content, streaming } => {
                self.render_model_response(content, *streaming, width, lines);
            }
            super::TranscriptEntry::ToolCall {
                tool,
                arguments,
                risk,
                description,
                task_context,
                scope,
                classification_reasoning,
                detail_level,
            } => {
                self.render_tool_call(ToolCallContext {
                    tool,
                    arguments,
                    risk,
                    description: description.as_deref(),
                    task_context: task_context.as_deref(),
                    scope: scope.as_deref(),
                    classification_reasoning: classification_reasoning.as_deref(),
                    rendering: RenderContext::new(width, *detail_level, lines),
                });
            }
            super::TranscriptEntry::ToolResult {
                tool,
                result,
                success,
                error,
                exit_code,
                next_steps,
                detail_level,
            } => {
                self.render_tool_result(ToolResultContext {
                    tool,
                    result,
                    success: *success,
                    error: error.as_deref(),
                    exit_code: *exit_code,
                    next_steps: next_steps.as_ref(),
                    rendering: RenderContext::new(width, *detail_level, lines),
                });
            }
            super::TranscriptEntry::PatchDisplay { patch_name, file_path, diff_content, hunk_labels, detail_level } => {
                self.render_patch_display(PatchDisplayContext {
                    patch_name,
                    file_path,
                    diff_content,
                    hunk_labels,
                    rendering: RenderContext::new(width, *detail_level, lines),
                });
            }
            super::TranscriptEntry::ApprovalPrompt {
                action,
                risk,
                description,
                task_context,
                scope,
                risk_reasoning,
                decision,
                detail_level,
            } => {
                self.render_approval_prompt(ApprovalPromptContext {
                    action,
                    risk,
                    description: description.as_deref(),
                    task_context: task_context.as_deref(),
                    scope: scope.as_deref(),
                    risk_reasoning: risk_reasoning.as_deref(),
                    decision: *decision,
                    rendering: RenderContext::new(width, *detail_level, lines),
                });
            }
            super::TranscriptEntry::SystemMessage { content } => {
                self.render_system_message(content, width, lines);
            }
            super::TranscriptEntry::ErrorEntry { message, error_type, can_retry, context } => {
                self.render_error_entry(message, *error_type, *can_retry, context.as_deref(), width, lines);
            }
        }
    }

    /// Render user message
    fn render_user_message(&self, content: &str, width: usize, lines: &mut Vec<Line<'static>>) {
        lines.push(Line::default());
        lines.push(Line::from(vec![
            Span::styled("You", Style::default().fg(Theme::GREEN)),
            Span::styled(": ", Style::default().fg(Theme::MUTED)),
        ]));
        self.wrap_text(content, Theme::FG, width, lines);
    }

    /// Render model response
    fn render_model_response(&self, content: &str, streaming: bool, width: usize, lines: &mut Vec<Line<'static>>) {
        lines.push(Line::default());
        let prefix = if streaming { "Agent (streaming...)" } else { "Agent" };
        lines.push(Line::from(vec![
            Span::styled(prefix, Style::default().fg(Theme::BLUE)),
            Span::styled(": ", Style::default().fg(Theme::MUTED)),
        ]));
        self.wrap_text(content, Theme::FG, width, lines);
    }

    /// Render tool call card with teaching context (WHAT, WHY, SCOPE, RISK)
    fn render_tool_call(&self, ctx: ToolCallContext) {
        let ToolCallContext {
            tool,
            arguments,
            risk,
            description,
            task_context,
            scope,
            classification_reasoning,
            rendering,
        } = ctx;

        rendering.lines.push(Line::default());
        let risk_color = super::TranscriptEntry::risk_level_color_str(risk);

        rendering.lines.push(Line::from(vec![
            Span::styled("🔧", Style::default().fg(Theme::YELLOW)),
            Span::raw(" "),
            Span::styled(tool.to_string(), Style::default().fg(Theme::FG)),
            Span::raw(" ["),
            Span::styled(risk.to_string(), Style::default().fg(risk_color)),
            Span::raw("]"),
        ]));

        match rendering.detail_level {
            CardDetailLevel::Brief => {
                if let Some(desc) = description {
                    rendering.lines.push(Line::from(vec![
                        Span::styled("  ", Style::default().fg(Theme::MUTED)),
                        Span::styled(desc.to_string(), Style::default().fg(Theme::FG)),
                    ]));
                }
            }
            CardDetailLevel::Detailed => {
                if let Some(desc) = description {
                    rendering.lines.push(Line::from(vec![
                        Span::styled("  ", Style::default().fg(Theme::MUTED)),
                        Span::styled("WHAT: ", Style::default().fg(Theme::CYAN).bold()),
                        Span::styled(desc.to_string(), Style::default().fg(Theme::FG)),
                    ]));
                }

                if let Some(ctx) = task_context {
                    rendering.lines.push(Line::from(vec![
                        Span::styled("  ", Style::default().fg(Theme::MUTED)),
                        Span::styled("WHY:  ", Style::default().fg(Theme::CYAN).bold()),
                        Span::styled(ctx.to_string(), Style::default().fg(Theme::FG)),
                    ]));
                }

                if let Some(sc) = scope {
                    rendering.lines.push(Line::from(vec![
                        Span::styled("  ", Style::default().fg(Theme::MUTED)),
                        Span::styled("SCOPE:", Style::default().fg(Theme::CYAN).bold()),
                    ]));
                    rendering.lines.push(Line::from(vec![
                        Span::styled("    ", Style::default().fg(Theme::MUTED)),
                        Span::styled(sc.to_string(), Style::default().fg(Theme::FG)),
                    ]));
                }

                rendering.lines.push(Line::from(vec![Span::styled(
                    "  Args: ",
                    Style::default().fg(Theme::MUTED),
                )]));
                self.wrap_text(arguments, Theme::FG, rendering.width, rendering.lines);
            }
            CardDetailLevel::Verbose => {
                if let Some(desc) = description {
                    rendering.lines.push(Line::from(vec![
                        Span::styled("  ", Style::default().fg(Theme::MUTED)),
                        Span::styled("WHAT: ", Style::default().fg(Theme::CYAN).bold()),
                        Span::styled(desc.to_string(), Style::default().fg(Theme::FG)),
                    ]));
                }

                if let Some(ctx) = task_context {
                    rendering.lines.push(Line::from(vec![
                        Span::styled("  ", Style::default().fg(Theme::MUTED)),
                        Span::styled("WHY:  ", Style::default().fg(Theme::CYAN).bold()),
                        Span::styled(ctx.to_string(), Style::default().fg(Theme::FG)),
                    ]));
                }

                if let Some(sc) = scope {
                    rendering.lines.push(Line::from(vec![
                        Span::styled("  ", Style::default().fg(Theme::MUTED)),
                        Span::styled("SCOPE:", Style::default().fg(Theme::CYAN).bold()),
                    ]));
                    rendering.lines.push(Line::from(vec![
                        Span::styled("    ", Style::default().fg(Theme::MUTED)),
                        Span::styled(sc.to_string(), Style::default().fg(Theme::FG)),
                    ]));
                }

                if let Some(reasoning) = classification_reasoning {
                    rendering.lines.push(Line::from(vec![
                        Span::styled("  ", Style::default().fg(Theme::MUTED)),
                        Span::styled("RISK:  ", Style::default().fg(Theme::YELLOW).bold()),
                        Span::styled(reasoning.to_string(), Style::default().fg(Theme::FG).italic()),
                    ]));
                }

                rendering.lines.push(Line::from(vec![Span::styled(
                    "  Args: ",
                    Style::default().fg(Theme::MUTED),
                )]));
                self.wrap_text(arguments, Theme::FG, rendering.width, rendering.lines);

                rendering.lines.push(Line::from(vec![Span::styled(
                    "  [verbose mode - full execution trace]",
                    Style::default().fg(Theme::MUTED),
                )]));
            }
        }
    }

    /// Render tool result card with RESULT fields (exit code, next steps)
    fn render_tool_result(&self, ctx: ToolResultContext) {
        let ToolResultContext { tool, result, success, error, exit_code, next_steps, rendering } = ctx;

        rendering.lines.push(Line::default());
        let status = if success { ("✅", Theme::GREEN) } else { ("❌", Theme::RED) };

        rendering.lines.push(Line::from(vec![
            Span::styled(status.0, Style::default().fg(status.1)),
            Span::raw(" "),
            Span::styled(tool.to_string(), Style::default().fg(Theme::FG)),
        ]));

        if let Some(err) = error {
            rendering.lines.push(Line::from(vec![
                Span::styled("  Error: ", Style::default().fg(Theme::RED)),
                Span::styled(err.to_string(), Style::default().fg(Theme::RED)),
            ]));
        }

        match rendering.detail_level {
            CardDetailLevel::Brief => {
                let preview = if let Some(first_line) = result.lines().next() {
                    if first_line.len() > 80 { format!("{}...", &first_line[..77]) } else { first_line.to_string() }
                } else {
                    String::new()
                };

                if !preview.is_empty() {
                    rendering.lines.push(Line::from(vec![
                        Span::styled("  ", Style::default().fg(Theme::MUTED)),
                        Span::styled(preview, Style::default().fg(Theme::FG)),
                    ]));
                }

                rendering.lines.push(Line::from(vec![Span::styled(
                    "  [output truncated - press 'v' for verbose]",
                    Style::default().fg(Theme::MUTED),
                )]));
            }
            CardDetailLevel::Detailed => {
                rendering
                    .lines
                    .push(Line::from(vec![Span::styled("  ", Style::default().fg(Theme::MUTED))]));
                self.render_with_code_highlighting(result, Theme::FG, rendering.width, rendering.lines);

                if let Some(code) = exit_code
                    && code != 0
                {
                    rendering.lines.push(Line::from(vec![
                        Span::styled("  ", Style::default().fg(Theme::MUTED)),
                        Span::styled("RESULT:", Style::default().fg(Theme::CYAN).bold()),
                        Span::styled(format!(" Exit code {}", code), Style::default().fg(Theme::RED).bold()),
                    ]));
                }

                if let Some(steps) = next_steps
                    && !steps.is_empty()
                {
                    rendering.lines.push(Line::from(vec![
                        Span::styled("  ", Style::default().fg(Theme::MUTED)),
                        Span::styled("RESULT:", Style::default().fg(Theme::CYAN).bold()),
                        Span::styled(" Next steps", Style::default().fg(Theme::FG)),
                    ]));
                    for step in steps {
                        rendering.lines.push(Line::from(vec![
                            Span::styled("    • ", Style::default().fg(Theme::MUTED)),
                            Span::styled(step.clone(), Style::default().fg(Theme::FG)),
                        ]));
                    }
                }
            }
            CardDetailLevel::Verbose => {
                rendering
                    .lines
                    .push(Line::from(vec![Span::styled("  ", Style::default().fg(Theme::MUTED))]));
                self.render_with_code_highlighting(result, Theme::FG, rendering.width, rendering.lines);

                rendering.lines.push(Line::from(vec![
                    Span::styled("  ", Style::default().fg(Theme::MUTED)),
                    Span::styled("RESULT:", Style::default().fg(Theme::CYAN).bold()),
                ]));
                if let Some(code) = exit_code {
                    let color = if code == 0 { Theme::GREEN } else { Theme::RED };
                    rendering.lines.push(Line::from(vec![
                        Span::styled("    Exit code: ", Style::default().fg(Theme::MUTED)),
                        Span::styled(code.to_string(), Style::default().fg(color).bold()),
                    ]));
                } else {
                    rendering.lines.push(Line::from(vec![
                        Span::styled("    Exit code: ", Style::default().fg(Theme::MUTED)),
                        Span::styled("unknown", Style::default().fg(Theme::MUTED)),
                    ]));
                }

                if let Some(steps) = next_steps
                    && !steps.is_empty()
                {
                    rendering.lines.push(Line::from(vec![
                        Span::styled("    ", Style::default().fg(Theme::MUTED)),
                        Span::styled("Next steps:", Style::default().fg(Theme::FG)),
                    ]));
                    for step in steps {
                        rendering.lines.push(Line::from(vec![
                            Span::styled("      • ", Style::default().fg(Theme::MUTED)),
                            Span::styled(step.clone(), Style::default().fg(Theme::FG)),
                        ]));
                    }
                }

                rendering.lines.push(Line::from(vec![Span::styled(
                    "  [verbose mode - full execution trace]",
                    Style::default().fg(Theme::MUTED),
                )]));
            }
        }
    }

    /// Render approval prompt card with teaching context (WHAT, WHY, SCOPE, RISK)
    fn render_approval_prompt(&self, ctx: ApprovalPromptContext) {
        let ApprovalPromptContext {
            action,
            risk,
            description,
            task_context,
            scope,
            risk_reasoning,
            decision,
            rendering,
        } = ctx;

        rendering.lines.push(Line::default());
        let risk_color = super::TranscriptEntry::risk_level_color_str(risk);

        rendering.lines.push(Line::from(vec![
            Span::styled("⚠️", Style::default().fg(Theme::YELLOW)),
            Span::raw(" "),
            Span::styled("Approval Required", Style::default().fg(risk_color)),
        ]));

        rendering.lines.push(Line::from(vec![Span::styled(
            "  Action: ",
            Style::default().fg(Theme::MUTED),
        )]));

        self.wrap_text(
            &format!("{} [{}]", action, risk),
            Theme::FG,
            rendering.width,
            rendering.lines,
        );

        match rendering.detail_level {
            CardDetailLevel::Brief => {
                if let Some(desc) = description {
                    rendering.lines.push(Line::from(vec![
                        Span::styled("  ", Style::default().fg(Theme::MUTED)),
                        Span::styled(desc.to_string(), Style::default().fg(Theme::FG)),
                    ]));
                }
            }
            CardDetailLevel::Detailed => {
                if let Some(desc) = description {
                    rendering.lines.push(Line::from(vec![
                        Span::styled("  ", Style::default().fg(Theme::MUTED)),
                        Span::styled("WHAT: ", Style::default().fg(Theme::CYAN).bold()),
                        Span::styled(desc.to_string(), Style::default().fg(Theme::FG)),
                    ]));
                }

                if let Some(ctx) = task_context {
                    rendering.lines.push(Line::from(vec![
                        Span::styled("  ", Style::default().fg(Theme::MUTED)),
                        Span::styled("WHY:  ", Style::default().fg(Theme::CYAN).bold()),
                        Span::styled(ctx.to_string(), Style::default().fg(Theme::FG)),
                    ]));
                }

                if let Some(sc) = scope {
                    rendering.lines.push(Line::from(vec![
                        Span::styled("  ", Style::default().fg(Theme::MUTED)),
                        Span::styled("SCOPE:", Style::default().fg(Theme::CYAN).bold()),
                    ]));
                    rendering.lines.push(Line::from(vec![
                        Span::styled("    ", Style::default().fg(Theme::MUTED)),
                        Span::styled(sc.to_string(), Style::default().fg(Theme::FG)),
                    ]));
                }
            }
            CardDetailLevel::Verbose => {
                if let Some(desc) = description {
                    rendering.lines.push(Line::from(vec![
                        Span::styled("  ", Style::default().fg(Theme::MUTED)),
                        Span::styled("WHAT: ", Style::default().fg(Theme::CYAN).bold()),
                        Span::styled(desc.to_string(), Style::default().fg(Theme::FG)),
                    ]));
                }

                if let Some(ctx) = task_context {
                    rendering.lines.push(Line::from(vec![
                        Span::styled("  ", Style::default().fg(Theme::MUTED)),
                        Span::styled("WHY:  ", Style::default().fg(Theme::CYAN).bold()),
                        Span::styled(ctx.to_string(), Style::default().fg(Theme::FG)),
                    ]));
                }

                if let Some(sc) = scope {
                    rendering.lines.push(Line::from(vec![
                        Span::styled("  ", Style::default().fg(Theme::MUTED)),
                        Span::styled("SCOPE:", Style::default().fg(Theme::CYAN).bold()),
                    ]));
                    rendering.lines.push(Line::from(vec![
                        Span::styled("    ", Style::default().fg(Theme::MUTED)),
                        Span::styled(sc.to_string(), Style::default().fg(Theme::FG)),
                    ]));
                }

                if let Some(reasoning) = risk_reasoning {
                    rendering.lines.push(Line::from(vec![
                        Span::styled("  ", Style::default().fg(Theme::MUTED)),
                        Span::styled("RISK:  ", Style::default().fg(Theme::YELLOW).bold()),
                        Span::styled(reasoning.to_string(), Style::default().fg(Theme::FG).italic()),
                    ]));
                }
            }
        }

        match decision {
            None => {
                rendering.lines.push(Line::default());
                rendering.lines.push(Line::from(vec![
                    Span::styled("  [", Style::default().fg(Theme::MUTED)),
                    Span::styled("y", Style::default().fg(Theme::GREEN).bold()),
                    Span::styled("] approve  ", Style::default().fg(Theme::MUTED)),
                    Span::styled("[", Style::default().fg(Theme::MUTED)),
                    Span::styled("n", Style::default().fg(Theme::RED).bold()),
                    Span::styled("] reject  ", Style::default().fg(Theme::MUTED)),
                    Span::styled("[", Style::default().fg(Theme::MUTED)),
                    Span::styled("c", Style::default().fg(Theme::YELLOW).bold()),
                    Span::styled("] cancel", Style::default().fg(Theme::MUTED)),
                ]));

                match rendering.detail_level {
                    CardDetailLevel::Brief => {}
                    CardDetailLevel::Detailed => {
                        rendering.lines.push(Line::from(vec![Span::styled(
                            "  [detailed mode - scope and risk assessment shown]",
                            Style::default().fg(Theme::MUTED),
                        )]));
                    }
                    CardDetailLevel::Verbose => {
                        rendering.lines.push(Line::from(vec![Span::styled(
                            "  [verbose mode - full approval context available]",
                            Style::default().fg(Theme::MUTED),
                        )]));
                    }
                }
            }
            Some(super::ApprovalDecision::Approved) => {
                rendering.lines.push(Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled("✓", Style::default().fg(Theme::GREEN)),
                    Span::styled(" Approved", Style::default().fg(Theme::GREEN)),
                ]));
            }
            Some(super::ApprovalDecision::Rejected) => {
                rendering.lines.push(Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled("✗", Style::default().fg(Theme::RED)),
                    Span::styled(" Rejected", Style::default().fg(Theme::RED)),
                ]));
            }
            Some(super::ApprovalDecision::Cancelled) => {
                rendering.lines.push(Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled("⏹", Style::default().fg(Theme::YELLOW)),
                    Span::styled(" Cancelled", Style::default().fg(Theme::YELLOW)),
                ]));
            }
        }
    }

    /// Render patch display with hunk-level intent labels
    fn render_patch_display(&self, ctx: PatchDisplayContext) {
        let PatchDisplayContext { patch_name, file_path, diff_content, hunk_labels, rendering } = ctx;

        rendering.lines.push(Line::default());

        rendering.lines.push(Line::from(vec![
            Span::styled("📝", Style::default().fg(Theme::CYAN)),
            Span::raw(" "),
            Span::styled("Patch: ", Style::default().fg(Theme::MUTED)),
            Span::styled(patch_name.to_string(), Style::default().fg(Theme::FG).bold()),
        ]));

        rendering.lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled("File: ", Style::default().fg(Theme::MUTED)),
            Span::styled(file_path.to_string(), Style::default().fg(Theme::CYAN)),
        ]));

        let mut hunk_index = 0;
        let mut in_hunk = false;
        let mut current_hunk_lines = Vec::new();

        for line in diff_content.lines() {
            if line.starts_with("@@") {
                if !current_hunk_lines.is_empty() && in_hunk {
                    if hunk_index < hunk_labels.len()
                        && let Some(label) = &hunk_labels[hunk_index]
                    {
                        rendering.lines.push(Line::default());
                        rendering.lines.push(Line::from(vec![
                            Span::styled("  ", Style::default()),
                            Span::styled("📋 ", Style::default()),
                            Span::styled(label.clone(), Style::default().fg(Theme::YELLOW).italic()),
                        ]));
                    }

                    for hunk_line in &current_hunk_lines {
                        rendering
                            .lines
                            .push(Line::from(vec![Span::raw(format!("  {}", hunk_line))]));
                    }
                    current_hunk_lines.clear();
                    hunk_index += 1;
                }

                in_hunk = true;
                current_hunk_lines.push(line.to_string());
            } else if in_hunk {
                current_hunk_lines.push(line.to_string());
            } else if line.starts_with("diff --git") || line.starts_with("index") {
                rendering.lines.push(Line::from(vec![Span::styled(
                    format!("  {}", line),
                    Style::default().fg(Theme::MUTED),
                )]));
            }
        }

        if !current_hunk_lines.is_empty() && in_hunk {
            if hunk_index < hunk_labels.len()
                && let Some(label) = &hunk_labels[hunk_index]
            {
                rendering.lines.push(Line::default());
                rendering.lines.push(Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled("📋 ", Style::default()),
                    Span::styled(label.clone(), Style::default().fg(Theme::YELLOW).italic()),
                ]));
            }

            for hunk_line in &current_hunk_lines {
                rendering
                    .lines
                    .push(Line::from(vec![Span::raw(format!("  {}", hunk_line))]));
            }
        }

        match rendering.detail_level {
            CardDetailLevel::Brief => {
                rendering.lines.push(Line::from(vec![Span::styled(
                    "  [press 'v' for detailed diff view]",
                    Style::default().fg(Theme::MUTED).italic(),
                )]));
            }
            CardDetailLevel::Detailed => {
                rendering.lines.push(Line::from(vec![Span::styled(
                    "  [detailed mode - full diff with intent labels]",
                    Style::default().fg(Theme::MUTED).italic(),
                )]));
            }
            CardDetailLevel::Verbose => {
                rendering.lines.push(Line::from(vec![Span::styled(
                    "  [verbose mode - all patch details shown]",
                    Style::default().fg(Theme::MUTED).italic(),
                )]));
            }
        }
    }

    /// Render text with code block highlighting
    fn render_with_code_highlighting(
        &self, text: &str, default_color: ratatui::style::Color, max_width: usize, lines: &mut Vec<Line<'static>>,
    ) {
        let mut remaining = text;

        while let Some(start_idx) = remaining.find(CODE_BLOCK_START) {
            if start_idx > 0 {
                let before_code = &remaining[..start_idx];
                self.wrap_text(before_code, default_color, max_width, lines);
            }

            let after_start = &remaining[start_idx + CODE_BLOCK_START.len()..];

            let lang_end_idx = after_start.find('\n').unwrap_or(after_start.len());
            let lang = after_start[..lang_end_idx].trim();

            let code_start = if lang_end_idx < after_start.len() { lang_end_idx + 1 } else { lang_end_idx };
            let code_block = &after_start[code_start..];

            if let Some(end_idx) = code_block.find(CODE_BLOCK_END) {
                let code = &code_block[..end_idx];
                remaining = &code_block[end_idx + CODE_BLOCK_END.len()..];

                self.render_code_block(code, lang, max_width, lines);
            } else {
                self.wrap_text(after_start, Theme::CYAN, max_width, lines);
                break;
            }
        }

        if !remaining.is_empty() {
            self.wrap_text(remaining, default_color, max_width, lines);
        }
    }

    /// Render a code block with syntax highlighting
    fn render_code_block(&self, code: &str, lang: &str, _max_width: usize, lines: &mut Vec<Line<'static>>) {
        let highlighter = SyntaxHighlighter::new();
        let highlighted = highlighter.highlight_code(code, lang.trim());

        for span in &highlighted {
            lines.push(Line::from(vec![Span::raw(format!("  {}", span.content))]));
        }

        if !highlighted.is_empty() && !code.lines().last().map(|l| l.trim().is_empty()).unwrap_or(true) {
            lines.push(Line::default());
        }
    }

    /// Render system message
    fn render_system_message(&self, content: &str, width: usize, lines: &mut Vec<Line<'static>>) {
        lines.push(Line::from(vec![
            Span::styled("[", Style::default().fg(Theme::MUTED)),
            Span::styled("System", Style::default().fg(Theme::PURPLE)),
            Span::styled("] ", Style::default().fg(Theme::MUTED)),
        ]));
        self.wrap_text(content, Theme::FG, width, lines);
    }

    /// Render error entry with retry hint
    fn render_error_entry(
        &self, message: &str, error_type: ErrorType, can_retry: bool, context: Option<&str>, _width: usize,
        lines: &mut Vec<Line<'static>>,
    ) {
        lines.push(Line::default());

        let (type_label, type_color) = match error_type {
            ErrorType::Provider => ("Provider", Theme::YELLOW),
            ErrorType::Network => ("Network", Theme::YELLOW),
            ErrorType::SessionWrite => ("Session", Theme::YELLOW),
            ErrorType::Terminal => ("Terminal", Theme::RED),
            ErrorType::Cancelled => ("Cancelled", Theme::MUTED),
            ErrorType::Other => ("Error", Theme::RED),
        };

        let message = message.to_string();
        lines.push(Line::from(vec![
            Span::styled("[", Style::default().fg(Theme::MUTED)),
            Span::styled(type_label, Style::default().fg(type_color)),
            Span::styled(" Error] ", Style::default().fg(Theme::MUTED)),
            Span::styled(message.clone(), Style::default().fg(Theme::RED)),
        ]));

        if let Some(ctx) = context {
            let ctx = ctx.to_string();
            lines.push(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(ctx, Style::default().fg(Theme::MUTED).italic()),
            ]));
        }

        if can_retry {
            lines.push(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled("Press R to retry", Style::default().fg(Theme::CYAN).italic()),
            ]));
        }
    }

    /// Wrap text into lines with proper word wrapping based on width
    ///
    /// This implementation:
    /// - Respects newlines in the source text
    /// - Wraps at word boundaries when possible
    /// - Breaks long words if they exceed width
    /// - Uses Unicode-aware width calculation
    /// - Smart wrapping for file paths and URLs
    fn wrap_text(&self, text: &str, color: ratatui::style::Color, max_width: usize, lines: &mut Vec<Line<'static>>) {
        if max_width == 0 {
            return;
        }

        for source_line in text.lines() {
            if source_line.is_empty() {
                lines.push(Line::default());
                continue;
            }

            if self.is_path_or_url(source_line) {
                self.smart_wrap_path(source_line, color, max_width, lines);
            } else {
                self.wrap_normal_text(source_line, color, max_width, lines);
            }
        }
    }

    /// Check if text looks like a file path or URL
    fn is_path_or_url(&self, text: &str) -> bool {
        text.starts_with('/')
            || text.starts_with("./")
            || text.starts_with("../")
            || text.starts_with("http://")
            || text.starts_with("https://")
            || text.starts_with("git@")
            || text.starts_with("file://")
    }

    /// Smart wrap for paths and URLs (prefer breaking at path separators)
    fn smart_wrap_path(
        &self, path: &str, color: ratatui::style::Color, max_width: usize, lines: &mut Vec<Line<'static>>,
    ) {
        if path.width() <= max_width {
            lines.push(Line::from(vec![Span::styled(
                path.to_string(),
                Style::default().fg(color),
            )]));
            return;
        }

        let mut remaining = path;
        while remaining.width() > max_width {
            if let Some(idx) = self.find_break_point(remaining, max_width) {
                let chunk = &remaining[..idx];
                lines.push(Line::from(vec![Span::styled(
                    chunk.to_string(),
                    Style::default().fg(color),
                )]));
                remaining = &remaining[idx..];
            } else {
                lines.push(Line::from(vec![Span::styled(
                    remaining.to_string(),
                    Style::default().fg(color),
                )]));
                break;
            }
        }

        if !remaining.is_empty() {
            lines.push(Line::from(vec![Span::styled(
                remaining.to_string(),
                Style::default().fg(color),
            )]));
        }
    }

    /// Find a good break point in path/URL (prefer /, ., etc.)
    fn find_break_point(&self, text: &str, max_width: usize) -> Option<usize> {
        let mut break_idx = None;
        for (i, ch) in text.char_indices() {
            if i > 0
                && i % max_width == 0
                && let Some(idx) = break_idx
            {
                return Some(idx);
            }
            if matches!(ch, '/' | '.' | '-' | '_') {
                break_idx = Some(i + ch.len_utf8());
            }
        }
        break_idx
    }

    /// Normal word-based text wrapping
    fn wrap_normal_text(
        &self, text: &str, color: ratatui::style::Color, max_width: usize, lines: &mut Vec<Line<'static>>,
    ) {
        let words: Vec<&str> = text.split_whitespace().collect();
        if words.is_empty() {
            lines.push(Line::default());
            return;
        }

        let mut current_line = String::new();
        let mut current_width = 0;

        for word in words {
            let word_width = word.width();
            let space_width = if current_line.is_empty() { 0 } else { 1 };

            if current_width + space_width + word_width > max_width {
                if !current_line.is_empty() {
                    lines.push(Line::from(vec![Span::styled(
                        current_line.clone(),
                        Style::default().fg(color),
                    )]));
                    current_line = String::new();
                    current_width = 0;
                }

                if word_width > max_width {
                    let chars = word.chars().peekable();
                    let mut chunk_width = 0;
                    let mut chunk = String::new();

                    for ch in chars {
                        let ch_width = ch.width().unwrap_or(0);

                        if chunk_width + ch_width > max_width {
                            lines.push(Line::from(vec![Span::styled(
                                chunk.clone(),
                                Style::default().fg(color),
                            )]));
                            chunk.clear();
                            chunk_width = 0;
                        }

                        chunk.push(ch);
                        chunk_width += ch_width;
                    }

                    if !chunk.is_empty() {
                        lines.push(Line::from(vec![Span::styled(
                            chunk.clone(),
                            Style::default().fg(color),
                        )]));
                    }
                    continue;
                }
            }
            if !current_line.is_empty() {
                current_line.push(' ');
                current_width += 1;
            }
            current_line.push_str(word);
            current_width += word_width;
        }

        if !current_line.is_empty() {
            lines.push(Line::from(vec![Span::styled(current_line, Style::default().fg(color))]));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::Transcript;

    #[test]
    fn test_renderer_new() {
        let transcript = Transcript::new();
        let _ = TranscriptRenderer::new(&transcript);
    }

    #[test]
    fn test_renderer_with_entries() {
        let mut transcript = Transcript::new();
        transcript.add_user_message("Hello");
        transcript.add_model_response("Hi there");
        let _ = TranscriptRenderer::new(&transcript);
    }

    #[test]
    fn test_wrap_text_basic() {
        let transcript = Transcript::new();
        let renderer = TranscriptRenderer::new(&transcript);
        let mut lines = Vec::new();

        renderer.wrap_text("Hello world", Theme::FG, 20, &mut lines);

        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].to_string(), "Hello world");
    }

    #[test]
    fn test_wrap_text_with_wrap() {
        let transcript = Transcript::new();
        let renderer = TranscriptRenderer::new(&transcript);
        let mut lines = Vec::new();
        renderer.wrap_text("This is a long line that should wrap", Theme::FG, 20, &mut lines);
        assert!(lines.len() > 1);
        assert!(lines[0].to_string().contains("This"));
    }

    #[test]
    fn test_wrap_text_empty() {
        let transcript = Transcript::new();
        let renderer = TranscriptRenderer::new(&transcript);
        let mut lines = Vec::new();
        renderer.wrap_text("", Theme::FG, 20, &mut lines);
        assert_eq!(lines.len(), 0);
    }

    #[test]
    fn test_wrap_text_newlines() {
        let transcript = Transcript::new();
        let renderer = TranscriptRenderer::new(&transcript);
        let mut lines = Vec::new();

        renderer.wrap_text("Line 1\nLine 2\nLine 3", Theme::FG, 20, &mut lines);

        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_wrap_text_zero_width() {
        let transcript = Transcript::new();
        let renderer = TranscriptRenderer::new(&transcript);
        let mut lines = Vec::new();
        renderer.wrap_text("Hello", Theme::FG, 0, &mut lines);
        assert_eq!(lines.len(), 0);
    }

    #[test]
    fn test_wrap_text_long_word() {
        let transcript = Transcript::new();
        let renderer = TranscriptRenderer::new(&transcript);
        let mut lines = Vec::new();
        renderer.wrap_text("supercalifragilisticexpialidocious", Theme::FG, 10, &mut lines);
        assert!(lines.len() > 1);
    }

    #[test]
    fn test_wrap_text_unicode() {
        let transcript = Transcript::new();
        let renderer = TranscriptRenderer::new(&transcript);
        let mut lines = Vec::new();
        renderer.wrap_text("Hello 世界 🌍", Theme::FG, 20, &mut lines);
        assert!(!lines.is_empty());
    }
}
