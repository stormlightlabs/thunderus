use super::context::{ApprovalPromptContext, PatchDisplayContext, ToolCallContext, ToolResultContext};
use crate::{TranscriptEntry, transcript::entry::CardDetailLevel};

use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};
use thunderus_core::ApprovalDecision;
use unicode_width::UnicodeWidthStr;

impl<'a> super::TranscriptRenderer<'a> {
    pub(super) fn render_card(
        &self, title: &str, border_color: Color, width: usize, content: Vec<Line<'static>>,
        lines: &mut Vec<Line<'static>>,
    ) {
        let card_width = width.max(20).min(width);
        let border_style = Style::default().fg(border_color).bg(self.theme.panel_bg);
        let content_bg = Style::default().bg(self.theme.panel_bg);
        let padding = 2usize;
        let prefix = "┌─ ";
        let title_width = title.width();
        let base_len = prefix.width() + title_width + 1;
        let fill_len = card_width.saturating_sub(base_len + 1);

        lines.push(Line::from(vec![
            Span::styled(prefix, border_style),
            Span::styled(title.to_string(), border_style),
            Span::styled(" ", border_style),
            Span::styled("─".repeat(fill_len), border_style),
            Span::styled("┐", border_style),
        ]));

        let mut padded_content = Vec::with_capacity(content.len() + 2);
        padded_content.push(Line::default());
        padded_content.extend(content);
        padded_content.push(Line::default());

        for line in padded_content {
            let line_width = line.to_string().width();
            let inner_width = card_width.saturating_sub(2);
            let content_width = inner_width.saturating_sub(padding * 2);
            let padding_needed = content_width.saturating_sub(line_width);
            let mut spans = Vec::new();
            spans.push(Span::styled("│  ", border_style));
            spans.extend(line.spans);
            spans.push(Span::styled(" ".repeat(padding_needed), content_bg));
            spans.push(Span::styled("  │", border_style));
            lines.push(Line::from(spans));
        }

        lines.push(Line::from(vec![
            Span::styled("└", border_style),
            Span::styled("─".repeat(card_width.saturating_sub(2)), border_style),
            Span::styled("┘", border_style),
        ]));
    }

    /// Render tool call as compact inline format
    ///
    /// Format: `* ToolName "args" (result summary)`
    pub(super) fn render_tool_call(&self, ctx: ToolCallContext) {
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

        let theme = rendering.theme;
        let risk_color = TranscriptEntry::risk_level_color(theme, risk);

        if matches!(rendering.detail_level, CardDetailLevel::Brief) {
            let info = description
                .or(scope)
                .map(|s| if s.len() > 50 { format!("{}...", &s[..47]) } else { s.to_string() })
                .unwrap_or_else(|| {
                    if arguments.contains("path") {
                        arguments
                            .split('"')
                            .nth(1)
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| arguments.chars().take(40).collect())
                    } else {
                        arguments.chars().take(40).collect()
                    }
                });

            rendering.lines.push(Line::from(vec![
                Span::styled("* ", Style::default().fg(theme.yellow)),
                Span::styled(tool.to_string(), Style::default().fg(theme.fg).bold()),
                Span::styled(format!(" \"{}\"", info), Style::default().fg(theme.muted)),
            ]));
            return;
        }

        let content_width = rendering.width.saturating_sub(6);
        let content_style = Style::default().fg(theme.fg).bg(theme.panel_bg);
        let label_style = Style::default().fg(theme.cyan).bg(theme.panel_bg).bold();
        let muted_style = Style::default().fg(theme.muted).bg(theme.panel_bg);

        let mut content_lines: Vec<Line<'static>> = Vec::new();

        content_lines.push(Line::from(vec![
            Span::styled("Risk: ", muted_style),
            Span::styled(
                risk.to_string(),
                Style::default().fg(risk_color).bg(theme.panel_bg).bold(),
            ),
        ]));

        if let Some(desc) = description {
            content_lines.push(Line::from(vec![
                Span::styled("WHAT: ", label_style),
                Span::styled(desc.to_string(), content_style),
            ]));
        }

        if let Some(ctx) = task_context {
            content_lines.push(Line::from(vec![
                Span::styled("WHY:  ", label_style),
                Span::styled(ctx.to_string(), content_style),
            ]));
        }

        if let Some(sc) = scope {
            content_lines.push(Line::from(vec![Span::styled("SCOPE:", label_style)]));
            self.wrap_text_styled(sc, content_style, content_width, &mut content_lines);
        }

        if matches!(rendering.detail_level, CardDetailLevel::Verbose)
            && let Some(reasoning) = classification_reasoning
        {
            content_lines.push(Line::from(vec![
                Span::styled("RISK:  ", Style::default().fg(theme.yellow).bg(theme.panel_bg).bold()),
                Span::styled(
                    reasoning.to_string(),
                    Style::default().fg(theme.fg).bg(theme.panel_bg).italic(),
                ),
            ]));
        }

        content_lines.push(Line::from(vec![Span::styled("Args:", muted_style)]));
        self.wrap_text_styled(arguments, content_style, content_width, &mut content_lines);

        let title = format!("Tool: {}", tool);
        self.render_card(&title, risk_color, rendering.width, content_lines, rendering.lines);
    }

    /// Render tool result as compact inline format
    ///
    /// Format: `-> Result summary` or `✓ ToolName` / `× ToolName error`
    pub(super) fn render_tool_result(&self, ctx: ToolResultContext) {
        let ToolResultContext { tool, result, success, error, exit_code, next_steps, rendering } = ctx;

        let theme = rendering.theme;

        if matches!(rendering.detail_level, CardDetailLevel::Brief) {
            let (symbol, color) = if success { ("✓", theme.green) } else { ("×", theme.red) };

            let preview = if let Some(err) = error {
                format!(
                    " {}",
                    if err.len() > 50 { format!("{}...", &err[..47]) } else { err.to_string() }
                )
            } else if let Some(first_line) = result.lines().next() {
                let line = first_line.trim();
                if line.is_empty() {
                    String::new()
                } else if line.len() > 50 {
                    format!(" ({}...)", &line[..47])
                } else {
                    format!(" ({})", line)
                }
            } else {
                String::new()
            };

            rendering.lines.push(Line::from(vec![
                Span::styled(format!("{} ", symbol), Style::default().fg(color)),
                Span::styled(tool.to_string(), Style::default().fg(theme.fg)),
                Span::styled(preview, Style::default().fg(theme.muted)),
            ]));
            return;
        }

        rendering.lines.push(Line::default());

        let content_width = rendering.width.saturating_sub(6);
        let content_style = Style::default().fg(theme.fg).bg(theme.panel_bg);
        let muted_style = Style::default().fg(theme.muted).bg(theme.panel_bg);
        let label_style = Style::default().fg(theme.cyan).bg(theme.panel_bg).bold();

        let (status_symbol, status_color) = if success { ("✓", theme.green) } else { ("✗", theme.red) };

        let mut content_lines: Vec<Line<'static>> = Vec::new();
        content_lines.push(Line::from(vec![
            Span::styled("Status: ", muted_style),
            Span::styled(
                status_symbol,
                Style::default().fg(status_color).bg(theme.panel_bg).bold(),
            ),
        ]));

        if let Some(err) = error {
            content_lines.push(Line::from(vec![
                Span::styled("Error: ", Style::default().fg(theme.red).bg(theme.panel_bg)),
                Span::styled(err.to_string(), Style::default().fg(theme.red).bg(theme.panel_bg)),
            ]));
        }

        self.render_with_code_highlighting(result, theme.fg, content_width, &mut content_lines);

        if let Some(code) = exit_code {
            let color = if code == 0 { theme.green } else { theme.red };
            content_lines.push(Line::from(vec![
                Span::styled("Exit code: ", muted_style),
                Span::styled(code.to_string(), Style::default().fg(color).bg(theme.panel_bg).bold()),
            ]));
        }

        if let Some(steps) = next_steps
            && !steps.is_empty()
        {
            content_lines.push(Line::from(vec![Span::styled("Next steps:", label_style)]));
            for step in steps {
                content_lines.push(Line::from(vec![
                    Span::styled("- ", muted_style),
                    Span::styled(step.clone(), content_style),
                ]));
            }
        }

        let title = format!("Result: {}", tool);
        let border_color = if success { theme.green } else { theme.red };
        self.render_card(&title, border_color, rendering.width, content_lines, rendering.lines);
    }

    /// Render approval prompt card with teaching context (WHAT, WHY, SCOPE, RISK)
    pub(super) fn render_approval_prompt(&self, ctx: ApprovalPromptContext) {
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
        let theme = rendering.theme;
        let risk_color = TranscriptEntry::risk_level_color(theme, risk);

        if rendering.is_compact() {
            let action_preview = description
                .or(scope)
                .map(|s| s.to_string())
                .unwrap_or_else(|| action.to_string());

            rendering.lines.push(Line::from(vec![
                Span::styled("[", Style::default().fg(theme.muted)),
                Span::styled("Approve:", Style::default().fg(theme.yellow)),
                Span::raw(" "),
                Span::styled(action_preview.clone(), Style::default().fg(risk_color)),
                Span::raw(" | "),
                Span::styled("[y/n/c]", Style::default().fg(theme.muted)),
            ]));
            return;
        }

        let content_width = rendering.width.saturating_sub(6);
        let content_style = Style::default().fg(theme.fg).bg(theme.panel_bg);
        let muted_style = Style::default().fg(theme.muted).bg(theme.panel_bg);
        let label_style = Style::default().fg(theme.cyan).bg(theme.panel_bg).bold();

        let mut content_lines: Vec<Line<'static>> = Vec::new();
        content_lines.push(Line::from(vec![
            Span::styled("Action: ", muted_style),
            Span::styled(format!("{} [{}]", action, risk), content_style),
        ]));

        match rendering.detail_level {
            CardDetailLevel::Brief => {
                if let Some(desc) = description {
                    content_lines.push(Line::from(vec![Span::styled(desc.to_string(), content_style)]));
                }
            }
            CardDetailLevel::Detailed => {
                if let Some(desc) = description {
                    content_lines.push(Line::from(vec![
                        Span::styled("WHAT: ", label_style),
                        Span::styled(desc.to_string(), content_style),
                    ]));
                }

                if let Some(ctx) = task_context {
                    content_lines.push(Line::from(vec![
                        Span::styled("WHY:  ", label_style),
                        Span::styled(ctx.to_string(), content_style),
                    ]));
                }

                if let Some(sc) = scope {
                    content_lines.push(Line::from(vec![Span::styled("SCOPE:", label_style)]));
                    self.wrap_text_styled(sc, content_style, content_width, &mut content_lines);
                }
            }
            CardDetailLevel::Verbose => {
                if let Some(desc) = description {
                    content_lines.push(Line::from(vec![
                        Span::styled("WHAT: ", label_style),
                        Span::styled(desc.to_string(), content_style),
                    ]));
                }

                if let Some(ctx) = task_context {
                    content_lines.push(Line::from(vec![
                        Span::styled("WHY:  ", label_style),
                        Span::styled(ctx.to_string(), content_style),
                    ]));
                }

                if let Some(sc) = scope {
                    content_lines.push(Line::from(vec![Span::styled("SCOPE:", label_style)]));
                    self.wrap_text_styled(sc, content_style, content_width, &mut content_lines);
                }

                if let Some(reasoning) = risk_reasoning {
                    content_lines.push(Line::from(vec![
                        Span::styled("RISK:  ", Style::default().fg(theme.yellow).bg(theme.panel_bg).bold()),
                        Span::styled(
                            reasoning.to_string(),
                            Style::default().fg(theme.fg).bg(theme.panel_bg).italic(),
                        ),
                    ]));
                }
            }
        }

        match decision {
            None => {
                let blink = rendering.animation_frame % 2 == 0;
                let focus_style = if blink {
                    Style::default().fg(theme.green).bg(theme.panel_bg).bold().reversed()
                } else {
                    Style::default().fg(theme.green).bg(theme.panel_bg).bold()
                };

                content_lines.push(Line::default());
                content_lines.push(Line::from(vec![
                    Span::styled("[", muted_style),
                    Span::styled("y", focus_style),
                    Span::styled("] approve  ", muted_style),
                    Span::styled("[", muted_style),
                    Span::styled("n", Style::default().fg(theme.red).bg(theme.panel_bg).bold()),
                    Span::styled("] reject  ", muted_style),
                    Span::styled("[", muted_style),
                    Span::styled("c", Style::default().fg(theme.yellow).bg(theme.panel_bg).bold()),
                    Span::styled("] cancel", muted_style),
                ]));

                match rendering.detail_level {
                    CardDetailLevel::Brief => {}
                    CardDetailLevel::Detailed => {
                        content_lines.push(Line::from(vec![Span::styled(
                            "[detailed mode - scope and risk assessment shown]",
                            muted_style,
                        )]));
                    }
                    CardDetailLevel::Verbose => {
                        content_lines.push(Line::from(vec![Span::styled(
                            "[verbose mode - full approval context available]",
                            muted_style,
                        )]));
                    }
                }
            }
            Some(ApprovalDecision::Approved) => {
                content_lines.push(Line::from(vec![
                    Span::styled("✓", Style::default().fg(theme.green).bg(theme.panel_bg)),
                    Span::styled(" Approved", Style::default().fg(theme.green).bg(theme.panel_bg)),
                ]));
            }
            Some(ApprovalDecision::Rejected) => {
                content_lines.push(Line::from(vec![
                    Span::styled("✗", Style::default().fg(theme.red).bg(theme.panel_bg)),
                    Span::styled(" Rejected", Style::default().fg(theme.red).bg(theme.panel_bg)),
                ]));
            }
            Some(ApprovalDecision::Cancelled) => {
                content_lines.push(Line::from(vec![
                    Span::styled("-", Style::default().fg(theme.yellow).bg(theme.panel_bg)),
                    Span::styled(" Cancelled", Style::default().fg(theme.yellow).bg(theme.panel_bg)),
                ]));
            }
        }

        let pulse_border = if decision.is_none() && rendering.animation_frame % 2 == 0 {
            risk_color
        } else if decision.is_none() {
            theme.muted
        } else {
            risk_color
        };

        self.render_card(
            "Approval Required",
            pulse_border,
            rendering.width,
            content_lines,
            rendering.lines,
        );
    }

    /// Render patch display with hunk-level intent labels
    pub(super) fn render_patch_display(&self, ctx: PatchDisplayContext) {
        let PatchDisplayContext { patch_name, file_path, diff_content, hunk_labels, rendering } = ctx;
        let theme = rendering.theme;

        rendering.lines.push(Line::default());

        rendering.lines.push(Line::from(vec![
            Span::styled("Patch: ", Style::default().fg(theme.muted)),
            Span::styled(patch_name.to_string(), Style::default().fg(theme.fg).bold()),
        ]));

        rendering.lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled("File: ", Style::default().fg(theme.muted)),
            Span::styled(file_path.to_string(), Style::default().fg(theme.cyan)),
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
                            Span::styled("Note: ", Style::default().fg(theme.muted)),
                            Span::styled(label.clone(), Style::default().fg(theme.yellow).italic()),
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
                    Style::default().fg(theme.muted),
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
                    Span::styled("Note: ", Style::default().fg(theme.muted)),
                    Span::styled(label.clone(), Style::default().fg(theme.yellow).italic()),
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
                    Style::default().fg(theme.muted).italic(),
                )]));
            }
            CardDetailLevel::Detailed => {
                rendering.lines.push(Line::from(vec![Span::styled(
                    "  [detailed mode - full diff with intent labels]",
                    Style::default().fg(theme.muted).italic(),
                )]));
            }
            CardDetailLevel::Verbose => {
                rendering.lines.push(Line::from(vec![Span::styled(
                    "  [verbose mode - all patch details shown]",
                    Style::default().fg(theme.muted).italic(),
                )]));
            }
        }
    }
}
