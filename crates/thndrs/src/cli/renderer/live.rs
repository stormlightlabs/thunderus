//! Live prompt and focused-surface row builders.
//!
//! The live chrome is rebuilt after dirty updates and composed into the full
//! viewport by [`super::alternate::AlternateViewport`].

#[cfg(test)]
mod tests;

use crate::app::{App, Entry, Mode, PromptState, RecoveryStage, ToolStatus};
use crate::renderer::cursor::{prompt_cursor, prompt_rows};
use crate::renderer::row::{CursorCoord, Row};
use crate::renderer::style::{CellStyle, Color, Span};
use crate::renderer::transcript::GUTTER;
use crate::renderer::view::{FocusedSurfaceView, SurfaceRenderInput, SurfaceThemeView};
use crate::{renderer, utils};

/// Maximum rows the prompt input can occupy before scrolling within the live region.
pub const MAX_PROMPT_ROWS: usize = 8;

/// Maximum accessory rows (help/commands/files) shown in the live region.
pub const MAX_ACCESSORY_ROWS: usize = 8;

const LIVE_INSET: usize = 2;
const COMPOSER_MIN_CONTENT_WIDTH: usize = 8;

/// Build prompt input rows from app state.
///
/// Returns the rows and the cursor coordinate (relative to the first row).
pub fn prompt_rows_for(app: &App, width: usize) -> (Vec<Row>, Option<CursorCoord>) {
    let p = super::style::palette();
    let surface = p.panel_bg;
    let prompt_state = app.prompt_state();

    let (prompt_color, icon) = match prompt_state {
        PromptState::Editable => (p.yellow, "❯"),
        PromptState::Submitted => (p.teal, "»"),
        PromptState::Streaming | PromptState::RunningTool => (p.teal, "»"),
        PromptState::Stopped => (p.teal, "○"),
        PromptState::Errored => (p.red, "✕"),
    };

    let prefix_width = if app.mode == Mode::Command { 4 } else { 3 };
    let row_body_width = super::layout::content_width(width);
    let framed = composer_frame_height(width) > 0;
    let horizontal_chrome = if framed { 4 } else { LIVE_INSET };
    let body_width = row_body_width.saturating_sub(horizontal_chrome + prefix_width).max(1);
    let cursor_indent = width.min(2) + LIVE_INSET + prefix_width;
    let hidden_entry_active = app.first_run_recovery.as_ref().is_some_and(|recovery| {
        matches!(
            recovery.stage,
            RecoveryStage::EnterKey | RecoveryStage::ChatGptOAuthPasteRedirect
        )
    });
    let hidden_display = String::from("credential: [hidden]");
    let input_text = if hidden_entry_active { hidden_display.as_str() } else { app.input.as_str() };
    let cursor_pos = if hidden_entry_active { input_text.len() } else { app.input.cursor() };

    let visual_rows = prompt_rows(input_text, body_width);
    let cursor = prompt_cursor(input_text, cursor_pos, body_width, cursor_indent);

    let text_style = CellStyle::new().fg(p.text).bg(surface);
    let mention_style = CellStyle::new().fg(p.accent).bg(surface).bold();

    let border_style = CellStyle::new().fg(prompt_color).bg(surface);
    let mut rows = Vec::with_capacity(visual_rows.len());
    for (idx, line) in visual_rows.into_iter().enumerate() {
        let mut spans: Vec<Span> = if framed && idx == 0 {
            let mut s = vec![
                Span::styled("│", border_style),
                Span::styled(" ", CellStyle::new().bg(surface)),
                Span::styled(icon, CellStyle::new().fg(prompt_color).bg(surface)),
                Span::styled("  ", CellStyle::new().bg(surface)),
            ];
            if app.mode == Mode::Command {
                s.push(Span::styled(":", CellStyle::new().fg(p.accent).bg(surface)));
            }
            s
        } else if framed {
            vec![
                Span::styled("│", border_style),
                Span::styled(" ".repeat(1 + prefix_width), CellStyle::new().bg(surface)),
            ]
        } else if idx == 0 {
            let mut s = vec![
                Span::styled(" ".repeat(LIVE_INSET), CellStyle::new().bg(surface)),
                Span::styled(icon, CellStyle::new().fg(prompt_color).bg(surface)),
                Span::styled("  ", CellStyle::new().bg(surface)),
            ];
            if app.mode == Mode::Command {
                s.push(Span::styled(":", CellStyle::new().fg(p.accent).bg(surface)));
            }
            s
        } else {
            vec![Span::styled(
                " ".repeat(LIVE_INSET + prefix_width),
                CellStyle::new().bg(surface),
            )]
        };

        if !line.is_empty() {
            spans.extend(mention_styled_spans(&line, text_style, mention_style, surface));
        }
        if framed {
            let used = super::layout::spans_width(&spans);
            let fill = row_body_width.saturating_sub(used + 1);
            spans.push(Span::styled(" ".repeat(fill), CellStyle::new().bg(surface)));
            spans.push(Span::styled("│", border_style));
        }
        rows.push(Row::padded(spans, width, CellStyle::new().bg(surface)));
    }

    if rows.is_empty() {
        let mut spans = if framed {
            vec![
                Span::styled("│", border_style),
                Span::styled(" ", CellStyle::new().bg(surface)),
                Span::styled(icon, CellStyle::new().fg(prompt_color).bg(surface)),
                Span::styled("  ", CellStyle::new().bg(surface)),
            ]
        } else {
            vec![
                Span::styled(" ".repeat(LIVE_INSET), CellStyle::new().bg(surface)),
                Span::styled(icon, CellStyle::new().fg(prompt_color).bg(surface)),
                Span::styled("  ", CellStyle::new().bg(surface)),
            ]
        };
        if app.mode == Mode::Command {
            spans.push(Span::styled(":", CellStyle::new().fg(p.accent).bg(surface)));
        }
        if framed {
            let used = super::layout::spans_width(&spans);
            let fill = row_body_width.saturating_sub(used + 1);
            spans.push(Span::styled(" ".repeat(fill), CellStyle::new().bg(surface)));
            spans.push(Span::styled("│", border_style));
        }
        rows.push(Row::padded(spans, width, CellStyle::new().bg(surface)));
    }

    (rows, if hidden_entry_active { None } else { Some(cursor) })
}

/// Number of horizontal frame rows reserved by the composer at this width.
pub fn composer_frame_height(width: usize) -> usize {
    usize::from(super::layout::content_width(width) >= COMPOSER_MIN_CONTENT_WIDTH) * 2
}

/// Add the horizontal frame around already-wrapped prompt body rows.
pub fn frame_prompt_rows(
    app: &App, width: usize, rows: Vec<Row>, cursor: Option<CursorCoord>,
) -> (Vec<Row>, Option<CursorCoord>) {
    if composer_frame_height(width) == 0 {
        return (rows, cursor);
    }

    let p = super::style::palette();
    let bg = p.panel_bg;
    let border_color = match app.prompt_state() {
        PromptState::Editable => p.yellow,
        PromptState::Submitted | PromptState::Streaming | PromptState::RunningTool | PromptState::Stopped => p.teal,
        PromptState::Errored => p.red,
    };
    let border_style = CellStyle::new().fg(border_color).bg(bg);
    let label_style = border_style.bold();
    let content_width = super::layout::content_width(width);

    let left = "╭─";
    let session = app.run_label();
    let session_budget = content_width.saturating_sub(5).max(1);
    let label = format!(" {} ", utils::truncate_ellipsis(session, session_budget));
    let status_label = app.status_label();
    let status = (status_label != "Ready").then(|| {
        let icon = super::style::status_icon(
            &status_label,
            super::style::spinner_tick(app.ui_tick, app.cli.tick_rate_ms),
        );
        format!(" {icon} {status_label} ")
    });
    let status_width = status.as_deref().map_or(0, utils::text_width);
    let fixed = utils::text_width(left) + utils::text_width(&label) + status_width + 1;
    let top_spans = if fixed + 2 <= content_width {
        let mut spans = vec![
            Span::styled(left, border_style),
            Span::styled(label, label_style),
            Span::styled("─".repeat(content_width - fixed), border_style),
        ];
        if let Some(status) = status {
            spans.push(Span::styled(
                status,
                CellStyle::new().fg(super::style::status_color(&status_label)).bg(bg),
            ));
        }
        spans.push(Span::styled("╮", border_style));
        spans
    } else if utils::text_width(left) + utils::text_width(&label) < content_width {
        let fixed = utils::text_width(left) + utils::text_width(&label) + 1;
        vec![
            Span::styled(left, border_style),
            Span::styled(label, label_style),
            Span::styled("─".repeat(content_width - fixed), border_style),
            Span::styled("╮", border_style),
        ]
    } else {
        vec![Span::styled(
            format!("╭{}╮", "─".repeat(content_width.saturating_sub(2))),
            border_style,
        )]
    };

    let mut framed = Vec::with_capacity(rows.len() + 2);
    framed.push(Row::padded(top_spans, width, CellStyle::new().bg(bg)));
    framed.extend(rows);
    framed.push(Row::padded(
        vec![Span::styled(
            format!("╰{}╯", "─".repeat(content_width.saturating_sub(2))),
            border_style,
        )],
        width,
        CellStyle::new().bg(bg),
    ));

    let cursor = cursor.map(|mut cursor| {
        cursor.row += 1;
        cursor
    });
    (framed, cursor)
}

/// Build accessory rows (help, commands, or file picker) if active.
///
/// Returns an empty vec when no accessory is visible.
pub fn accessory_rows(app: &App, width: usize, max_height: usize) -> Vec<Row> {
    if max_height == 0 {
        return Vec::new();
    }

    let focused_surface = FocusedSurfaceView::from(app);
    if !matches!(focused_surface, FocusedSurfaceView::None) {
        return super::adapter::render_surface(&SurfaceRenderInput {
            surface: &focused_surface,
            theme: &SurfaceThemeView::new(),
            width,
            height: max_height,
        });
    }

    Vec::new()
}

/// Build a queued-prompt summary row when steering or follow-up prompts are pending.
///
/// Returns `None` when the queue is empty or the agent is idle.
pub fn queued_summary_row(app: &App, width: usize) -> Option<Row> {
    let steering = app.queued_steering.len();
    let followups = app.queued_followups.len();
    if steering == 0 && followups == 0 {
        return None;
    }

    let p = super::style::palette();
    let bg = Color::Reset;
    let label_style = CellStyle::new().fg(p.peach).bg(bg).bold();
    let muted_style = CellStyle::new().fg(p.subtext0).bg(bg);
    let mut spans = vec![
        Span::styled(" ".repeat(LIVE_INSET), CellStyle::new().bg(bg)),
        Span::styled("queued", label_style),
    ];

    if steering > 0 {
        spans.push(Span::styled(format!("  {steering} steering"), muted_style));
    }
    if followups > 0 {
        spans.push(Span::styled(format!("  {followups} follow-up"), muted_style));
    }
    Some(Row::padded(spans, width, CellStyle::new().bg(bg)))
}

/// Build detail pane rows for the expanded tool entry.
///
/// Shows a title bar with the tool name and status, then the full output
/// wrapped into visual rows. The scroll offset is applied to those rendered
/// rows so long lines scroll by visible terminal row rather than by raw output
/// line.
pub fn detail_pane_rows(app: &App, width: usize, max_height: usize) -> Vec<Row> {
    if max_height == 0 {
        return Vec::new();
    }

    let Some(entry) = app.transcript.get(app.detail_pane.entry_index) else {
        return Vec::new();
    };
    let Entry::Tool { name, arguments, status, output } = entry else {
        return Vec::new();
    };

    let p = super::style::palette();
    let bg = p.surface0;
    let title_style = CellStyle::new().fg(p.accent).bg(bg).bold();
    let status_color = match status {
        ToolStatus::Running => p.peach,
        ToolStatus::Ok => p.green,
        ToolStatus::Failed => p.red,
        ToolStatus::Cancelled => p.peach,
    };
    let status_style = CellStyle::new().fg(status_color).bg(bg);
    let muted_style = CellStyle::new().fg(p.subtext0).bg(bg);
    let body_style = CellStyle::new().fg(p.text).bg(bg);
    let gutter_style = CellStyle::new().fg(p.overlay0).bg(bg);

    let status_label = match status {
        ToolStatus::Running => "running",
        ToolStatus::Ok => "ok",
        ToolStatus::Failed => "failed",
        ToolStatus::Cancelled => "cancelled",
    };

    let base_name = name.split('#').next().unwrap_or(name);
    let mut title_spans = vec![
        Span::styled(" ".repeat(LIVE_INSET), CellStyle::new().bg(bg)),
        Span::styled(base_name.to_string(), title_style),
        Span::styled(format!(" [{status_label}]"), status_style),
    ];

    let args_summary = renderer::transcript::summarize_tool_invocation(base_name, arguments, &app.cwd);
    if !args_summary.is_empty() {
        title_spans.push(Span::styled("  ", CellStyle::new().bg(bg)));
        title_spans.push(Span::styled(args_summary, muted_style));
    }

    let body_width = super::layout::content_width(width).saturating_sub(utils::text_width(GUTTER));
    let mut body_rows = Vec::new();

    for line in output {
        let line = renderer::path_display::transcript_line(line, &app.cwd);
        for wrapped in super::layout::wrap_text(&line, body_width) {
            let spans = vec![Span::styled(GUTTER, gutter_style), Span::styled(wrapped, body_style)];
            body_rows.push(Row::padded(spans, width, CellStyle::new().bg(bg)));
        }
    }

    let mut rows = Vec::with_capacity(max_height);
    let scroll = app.detail_pane.scroll.min(body_rows.len().saturating_sub(1));
    let body_budget = max_height.saturating_sub(1);
    let hidden_above = scroll;
    let hidden_below = body_rows.len().saturating_sub(scroll + body_budget);

    rows.push(Row::padded(title_spans, width, CellStyle::new().bg(bg)));
    rows.extend(body_rows.into_iter().skip(scroll).take(body_budget));
    if (hidden_above > 0 || hidden_below > 0)
        && let Some(row) = rows.last_mut()
    {
        *row = clipped_detail_indicator_row(width, bg, muted_style, hidden_above, hidden_below);
    }
    rows
}

/// Build the immediate operational state row below the prompt.
pub fn static_status_row(app: &App, width: usize) -> Row {
    let p = super::style::palette();
    let bg = p.panel_bg;
    let muted = CellStyle::new().fg(p.overlay0).bg(bg);
    if width < 12 {
        return Row::blank(width, CellStyle::new().bg(bg));
    }
    let state = app.status_label();
    let state_style = CellStyle::new().fg(super::style::status_color(&state)).bg(bg).bold();
    let mut spans = vec![
        Span::styled(" ".repeat(LIVE_INSET), CellStyle::new().bg(bg)),
        Span::styled(state, state_style),
    ];
    if width >= 30 {
        spans.push(Span::styled("   /status details", muted));
    }
    Row::padded(spans, width, CellStyle::new().bg(bg))
}

fn clipped_detail_indicator_row(
    width: usize, bg: Color, style: CellStyle, hidden_above: usize, hidden_below: usize,
) -> Row {
    let text = match (hidden_above, hidden_below) {
        (0, below) => format!("   │ … {below} rows below"),
        (above, 0) => format!("   │ … {above} rows above"),
        (above, below) => format!("   │ … {above} rows above, {below} below"),
    };
    Row::padded(vec![Span::styled(text, style)], width, CellStyle::new().bg(bg))
}

fn mention_styled_spans(line: &str, text_style: CellStyle, mention_style: CellStyle, _bg: Color) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '@' {
            if !current.is_empty() {
                spans.push(Span::styled(std::mem::take(&mut current), text_style));
            }

            let mut mention = String::from('@');
            i += 1;
            while i < chars.len() && is_mention_char(chars[i]) {
                mention.push(chars[i]);
                i += 1;
            }

            if mention.len() > 1 {
                spans.push(Span::styled(mention, mention_style));
            } else {
                spans.push(Span::styled("@", text_style));
            }
        } else {
            current.push(chars[i]);
            i += 1;
        }
    }

    if !current.is_empty() {
        spans.push(Span::styled(current, text_style));
    }

    if spans.is_empty() {
        spans.push(Span::styled(line.to_string(), text_style));
    }

    spans
}

/// Whether a character is valid in a file mention path after `@`.
fn is_mention_char(ch: char) -> bool {
    ch.is_alphanumeric() || matches!(ch, '/' | '.' | '-' | '_' | '~')
}
