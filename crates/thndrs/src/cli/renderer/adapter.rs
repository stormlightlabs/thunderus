//! iocraft adapter for bounded surface rendering.
//!
//! This module is the only place the direct TUI renderer calls iocraft. It
//! renders declarative iocraft elements into an inspectable canvas, then
//! converts the canvas back into the existing [`Row`] contract. It does not call
//! iocraft render loops, fullscreen mode, stdout, or stderr.

use iocraft::prelude::*;
use thndrs_agent::ToolStatus;

use crate::renderer::row::Row;
use crate::renderer::style::{self, CellStyle, Color as RendererColor, Span};
use crate::renderer::view::{
    ColumnAlignment, ColumnWidthPolicy, DiffDetailView, FocusedSurfaceView, HelpView, PermissionView, PickerView,
    SetupFormView, SurfaceRenderInput, SurfaceRenderer, SurfaceThemeView, TableCellView, TableView, ThemeRole,
    ToolDetailView,
};
use crate::utils;

const HELP_ROWS: &[(&str, &str)] = &[
    ("── Navigation ──", ""),
    ("Ctrl+O", "open output, diff, warning, or error detail"),
    ("Enter", "accept highlighted item"),
    ("Escape", "close help, files, or commands"),
    ("Up/Down", "move cursor or recall history"),
    ("Ctrl+P", "open workspace file picker"),
    ("Ctrl+T", "transpose characters"),
    ("── Editing ──", ""),
    ("Shift+Enter", "insert newline"),
    ("Ctrl+A/E", "move to start/end of line"),
    ("Ctrl+B/F", "move cursor left/right"),
    ("Ctrl+W", "delete previous word"),
    ("Ctrl+K", "delete to end of line"),
    ("Ctrl+U", "delete to start of line"),
    ("Ctrl+Y", "yank (paste) last kill"),
    ("Alt+B/F", "move word left/right"),
    ("Alt+D", "delete next word"),
    ("Alt+Bksp", "delete previous word"),
    ("── Files ──", ""),
    ("@path", "mention a file from fuzzy search"),
    ("── App ──", ""),
    ("Ctrl+C", "stop a running turn"),
    ("Tab", "accept a command or file suggestion"),
    ("Ctrl+D", "quit after double-press"),
];

/// iocraft-backed renderer for bounded focused surfaces.
#[derive(Default)]
pub struct IocraftSurfaceRenderer;

impl SurfaceRenderer for IocraftSurfaceRenderer {
    fn render_surface(&mut self, input: SurfaceRenderInput<'_>) -> Vec<Row> {
        render_surface(&input)
    }
}

struct ViewContent {
    title: String,
    status: String,
    body: Vec<SurfaceLine>,
    focus: Option<usize>,
    hints: String,
    border: ThemeRole,
}

#[derive(Clone)]
struct SurfaceLine {
    text: String,
    role: ThemeRole,
}

impl SurfaceLine {
    fn new(text: impl Into<String>, role: ThemeRole) -> Self {
        Self { text: text.into(), role }
    }

    fn text(text: impl Into<String>) -> Self {
        Self::new(text, ThemeRole::Text)
    }

    fn muted(text: impl Into<String>) -> Self {
        Self::new(text, ThemeRole::Muted)
    }

    fn selected(text: impl Into<String>) -> Self {
        Self::new(text, ThemeRole::Selected)
    }

    fn title(text: impl Into<String>) -> Self {
        Self::new(text, ThemeRole::Selected)
    }
}

/// Render a semantic focused surface through iocraft.
pub fn render_surface(input: &SurfaceRenderInput<'_>) -> Vec<Row> {
    if input.width == 0 || input.height == 0 {
        return Vec::new();
    }

    match input.surface {
        FocusedSurfaceView::None => Vec::new(),
        FocusedSurfaceView::Permission(permission) => {
            permission_rows(permission, input.width, input.height, input.theme)
        }
        FocusedSurfaceView::CommandPicker(picker) | FocusedSurfaceView::FilePicker(picker) => {
            quiet_picker_rows(picker, input.width, input.height)
        }
        FocusedSurfaceView::Help(help) => help_rows(help, input.width, input.height, input.theme),
        FocusedSurfaceView::ToolDetail(detail) => tool_detail_rows(detail, input.width, input.height, input.theme),
        FocusedSurfaceView::DiffDetail(detail) => diff_detail_rows(detail, input.width, input.height, input.theme),
        FocusedSurfaceView::TranscriptLens { selected_entry, scroll } => {
            transcript_lens_surface_rows(selected_entry, *scroll, input.width, input.height, input.theme)
        }
        FocusedSurfaceView::SetupForm(form) => setup_form_rows(form, input.width, input.height, input.theme),
        FocusedSurfaceView::StructuredTable(table) => table_rows(table, input.width, input.height, input.theme),
    }
}

/// Render a bounded transcript/detail lens with iocraft.
pub fn transcript_lens_rows(title: &str, body: &[String], width: usize, height: usize) -> Vec<Row> {
    if width == 0 || height == 0 {
        return Vec::new();
    }

    let content = body.join("\n");
    let view_width = width.min(u32::MAX as usize) as u32;
    let view_height = height.min(u32::MAX as usize) as u32;
    let body_height = height.saturating_sub(1).min(u32::MAX as usize) as u32;
    let mut element = element! {
        View(
            flex_direction: FlexDirection::Column,
            width: view_width,
            height: view_height,
        ) {
            Text(content: title.to_string(), weight: Weight::Bold)
            View(
                height: body_height,
            ) {
                ScrollView {
                    Text(content: content)
                }
            }
        }
    };

    let canvas = element.render(Some(width));
    canvas_to_rows(&canvas, width, CellStyle::default())
}

fn picker_content(picker: &PickerView, width: usize) -> ViewContent {
    let mut body = vec![SurfaceLine::muted(format!("filter: {}", query_label(&picker.query)))];
    let focus = if picker.items.is_empty() { None } else { Some(1 + picker.selected) };
    if picker.items.is_empty() {
        body.push(SurfaceLine::muted("no matches"));
    } else {
        body.extend(picker.items.iter().enumerate().map(|(index, item)| {
            let selected = index == picker.selected;
            let marker = if selected { "❯" } else { " " };
            let detail = if item.detail.is_empty() {
                String::new()
            } else {
                format!(
                    "  {}",
                    utils::truncate_ellipsis(&item.detail, width.saturating_sub(28).min(32))
                )
            };
            let label = utils::truncate_ellipsis_start(
                &item.label,
                width.saturating_sub(4 + utils::text_width(&detail)).max(1),
            );
            let text = format!("{marker} {label}{detail}");
            if selected { SurfaceLine::selected(text) } else { SurfaceLine::text(text) }
        }));
    }
    ViewContent {
        title: picker.title.clone(),
        status: format!(
            "focus: option {}/{}",
            picker.selected.saturating_add(1),
            picker.items.len().max(1)
        ),
        body,
        focus,
        hints: "Enter select · Esc close".to_string(),
        border: ThemeRole::Selected,
    }
}

fn quiet_picker_rows(picker: &PickerView, width: usize, height: usize) -> Vec<Row> {
    if width == 0 || height == 0 {
        return Vec::new();
    }
    let p = style::palette();
    let content = picker_content(picker, width);
    let rail_style = CellStyle::new().fg(p.overlay0);
    let mut header_spans = vec![
        Span::styled("│  ", rail_style),
        Span::styled(content.title.to_uppercase(), CellStyle::new().fg(p.yellow).bold()),
    ];
    let used = super::layout::spans_width(&header_spans);
    let status = content.status.strip_prefix("focus: ").unwrap_or(&content.status);
    let status_width = utils::text_width(status);
    let body_width = super::layout::content_width(width);
    if used + status_width + 2 <= body_width {
        header_spans.push(Span::plain(" ".repeat(body_width - used - status_width)));
        header_spans.push(Span::styled(status, CellStyle::new().fg(p.overlay1)));
    }

    let mut rows = vec![Row::padded(header_spans, width, CellStyle::new())];
    let body = layout_surface_body(&content, height.saturating_sub(1));
    for line in body {
        let text_style = match line.role {
            ThemeRole::Selected => CellStyle::new().fg(p.text).bold(),
            ThemeRole::Muted => CellStyle::new().fg(p.overlay0),
            role => theme_role_style(role),
        };
        rows.push(Row::padded(
            vec![Span::styled("│  ", rail_style), Span::styled(line.text, text_style)],
            width,
            CellStyle::new(),
        ));
    }
    rows.truncate(height);
    rows
}

fn help_rows(help: &HelpView, width: usize, height: usize, theme: &SurfaceThemeView) -> Vec<Row> {
    let body = HELP_ROWS
        .iter()
        .map(|(key, description)| {
            if description.is_empty() {
                SurfaceLine::new((*key).to_string(), ThemeRole::Selected)
            } else {
                let description =
                    if *key == "Ctrl+T" && help.queue_target_toggle { "toggle queue target" } else { description };
                SurfaceLine::text(format!("{key:<16}{description}"))
            }
        })
        .collect();
    render_bounded_view(
        &ViewContent {
            title: "help".to_string(),
            status: "focus: keyboard".to_string(),
            body,
            focus: Some(0),
            hints: "Esc close · keys remain active".to_string(),
            border: ThemeRole::Selected,
        },
        width,
        height,
        theme,
    )
}

fn permission_rows(permission: &PermissionView, width: usize, height: usize, theme: &SurfaceThemeView) -> Vec<Row> {
    let mut body = vec![SurfaceLine::new(format!("scope: {}", permission.scope), theme.warning)];
    let focus = if permission.options.is_empty() { None } else { Some(1 + permission.selected) };
    body.extend(permission.options.iter().enumerate().map(|(index, option)| {
        let selected = index == permission.selected;
        let marker = if selected { "❯" } else { " " };
        let text = format!("{marker} {}  [{}]", option.label, option.kind);
        if selected { SurfaceLine::selected(text) } else { SurfaceLine::text(text) }
    }));
    render_bounded_view(
        &ViewContent {
            title: format!("permission · {}", permission.title),
            status: "approval required · focused".to_string(),
            body,
            focus,
            hints: "Enter choose · Esc cancel".to_string(),
            border: ThemeRole::Warning,
        },
        width,
        height,
        theme,
    )
}

fn tool_detail_rows(detail: &ToolDetailView, width: usize, height: usize, theme: &SurfaceThemeView) -> Vec<Row> {
    let output_rows = detail
        .output
        .iter()
        .flat_map(|line| super::layout::wrap_text(line, width.saturating_sub(2).max(1)))
        .collect::<Vec<_>>();
    let mut body = if output_rows.is_empty() {
        vec![SurfaceLine::muted("no output")]
    } else {
        output_rows
            .into_iter()
            .skip(detail.scroll)
            .map(SurfaceLine::text)
            .collect()
    };
    body.insert(
        0,
        SurfaceLine::muted(format!("scroll: {} · entry: {}", detail.scroll, detail.entry_index)),
    );
    if detail.scroll > 0 {
        body.insert(1, SurfaceLine::muted(format!("… {} rows above", detail.scroll)));
    }
    let border = match detail.status {
        ToolStatus::Failed => ThemeRole::Error,
        ToolStatus::Cancelled => ThemeRole::Warning,
        _ => ThemeRole::Selected,
    };
    render_bounded_view(
        &ViewContent {
            title: detail.title.clone(),
            status: format!("{} · focus: output", detail.status.label()),
            body,
            focus: Some(1),
            hints: "↑/↓ scroll · Esc close".to_string(),
            border,
        },
        width,
        height,
        theme,
    )
}

fn diff_detail_rows(detail: &DiffDetailView, width: usize, height: usize, theme: &SurfaceThemeView) -> Vec<Row> {
    let body = detail
        .lines
        .iter()
        .map(|line| {
            if line.starts_with('+') && !line.starts_with("+++") {
                SurfaceLine::new(line.clone(), theme.diff_added)
            } else if line.starts_with('-') && !line.starts_with("---") {
                SurfaceLine::new(line.clone(), theme.diff_removed)
            } else {
                SurfaceLine::text(line.clone())
            }
        })
        .collect();
    render_bounded_view(
        &ViewContent {
            title: "diff".to_string(),
            status: format!(
                "+{} -{} · focus: output · {}",
                detail.summary.added,
                detail.summary.removed,
                if detail.summary.files.is_empty() {
                    "working tree".to_string()
                } else {
                    detail.summary.files.join(", ")
                }
            ),
            body,
            focus: None,
            hints: "↑/↓ scroll · Esc close".to_string(),
            border: ThemeRole::Selected,
        },
        width,
        height,
        theme,
    )
}

fn setup_form_rows(form: &SetupFormView, width: usize, height: usize, theme: &SurfaceThemeView) -> Vec<Row> {
    let mut body = form
        .validation_errors
        .iter()
        .map(|error| SurfaceLine::new(format!("! {error}"), theme.error))
        .collect::<Vec<_>>();
    body.extend(form.details.iter().cloned().map(SurfaceLine::muted));

    let field_focus = if form.actions.is_empty() { Some(form.focus_index) } else { None };
    body.extend(form.fields.iter().enumerate().map(|(index, field)| {
        let focused = index == form.focus_index || field.focused;
        let marker = if focused { "❯" } else { " " };
        let value = if field.secret && !field.value.is_empty() { "[hidden]".to_string() } else { field.value.clone() };
        let multiline = if field.multiline { " multiline" } else { "" };
        let error = field
            .error
            .as_ref()
            .map_or(String::new(), |error| format!("  ! {error}"));
        let text = format!("{marker} {}: {value}{multiline}{error}", field.label);
        if focused { SurfaceLine::selected(text) } else { SurfaceLine::text(text) }
    }));
    let action_offset = body.len();
    body.extend(form.actions.iter().enumerate().map(|(index, action)| {
        let selected = index == form.selected;
        let marker = if selected { "❯" } else { " " };
        let text = format!("{marker} {}", action.label);
        if selected { SurfaceLine::selected(text) } else { SurfaceLine::text(text) }
    }));
    render_bounded_view(
        &ViewContent {
            title: form.title.clone(),
            status: format!(
                "{} · focus: {}",
                form.status,
                if form.actions.is_empty() { "input" } else { "choice" }
            ),
            body,
            focus: form
                .actions
                .is_empty()
                .then_some(field_focus.unwrap_or_default())
                .or_else(|| (!form.actions.is_empty()).then_some(action_offset + form.selected)),
            hints: format!("{} · {} · Esc cancel", form.submit_label, form.cancel_label),
            border: ThemeRole::Selected,
        },
        width,
        height,
        theme,
    )
}

fn table_rows(table: &TableView, width: usize, height: usize, theme: &SurfaceThemeView) -> Vec<Row> {
    let body_width = width.saturating_sub(2);
    if width < 24 && !table.narrow_fallback.is_empty() {
        return render_bounded_view(
            &ViewContent {
                title: "context".to_string(),
                status: "focus: inspect".to_string(),
                body: table.narrow_fallback.iter().cloned().map(SurfaceLine::text).collect(),
                focus: None,
                hints: "Esc close".to_string(),
                border: ThemeRole::Selected,
            },
            width,
            height,
            theme,
        );
    }

    let widths = table_column_widths(table, body_width);
    let mut body = vec![SurfaceLine::title(table_line(&table.header, &widths))];
    body.push(SurfaceLine::muted(
        widths
            .iter()
            .map(|width| "─".repeat(*width))
            .collect::<Vec<_>>()
            .join(" "),
    ));
    body.extend(table.rows.iter().enumerate().map(|(index, row)| {
        let selected = table.selected_row == Some(index);
        let marker = if selected { "❯" } else { " " };
        let text = format!("{marker} {}", table_line(row, &widths));
        if selected { SurfaceLine::selected(text) } else { SurfaceLine::text(text) }
    }));
    let status = table.selected_row.map_or_else(
        || "focus: inspect".to_string(),
        |row| format!("focus: row {}/{}", row + 1, table.rows.len()),
    );
    render_bounded_view(
        &ViewContent {
            title: "context".to_string(),
            status,
            body,
            focus: table.selected_row.map(|row| row + 2),
            hints: "↑/↓ inspect · Esc close".to_string(),
            border: ThemeRole::Selected,
        },
        width,
        height,
        theme,
    )
}

fn transcript_lens_surface_rows(
    selected_entry: &Option<usize>, scroll: usize, width: usize, height: usize, theme: &SurfaceThemeView,
) -> Vec<Row> {
    render_bounded_view(
        &ViewContent {
            title: "transcript".to_string(),
            status: "focus: history".to_string(),
            body: vec![
                SurfaceLine::text(format!(
                    "entry: {}",
                    selected_entry.map_or_else(|| "latest".to_string(), |entry| entry.to_string())
                )),
                SurfaceLine::text(format!("scroll: {scroll}")),
            ],
            focus: None,
            hints: "↑/↓ scroll · Esc close".to_string(),
            border: ThemeRole::Selected,
        },
        width,
        height,
        theme,
    )
}

fn render_bounded_view(content: &ViewContent, width: usize, height: usize, theme: &SurfaceThemeView) -> Vec<Row> {
    if width == 0 || height == 0 {
        return Vec::new();
    }

    let palette = style::palette();
    let background = palette.surface0;
    let header = format!("{} · {}", content.title, content.status);
    let framed = width >= 8 && height >= 3;
    if framed {
        let inner_width = width.saturating_sub(2);
        let max_inner_height = height.saturating_sub(2);
        let body_rows = layout_surface_body(content, max_inner_height);
        let inner_height = body_rows.len().max(1).min(max_inner_height.max(1));
        let inner_rows = render_lines(&body_rows, inner_width, inner_height, theme);
        let border_style = theme_role_style(content.border).bg(background);
        let mut rows = Vec::with_capacity(inner_height + 2);
        rows.push(frame_edge(width, &header, true, border_style, background));
        rows.extend(
            inner_rows
                .into_iter()
                .map(|row| framed_content_row(row, width, border_style, background)),
        );
        rows.push(frame_edge(width, "", false, border_style, background));
        rows
    } else {
        let max_body_height = height.saturating_sub(1);
        let body_rows = layout_surface_body(content, max_body_height);
        let mut lines = Vec::with_capacity(body_rows.len() + 1);
        lines.push(SurfaceLine::new(header, content.border));
        lines.extend(body_rows);
        render_lines(&lines, width, height, theme)
    }
}

fn layout_surface_body(content: &ViewContent, max_lines: usize) -> Vec<SurfaceLine> {
    if max_lines == 0 {
        return Vec::new();
    }

    let hint = (!content.hints.is_empty()).then(|| SurfaceLine::muted(content.hints.clone()));
    let hint_lines = usize::from(hint.is_some());
    let body_budget = max_lines.saturating_sub(hint_lines);
    if content.body.len() <= body_budget {
        let mut rows = content.body.clone();
        if let Some(hint) = hint {
            rows.push(hint);
        }
        return rows;
    }

    let marker_budget = body_budget.saturating_sub(1);
    if marker_budget == 0 {
        if content.body.len() == 1 {
            return content.body.clone();
        }
        let (mut visible, above, below) = clip_surface_body(&content.body, content.focus, 1);
        if let Some(line) = visible.first_mut() {
            let hidden = match (above, below) {
                (0, below) => format!("… {below} below · "),
                (above, 0) => format!("… {above} above · "),
                (above, below) => format!("… {above} above · {below} below · "),
            };
            line.text = format!("{hidden}{}", line.text);
        } else {
            visible.push(SurfaceLine::muted("… content clipped"));
        }
        return visible;
    }
    let (visible, above, below) = clip_surface_body(&content.body, content.focus, marker_budget);
    let hidden = match (above, below) {
        (0, below) => format!("… {below} rows below"),
        (above, 0) => format!("… {above} rows above"),
        (above, below) => format!("… {above} rows above · {below} below"),
    };
    let mut rows = visible;
    if body_budget > 0 {
        rows.push(SurfaceLine::muted(hidden));
    }
    if let Some(hint) = hint {
        if rows.len() >= max_lines {
            rows.pop();
        }
        rows.push(hint);
    }
    rows.truncate(max_lines);
    rows
}

fn clip_surface_body(lines: &[SurfaceLine], focus: Option<usize>, budget: usize) -> (Vec<SurfaceLine>, usize, usize) {
    if budget == 0 || lines.is_empty() {
        return (Vec::new(), lines.len(), 0);
    }
    if lines.len() <= budget {
        return (lines.to_vec(), 0, 0);
    }

    let focus = focus
        .unwrap_or_else(|| lines.len().saturating_sub(1))
        .min(lines.len() - 1);
    let start = focus
        .saturating_add(1)
        .saturating_sub(budget)
        .min(lines.len().saturating_sub(budget));
    let end = start + budget;
    (lines[start..end].to_vec(), start, lines.len().saturating_sub(end))
}

fn frame_edge(width: usize, label: &str, top: bool, style: CellStyle, background: RendererColor) -> Row {
    let available = width.saturating_sub(2);
    let label = if top {
        format!(" {} ", utils::truncate_ellipsis(label, available.saturating_sub(2)))
    } else {
        String::new()
    };
    let label_width = utils::text_width(&label).min(available);
    let edge = "─".repeat(available.saturating_sub(label_width));
    let text = if top { format!("╭{label}{edge}╮") } else { format!("╰{}╯", "─".repeat(available)) };
    Row::padded(vec![Span::styled(text, style)], width, CellStyle::new().bg(background))
}

fn framed_content_row(row: Row, width: usize, border_style: CellStyle, background: RendererColor) -> Row {
    let mut spans = Vec::with_capacity(row.spans.len() + 2);
    spans.push(Span::styled("│", border_style));
    spans.extend(row.spans);
    spans.push(Span::styled("│", border_style));
    Row::padded(spans, width, CellStyle::new().bg(background))
}

fn render_lines(lines: &[SurfaceLine], width: usize, height: usize, theme: &SurfaceThemeView) -> Vec<Row> {
    if width == 0 || height == 0 {
        return Vec::new();
    }

    let p = style::palette();
    let bg = p.surface0;
    let view_width = width.min(u32::MAX as usize) as u32;
    let view_height = height.min(u32::MAX as usize) as u32;
    let content = lines
        .iter()
        .map(|line| utils::truncate_ellipsis(&line.text, width))
        .collect::<Vec<_>>()
        .join("\n");
    let mut element = element! {
        View(
            background_color: Some(bg),
            flex_direction: FlexDirection::Column,
            width: view_width,
            height: view_height,
        ) {
            Text(color: Some(p.text), content: content, wrap: TextWrap::NoWrap)
        }
    };
    let canvas = element.render(Some(width));
    let pad_style = CellStyle::default().bg(bg);
    let mut rows = canvas_to_rows(&canvas, width, pad_style);
    rows.truncate(height.min(lines.len()));
    while rows.len() < height.min(lines.len()) {
        rows.push(Row::blank(width, pad_style));
    }
    for (row, line) in rows.iter_mut().zip(lines.iter()) {
        restyle_row(row, line.role, theme);
    }
    rows
}

fn restyle_row(row: &mut Row, role: ThemeRole, theme: &SurfaceThemeView) {
    let p = style::palette();
    let bg = if role == theme.selected { p.surface1 } else { p.surface0 };
    let style = theme_role_style(role).bg(bg);
    let text = {
        let mut out = String::new();
        for span in &row.spans {
            out.push_str(&span.text);
        }
        out
    };
    *row = Row::padded(vec![Span::styled(text, style)], row.width, CellStyle::default().bg(bg));
}

fn canvas_to_rows(canvas: &Canvas, width: usize, pad_style: CellStyle) -> Vec<Row> {
    (0..canvas.height())
        .map(|y| {
            let mut spans = Vec::new();
            let mut current_text = String::new();
            let mut current_style: Option<CellStyle> = None;
            let mut skip_cells = 0usize;
            for x in 0..canvas.width().min(width) {
                if skip_cells > 0 {
                    skip_cells -= 1;
                    continue;
                }
                let cell = canvas.cell(x, y);
                let style = cell.map_or(pad_style, cell_style);
                let text = cell.and_then(CanvasCell::text).unwrap_or(" ");
                skip_cells = utils::text_width(text).saturating_sub(1);
                if current_style == Some(style) {
                    current_text.push_str(text);
                } else {
                    if let Some(style) = current_style.take() {
                        spans.push(Span::styled(std::mem::take(&mut current_text), style));
                    }
                    current_style = Some(style);
                    current_text.push_str(text);
                }
            }
            if let Some(style) = current_style {
                spans.push(Span::styled(current_text, style));
            }
            Row::padded(spans, width, pad_style)
        })
        .collect()
}

fn cell_style(cell: &CanvasCell) -> CellStyle {
    let mut style = CellStyle::new()
        .fg(cell
            .text_style()
            .and_then(|style| style.color)
            .unwrap_or(RendererColor::Reset))
        .bg(cell.background_color.unwrap_or(RendererColor::Reset));
    if let Some(text_style) = cell.text_style() {
        style.bold = text_style.weight == Weight::Bold;
        style.dim = text_style.weight == Weight::Light;
        style.italic = text_style.italic;
        style.underlined = text_style.underline;
        if text_style.invert {
            std::mem::swap(&mut style.fg, &mut style.bg);
        }
    }
    style
}

fn table_column_widths(table: &TableView, width: usize) -> Vec<usize> {
    let columns = table
        .header
        .len()
        .max(table.rows.iter().map(Vec::len).max().unwrap_or(0));
    if columns == 0 {
        return Vec::new();
    }
    let separators = columns.saturating_sub(1);
    let available = width.saturating_sub(2 + separators).max(columns);
    let mut widths = vec![1; columns];
    let mut flexible = Vec::new();
    let mut used = 0usize;

    for (index, column_width) in widths.iter_mut().enumerate().take(columns) {
        let policy = table
            .header
            .get(index)
            .map(|cell| cell.width)
            .unwrap_or(ColumnWidthPolicy::Flexible);
        *column_width = match policy {
            ColumnWidthPolicy::Fixed(width) => width.max(1),
            ColumnWidthPolicy::Percent(percent) => (available * percent as usize / 100).max(1),
            ColumnWidthPolicy::Flexible => {
                flexible.push(index);
                1
            }
        };
        used += *column_width;
    }

    if !flexible.is_empty() && used < available {
        let extra = (available - used) / flexible.len();
        for index in flexible {
            widths[index] += extra;
        }
    }
    widths
}

fn table_line(cells: &[TableCellView], widths: &[usize]) -> String {
    cells
        .iter()
        .enumerate()
        .map(|(index, cell)| align_cell(&cell.text, widths.get(index).copied().unwrap_or(1), cell.alignment))
        .collect::<Vec<_>>()
        .join(" ")
}

fn align_cell(text: &str, width: usize, alignment: ColumnAlignment) -> String {
    let text = utils::truncate_ellipsis(text, width);
    let text_width = utils::text_width(&text);
    if text_width >= width {
        return text;
    }
    let pad = width - text_width;
    match alignment {
        ColumnAlignment::Left => format!("{text}{}", " ".repeat(pad)),
        ColumnAlignment::Right => format!("{}{text}", " ".repeat(pad)),
        ColumnAlignment::Center => {
            let left = pad / 2;
            format!("{}{}{}", " ".repeat(left), text, " ".repeat(pad - left))
        }
    }
}

fn query_label(query: &str) -> String {
    if query.is_empty() { "type to filter".to_string() } else { query.to_string() }
}

fn theme_role_style(role: ThemeRole) -> CellStyle {
    let p = style::palette();
    match role {
        ThemeRole::Text => CellStyle::new().fg(p.text),
        ThemeRole::Muted => CellStyle::new().fg(p.overlay0),
        ThemeRole::Selected => CellStyle::new().fg(p.text).bold(),
        ThemeRole::Warning => CellStyle::new().fg(p.peach),
        ThemeRole::Error => CellStyle::new().fg(p.red),
        ThemeRole::DiffAdded => CellStyle::new().fg(p.green),
        ThemeRole::DiffRemoved => CellStyle::new().fg(p.red),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::row::Frame;
    use crate::renderer::view::{
        DiffSummaryView, PickerItemView, SetupFieldView, SurfaceThemeView, TableCellView, ThemeRole,
    };

    #[test]
    fn transcript_lens_uses_iocraft_canvas_without_render_loop() {
        let rows = transcript_lens_rows(
            "details",
            &["one".to_string(), "two".to_string(), "three".to_string()],
            24,
            3,
        );
        let text = rows.iter().map(Row::text).collect::<Vec<_>>().join("\n");

        assert!(
            text.contains("details"),
            "title should render through iocraft canvas:\n{text}"
        );
        assert!(
            text.contains("one"),
            "body should render through iocraft canvas:\n{text}"
        );
        assert!(
            rows.iter().all(|row| row.width == 24),
            "converted rows should preserve the existing row contract"
        );
    }

    #[test]
    fn focused_surface_renderer_renders_picker_rows() {
        let surface = FocusedSurfaceView::CommandPicker(PickerView {
            title: "commands".to_string(),
            query: "he".to_string(),
            selected: 1,
            items: vec![
                PickerItemView { label: "help".to_string(), detail: "show help".to_string() },
                PickerItemView { label: "health".to_string(), detail: "run doctor".to_string() },
            ],
        });
        let mut renderer = IocraftSurfaceRenderer;
        let rows = renderer.render_surface(SurfaceRenderInput {
            surface: &surface,
            theme: &test_theme(),
            width: 32,
            height: 4,
        });
        let text = rows.iter().map(Row::text).collect::<Vec<_>>().join("\n");

        assert!(text.contains("COMMANDS"));
        assert!(text.contains("❯ health"));
        assert!(
            !text.contains('╭'),
            "command picker should use a quiet rail instead of a box"
        );
        assert!(
            rows.iter()
                .all(|row| row.spans.iter().all(|span| span.style.bg == RendererColor::Reset))
        );
        assert!(rows.iter().all(|row| row.width == 32));
    }

    #[test]
    fn canvas_conversion_preserves_iocraft_text_style() {
        let mut element = element! {
            View(width: 16, height: 1) {
                Text(
                    color: Color::Red,
                    content: "warn",
                    weight: Weight::Bold,
                    decoration: TextDecoration::Underline,
                    italic: true,
                )
            }
        };
        let rows = canvas_to_rows(&element.render(Some(16)), 16, CellStyle::default());
        let style = rows[0]
            .spans
            .iter()
            .find(|span| span.text.contains("warn"))
            .expect("styled text span")
            .style;

        assert_eq!(style.fg, RendererColor::Red);
        assert!(style.bold);
        assert!(style.underlined);
        assert!(style.italic);
    }

    #[test]
    fn table_surface_uses_width_policy_and_narrow_fallback() {
        let surface = FocusedSurfaceView::StructuredTable(TableView {
            header: vec![
                TableCellView {
                    text: "name".to_string(),
                    alignment: ColumnAlignment::Left,
                    width: ColumnWidthPolicy::Flexible,
                },
                TableCellView {
                    text: "n".to_string(),
                    alignment: ColumnAlignment::Right,
                    width: ColumnWidthPolicy::Fixed(3),
                },
            ],
            rows: vec![vec![
                TableCellView {
                    text: "compile".to_string(),
                    alignment: ColumnAlignment::Left,
                    width: ColumnWidthPolicy::Flexible,
                },
                TableCellView {
                    text: "42".to_string(),
                    alignment: ColumnAlignment::Right,
                    width: ColumnWidthPolicy::Fixed(3),
                },
            ]],
            selected_row: Some(0),
            narrow_fallback: vec!["compile 42".to_string()],
        });
        let rows =
            render_surface(&SurfaceRenderInput { surface: &surface, theme: &test_theme(), width: 16, height: 3 });
        let text = rows.iter().map(Row::text).collect::<Vec<_>>().join("\n");

        assert!(text.contains("compile 42"));
    }

    #[test]
    fn file_picker_uses_quiet_rail_and_preserves_content_rows() {
        let surface = FocusedSurfaceView::FilePicker(PickerView {
            title: "files".to_string(),
            query: "missing".to_string(),
            selected: 0,
            items: Vec::new(),
        });

        let rows =
            render_surface(&SurfaceRenderInput { surface: &surface, theme: &test_theme(), width: 40, height: 8 });

        assert_eq!(rows.len(), 4);
        assert!(rows[0].text().contains("FILES"));
        assert!(!rows.iter().any(|row| row.text().contains('╭')));
        assert!(
            rows.iter()
                .all(|row| row.spans.iter().all(|span| span.style.bg == RendererColor::Reset))
        );
        assert!(rows.iter().any(|row| row.text().contains("no matches")));
    }

    #[test]
    fn snapshot_iocraft_focused_surfaces() {
        let cases = vec![
            (
                "command picker",
                FocusedSurfaceView::CommandPicker(PickerView {
                    title: "commands".to_string(),
                    query: "c".to_string(),
                    selected: 0,
                    items: vec![
                        PickerItemView { label: "clear".to_string(), detail: "clear transcript".to_string() },
                        PickerItemView { label: "config show".to_string(), detail: "redacted config".to_string() },
                    ],
                }),
            ),
            (
                "file picker",
                FocusedSurfaceView::FilePicker(PickerView {
                    title: "files".to_string(),
                    query: "src".to_string(),
                    selected: 1,
                    items: vec![
                        PickerItemView { label: "src/main.rs".to_string(), detail: "binary".to_string() },
                        PickerItemView {
                            label: "src/cli/renderer/adapter.rs".to_string(),
                            detail: "focused UI".to_string(),
                        },
                    ],
                }),
            ),
            (
                "help",
                FocusedSurfaceView::Help(HelpView { queue_target_toggle: false }),
            ),
            (
                "tool detail",
                FocusedSurfaceView::ToolDetail(ToolDetailView {
                    entry_index: 2,
                    title: "run_shell".to_string(),
                    status: ToolStatus::Failed,
                    scroll: 1,
                    output: vec![
                        "line 0".to_string(),
                        "error: missing semicolon".to_string(),
                        "line 2".to_string(),
                    ],
                }),
            ),
            (
                "diff detail",
                FocusedSurfaceView::DiffDetail(DiffDetailView {
                    summary: DiffSummaryView { files: vec!["src/lib.rs".to_string()], added: 1, removed: 1 },
                    lines: vec![
                        "--- a/src/lib.rs".to_string(),
                        "+++ b/src/lib.rs".to_string(),
                        "-old".to_string(),
                        "+new".to_string(),
                    ],
                }),
            ),
            (
                "setup form",
                FocusedSurfaceView::SetupForm(SetupFormView {
                    title: "setup".to_string(),
                    stage: "credential entry".to_string(),
                    status: "OpenCode Go · credential entry".to_string(),
                    details: vec!["Input is hidden.".to_string()],
                    fields: vec![SetupFieldView {
                        label: "credential".to_string(),
                        value: "sk-hidden".to_string(),
                        focused: true,
                        secret: true,
                        multiline: false,
                        error: None,
                    }],
                    focus_index: 0,
                    actions: Vec::new(),
                    selected: 0,
                    validation_errors: vec!["credential is required".to_string()],
                    submit_label: "submit".to_string(),
                    cancel_label: "cancel".to_string(),
                    complete: false,
                }),
            ),
            (
                "table",
                FocusedSurfaceView::StructuredTable(TableView {
                    header: vec![
                        TableCellView {
                            text: "command".to_string(),
                            alignment: ColumnAlignment::Left,
                            width: ColumnWidthPolicy::Percent(55),
                        },
                        TableCellView {
                            text: "status".to_string(),
                            alignment: ColumnAlignment::Center,
                            width: ColumnWidthPolicy::Flexible,
                        },
                    ],
                    rows: vec![
                        vec![
                            TableCellView {
                                text: "cargo test".to_string(),
                                alignment: ColumnAlignment::Left,
                                width: ColumnWidthPolicy::Percent(55),
                            },
                            TableCellView {
                                text: "ok".to_string(),
                                alignment: ColumnAlignment::Center,
                                width: ColumnWidthPolicy::Flexible,
                            },
                        ],
                        vec![
                            TableCellView {
                                text: "cargo clippy".to_string(),
                                alignment: ColumnAlignment::Left,
                                width: ColumnWidthPolicy::Percent(55),
                            },
                            TableCellView {
                                text: "fix".to_string(),
                                alignment: ColumnAlignment::Center,
                                width: ColumnWidthPolicy::Flexible,
                            },
                        ],
                    ],
                    selected_row: Some(1),
                    narrow_fallback: vec!["cargo test ok".to_string(), "cargo clippy fix".to_string()],
                }),
            ),
        ];

        let mut rendered = String::new();
        for (label, surface) in cases {
            let rows =
                render_surface(&SurfaceRenderInput { surface: &surface, theme: &test_theme(), width: 48, height: 5 });
            rendered.push_str(label);
            rendered.push_str(":\n");
            rendered.push_str(&Frame { rows, width: 48, cursor: None, cursor_visible: true }.render_styled());
            rendered.push('\n');
        }

        assert!(!rendered.contains("sk-hidden"));
        insta::assert_snapshot!("iocraft_focused_surfaces", rendered);
    }

    fn test_theme() -> SurfaceThemeView {
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
