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
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Clear, Paragraph};

use crate::app::{
    App, Entry, FILE_PICKER_VISIBLE_ROWS, Mode, PromptAccessory, PromptState, RunState, command_suggestions_for_app,
};
use crate::{banner, utils};

use transcript::entry_blocks;

/// Maximum tool output lines rendered in the transcript before a truncation
/// marker is shown.
pub const MAX_TOOL_OUTPUT_LINES: usize = 6;

/// Minimum prompt region height: dynamic status line, input line.
const PROMPT_HEIGHT: u16 = 2;

const MAX_INPUT_ROWS: u16 = 8;

/// Footer region height: top padding, status line, bottom padding.
const FOOTER_HEIGHT: u16 = 3;

/// Height reserved by the inline Ratatui viewport. Ratatui fixes this height
/// when the terminal is created, so inline rendering places the prompt first
/// and uses the remaining rows only when overlays are open.
pub const INLINE_VIEWPORT_HEIGHT: u16 = PROMPT_HEIGHT + FOOTER_HEIGHT + FILE_PICKER_VISIBLE_ROWS as u16 + 5;

/// Precomputed layout rectangles plus display flags.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct ViewState {
    /// Full screen rect handed to `compute_view`.
    pub area: Rect,
    /// Live overlay area above prompt/footer.
    pub body: Rect,
    /// Prompt rect.
    pub prompt: Rect,
    /// Footer rect.
    pub footer: Rect,
}

/// Compute view geometry from a terminal rect. Pure function.
///
/// - Vertical: body (fills), prompt, footer.
#[cfg_attr(not(test), allow(dead_code))]
pub fn compute_view(area: Rect) -> ViewState {
    compute_view_with_prompt_height(area, PROMPT_HEIGHT)
}

fn compute_view_with_prompt_height(area: Rect, prompt_height: u16) -> ViewState {
    compute_view_with_chrome_heights(area, prompt_height, FOOTER_HEIGHT)
}

fn compute_view_with_chrome_heights(area: Rect, prompt_height: u16, footer_height: u16) -> ViewState {
    let [body, prompt, footer] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(prompt_height),
        Constraint::Length(footer_height),
    ])
    .areas(area);

    ViewState { area, body, prompt, footer }
}

/// Render the live shell from `app` state into `frame`.
pub fn render(frame: &mut Frame, app: &App) {
    render_shell(frame, app, true);
}

/// Render the live shell for Ratatui inline viewport mode.
pub fn render_inline(frame: &mut Frame, app: &App) {
    render_inline_shell(frame, app);
}

fn render_shell(frame: &mut Frame, app: &App, show_empty_banner: bool) {
    style::set_theme(app.theme);
    let area = frame.area();
    let inner_width = area.width as usize;
    let body_lines = shell_body_lines(app, inner_width, show_empty_banner);
    let prompt_height = prompt_height(app, area.width);
    let footer_height = FOOTER_HEIGHT;
    let view = compact_view(area, body_lines.len(), prompt_height, footer_height);
    render_body_lines(frame, view.body, body_lines);
    render_prompt(frame, app, view.prompt);
    render_footer(frame, app, view.footer);

    let _ = app.mode;
}

fn compact_view(area: Rect, body_lines: usize, prompt_height: u16, footer_height: u16) -> ViewState {
    let fixed_height = prompt_height + footer_height;
    if body_lines == 0 || body_lines as u16 + fixed_height >= area.height {
        return compute_view_with_chrome_heights(area, prompt_height, footer_height);
    }

    let body_height = (body_lines as u16).min(area.height.saturating_sub(fixed_height));
    let [body, prompt, footer, _rest] = Layout::vertical([
        Constraint::Length(body_height),
        Constraint::Length(prompt_height),
        Constraint::Length(footer_height),
        Constraint::Fill(1),
    ])
    .areas(area);

    ViewState { area, body, prompt, footer }
}

fn shell_body_lines(app: &App, width: usize, show_empty_banner: bool) -> Vec<Line<'static>> {
    if app.transcript.is_empty() && show_empty_banner {
        startup_screen_lines(app, width)
    } else {
        transcript_lines(&app.transcript, &app.user_label, width)
    }
}

fn render_inline_shell(frame: &mut Frame, app: &App) {
    style::set_theme(app.theme);
    let prompt_height = prompt_height(app, frame.area().width);
    let footer_height = FOOTER_HEIGHT;
    let [prompt, footer, _overlay] = Layout::vertical([
        Constraint::Length(prompt_height),
        Constraint::Length(footer_height),
        Constraint::Fill(1),
    ])
    .areas(frame.area());

    render_prompt(frame, app, prompt);
    render_footer(frame, app, footer);

    let _ = app.mode;
}

/// Render transcript entries as lines suitable for insertion into terminal
/// scrollback above the inline viewport.
pub fn transcript_lines(entries: &[Entry], user_label: &str, width: usize) -> Vec<Line<'static>> {
    entry_blocks(entries, user_label, width)
        .into_iter()
        .flat_map(|block| block.into_lines())
        .collect()
}

pub fn startup_banner_lines(app: &App, width: usize) -> Vec<Line<'static>> {
    style::set_theme(app.theme);
    startup_screen_lines(app, width)
}

fn startup_screen_lines(_app: &App, width: usize) -> Vec<Line<'static>> {
    let p = style::palette();
    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(surface_blank_line(p.surface0, width));
    let banner_width = width.saturating_sub(4) as u16;
    let banner_lines = banner::banner_lines(banner_width);
    let banner_is_art = banner_lines.len() > 1;
    lines.extend(banner_lines.into_iter().map(|line| {
        surface_line(
            vec![Span::styled(line, Style::default().fg(p.accent))],
            p.surface0,
            width,
        )
    }));
    if banner_is_art {
        lines.push(surface_divider_line(p.surface0, width));
    }
    lines.push(surface_line(
        vec![
            Span::styled("thndrs", Style::default().fg(p.accent).add_modifier(Modifier::BOLD)),
            Span::styled("  coding agent", style::subtle_style()),
        ],
        p.surface0,
        width,
    ));
    lines.push(surface_divider_line(p.surface0, width));
    let primary_tip = if width < 56 {
        "Ask for changes or commands."
    } else {
        "Ask for a change, run a command, or inspect this repo."
    };
    lines.push(surface_line(
        vec![
            Span::styled("›  ", Style::default().fg(p.accent)),
            Span::styled(
                utils::truncate_ellipsis(primary_tip, width.saturating_sub(4)),
                style::subtle_style(),
            ),
        ],
        p.surface0,
        width,
    ));
    lines.push(surface_line(
        vec![
            Span::styled("?  ", Style::default().fg(p.accent)),
            Span::styled("help", style::subtle_style()),
            Span::styled("   Ctrl+P ", Style::default().fg(p.accent)),
            Span::styled("files", style::subtle_style()),
        ],
        p.surface0,
        width,
    ));
    lines.push(surface_blank_line(p.surface0, width));
    lines
}

fn surface_divider_line(bg: Color, width: usize) -> Line<'static> {
    let p = style::palette();
    surface_line(
        vec![Span::styled(
            "─".repeat(width.saturating_sub(4)),
            Style::default().fg(p.overlay0),
        )],
        bg,
        width,
    )
}

fn surface_line(spans: Vec<Span<'static>>, bg: Color, width: usize) -> Line<'static> {
    if width == 0 {
        let mut line = Line::from("");
        line.style = Style::default().bg(bg);
        return line;
    }

    let left_pad = width.min(2);
    let right_pad = width.saturating_sub(left_pad).min(2);
    let body_width = width.saturating_sub(left_pad + right_pad);
    let mut out = Vec::new();
    let mut used = 0usize;
    if left_pad > 0 {
        out.push(Span::styled(" ".repeat(left_pad), Style::default().bg(bg)));
    }

    for span in spans {
        if used >= body_width {
            break;
        }
        let take = body_width - used;
        let content: String = span.content.chars().take(take).collect();
        used += content.chars().count();
        out.push(Span::styled(content, span.style.bg(bg)));
    }

    if used < body_width {
        out.push(Span::styled(" ".repeat(body_width - used), Style::default().bg(bg)));
    }
    if right_pad > 0 {
        out.push(Span::styled(" ".repeat(right_pad), Style::default().bg(bg)));
    }

    let mut line = Line::from(out);
    line.style = Style::default().bg(bg);
    line
}

fn surface_blank_line(bg: Color, width: usize) -> Line<'static> {
    surface_line(Vec::new(), bg, width)
}

fn surface_padding_width(width: usize) -> usize {
    let left_pad = width.min(2);
    let right_pad = width.saturating_sub(left_pad).min(2);
    left_pad + right_pad
}

fn render_body_lines(frame: &mut Frame, area: Rect, lines: Vec<Line<'static>>) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    frame.render_widget(Clear, area);

    if lines.is_empty() {
        render_surface_blank(frame, area, style::palette().panel_bg);
        return;
    }

    let start = lines.len().saturating_sub(area.height as usize);
    let visible = lines.into_iter().skip(start).collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(Text::from(visible)).style(style::panel_style()), area);
}

fn render_prompt(frame: &mut Frame, app: &App, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    frame.render_widget(Clear, area);

    let accessory_height = accessory_height(app, area.width).min(area.height.saturating_sub(3));
    let input_height = input_height(app, area.width).min(area.height.saturating_sub(2 + accessory_height));
    let [top_pad_area, dynamic_area, accessory_area, input_area, bottom_pad_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(accessory_height),
        Constraint::Length(input_height),
        Constraint::Length(1),
    ])
    .areas(area);

    render_surface_blank(frame, top_pad_area, style::palette().surface0);
    render_dynamic_status(frame, app, dynamic_area);
    render_prompt_accessory(frame, app, accessory_area);
    render_prompt_input(frame, app, input_area);
    render_surface_blank(frame, bottom_pad_area, style::palette().surface0);
}

fn render_surface_blank(frame: &mut Frame, area: Rect, bg: Color) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let lines = (0..area.height)
        .map(|_| surface_blank_line(bg, area.width as usize))
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(Text::from(lines)).style(style::panel_style()), area);
}

fn render_prompt_input(frame: &mut Frame, app: &App, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let p = style::palette();
    let state = app.prompt_state();
    let (prompt_color, show_input, icon) = match state {
        PromptState::Editable => (p.yellow, true, "›"),
        PromptState::Submitted => (p.yellow, false, "·"),
        PromptState::Streaming | PromptState::RunningTool => (p.teal, true, "›"),
        PromptState::Stopped => (p.teal, true, "○"),
        PromptState::Errored => (p.red, true, "✕"),
    };

    let mut rows = if show_input {
        prompt_input_lines(app, area.width as usize, icon, prompt_color)
    } else {
        vec![PromptRow {
            line: surface_line(
                vec![
                    Span::styled(icon, Style::default().fg(prompt_color).bg(p.surface0)),
                    Span::styled("  submitted", style::muted_style()),
                ],
                p.surface0,
                area.width as usize,
            ),
            cursor: true,
        }]
    };

    if rows.len() > area.height as usize {
        let cursor_row = rows
            .iter()
            .position(PromptRow::has_cursor)
            .unwrap_or_else(|| rows.len().saturating_sub(1));
        let max_rows = area.height as usize;
        let start = cursor_row.saturating_add(1).saturating_sub(max_rows);
        rows = rows.into_iter().skip(start).take(max_rows).collect();
    }

    let lines = rows.into_iter().map(|row| row.line).collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(Text::from(lines)).style(style::panel_style()), area);
}

fn render_dynamic_status(frame: &mut Frame, app: &App, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let p = style::palette();
    let label = app.status_label();
    let status_color = style::status_color(label);
    let status_text = format!("{} {label}", style::status_icon(label, app.ui_tick));
    let session = if app.session_id.is_empty() { "thndrs" } else { &app.session_id };

    let mut spans: Vec<Span<'static>> = vec![
        Span::styled(session.to_string(), style::title_style()),
        Span::styled("  ", style::text_style()),
        Span::styled(status_text, Style::default().fg(status_color)),
    ];

    if matches!(app.run_state, RunState::Working) {
        spans.push(Span::styled("  ", style::text_style()));
        spans.push(Span::styled(
            format!(
                "target: {}  queued: {}/{}",
                app.queue_target.label(),
                app.queued_steering.len(),
                app.queued_followups.len()
            ),
            style::subtle_style(),
        ));
    }

    frame.render_widget(
        Paragraph::new(surface_line(spans, p.surface0, area.width as usize)).style(style::panel_style()),
        area,
    );
}

fn render_prompt_accessory(frame: &mut Frame, app: &App, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let lines = match app.prompt_accessory {
        PromptAccessory::None => Vec::new(),
        PromptAccessory::Help => inline_help_lines(area.width as usize),
        PromptAccessory::Commands { selected } => command_accessory_lines(app, selected, area.width as usize),
        PromptAccessory::Files(_) => file_accessory_lines(app, area.width as usize),
    };

    let visible = lines.into_iter().take(area.height as usize).collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(Text::from(visible)).style(style::panel_style()), area);
}

fn inline_help_lines(width: usize) -> Vec<Line<'static>> {
    let p = style::palette();
    let rows = [
        ("Enter", "submit / accept highlighted item"),
        ("Shift+Enter", "insert newline"),
        ("Esc", "close help, files, or commands"),
        ("Ctrl+P", "pick a file"),
        ("@path", "mention a file from fuzzy search"),
        ("Up/Down", "select item or recall history"),
        ("Ctrl+A/E", "move to start/end"),
        ("Ctrl+B/F", "move left/right"),
    ];

    rows.into_iter()
        .map(|(key, desc)| {
            surface_line(
                vec![
                    Span::styled(
                        format!("{key:<12}"),
                        Style::default().fg(p.accent).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(desc, style::text_style()),
                ],
                p.surface0,
                width,
            )
        })
        .collect()
}

fn command_accessory_lines(app: &App, selected: usize, width: usize) -> Vec<Line<'static>> {
    let p = style::palette();
    let suggestions = command_suggestions_for_app(app);
    if suggestions.is_empty() {
        return vec![surface_line(
            vec![Span::styled("no commands", style::muted_style())],
            p.surface0,
            width,
        )];
    }

    suggestions
        .iter()
        .enumerate()
        .take(FILE_PICKER_VISIBLE_ROWS)
        .map(|(idx, (cmd, desc))| {
            let active = idx == selected.min(suggestions.len().saturating_sub(1));
            let bg = if active { p.surface1 } else { p.surface0 };
            let marker = if active { "›" } else { " " };
            surface_line(
                vec![
                    Span::styled(marker, Style::default().fg(p.accent).add_modifier(Modifier::BOLD)),
                    Span::styled("  /", style::muted_style()),
                    Span::styled(*cmd, Style::default().fg(p.text).add_modifier(Modifier::BOLD)),
                    Span::styled("  ", style::text_style()),
                    Span::styled(*desc, style::subtle_style()),
                ],
                bg,
                width,
            )
        })
        .collect()
}

fn file_accessory_lines(app: &App, width: usize) -> Vec<Line<'static>> {
    let p = style::palette();
    let Some(picker) = app.file_picker.as_ref() else {
        return vec![surface_line(
            vec![Span::styled("files loading", style::muted_style())],
            p.surface0,
            width,
        )];
    };

    let mut lines = Vec::new();
    lines.push(surface_line(
        vec![
            Span::styled("files", Style::default().fg(p.accent).add_modifier(Modifier::BOLD)),
            Span::styled("  ", style::text_style()),
            Span::styled(
                if picker.query.is_empty() { String::from("type to filter") } else { picker.query.clone() },
                style::muted_style(),
            ),
        ],
        p.surface0,
        width,
    ));

    if picker.matches.is_empty() {
        lines.push(surface_line(
            vec![Span::styled("no matches", style::muted_style())],
            p.surface0,
            width,
        ));
    } else {
        let rows = picker.matches.len().clamp(1, FILE_PICKER_VISIBLE_ROWS);
        let end = (picker.scroll + rows).min(picker.matches.len());
        for (idx, path) in picker.matches[picker.scroll..end].iter().enumerate() {
            let absolute_idx = picker.scroll + idx;
            let active = absolute_idx == picker.selected;
            let bg = if active { p.surface1 } else { p.surface0 };
            let marker = if active { "›" } else { " " };
            let available = width.saturating_sub(6);
            lines.push(surface_line(
                vec![
                    Span::styled(marker, Style::default().fg(p.accent).add_modifier(Modifier::BOLD)),
                    Span::styled("  ", style::text_style()),
                    Span::styled(utils::truncate_ellipsis(path, available), style::text_style()),
                ],
                bg,
                width,
            ));
        }
    }

    lines.push(surface_line(
        vec![
            Span::styled("Enter", Style::default().fg(p.accent).add_modifier(Modifier::BOLD)),
            Span::styled(" select   ", style::muted_style()),
            Span::styled("Esc", Style::default().fg(p.accent).add_modifier(Modifier::BOLD)),
            Span::styled(" close", style::muted_style()),
        ],
        p.surface0,
        width,
    ));
    lines
}

fn prompt_height(app: &App, width: u16) -> u16 {
    3 + accessory_height(app, width) + input_height(app, width)
}

fn accessory_height(app: &App, width: u16) -> u16 {
    match app.prompt_accessory {
        PromptAccessory::None => 0,
        PromptAccessory::Help => inline_help_lines(width as usize).len() as u16,
        PromptAccessory::Commands { .. } => command_suggestions_for_app(app)
            .len()
            .clamp(1, FILE_PICKER_VISIBLE_ROWS) as u16,
        PromptAccessory::Files(_) => {
            let rows = app
                .file_picker
                .as_ref()
                .map(|picker| picker.matches.len().clamp(1, FILE_PICKER_VISIBLE_ROWS) + 2)
                .unwrap_or(1);
            rows as u16
        }
    }
}

fn input_height(app: &App, width: u16) -> u16 {
    let body_width = input_body_width(app, width).max(1);
    prompt_wrapped_rows(app.input.as_str(), body_width)
        .len()
        .clamp(1, MAX_INPUT_ROWS as usize) as u16
}

fn input_body_width(app: &App, width: u16) -> usize {
    let horizontal_padding = surface_padding_width(width as usize);
    (width as usize)
        .saturating_sub(horizontal_padding + prompt_prefix_width(app))
        .max(1)
}

struct PromptRow {
    line: Line<'static>,
    cursor: bool,
}

impl PromptRow {
    fn has_cursor(&self) -> bool {
        self.cursor
    }
}

fn prompt_input_lines(app: &App, width: usize, icon: &'static str, prompt_color: Color) -> Vec<PromptRow> {
    let p = style::palette();
    let body_width = input_body_width(app, width as u16).max(1);
    let rows = prompt_wrapped_rows_with_cursor(app.input.as_str(), app.input.cursor(), body_width);
    rows.into_iter()
        .enumerate()
        .map(|(idx, row)| {
            let mut spans: Vec<Span<'static>> = if idx == 0 {
                let mut spans = vec![
                    Span::styled(icon, Style::default().fg(prompt_color).bg(p.surface0)),
                    Span::styled("  ", style::text_style()),
                ];
                if app.mode == Mode::Command {
                    spans.push(Span::styled(":", Style::default().fg(p.accent).bg(p.surface0)));
                }
                spans
            } else {
                let indent = " ".repeat(prompt_prefix_width(app));
                vec![Span::styled(indent, style::text_style())]
            };

            for part in row.parts {
                match part {
                    PromptPart::Text(text) => spans.push(Span::styled(text, style::text_style())),
                    PromptPart::Cursor => {
                        spans.push(Span::styled("▏", Style::default().fg(prompt_color).bg(p.surface0)))
                    }
                }
            }

            PromptRow { line: surface_line(spans, p.surface0, width), cursor: row.cursor }
        })
        .collect()
}

fn prompt_prefix_width(app: &App) -> usize {
    if app.mode == Mode::Command { 4 } else { 3 }
}

fn prompt_wrapped_rows(text: &str, width: usize) -> Vec<String> {
    let mut rows = Vec::new();
    let mut current = String::new();
    let mut used = 0;

    for ch in text.chars() {
        if ch == '\n' {
            rows.push(current);
            current = String::new();
            used = 0;
            continue;
        }

        if used >= width {
            rows.push(current);
            current = String::new();
            used = 0;
        }
        current.push(ch);
        used += 1;
    }

    rows.push(current);
    rows
}

enum PromptPart {
    Text(String),
    Cursor,
}

struct WrappedPromptRow {
    parts: Vec<PromptPart>,
    cursor: bool,
}

fn prompt_wrapped_rows_with_cursor(text: &str, cursor: usize, width: usize) -> Vec<WrappedPromptRow> {
    let mut rows = Vec::new();
    let mut parts = Vec::new();
    let mut buf = String::new();
    let mut used = 0;
    let mut pos = 0;
    let mut row_has_cursor = false;
    let mut placed_cursor = false;

    for ch in text.chars() {
        if pos == cursor {
            if !buf.is_empty() {
                parts.push(PromptPart::Text(std::mem::take(&mut buf)));
            }
            parts.push(PromptPart::Cursor);
            row_has_cursor = true;
            placed_cursor = true;
        }

        if ch == '\n' {
            if !buf.is_empty() {
                parts.push(PromptPart::Text(std::mem::take(&mut buf)));
            }
            rows.push(WrappedPromptRow { parts, cursor: row_has_cursor });
            parts = Vec::new();
            used = 0;
            row_has_cursor = false;
            pos += 1;
            continue;
        }

        if used >= width {
            if !buf.is_empty() {
                parts.push(PromptPart::Text(std::mem::take(&mut buf)));
            }
            rows.push(WrappedPromptRow { parts, cursor: row_has_cursor });
            parts = Vec::new();
            used = 0;
            row_has_cursor = false;
        }

        buf.push(ch);
        used += 1;
        pos += 1;
    }

    if !placed_cursor {
        if !buf.is_empty() {
            parts.push(PromptPart::Text(std::mem::take(&mut buf)));
        }
        parts.push(PromptPart::Cursor);
        row_has_cursor = true;
    } else if !buf.is_empty() {
        parts.push(PromptPart::Text(buf));
    }

    rows.push(WrappedPromptRow { parts, cursor: row_has_cursor });
    rows
}

fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    frame.render_widget(Clear, area);
    let p = style::palette();
    let search_label = app.websearch.label();
    let model_label = format!("model: {}", app.model);
    let search_text = format!("search: {search_label}");
    let token_text = format!("tok: ↑{} ↓{}", app.session_tokens_in, app.session_tokens_out);

    let status_area = if area.height >= 3 {
        let [top_pad_area, status_area, bottom_pad_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Length(1), Constraint::Length(1)]).areas(area);
        render_surface_blank(frame, top_pad_area, p.surface0);
        render_surface_blank(frame, bottom_pad_area, p.surface0);
        status_area
    } else {
        area
    };

    let (show_model, show_search, show_tokens, show_cwd) = match status_area.width {
        w if w < 24 => (false, false, false, false),
        w if w < 42 => (true, false, false, false),
        w if w < 56 => (true, true, false, false),
        w if w < 80 => (true, true, true, false),
        _ => (true, true, true, true),
    };

    let mut spans: Vec<Span<'static>> = Vec::new();

    if show_model {
        spans.push(Span::styled(model_label.clone(), style::subtle_style()));
    }
    if show_search {
        spans.push(Span::styled("   ", style::text_style()));
        spans.push(Span::styled(search_text.clone(), style::subtle_style()));
    }
    if show_tokens {
        spans.push(Span::styled("   ", style::text_style()));
        spans.push(Span::styled(token_text.clone(), style::subtle_style()));
    }
    if show_cwd {
        let model_len = if show_model { text_width(&model_label) } else { 0 };
        let search_len = if show_search { text_width(&search_text) + 3 } else { 0 };
        let token_len = if show_tokens { text_width(&token_text) + 3 } else { 0 };
        let used = 4 + model_len + search_len + token_len + 3;
        spans.push(Span::styled("   ", style::text_style()));
        spans.push(Span::styled(
            path_display::footer_segment(&app.cwd, status_area.width as usize, used),
            style::muted_style(),
        ));
    }

    frame.render_widget(
        Paragraph::new(surface_line(spans, p.surface0, status_area.width as usize)).style(style::panel_style()),
        status_area,
    );
}

fn text_width(text: &str) -> usize {
    text.chars().count()
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
    fn surface_line_paints_full_row_with_internal_padding() {
        let bg = Color::Blue;
        let line = surface_line(vec![Span::styled("hi", Style::default().fg(Color::White))], bg, 10);
        let rendered: String = line.spans.iter().map(|span| span.content.as_ref()).collect();

        assert_eq!(line.style.bg, Some(bg));
        assert_eq!(text_width(&rendered), 10);
        assert_eq!(line.spans.first().unwrap().content.as_ref(), "  ");
        assert_eq!(line.spans.last().unwrap().content.as_ref(), "  ");
        assert!(line.spans.iter().all(|span| span.style.bg == Some(bg)));
    }

    #[test]
    fn surface_line_does_not_exceed_tiny_width() {
        let bg = Color::Blue;
        let line = surface_line(vec![Span::styled("hello", Style::default().fg(Color::White))], bg, 3);
        let rendered: String = line.spans.iter().map(|span| span.content.as_ref()).collect();

        assert_eq!(line.style.bg, Some(bg));
        assert_eq!(text_width(&rendered), 3);
        assert!(line.spans.iter().all(|span| span.style.bg == Some(bg)));
    }

    #[test]
    fn startup_screen_lines_are_full_width_surface_rows() {
        let app = app();
        let width = 80;
        let bg = style::palette().surface0;
        let mut rendered_startup = String::new();

        for line in startup_screen_lines(&app, width) {
            let rendered: String = line.spans.iter().map(|span| span.content.as_ref()).collect();
            rendered_startup.push_str(&rendered);
            rendered_startup.push('\n');
            assert_eq!(line.style.bg, Some(bg));
            assert_eq!(text_width(&rendered), width);
            assert!(line.spans.iter().all(|span| span.style.bg == Some(bg)));
        }

        assert!(!rendered_startup.contains("model umans-coder"));
        assert!(!rendered_startup.contains("search auto"));
    }

    #[test]
    fn narrow_startup_uses_plain_title_banner() {
        let app = app();
        let first_line = startup_screen_lines(&app, 30)
            .into_iter()
            .find(|line| line.spans.iter().any(|span| !span.content.trim().is_empty()))
            .expect("non-empty startup line");
        let rendered: String = first_line.spans.iter().map(|span| span.content.as_ref()).collect();

        assert!(rendered.contains("THNDRS"));
        assert_eq!(text_width(&rendered), 30);
        assert_eq!(first_line.style.bg, Some(style::palette().surface0));
    }

    #[test]
    fn multiline_prompt_continuation_aligns_with_input_column() {
        let mut app = app();
        app.input = PromptInput::from_str("x\nx");
        let rows = prompt_input_lines(&app, 20, "›", style::palette().yellow);
        let rendered = rows
            .into_iter()
            .map(|row| {
                row.line
                    .spans
                    .into_iter()
                    .map(|span| span.content.to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>();

        let first_x = rendered[0].chars().position(|ch| ch == 'x');
        let second_x = rendered[1].chars().position(|ch| ch == 'x');
        assert_eq!(first_x, second_x);
    }

    #[test]
    fn compute_view_uses_full_width_for_body() {
        let area = Rect::new(0, 0, 80, 24);
        let view = compute_view(area);
        assert_eq!(view.prompt.height, PROMPT_HEIGHT);
        assert_eq!(view.footer.height, FOOTER_HEIGHT);
        assert_eq!(view.body.width, 80);
        assert!(view.body.height > 0);
    }

    #[test]
    fn compute_view_narrow_width_still_uses_full_body() {
        let area = Rect::new(0, 0, 40, 24);
        let view = compute_view(area);
        assert_eq!(view.body.width, 40);
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

        assert_eq!(view.body.x, 0);
        assert_eq!(view.body.width, 80);

        assert!(view.body.y + view.body.height <= view.prompt.y);
        assert_eq!(view.prompt.height, PROMPT_HEIGHT);
        assert_eq!(view.prompt.y + view.prompt.height, view.footer.y);
        assert_eq!(view.footer.height, FOOTER_HEIGHT);
        assert_eq!(view.footer.y + view.footer.height, area.height);
    }

    #[test]
    fn compute_view_narrow_rect_full_width() {
        let area = Rect::new(0, 0, 40, 24);
        let view = compute_view(area);
        assert_eq!(view.body.x, 0);
        assert_eq!(view.body.width, 40);
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
    /// the body, and that no rect extends past the area boundary.
    fn assert_no_overlap(view: &ViewState) {
        assert!(view.body.right() <= view.area.right());
        assert!(view.prompt.right() <= view.area.right());
        assert!(view.footer.right() <= view.area.right());
        assert!(view.body.bottom() <= view.area.bottom());
        assert!(view.prompt.bottom() <= view.area.bottom());
        assert!(view.footer.bottom() <= view.area.bottom());

        assert!(view.prompt.y >= view.body.y + view.body.height, "prompt overlaps body");

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
    fn inline_shell_places_prompt_at_top() {
        let backend = TestBackend::new(80, INLINE_VIEWPORT_HEIGHT);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        let app = app();

        terminal.draw(|f| render_inline(f, &app)).expect("draw inline shell");

        let output = terminal.backend().to_string();
        let session = output.find("session-20260701-120000").expect("session status");
        let input = output[session..].find("›").map(|idx| session + idx).expect("input row");
        let footer = output.find("model: umans-coder").expect("footer status");

        assert!(session < input);
        assert!(input < footer);
        assert!(!output.contains('│'));
        assert!(!output.contains('└'));
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
        app.prompt_accessory = PromptAccessory::Help;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw inline help");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn command_mode_prompt_snapshot_80x24() {
        let mut app = app();
        app.mode = Mode::Command;
        app.prompt_accessory = PromptAccessory::Commands { selected: 0 };
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
