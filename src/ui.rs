//! View geometry computation and rendering.
//!
//! Layout follows the Herdr lesson: compute all rectangles in [`compute_view`]
//! before drawing, then [`render`] reads the stored [`ViewState`].
//!
//! This keeps draw code stateless and makes layout testable with plain [`Rect`]
//! assertions.

mod highlight;
mod path_display;
mod style;
mod transcript;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::{App, Entry, FILE_PICKER_VISIBLE_ROWS, Mode, PromptState};
use crate::{banner, utils};

use transcript::entry_lines_with_width;

/// Maximum tool output lines rendered in the transcript before a truncation
/// marker is shown.
pub const MAX_TOOL_OUTPUT_LINES: usize = 6;

/// Prompt region height: divider line, input line, status line.
const PROMPT_HEIGHT: u16 = 3;

/// Footer region height: one status line.
const FOOTER_HEIGHT: u16 = 1;

/// Semantic group classification for transcript spacing.
#[derive(PartialEq)]
enum EntryGroup {
    User,
    Assistant,
    Reasoning,
    Tool,
    Transient,
}

impl From<&Entry> for EntryGroup {
    fn from(e: &Entry) -> Self {
        match e {
            Entry::User { .. } => EntryGroup::User,
            Entry::Assistant { .. } => EntryGroup::Assistant,
            Entry::Reasoning { .. } => EntryGroup::Reasoning,
            Entry::Tool { .. } => EntryGroup::Tool,
            Entry::Status { .. } | Entry::Error { .. } => EntryGroup::Transient,
        }
    }
}

/// Precomputed layout rectangles plus display flags.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct ViewState {
    /// Full screen rect handed to `compute_view`.
    pub area: Rect,
    /// Transcript rect.
    pub transcript: Rect,
    /// Prompt rect.
    pub prompt: Rect,
    /// Footer rect.
    pub footer: Rect,
}

/// Compute view geometry from a terminal rect. Pure function.
///
/// - Vertical: body (fills), prompt, footer.
/// - Body: transcript.
pub fn compute_view(area: Rect) -> ViewState {
    let [body, prompt, footer] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(PROMPT_HEIGHT),
        Constraint::Length(FOOTER_HEIGHT),
    ])
    .areas(area);

    ViewState { area, transcript: body, prompt, footer }
}

/// Render the whole screen from `app` state into `frame`.
pub fn render(frame: &mut Frame, app: &App) {
    style::set_theme(app.theme);
    let view = compute_view(frame.area());
    render_transcript(frame, app, view.transcript);
    render_prompt(frame, app, view.prompt);
    render_footer(frame, app, view.footer);

    match app.mode {
        Mode::Help => render_help_overlay(frame, frame.area()),
        Mode::FilePicker => render_file_picker_overlay(frame, frame.area(), app),
        _ => (),
    }
}

/// Render a centered help overlay listing available commands and keybindings.
fn render_help_overlay(frame: &mut Frame, area: Rect) {
    let p = style::palette();
    let help_lines = vec![
        Line::from(vec![Span::styled("  Key          Action", style::title_style())]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Enter        ", style::subtle_style()),
            Span::styled("submit prompt / execute command", style::text_style()),
        ]),
        Line::from(vec![
            Span::styled("  Shift+Enter  ", style::subtle_style()),
            Span::styled("insert newline (Ctrl+J)", style::text_style()),
        ]),
        Line::from(vec![
            Span::styled("  Esc          ", style::subtle_style()),
            Span::styled("cancel stream / close overlay", style::text_style()),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+T       ", style::subtle_style()),
            Span::styled("toggle running input target", style::text_style()),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+P       ", style::subtle_style()),
            Span::styled("open file picker", style::text_style()),
        ]),
        Line::from(vec![
            Span::styled("  Up/Down      ", style::subtle_style()),
            Span::styled("recall prompt history", style::text_style()),
        ]),
        Line::from(vec![
            Span::styled("  ← →          ", style::subtle_style()),
            Span::styled("move cursor (Ctrl+B/F)", style::text_style()),
        ]),
        Line::from(vec![
            Span::styled("  Home/End     ", style::subtle_style()),
            Span::styled("line start/end (Ctrl+A/E)", style::text_style()),
        ]),
        Line::from(vec![
            Span::styled("  Alt+← →      ", style::subtle_style()),
            Span::styled("move by word (Ctrl+← →, Alt+B/F)", style::text_style()),
        ]),
        Line::from(vec![
            Span::styled("  Backspace    ", style::subtle_style()),
            Span::styled("delete char before cursor", style::text_style()),
        ]),
        Line::from(vec![
            Span::styled("  Delete       ", style::subtle_style()),
            Span::styled("delete char after cursor", style::text_style()),
        ]),
        Line::from(vec![
            Span::styled("  PgUp/PgDn    ", style::subtle_style()),
            Span::styled("jump 10; Ctrl+Alt+U/D", style::text_style()),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+Alt+Y/E ", style::subtle_style()),
            Span::styled("scroll by 1 line", style::text_style()),
        ]),
        Line::from(vec![
            Span::styled("  Mouse        ", style::subtle_style()),
            Span::styled("select text; --mouse enables wheel", style::text_style()),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+C       ", style::subtle_style()),
            Span::styled("quit immediately", style::text_style()),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+D x2    ", style::subtle_style()),
            Span::styled("quit (double-press within 3s)", style::text_style()),
        ]),
        Line::from(vec![Span::styled("  Command      Description", style::title_style())]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  :            ", style::subtle_style()),
            Span::styled("enter command mode", style::text_style()),
        ]),
        Line::from(vec![
            Span::styled("  ?            ", style::subtle_style()),
            Span::styled("toggle this help overlay", style::text_style()),
        ]),
        Line::from(vec![
            Span::styled("  /clear       ", style::subtle_style()),
            Span::styled("clear the transcript", style::text_style()),
        ]),
        Line::from(vec![
            Span::styled("  /quit  /exit ", style::subtle_style()),
            Span::styled("quit the app", style::text_style()),
        ]),
        Line::from(vec![
            Span::styled("  :help        ", style::subtle_style()),
            Span::styled("show this help overlay", style::text_style()),
        ]),
        Line::from(vec![
            Span::styled("  :bg          ", style::subtle_style()),
            Span::styled("list background processes", style::text_style()),
        ]),
        Line::from(""),
        Line::styled("  Press ? or Esc to close.", style::muted_style()),
    ];

    let overlay_height = help_lines.len() as u16 + 2;
    let overlay_width = 58.min(area.width);
    let overlay_y = area.height.saturating_sub(overlay_height) / 2;
    let overlay_x = (area.width.saturating_sub(overlay_width)) / 2;

    let overlay_area = Rect::new(overlay_x, overlay_y, overlay_width, overlay_height);

    frame.render_widget(Clear, overlay_area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Line::styled(" Help ", style::title_style()))
        .border_style(style::border_style())
        .style(Style::default().bg(p.panel_bg));
    let inner = block.inner(overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(help_lines)).style(style::text_style()), inner);

    let _ = Layout::vertical([Constraint::Length(0)]);
}

fn render_file_picker_overlay(frame: &mut Frame, area: Rect, app: &App) {
    let Some(picker) = app.file_picker.as_ref() else {
        return;
    };
    let p = style::palette();
    let rows = picker.matches.len().clamp(1, FILE_PICKER_VISIBLE_ROWS);
    let overlay_height = (rows as u16 + 4).min(area.height);
    let overlay_width = 72.min(area.width);
    let overlay_y = area
        .height
        .saturating_sub(overlay_height + PROMPT_HEIGHT + FOOTER_HEIGHT);
    let overlay_x = (area.width.saturating_sub(overlay_width)) / 2;
    let overlay_area = Rect::new(overlay_x, overlay_y, overlay_width, overlay_height);

    frame.render_widget(Clear, overlay_area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Line::styled(" Files ", style::title_style()))
        .border_style(style::border_style())
        .style(Style::default().bg(p.panel_bg));
    let inner = block.inner(overlay_area);
    frame.render_widget(block, overlay_area);

    let mut lines = Vec::new();
    lines.push(Line::from(vec![
        Span::styled("  › ", Style::default().fg(p.accent).bg(p.panel_bg)),
        Span::styled(picker.query.clone(), style::text_style()),
        Span::styled("█", Style::default().fg(p.accent).bg(p.panel_bg)),
    ]));
    lines.push(Line::styled("", style::panel_style()));

    if picker.matches.is_empty() {
        lines.push(Line::styled("  no matches", style::muted_style()));
    } else {
        let end = (picker.scroll + rows).min(picker.matches.len());
        for (idx, path) in picker.matches[picker.scroll..end].iter().enumerate() {
            let absolute_idx = picker.scroll + idx;
            let selected = absolute_idx == picker.selected;
            let marker = if selected { "›" } else { " " };
            let marker_style = if selected {
                Style::default()
                    .fg(p.accent)
                    .bg(p.surface0)
                    .add_modifier(Modifier::BOLD)
            } else {
                style::muted_style()
            };
            let path_style = if selected {
                Style::default().fg(p.text).bg(p.surface0).add_modifier(Modifier::BOLD)
            } else {
                style::text_style()
            };
            let available = inner.width.saturating_sub(5) as usize;
            lines.push(Line::from(vec![
                Span::styled("  ", style::text_style()),
                Span::styled(marker, marker_style),
                Span::styled(" ", style::text_style()),
                Span::styled(utils::truncate_ellipsis(path, available), path_style),
            ]));
        }
    }

    let status = format!(
        "  {}/{}  Enter select  Esc close",
        picker.selected.saturating_add(usize::from(!picker.matches.is_empty())),
        picker.matches.len()
    );
    lines.push(Line::styled(status, style::muted_style()));

    frame.render_widget(Paragraph::new(Text::from(lines)).style(style::panel_style()), inner);
}

/// Render newest entries fitting the viewport.
///
/// We show the FIGlet banner in the empty transcript state when the
/// transcript area is wide enough; otherwise plain placeholder text.
fn render_transcript(frame: &mut Frame, app: &App, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let p = style::palette();

    let preview_inner = Rect::new(
        area.x.saturating_add(1),
        area.y.saturating_add(1),
        area.width.saturating_sub(2),
        area.height.saturating_sub(2),
    );
    let banner_lines = if app.transcript.is_empty() { banner::banner_lines(preview_inner.width) } else { Vec::new() };
    let banner_height = banner_lines.len() as u16;
    let banner_max_width = banner_lines.iter().map(|l| l.len()).max().unwrap_or(0) as u16;
    let show_banner = app.transcript.is_empty()
        && banner_height > 1
        && preview_inner.height > banner_height
        && banner_max_width <= preview_inner.width;

    let session_title = transcript_title(app);
    let title = if area.width < 55 { Some("thndrs") } else { Some(session_title.as_str()) };
    let mut block = Block::bordered()
        .border_style(style::border_style())
        .style(style::panel_style());
    if let Some(title) = title {
        block = block.title(Line::styled(title, style::title_style()));
    }
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    if app.transcript.is_empty() {
        if show_banner {
            let total_padding = inner.height.saturating_sub(banner_height);
            let top_pad = total_padding / 3;

            let banner_text = Text::from(
                banner_lines
                    .iter()
                    .map(|l| {
                        let line_len = l.len() as u16;
                        let h_pad = inner.width.saturating_sub(line_len) / 2;
                        Line::styled(
                            format!("{}{}", " ".repeat(h_pad as usize), l),
                            Style::default().fg(p.accent).bg(p.panel_bg),
                        )
                    })
                    .collect::<Vec<Line>>(),
            );

            let mut all_lines: Vec<Line> = Vec::new();
            for _ in 0..top_pad {
                all_lines.push(Line::styled("", style::panel_style()));
            }
            all_lines.extend(banner_text.lines);
            frame.render_widget(Paragraph::new(Text::from(all_lines)).left_aligned(), inner);
        } else {
            let placeholder_lines = vec![
                Line::styled(
                    "thndrs",
                    Style::default()
                        .fg(p.accent)
                        .bg(p.panel_bg)
                        .add_modifier(Modifier::BOLD),
                ),
                Line::styled("Type your message below.", style::muted_style()),
            ];
            let total_height = placeholder_lines.len() as u16;
            let top_pad = inner.height.saturating_sub(total_height) / 3;

            let mut all_lines: Vec<Line> = Vec::new();
            for _ in 0..top_pad {
                all_lines.push(Line::styled("", style::panel_style()));
            }
            for pl in &placeholder_lines {
                let line_len = pl.width() as u16;
                let h_pad = inner.width.saturating_sub(line_len) / 2;
                all_lines.push(Line::styled(format!("{}{}", " ".repeat(h_pad as usize), pl), pl.style));
            }
            frame.render_widget(Paragraph::new(Text::from(all_lines)).left_aligned(), inner);
        }
        return;
    }

    let mut lines: Vec<Line> = Vec::new();
    for (i, e) in app.transcript.iter().enumerate() {
        if i > 0 {
            let prev = &app.transcript[i - 1];
            if is_group_boundary(prev, e) {
                lines.push(Line::styled("", style::panel_style()));
            }
        }
        lines.extend(entry_lines_with_width(
            e,
            app.ui_tick,
            &app.user_label,
            inner.width as usize,
        ));
    }
    let available = inner.height as usize;
    let from_bottom = app.scroll_offset.min(lines.len().saturating_sub(1));
    let end = lines.len().saturating_sub(from_bottom);
    let start = end.saturating_sub(available);
    let visible: Vec<Line> = lines[start..end].to_vec();
    frame.render_widget(Paragraph::new(Text::from(visible)), inner);
}

fn render_prompt(frame: &mut Frame, app: &App, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let p = style::palette();

    let state = app.prompt_state();
    let divider_color = match state {
        PromptState::Editable => p.overlay0,
        PromptState::Submitted => p.yellow,
        PromptState::Streaming | PromptState::RunningTool => p.teal,
        PromptState::Stopped => p.overlay1,
        PromptState::Errored => p.red,
    };

    let block = Block::new()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(divider_color).bg(p.panel_bg))
        .style(style::panel_style());

    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    let (prompt_color, show_input, icon) = match state {
        PromptState::Editable => (p.yellow, true, "›"),
        PromptState::Submitted => (p.yellow, false, style::spinner_frame(app.ui_tick)),
        PromptState::Streaming | PromptState::RunningTool => (p.teal, true, style::spinner_frame(app.ui_tick)),
        PromptState::Stopped => (p.teal, true, "○"),
        PromptState::Errored => (p.red, true, "✕"),
    };

    let [input_area, status_area] = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(inner);

    let mut input_spans = vec![
        Span::styled("  ", style::text_style()),
        Span::styled(icon, Style::default().fg(prompt_color).bg(p.panel_bg)),
        Span::styled("  ", style::text_style()),
    ];

    if show_input {
        let prefix_width = if app.mode == Mode::Command { 5 } else { 4 };
        let input_width = input_area.width as usize;
        let visible_input_width = input_width.saturating_sub(prefix_width + 1);
        if app.mode == Mode::Command {
            input_spans.push(Span::styled(":", Style::default().fg(p.accent).bg(p.panel_bg)));
        }
        let input_text = app.input.as_str();
        let cursor = app.input.cursor();

        let text_len = input_text.chars().count();
        if text_len < visible_input_width {
            let before: String = input_text.chars().take(cursor).collect();
            let after: String = input_text.chars().skip(cursor).collect();
            input_spans.push(Span::styled(before, style::text_style()));
            input_spans.push(Span::styled("▏", Style::default().fg(prompt_color).bg(p.panel_bg)));
            input_spans.push(Span::styled(after, style::text_style()));
        } else {
            let avail = visible_input_width.saturating_sub(1);
            let start = cursor.saturating_sub(avail);
            let end = (start + avail).min(text_len);
            let before: String = input_text
                .chars()
                .skip(start)
                .take(cursor.saturating_sub(start))
                .collect();
            let after: String = input_text
                .chars()
                .skip(cursor)
                .take(end.saturating_sub(cursor))
                .collect();
            if start > 0 {
                input_spans.push(Span::styled("…", style::muted_style()));
            }
            input_spans.push(Span::styled(before, style::text_style()));
            input_spans.push(Span::styled("▏", Style::default().fg(prompt_color).bg(p.panel_bg)));
            input_spans.push(Span::styled(after, style::text_style()));
            if end < text_len {
                input_spans.push(Span::styled("…", style::muted_style()));
            }
        }
    }

    frame.render_widget(Paragraph::new(Line::from(input_spans)), input_area);

    if app.mode == Mode::Command {
        let suggestions = command_suggestions(app.input.as_str());
        let mut sug_spans: Vec<Span<'static>> = vec![Span::styled("  ", style::text_style())];
        for (i, (cmd, desc)) in suggestions.iter().enumerate() {
            if i > 0 {
                sug_spans.push(Span::styled("  ", style::text_style()));
            }
            let is_match = cmd.starts_with(app.input.as_str());
            let cmd_style = if is_match {
                Style::default()
                    .fg(p.accent)
                    .bg(p.surface0)
                    .add_modifier(Modifier::BOLD)
            } else {
                style::muted_style()
            };
            sug_spans.push(Span::styled(format!(":{cmd}"), cmd_style));
            sug_spans.push(Span::styled(format!(" {desc}"), style::subtle_style()));
        }
        if sug_spans.len() <= 1 {
            sug_spans.push(Span::styled("  (type a command…)", style::muted_style()));
        }
        frame.render_widget(Paragraph::new(Line::from(sug_spans)), status_area);
    } else {
        let mut status_spans: Vec<Span<'static>> = Vec::new();

        if matches!(state, PromptState::Stopped | PromptState::Errored) {
            let label = match state {
                PromptState::Stopped => "Stopped",
                PromptState::Errored => "Error",
                _ => "",
            };
            status_spans.push(Span::styled("  ", style::text_style()));
            status_spans.push(Span::styled(
                label.to_string(),
                Style::default().fg(prompt_color).bg(p.panel_bg),
            ));
        }

        if matches!(
            state,
            PromptState::Submitted | PromptState::Streaming | PromptState::RunningTool
        ) {
            let queue = format!(
                "  target: {}  queued: {}/{}  Ctrl+T toggles",
                app.queue_target.label(),
                app.queued_steering.len(),
                app.queued_followups.len()
            );
            status_spans.push(Span::styled(queue, style::subtle_style()));
        }

        if status_spans.is_empty() {
            status_spans.push(Span::styled(" ", style::text_style()));
        }

        frame.render_widget(Paragraph::new(Line::from(status_spans)), status_area);
    }
}

fn transcript_title(app: &App) -> String {
    app.session_id.clone()
}

/// Available slash commands with short descriptions, used for the suggestion UI.
fn command_suggestions(_input: &str) -> Vec<(&'static str, &'static str)> {
    vec![
        ("clear", "clear transcript"),
        ("quit", "exit app"),
        ("exit", "exit app"),
        ("help", "show help"),
        ("bg", "list background processes"),
    ]
}

fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let p = style::palette();

    let label = app.status_label();
    let status_color = style::status_color(label);
    let search_label = app.websearch.label();
    let status_label = format!("{} {label}", style::status_icon(label, app.ui_tick));
    let model_label = format!("model: {}", app.model);
    let search_text = format!("search: {search_label}");
    let token_text = format!("tok: ↑{} ↓{}", app.session_tokens_in, app.session_tokens_out);

    let (show_model, show_search, show_tokens, show_cwd) = match area.width {
        w if w < 24 => (false, false, false, false),
        w if w < 42 => (true, false, false, false),
        w if w < 56 => (true, true, false, false),
        w if w < 80 => (true, true, true, false),
        _ => (true, true, true, true),
    };

    let mut spans: Vec<Span<'static>> = vec![
        Span::styled(" ", style::text_style()),
        Span::styled(status_label.clone(), Style::default().fg(status_color).bg(p.panel_bg)),
    ];

    if show_model {
        spans.push(Span::styled("  ", style::text_style()));
        spans.push(style::muted_chip(&model_label));
    }
    if show_search {
        spans.push(Span::styled(" ", style::text_style()));
        spans.push(style::muted_chip(&search_text));
    }
    if show_tokens {
        spans.push(Span::styled(" ", style::text_style()));
        spans.push(style::muted_chip(&token_text));
    }
    if show_cwd {
        let status_len = text_width(&status_label) + 1;
        let model_len = if show_model { text_width(&model_label) + 4 } else { 0 };
        let search_len = if show_search { text_width(&search_text) + 3 } else { 0 };
        let token_len = if show_tokens { text_width(&token_text) + 3 } else { 0 };
        let used = status_len + model_len + search_len + token_len;
        spans.push(Span::styled(" ", style::text_style()));
        spans.push(Span::styled(
            path_display::footer_segment(&app.cwd, area.width as usize, used),
            style::muted_style(),
        ));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)).style(style::panel_style()), area);
}

fn text_width(text: &str) -> usize {
    text.chars().count()
}

/// Whether a blank separator line should be inserted between two consecutive
/// transcript entries.
///
/// Gaps are inserted between different semantic groups: user→assistant,
/// assistant→tool, tool→assistant, etc. No gap within the same type (e.g.
/// streaming deltas), and no gap around Status/Error entries (they're
/// transient and sit close to their context).
fn is_group_boundary(prev: &Entry, curr: &Entry) -> bool {
    let prev_type = EntryGroup::from(prev);
    let curr_type = EntryGroup::from(curr);

    if prev_type == EntryGroup::Transient || curr_type == EntryGroup::Transient {
        false
    } else {
        prev_type != curr_type
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{Entry, Mode, RunState, ToolStatus};
    use crate::cli::Cli;
    use crate::input::PromptInput;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::path::PathBuf;

    fn app() -> App {
        let dir = tempfile::tempdir().expect("create temp dir");
        let cli = Cli { cwd: dir.path().to_path_buf(), ..Cli::default() };
        let mut app = App::from_cli(&cli);
        app.session_id = String::from("session-20260701-120000");
        app.user_label = String::from("User (owais)");
        app.session_writer = None;
        app.cwd = PathBuf::from("/repo");
        app
    }

    #[test]
    fn compute_view_uses_full_width_for_transcript() {
        let area = Rect::new(0, 0, 80, 24);
        let view = compute_view(area);
        assert_eq!(view.prompt.height, PROMPT_HEIGHT);
        assert_eq!(view.footer.height, FOOTER_HEIGHT);
        assert_eq!(view.transcript.width, 80);
        assert!(view.transcript.height > 0);
    }

    #[test]
    fn compute_view_narrow_width_still_uses_full_transcript() {
        let area = Rect::new(0, 0, 40, 24);
        let view = compute_view(area);
        assert_eq!(view.transcript.width, 40);
    }

    #[test]
    fn compute_view_tiny_terminal_does_not_panic() {
        let area = Rect::new(0, 0, 20, 5);
        let view = compute_view(area);
        assert_eq!(view.prompt.height, PROMPT_HEIGHT);
        assert_eq!(view.footer.height, FOOTER_HEIGHT);
    }

    #[test]
    fn compute_view_normal_rect_full_layout() {
        let area = Rect::new(0, 0, 80, 24);
        let view = compute_view(area);

        assert_eq!(view.transcript.x, 0);
        assert_eq!(view.transcript.width, 80);

        assert!(view.transcript.y + view.transcript.height <= view.prompt.y);
        assert_eq!(view.prompt.height, PROMPT_HEIGHT);
        assert_eq!(view.prompt.y + view.prompt.height, view.footer.y);
        assert_eq!(view.footer.height, FOOTER_HEIGHT);
        assert_eq!(view.footer.y + view.footer.height, area.height);
    }

    #[test]
    fn compute_view_narrow_rect_full_width() {
        let area = Rect::new(0, 0, 40, 24);
        let view = compute_view(area);
        assert_eq!(view.transcript.x, 0);
        assert_eq!(view.transcript.width, 40);
    }

    #[test]
    fn compute_view_tiny_rect_reserves_prompt_and_footer() {
        let area = Rect::new(0, 0, 20, 5);
        let view = compute_view(area);
        assert_eq!(view.prompt.height, PROMPT_HEIGHT);
        assert_eq!(view.footer.height, FOOTER_HEIGHT);
        assert_no_overlap(&view);
    }

    #[test]
    fn compute_view_extreme_tiny_height() {
        let area = Rect::new(0, 0, 30, PROMPT_HEIGHT + FOOTER_HEIGHT);
        let view = compute_view(area);
        assert_no_overlap(&view);
        assert_eq!(view.prompt.height + view.footer.height, area.height);
    }

    #[test]
    fn compute_view_below_prompt_plus_footer() {
        let area = Rect::new(0, 0, 30, 1);
        let view = compute_view(area);
        assert_no_overlap(&view);
    }

    #[test]
    fn compute_view_single_column_width() {
        let area = Rect::new(0, 0, 1, 24);
        let view = compute_view(area);
        assert_no_overlap(&view);
    }

    /// Assert that prompt and footer rects do not overlap each other or
    /// the transcript, and that no rect extends past the area boundary.
    fn assert_no_overlap(view: &ViewState) {
        assert!(view.transcript.right() <= view.area.right());
        assert!(view.prompt.right() <= view.area.right());
        assert!(view.footer.right() <= view.area.right());
        assert!(view.transcript.bottom() <= view.area.bottom());
        assert!(view.prompt.bottom() <= view.area.bottom());
        assert!(view.footer.bottom() <= view.area.bottom());

        assert!(
            view.prompt.y >= view.transcript.y + view.transcript.height,
            "prompt overlaps transcript"
        );

        assert!(
            view.footer.y >= view.prompt.y + view.prompt.height,
            "footer overlaps prompt"
        );
    }

    #[test]
    fn empty_shell_snapshot_80x24() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        let app = app();
        terminal.draw(|f| render(f, &app)).expect("draw empty shell");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn submitted_prompt_snapshot_80x24() {
        let mut app = app();
        app.transcript
            .push(Entry::User { text: String::from("explain this repo") });

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw submitted prompt");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn streaming_assistant_snapshot_80x24() {
        let mut app = app();
        app.run_state = RunState::Working;
        app.transcript
            .push(Entry::User { text: String::from("explain this repo") });

        app.transcript
            .push(Entry::Assistant { text: String::from("This is a fake streaming res..."), streaming: true });

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw streaming assistant");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn reasoning_snapshot_80x24() {
        let mut app = app();
        app.run_state = RunState::Working;
        app.transcript
            .push(Entry::User { text: String::from("explain this repo") });

        app.transcript.push(Entry::Reasoning {
            text: String::from("Let me think about this... The repo is a Rust + Ratatui harness."),
            streaming: true,
        });

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw reasoning");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn running_tool_snapshot_80x24() {
        let mut app = app();
        app.run_state = RunState::Working;
        app.transcript
            .push(Entry::User { text: String::from("explain this repo") });

        app.transcript
            .push(Entry::Reasoning { text: String::from("Let me read the Cargo.toml first."), streaming: false });

        app.transcript.push(Entry::Tool {
            name: String::from("read_file#0"),
            arguments: String::from("{\"path\":\"Cargo.toml\"}"),
            status: ToolStatus::Running,
            output: vec![String::from("Cargo.toml: 47 lines")],
        });

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw running tool");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn finished_state_snapshot_80x24() {
        let mut app = app();
        app.transcript
            .push(Entry::User { text: String::from("explain this repo") });

        app.transcript.push(Entry::Reasoning {
            text: String::from("Let me think about this... The repo is a Rust + Ratatui harness."),
            streaming: false,
        });

        app.transcript.push(Entry::Tool {
            name: String::from("read_file#0"),
            arguments: String::from("{\"path\":\"Cargo.toml\"}"),
            status: ToolStatus::Ok,
            output: vec![String::from("Cargo.toml: 47 lines")],
        });

        app.transcript
            .push(Entry::Assistant { text: String::from("This is a fake streaming response."), streaming: false });

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw finished state");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn normal_width_layout_snapshot_80x24() {
        let mut app = app();
        app.input = PromptInput::from_str("hello");
        app.transcript
            .push(Entry::User { text: String::from("explain this repo") });

        app.transcript
            .push(Entry::Assistant { text: String::from("response"), streaming: false });

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw normal width layout");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn narrow_width_layout_snapshot_40x24() {
        let mut app = app();
        app.input = PromptInput::from_str("hello");
        app.transcript
            .push(Entry::User { text: String::from("explain this repo") });

        app.transcript
            .push(Entry::Assistant { text: String::from("response"), streaming: false });

        let backend = TestBackend::new(40, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw narrow width layout");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn successful_tool_snapshot_80x24() {
        let mut app = app();
        app.transcript
            .push(Entry::User { text: String::from("list files in src") });
        app.transcript.push(Entry::Tool {
            name: String::from("find_files#0"),
            arguments: String::from("{\"pattern\":\"*.rs\"}"),
            status: ToolStatus::Ok,
            output: vec![
                String::from("src/main.rs"),
                String::from("src/cli.rs"),
                String::from("src/app.rs"),
                String::from("src/ui.rs"),
                String::from("src/agent.rs"),
                String::from("src/context.rs"),
                String::from("src/tools.rs"),
            ],
        });

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw successful tool");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn failed_tool_snapshot_80x24() {
        let mut app = app();
        app.transcript
            .push(Entry::User { text: String::from("read /etc/passwd") });
        app.transcript.push(Entry::Tool {
            name: String::from("read_file_range#0"),
            arguments: String::from("{\"path\":\"/etc/passwd\"}"),
            status: ToolStatus::Failed,
            output: Vec::new(),
        });
        app.transcript
            .push(Entry::Error { text: String::from("path escapes workspace root: /etc/passwd") });

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw failed tool");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn cancelled_state_snapshot_80x24() {
        let mut app = app();
        app.transcript
            .push(Entry::User { text: String::from("explain this repo") });
        app.transcript
            .push(Entry::Assistant { text: String::from("partial"), streaming: false });
        app.transcript.push(Entry::Status { text: String::from("cancelled") });

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw cancelled state");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn provider_error_snapshot_80x24() {
        let mut app = app();
        app.transcript
            .push(Entry::User { text: String::from("explain this repo") });
        app.transcript
            .push(Entry::Error { text: String::from("authentication failed (HTTP 401)") });

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw provider error");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn tool_output_truncation_snapshot_80x24() {
        let mut app = app();
        app.transcript
            .push(Entry::User { text: String::from("list all files") });

        let many: Vec<String> = (0..12).map(|i| format!("src/file_{i}.rs")).collect();
        app.transcript.push(Entry::Tool {
            name: String::from("find_files#0"),
            arguments: String::from("{\"pattern\":\"*.rs\"}"),
            status: ToolStatus::Ok,
            output: many,
        });

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw truncated tool output");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn banner_normal_width_snapshot_80x24() {
        let app = app();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw banner normal width");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn banner_narrow_width_snapshot_50x24() {
        let app = app();
        let backend = TestBackend::new(50, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw banner narrow width");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn search_started_snapshot_80x24() {
        let mut app = app();
        app.run_state = RunState::Working;
        app.transcript
            .push(Entry::User { text: String::from("search for rust async patterns") });
        app.transcript.push(Entry::Tool {
            name: String::from("web_search#search-0"),
            arguments: String::from("{\"query\":\"rust async patterns\"}"),
            status: ToolStatus::Running,
            output: Vec::new(),
        });

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw search started");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn search_result_snapshot_80x24() {
        let mut app = app();
        app.transcript
            .push(Entry::User { text: String::from("search for rust async patterns") });
        app.transcript.push(Entry::Tool {
            name: String::from("web_search#search-0"),
            arguments: String::from("{\"query\":\"rust async patterns\"}"),
            status: ToolStatus::Ok,
            output: vec![
                String::from("server-side search: rust async patterns"),
                String::from("result: https://tokio.rs/tutorial/async"),
                String::from("result: https://rust-lang.org/async-book/"),
            ],
        });
        app.transcript
            .push(Entry::Assistant { text: String::from("Here are the results..."), streaming: false });

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw search result");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn search_error_snapshot_80x24() {
        let mut app = app();
        app.transcript
            .push(Entry::User { text: String::from("search for something") });
        app.transcript.push(Entry::Tool {
            name: String::from("web_search#search-0"),
            arguments: String::from("{\"query\":\"something\"}"),
            status: ToolStatus::Failed,
            output: Vec::new(),
        });
        app.transcript
            .push(Entry::Error { text: String::from("search backend unavailable") });

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw search error");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn read_url_result_snapshot_80x24() {
        let mut app = app();
        app.transcript
            .push(Entry::User { text: String::from("read the tokio docs page") });
        app.transcript.push(Entry::Tool {
            name: String::from("read_url#fetch-0"),
            arguments: String::from("{\"url\":\"https://docs.rs/tokio/latest/tokio/\"}"),
            status: ToolStatus::Ok,
            output: vec![
                String::from("title: tokio::lib - Rust"),
                String::from("url: https://docs.rs/tokio/latest/tokio/"),
                String::from("status: 200"),
                String::from("diagnostics: status: 200, content_type: text/html, max_redirects: 5, timeout_secs: 15, max_bytes: 1048576"),
                String::from("# tokio"),
                String::from("A runtime for writing reliable asynchronous applications."),
            ],
        });

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw read_url result");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn read_url_error_snapshot_80x24() {
        let mut app = app();
        app.run_state = RunState::Working;
        app.transcript
            .push(Entry::User { text: String::from("read the internal admin page") });
        app.transcript.push(Entry::Tool {
            name: String::from("read_url#fetch-0"),
            arguments: String::from("{\"url\":\"http://127.0.0.1/admin\"}"),
            status: ToolStatus::Failed,
            output: Vec::new(),
        });

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw read_url error");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn write_success_snapshot_80x24() {
        let mut app = app();
        app.transcript
            .push(Entry::User { text: String::from("create a new file src/new_module.rs") });
        app.transcript.push(Entry::Tool {
            name: String::from("create_file#0"),
            arguments: String::from("{\"path\":\"src/new_module.rs\",\"content\":\"pub fn hello() {}\"}"),
            status: ToolStatus::Ok,
            output: vec![String::from("create src/new_module.rs (new file → 19 bytes)")],
        });

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw write success");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn write_failure_snapshot_80x24() {
        let mut app = app();
        app.transcript
            .push(Entry::User { text: String::from("create src/existing.rs") });
        app.transcript.push(Entry::Tool {
            name: String::from("create_file#0"),
            arguments: String::from("{\"path\":\"src/existing.rs\",\"content\":\"new content\"}"),
            status: ToolStatus::Failed,
            output: Vec::new(),
        });
        app.transcript
            .push(Entry::Error { text: String::from("file already exists: src/existing.rs") });

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw write failure");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn shell_success_snapshot_80x24() {
        let mut app = app();
        app.transcript
            .push(Entry::User { text: String::from("run cargo test") });
        app.transcript.push(Entry::Tool {
            name: String::from("run_shell#0"),
            arguments: String::from("{\"program\":\"cargo\",\"args\":[\"test\"]}"),
            status: ToolStatus::Ok,
            output: vec![
                String::from("$ cargo test [one-shot ok 120ms]"),
                String::from("── stdout ──"),
                String::from("running 3 tests"),
                String::from("test result: ok. 3 passed; 0 failed"),
            ],
        });

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw shell success");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn shell_failure_snapshot_80x24() {
        let mut app = app();
        app.transcript
            .push(Entry::User { text: String::from("run cargo build") });
        app.transcript.push(Entry::Tool {
            name: String::from("run_shell#0"),
            arguments: String::from("{\"program\":\"cargo\",\"args\":[\"build\"]}"),
            status: ToolStatus::Failed,
            output: vec![
                String::from("$ cargo build [one-shot failed 340ms]"),
                String::from("── stderr ──"),
                String::from("error[E0308]: mismatched types"),
            ],
        });

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw shell failure");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn shell_timeout_snapshot_80x24() {
        let mut app = app();
        app.transcript
            .push(Entry::User { text: String::from("run a long build") });
        app.transcript.push(Entry::Tool {
            name: String::from("run_shell#0"),
            arguments: String::from("{\"program\":\"cargo\",\"args\":[\"build\"]}"),
            status: ToolStatus::Failed,
            output: vec![
                String::from("$ cargo build [one-shot timeout 10000ms]"),
                String::from("── stderr ──"),
                String::from("Compiling thndrs v0.1.0"),
            ],
        });

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw shell timeout");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn shell_running_snapshot_80x24() {
        let mut app = app();
        app.run_state = RunState::Working;
        app.transcript
            .push(Entry::User { text: String::from("run cargo test") });
        app.transcript.push(Entry::Tool {
            name: String::from("run_shell#0"),
            arguments: String::from("{\"program\":\"cargo\",\"args\":[\"test\"]}"),
            status: ToolStatus::Running,
            output: Vec::new(),
        });

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw shell running");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn help_overlay_snapshot_80x24() {
        let mut app = app();
        app.mode = Mode::Help;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw help overlay");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn command_mode_prompt_snapshot_80x24() {
        let mut app = app();
        app.mode = Mode::Command;
        app.input = PromptInput::from_str("cle");

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw command mode prompt");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn rust_code_highlight_snapshot_80x24() {
        let mut app = app();
        app.transcript.push(Entry::User { text: String::from("read main.rs") });
        app.transcript.push(Entry::Tool {
            name: String::from("read_file_range#0"),
            arguments: String::from(r#"{"path":"src/main.rs","start_line":1,"end_line":5}"#),
            status: ToolStatus::Ok,
            output: vec![
                String::from("fn main() {"),
                String::from("    println!(\"hello\");"),
                String::from("}"),
            ],
        });

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw rust code highlight");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn json_output_highlight_snapshot_80x24() {
        let mut app = app();
        app.transcript.push(Entry::Tool {
            name: String::from("read_file_range#1"),
            arguments: String::from(r#"{"path":"Cargo.toml","start_line":1,"end_line":5}"#),
            status: ToolStatus::Ok,
            output: vec![
                String::from("[package]"),
                String::from("name = \"thndrs\""),
                String::from("version = \"0.1.0\""),
            ],
        });

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw toml highlight");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn shell_output_highlight_snapshot_80x24() {
        let mut app = app();
        app.transcript.push(Entry::Tool {
            name: String::from("run_shell#0"),
            arguments: String::from(r#"{"program":"echo","args":["hello"]}"#),
            status: ToolStatus::Ok,
            output: vec![String::from("hello")],
        });

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw shell output highlight");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn plain_text_tool_no_highlight_snapshot_80x24() {
        let mut app = app();
        app.transcript.push(Entry::Tool {
            name: String::from("find_files#0"),
            arguments: String::from(r#"{"pattern":"main"}"#),
            status: ToolStatus::Ok,
            output: vec![String::from("src/main.rs"), String::from("src/lib.rs")],
        });

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw plain text tool");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn diff_highlight_snapshot_80x24() {
        let mut app = app();
        app.transcript.push(Entry::Tool {
            name: String::from("replace_range#0"),
            arguments: String::from(r#"{"path":"src/main.rs"}"#),
            status: ToolStatus::Ok,
            output: vec![
                String::from("--- old"),
                String::from("+++ new"),
                String::from("-fn main() {"),
                String::from("+fn main() -> Result<(), Box<dyn Error>> {"),
                String::from("     println!(\"hello\");"),
                String::from("+    Ok(())"),
                String::from(" }"),
            ],
        });

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw diff highlight");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn rust_compiler_error_highlight_snapshot_80x24() {
        let mut app = app();
        app.transcript.push(Entry::Tool {
            name: String::from("run_shell#0"),
            arguments: String::from(r#"{"program":"cargo","args":["build"]}"#),
            status: ToolStatus::Failed,
            output: vec![
                String::from("$ cargo build [one-shot failed 340ms]"),
                String::from("── stderr ──"),
                String::from("error[E0308]: mismatched types"),
                String::from("  --> src/main.rs:10:5"),
                String::from("   |"),
                String::from("10 |     let x: i32 = \"hello\";"),
                String::from("   |                     ^^^^^^^"),
                String::from("   |"),
                String::from("   = expected `i32`, found `&str`"),
            ],
        });

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw rust compiler error");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn json_diagnostics_highlight_snapshot_80x24() {
        let mut app = app();
        app.transcript.push(Entry::Tool {
            name: String::from("read_file_range#0"),
            arguments: String::from(r#"{"path":"config.json","start_line":1,"end_line":10}"#),
            status: ToolStatus::Ok,
            output: vec![
                String::from("{"),
                String::from("  \"name\": \"thndrs\","),
                String::from("  \"version\": \"0.1.0\","),
                String::from("  \"dependencies\": {"),
                String::from("    \"clap\": \"4.6\","),
                String::from("    \"ratatui\": \"0.30\""),
                String::from("  }"),
                String::from("}"),
            ],
        });

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw json diagnostics");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn plain_prose_no_highlight_snapshot_80x24() {
        let mut app = app();
        app.transcript
            .push(Entry::User { text: String::from("explain this codebase") });
        app.transcript.push(Entry::Assistant {
            text: String::from("This is a Rust project using Ratatui for the TUI. It includes an agent loop, tool dispatch, and session persistence. The code is organized into modules for app state, UI rendering, tools, providers, and context management."),
            streaming: false,
        });

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw plain prose");
        insta::assert_snapshot!(terminal.backend().to_string());
    }
}
