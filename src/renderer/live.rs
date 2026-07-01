//! Live region row builders for the direct renderer.
//!
//! The live region is redrawn each tick. Committed transcript content is
//! printed into native scrollback separately by [`super::region::LiveRegion`].

#![allow(dead_code)]

use crate::app::{App, Entry, Mode, PromptAccessory, PromptState, RunState, ToolStatus};
use crate::renderer::cursor::{prompt_cursor, prompt_rows};
use crate::renderer::layout::{content_width, truncate_spans, wrap_text};
use crate::renderer::row::{CursorCoord, Row};
use crate::renderer::style::{CellStyle, Color, Span};
use crate::ui::style as ui_style;
use crate::utils;

/// Build the dynamic status row: session id + status icon + queue info.
///
/// Sits above the prompt input in the live region.
pub fn dynamic_status_row(app: &App, width: usize) -> Row {
    let p = ui_style::palette();
    let bg = ratatui_color(p.surface0);
    let panel_bg = ratatui_color(p.panel_bg);

    let label = app.status_label();
    let status_color = ratatui_color(ui_style::status_color(label));
    let icon = ui_style::status_icon(label, app.ui_tick);
    let session = if app.session_id.is_empty() { "thndrs" } else { &app.session_id };

    let mut spans = vec![
        Span::styled(
            session.to_string(),
            CellStyle::new().fg(ratatui_color(p.accent)).bg(bg).bold(),
        ),
        Span::styled("  ", CellStyle::new().bg(bg)),
        Span::styled(format!("{icon} {label}"), CellStyle::new().fg(status_color).bg(bg)),
    ];

    if matches!(app.run_state, RunState::Working) {
        spans.push(Span::styled("  ", CellStyle::new().bg(bg)));
        spans.push(Span::styled(
            format!(
                "target: {}  queued: {}/{}",
                app.queue_target.label(),
                app.queued_steering.len(),
                app.queued_followups.len()
            ),
            CellStyle::new().fg(ratatui_color(p.subtext0)).bg(bg),
        ));
    }

    let _ = panel_bg;
    Row::padded(spans, width, bg_style(bg))
}

/// Build prompt input rows from app state.
///
/// Returns the rows and the cursor coordinate (relative to the first row).
pub fn prompt_rows_for(app: &App, width: usize) -> (Vec<Row>, Option<CursorCoord>) {
    let p = ui_style::palette();
    let surface = ratatui_color(p.surface0);
    let prompt_state = app.prompt_state();

    let (prompt_color, show_input, icon) = match prompt_state {
        PromptState::Editable => (ratatui_color(p.yellow), true, "›"),
        PromptState::Submitted => (ratatui_color(p.yellow), false, "·"),
        PromptState::Streaming | PromptState::RunningTool => (ratatui_color(p.teal), true, "›"),
        PromptState::Stopped => (ratatui_color(p.teal), true, "○"),
        PromptState::Errored => (ratatui_color(p.red), true, "✕"),
    };

    let indent = prompt_prefix_width(app);
    let body_width = width.saturating_sub(indent).max(1);
    let input_text = app.input.as_str();
    let cursor_pos = app.input.cursor();

    if !show_input {
        let spans = vec![
            Span::styled(icon, CellStyle::new().fg(prompt_color).bg(surface)),
            Span::styled(
                "  submitted",
                CellStyle::new().fg(ratatui_color(p.overlay0)).bg(surface),
            ),
        ];
        return (
            vec![Row::padded(spans, width, bg_style(surface))],
            Some(CursorCoord::new(0, 0)),
        );
    }

    let visual_rows = prompt_rows(input_text, body_width);
    let cursor = prompt_cursor(input_text, cursor_pos, body_width, indent);

    let mut rows = Vec::with_capacity(visual_rows.len());
    for (idx, line) in visual_rows.into_iter().enumerate() {
        let mut spans: Vec<Span> = if idx == 0 {
            let mut s = vec![
                Span::styled(icon, CellStyle::new().fg(prompt_color).bg(surface)),
                Span::styled("  ", CellStyle::new().bg(surface)),
            ];
            if app.mode == Mode::Command {
                s.push(Span::styled(
                    ":",
                    CellStyle::new().fg(ratatui_color(p.accent)).bg(surface),
                ));
            }
            s
        } else {
            vec![Span::styled(" ".repeat(indent), CellStyle::new().bg(surface))]
        };

        if !line.is_empty() {
            spans.push(Span::styled(
                line,
                CellStyle::new().fg(ratatui_color(p.text)).bg(surface),
            ));
        }
        rows.push(Row::padded(spans, width, bg_style(surface)));
    }

    if rows.is_empty() {
        let mut spans = vec![
            Span::styled(icon, CellStyle::new().fg(prompt_color).bg(surface)),
            Span::styled("  ", CellStyle::new().bg(surface)),
        ];
        if app.mode == Mode::Command {
            spans.push(Span::styled(
                ":",
                CellStyle::new().fg(ratatui_color(p.accent)).bg(surface),
            ));
        }
        rows.push(Row::padded(spans, width, bg_style(surface)));
    }

    (rows, Some(cursor))
}

/// Build accessory rows (help, commands, or file picker) if active.
///
/// Returns an empty vec when no accessory is visible.
pub fn accessory_rows(app: &App, width: usize, max_height: usize) -> Vec<Row> {
    if max_height == 0 {
        return Vec::new();
    }

    match app.prompt_accessory {
        PromptAccessory::None => Vec::new(),
        PromptAccessory::Help => help_rows(width, max_height),
        PromptAccessory::Commands { selected } => command_rows(app, selected, width, max_height),
        PromptAccessory::Files(_) => {
            let p = ui_style::palette();
            let bg = ratatui_color(p.surface0);
            vec![Row::padded(
                vec![Span::styled(
                    "(file picker)",
                    CellStyle::new().fg(ratatui_color(p.overlay0)).bg(bg),
                )],
                width,
                bg_style(bg),
            )]
        }
    }
}

/// Build the static status row (model/search/tokens/cwd) below the prompt.
///
/// Width-aware clipping hides segments that don't fit.
pub fn static_status_row(app: &App, width: usize) -> Row {
    let p = ui_style::palette();
    let bg = ratatui_color(p.surface0);
    let subtext = CellStyle::new().fg(ratatui_color(p.subtext0)).bg(bg);
    let muted = CellStyle::new().fg(ratatui_color(p.overlay0)).bg(bg);
    let _sep = CellStyle::new().bg(bg);

    let model_label = format!("model: {}", app.model);
    let search_label = app.websearch.label();
    let search_text = format!("search: {search_label}");
    let token_text = format!("tok: ↑{} ↓{}", app.session_tokens_in, app.session_tokens_out);

    let (show_model, show_search, show_tokens, show_cwd) = match width {
        w if w < 24 => (false, false, false, false),
        w if w < 42 => (true, false, false, false),
        w if w < 56 => (true, true, false, false),
        w if w < 80 => (true, true, true, false),
        _ => (true, true, true, true),
    };

    let model_len = model_label.chars().count();
    let search_len = search_text.chars().count();
    let token_len = token_text.chars().count();

    let mut spans: Vec<Span> = Vec::new();
    if show_model {
        spans.push(Span::styled(model_label, subtext));
    }
    if show_search {
        spans.push(Span::styled("   ", CellStyle::new().bg(bg)));
        spans.push(Span::styled(search_text, subtext));
    }
    if show_tokens {
        spans.push(Span::styled("   ", CellStyle::new().bg(bg)));
        spans.push(Span::styled(token_text, subtext));
    }
    if show_cwd {
        spans.push(Span::styled("   ", CellStyle::new().bg(bg)));
        let used =
            model_len + if show_search { search_len + 3 } else { 0 } + if show_tokens { token_len + 3 } else { 0 } + 6;
        let cwd_display = crate::ui::path_display::footer_segment(&app.cwd, width, used);
        spans.push(Span::styled(cwd_display, muted));
    }

    Row::padded(spans, width, bg_style(bg))
}

/// Build the active streaming block rows for the live region.
///
/// When the last transcript entry is still streaming (assistant/reasoning) or
/// a tool is running, those rows appear at the top of the live region above
/// the dynamic status.
pub fn active_streaming_rows(app: &App, width: usize) -> Vec<Row> {
    let Some(last) = app.transcript.last() else {
        return Vec::new();
    };

    let is_live = match last {
        Entry::Assistant { streaming, .. } | Entry::Reasoning { streaming, .. } => *streaming,
        Entry::Tool { status, .. } => *status == ToolStatus::Running,
        _ => false,
    };

    if !is_live {
        return Vec::new();
    }

    let p = ui_style::palette();
    let bg = ratatui_color(p.surface0);
    let body_width = content_width(width);

    match last {
        Entry::Assistant { text, .. } => {
            let label_style = CellStyle::new().fg(ratatui_color(p.green)).bg(bg).bold();
            let text_style = CellStyle::new().fg(ratatui_color(p.text)).bg(bg);
            build_text_block_rows("Assistant", label_style, text_style, text, width, body_width, bg)
        }
        Entry::Reasoning { text, streaming } => {
            let icon = if *streaming { "·" } else { "✓" };
            let label = format!("Thinking {icon}");
            let label_style = CellStyle::new().fg(ratatui_color(p.mauve)).bg(bg).bold();
            let text_style = CellStyle::new().fg(ratatui_color(p.subtext0)).bg(bg).italic();
            build_text_block_rows(&label, label_style, text_style, text, width, body_width, bg)
        }
        Entry::Tool { name, arguments, status, output } => {
            let (status_label, status_color, icon) = match status {
                ToolStatus::Running => ("running", ratatui_color(p.yellow), "·"),
                ToolStatus::Ok => ("ok", ratatui_color(p.green), "✓"),
                ToolStatus::Failed => ("failed", ratatui_color(p.red), "✕"),
            };
            let header_style = CellStyle::new().fg(ratatui_color(p.text)).bg(bg).bold();
            let status_style = CellStyle::new().fg(status_color).bg(bg);
            let muted_style = CellStyle::new().fg(ratatui_color(p.subtext0)).bg(bg);
            let gutter_style = CellStyle::new().fg(ratatui_color(p.overlay0)).bg(bg);

            let mut rows = vec![Row::blank(width, bg_style(bg))];
            let mut header_spans = vec![
                Span::styled(format!("{icon} "), status_style),
                Span::styled(name.to_string(), header_style),
                Span::styled(format!(" [{status_label}]"), status_style),
            ];

            let args_summary = summarize_args(arguments);
            if !args_summary.is_empty() {
                header_spans.push(Span::styled("  ", CellStyle::new().bg(bg)));
                header_spans.push(Span::styled(args_summary, muted_style));
            }
            rows.push(Row::padded(header_spans, width, bg_style(bg)));

            let base_name = name.split('#').next().unwrap_or(name);
            let lang = crate::renderer::highlight::tool_output_language(base_name, arguments);
            let max_lines = 4;
            match lang {
                Some(lang_str) => {
                    let joined: String = output.iter().take(max_lines).map(|l| format!("{l}\n")).collect();
                    let highlighted = crate::renderer::highlight::highlight_lines(&joined, Some(lang_str));
                    for hl_row in highlighted {
                        let mut spans = vec![Span::styled("   │ ", gutter_style)];
                        spans.extend(hl_row.into_iter().map(|s| Span { text: s.text, style: s.style.bg(bg) }));
                        rows.push(Row::padded(spans, width, bg_style(bg)));
                    }
                }
                None => {
                    for line in output.iter().take(max_lines) {
                        let content_style = CellStyle::new().fg(ratatui_color(p.subtext0)).bg(bg);
                        for wrapped in crate::renderer::layout::wrap_text(line, body_width.saturating_sub(5)) {
                            let spans = vec![
                                Span::styled("   │ ", gutter_style),
                                Span::styled(wrapped, content_style),
                            ];
                            rows.push(Row::padded(spans, width, bg_style(bg)));
                        }
                    }
                }
            }

            if output.len() > max_lines {
                rows.push(Row::padded(
                    vec![Span::styled(
                        format!("   │ …({} more lines)", output.len() - max_lines),
                        muted_style,
                    )],
                    width,
                    bg_style(bg),
                ));
            }

            rows.push(Row::blank(width, bg_style(bg)));
            rows
        }
        _ => Vec::new(),
    }
}

/// Produce a short summary of tool arguments.
fn summarize_args(arguments: &str) -> String {
    let trimmed = arguments.trim();
    if trimmed.is_empty() || trimmed == "{}" {
        return String::new();
    }
    let v: serde_json::Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(_) => return crate::utils::truncate_ellipsis(trimmed, 40),
    };
    let Some(obj) = v.as_object() else {
        return crate::utils::truncate_ellipsis(trimmed, 40);
    };
    for key in &["pattern", "path", "query", "root", "glob", "file", "program", "url"] {
        if let Some(val) = obj.get(*key).and_then(|f| f.as_str()) {
            return format!("{}: {}", key, utils::truncate_ellipsis(val, 30));
        }
    }
    for (k, val) in obj {
        if let Some(s) = val.as_str() {
            return format!("{k}: {}", utils::truncate_ellipsis(s, 30));
        }
    }
    utils::truncate_ellipsis(trimmed, 40)
}

/// Build a [`CellStyle`] with only a background color (for padding/fill).
fn bg_style(color: Color) -> CellStyle {
    CellStyle::new().bg(color)
}

/// Map a Ratatui [`ratatui::style::Color`] to a renderer [`Color`].
fn ratatui_color(c: ratatui::style::Color) -> Color {
    match c {
        ratatui::style::Color::Reset => Color::Reset,
        ratatui::style::Color::Black => Color::Black,
        ratatui::style::Color::Red => Color::DarkRed,
        ratatui::style::Color::Green => Color::DarkGreen,
        ratatui::style::Color::Yellow => Color::DarkYellow,
        ratatui::style::Color::Blue => Color::DarkBlue,
        ratatui::style::Color::Magenta => Color::DarkMagenta,
        ratatui::style::Color::Cyan => Color::DarkCyan,
        ratatui::style::Color::Gray => Color::Grey,
        ratatui::style::Color::DarkGray => Color::DarkGrey,
        ratatui::style::Color::LightRed => Color::Red,
        ratatui::style::Color::LightGreen => Color::Green,
        ratatui::style::Color::LightYellow => Color::Yellow,
        ratatui::style::Color::LightBlue => Color::Blue,
        ratatui::style::Color::LightMagenta => Color::Magenta,
        ratatui::style::Color::LightCyan => Color::Cyan,
        ratatui::style::Color::White => Color::White,
        ratatui::style::Color::Rgb(r, g, b) => Color::Rgb { r, g, b },
        ratatui::style::Color::Indexed(i) => {
            Color::Rgb { r: ((i >> 5) & 0x7) * 36, g: ((i >> 2) & 0x7) * 36, b: (i & 0x3) * 85 }
        }
    }
}

fn prompt_prefix_width(app: &App) -> usize {
    if app.mode == Mode::Command { 4 } else { 3 }
}

fn command_rows(app: &App, selected: usize, width: usize, max_height: usize) -> Vec<Row> {
    let p = ui_style::palette();
    let bg = ratatui_color(p.surface0);
    let commands = crate::app::command_suggestions_for_app(app);

    if commands.is_empty() {
        return vec![Row::padded(
            vec![Span::styled(
                "no matching commands",
                CellStyle::new().fg(ratatui_color(p.overlay0)).bg(bg),
            )],
            width,
            bg_style(bg),
        )];
    }

    commands
        .into_iter()
        .enumerate()
        .take(max_height)
        .map(|(i, (cmd, desc))| {
            let is_selected = i == selected;
            let marker = if is_selected { "›" } else { " " };
            let marker_style = if is_selected {
                CellStyle::new().fg(ratatui_color(p.yellow)).bg(bg).bold()
            } else {
                CellStyle::new().bg(bg)
            };
            let cmd_style = if is_selected {
                CellStyle::new().fg(ratatui_color(p.text)).bg(bg).bold()
            } else {
                CellStyle::new().fg(ratatui_color(p.subtext0)).bg(bg)
            };
            let desc_style = CellStyle::new().fg(ratatui_color(p.overlay0)).bg(bg);
            let spans = vec![
                Span::styled(marker, marker_style),
                Span::styled(" ", CellStyle::new().bg(bg)),
                Span::styled(cmd.to_string(), cmd_style),
                Span::styled(format!("  {desc}"), desc_style),
            ];
            Row::padded(spans, width, bg_style(bg))
        })
        .collect()
}

fn build_text_block_rows(
    label: &str, label_style: CellStyle, text_style: CellStyle, text: &str, width: usize, body_width: usize, bg: Color,
) -> Vec<Row> {
    let mut rows = vec![Row::blank(width, bg_style(bg))];
    rows.push(Row::padded(
        vec![Span::styled(label.to_string(), label_style)],
        width,
        bg_style(bg),
    ));

    for line in wrap_text(text, body_width) {
        if line.is_empty() {
            rows.push(Row::blank(width, bg_style(bg)));
        } else {
            rows.push(Row::padded(vec![Span::styled(line, text_style)], width, bg_style(bg)));
        }
    }
    rows.push(Row::blank(width, bg_style(bg)));
    rows
}

fn help_rows(width: usize, max_height: usize) -> Vec<Row> {
    let p = ui_style::palette();
    let bg = ratatui_color(p.surface0);
    let label_style = CellStyle::new().fg(ratatui_color(p.accent)).bg(bg).bold();
    let desc_style = CellStyle::new().fg(ratatui_color(p.subtext0)).bg(bg);

    let entries = [
        ("Enter", "submit / accept highlighted item"),
        ("Shift+Enter", "insert newline"),
        ("Esc", "close help, files, or commands"),
        ("Ctrl+P", "pick a file"),
        ("@path", "mention a file from fuzzy search"),
        ("Up/Down", "select item or recall history"),
        ("Ctrl+A/E", "move to start/end"),
        ("Ctrl+D", "quit after double-press"),
    ];

    entries
        .into_iter()
        .take(max_height)
        .map(|(key, desc)| {
            let spans = vec![
                Span::styled(format!("{key:<14}"), label_style),
                Span::styled(desc.to_string(), desc_style),
            ];
            Row::padded(spans, width, bg_style(bg))
        })
        .collect()
}

/// Truncate spans helper exposed for tests.
#[cfg(test)]
fn truncate_row(spans: &[Span], width: usize) -> Vec<Span> {
    truncate_spans(spans, width, CellStyle::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, Entry, Mode, RunState, ToolStatus};
    use crate::cli::{Cli, Theme, WebSearchMode};
    use std::path::PathBuf;

    fn test_app() -> App {
        App::from_cli(&Cli {
            cwd: PathBuf::from("."),
            model: "test-model".to_string(),
            websearch: WebSearchMode::Native,
            tick_rate_ms: 100,
            no_alt_screen: true,
            no_mouse: false,
            mouse: false,
            verbose: false,
            theme: Theme::EldritchMinimal,
            print_prompt: false,
        })
    }

    #[test]
    fn dynamic_status_row_has_session_and_label() {
        let app = test_app();
        let row = dynamic_status_row(&app, 80);
        let text = row.text();
        assert!(!text.is_empty(), "status row should have content");
        assert!(text.contains("idle"), "status label should appear");
    }

    #[test]
    fn dynamic_status_row_shows_queue_when_working() {
        let mut app = test_app();
        app.run_state = RunState::Working;
        app.queued_steering.push("steer".to_string());
        let row = dynamic_status_row(&app, 80);
        let text = row.text();
        assert!(text.contains("target:"), "queue target should appear when working");
        assert!(text.contains("queued: 1/0"), "queue counts should appear");
    }

    #[test]
    fn prompt_rows_empty_input() {
        let app = test_app();
        let (rows, cursor) = prompt_rows_for(&app, 80);
        assert_eq!(rows.len(), 1, "empty input should produce one row");
        assert!(cursor.is_some(), "cursor should be present");
        let text = rows[0].text();
        assert!(text.contains("›"), "prompt icon should appear");
    }

    #[test]
    fn prompt_rows_with_text() {
        let mut app = test_app();
        app.input.set_text("hello world");
        let (rows, cursor) = prompt_rows_for(&app, 80);
        assert_eq!(rows.len(), 1, "short text should fit on one row");
        assert!(cursor.is_some());
        assert!(rows[0].text().contains("hello world"));
    }

    #[test]
    fn prompt_rows_multiline() {
        let mut app = test_app();
        app.input.set_text("line one\nline two");
        let (rows, _cursor) = prompt_rows_for(&app, 80);
        assert_eq!(rows.len(), 2, "two logical lines should produce two rows");
    }

    #[test]
    fn prompt_rows_wraps_long_text() {
        let mut app = test_app();
        app.input.set_text(&"x".repeat(100));
        let (rows, _cursor) = prompt_rows_for(&app, 20);
        assert!(rows.len() > 1, "long text should wrap to multiple rows");
    }

    #[test]
    fn prompt_rows_command_mode_shows_colon() {
        let mut app = test_app();
        app.mode = Mode::Command;
        let (rows, _) = prompt_rows_for(&app, 80);
        assert!(rows[0].text().contains(':'), "command mode should show colon prefix");
    }

    #[test]
    fn prompt_rows_submitted_hides_input() {
        let mut app = test_app();
        app.run_state = RunState::Working;

        let (rows, _) = prompt_rows_for(&app, 80);
        assert!(
            rows[0].text().contains("submitted"),
            "submitted state should show 'submitted'"
        );
    }

    #[test]
    fn static_status_row_shows_model_at_narrow_width() {
        let app = test_app();
        let row = static_status_row(&app, 30);
        assert!(row.text().contains("model:"), "model should show at width 30");
    }

    #[test]
    fn static_status_row_hides_everything_at_tiny_width() {
        let app = test_app();
        let row = static_status_row(&app, 10);
        let text = row.text();
        assert!(text.trim().is_empty(), "nothing should show at width 10");
    }

    #[test]
    fn static_status_row_shows_all_at_wide_width() {
        let app = test_app();
        let row = static_status_row(&app, 120);
        let text = row.text();
        assert!(text.contains("model:"));
        assert!(text.contains("search:"));
        assert!(text.contains("tok:"));
    }

    #[test]
    fn accessory_rows_none_when_no_accessory() {
        let app = test_app();
        let rows = accessory_rows(&app, 80, 8);
        assert!(rows.is_empty(), "no accessory should produce no rows");
    }

    #[test]
    fn accessory_rows_help_has_entries() {
        let mut app = test_app();
        app.prompt_accessory = PromptAccessory::Help;

        let rows = accessory_rows(&app, 80, 8);
        assert!(!rows.is_empty(), "help should produce rows");
        assert!(rows[0].text().contains("Enter"), "help should include Enter key");
    }

    #[test]
    fn active_streaming_rows_empty_when_idle() {
        let app = test_app();
        let rows = active_streaming_rows(&app, 80);
        assert!(rows.is_empty(), "no active streaming when idle");
    }

    #[test]
    fn active_streaming_rows_for_streaming_assistant() {
        let mut app = test_app();
        app.transcript
            .push(Entry::Assistant { text: "hello there".to_string(), streaming: true });

        let rows = active_streaming_rows(&app, 80);
        assert!(!rows.is_empty(), "streaming assistant should produce rows");

        let combined: String = rows.iter().map(|r| r.text()).collect();
        assert!(combined.contains("Assistant"), "should have assistant label");
        assert!(combined.contains("hello there"), "should have text");
    }

    #[test]
    fn active_streaming_rows_for_running_tool() {
        let mut app = test_app();
        app.transcript.push(Entry::Tool {
            name: "search_text".to_string(),
            arguments: "{}".to_string(),
            status: ToolStatus::Running,
            output: vec![],
        });

        let rows = active_streaming_rows(&app, 80);
        assert!(!rows.is_empty());

        let combined: String = rows.iter().map(|r| r.text()).collect();
        assert!(combined.contains("search_text"), "should show tool name");
        assert!(combined.contains("running"), "should show running status");
    }

    #[test]
    fn ratatui_color_maps_rgb() {
        let c = ratatui_color(ratatui::style::Color::Rgb(10, 20, 30));
        assert_eq!(c, Color::Rgb { r: 10, g: 20, b: 30 });
    }

    #[test]
    fn ratatui_color_maps_reset() {
        assert_eq!(ratatui_color(ratatui::style::Color::Reset), Color::Reset);
    }

    #[test]
    fn truncate_row_helper_works() {
        let spans = vec![Span::plain("hello world")];
        let out = truncate_row(&spans, 5);
        assert_eq!(out.iter().map(|s| s.text.chars().count()).sum::<usize>(), 5);
    }
}
