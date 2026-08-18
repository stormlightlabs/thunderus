//! Pure presentation for bounded focused surfaces.
//!
//! Semantic surface state is projected into the shared [`Row`] contract.
//! Ratatui remains the sole terminal-cell renderer.

use thndrs_agent::ToolStatus;

use crate::renderer::key_hint::{KeyHint, render_key_hints};
use crate::renderer::row::Row;
use crate::renderer::style::{self, CellStyle, Span};
use crate::renderer::view::{
    ColumnAlignment, ColumnWidthPolicy, DiffDetailView, FocusedSurfaceView, HelpView, McpTrustView, PermissionView,
    PickerView, QueueView, SetupFormView, SurfaceRenderInput, SurfaceThemeView, TableCellView, TableView, ThemeRole,
    ToolDetailView, TranscriptSearchView,
};
use crate::utils;

struct ViewContent {
    title: String,
    status: String,
    body: Vec<SurfaceLine>,
    focus: Option<usize>,
    hints: Vec<KeyHint>,
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
        Self::new(text, ThemeRole::Primary)
    }

    fn muted(text: impl Into<String>) -> Self {
        Self::new(text, ThemeRole::Secondary)
    }

    fn selected(text: impl Into<String>) -> Self {
        Self::new(text, ThemeRole::Selection)
    }

    fn title(text: impl Into<String>) -> Self {
        Self::new(text, ThemeRole::Accent)
    }
}

/// Project a semantic focused surface into bounded presentation rows.
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
        FocusedSurfaceView::TranscriptSearch(search) => {
            transcript_search_rows(search, input.width, input.height, input.theme)
        }
        FocusedSurfaceView::Queue(queue) => queue_rows(queue, input.width, input.height, input.theme),
        FocusedSurfaceView::TranscriptLens { selected_entry, scroll } => {
            transcript_lens_surface_rows(selected_entry, *scroll, input.width, input.height, input.theme)
        }
        FocusedSurfaceView::SetupForm(form) => setup_form_rows(form, input.width, input.height, input.theme),
        FocusedSurfaceView::McpTrust(trust) => mcp_trust_rows(trust, input.width, input.height, input.theme),
        FocusedSurfaceView::StructuredTable(table) => table_rows(table, input.width, input.height, input.theme),
    }
}

fn transcript_search_rows(
    search: &TranscriptSearchView, width: usize, height: usize, theme: &SurfaceThemeView,
) -> Vec<Row> {
    let status = if search.query.is_empty() {
        "type to search".to_string()
    } else if search.total == 0 {
        "no matches".to_string()
    } else {
        format!(
            "{} of {}{}",
            search.current.unwrap_or(0),
            search.total,
            if search.truncated { "+" } else { "" }
        )
    };
    render_bounded_view(
        &ViewContent {
            title: format!("search  {}", search.query),
            status,
            body: Vec::new(),
            focus: None,
            hints: vec![
                KeyHint::new("enter/↓", "next"),
                KeyHint::new("shift+enter/↑", "previous"),
                KeyHint::new("esc", "cancel"),
            ],
            border: ThemeRole::Focus,
        },
        width,
        height,
        theme,
    )
}

fn queue_rows(queue: &QueueView, width: usize, height: usize, theme: &SurfaceThemeView) -> Vec<Row> {
    let mut body = Vec::new();
    if queue.items.is_empty() {
        body.push(SurfaceLine::muted("queue is empty"));
    } else {
        body.extend(queue.items.iter().enumerate().map(|(index, item)| {
            let line = format!(
                "{}  {}  {:<9} {:<10} {}",
                item.id, item.target, item.settlement, item.audit, item.preview
            );
            if index == queue.selected { SurfaceLine::selected(line) } else { SurfaceLine::text(line) }
        }));
        if let Some(item) = queue.items.get(queue.selected) {
            body.push(SurfaceLine::muted(format!("created {} · {}", item.created_at, item.id)));
        }
    }
    if let Some(editing) = &queue.editing {
        body.push(SurfaceLine::title(format!("edit: {editing}")));
    }
    render_bounded_view(
        &ViewContent {
            title: format!("queue  {} items", queue.items.len()),
            status: queue
                .items
                .get(queue.selected)
                .map(|item| format!("{} · {}", item.target, item.settlement))
                .unwrap_or_default(),
            body,
            focus: (!queue.items.is_empty()).then_some(queue.selected),
            hints: if queue.editing.is_some() {
                vec![KeyHint::new("enter", "save"), KeyHint::new("esc", "close")]
            } else {
                vec![
                    KeyHint::new("e", "edit"),
                    KeyHint::new("ctrl+↑↓", "reorder"),
                    KeyHint::new("t", "retarget"),
                    KeyHint::new("d", "delete"),
                    KeyHint::new("a", "after step"),
                    KeyHint::new("s", "send now"),
                    KeyHint::new("esc", "close"),
                ]
            },
            border: ThemeRole::Focus,
        },
        width,
        height,
        theme,
    )
}

/// Render a bounded transcript/detail lens.
pub fn transcript_lens_rows(title: &str, body: &[String], width: usize, height: usize) -> Vec<Row> {
    if width == 0 || height == 0 {
        return Vec::new();
    }

    let p = style::palette();
    let mut rows = Vec::with_capacity(height);
    rows.push(Row::padded(
        vec![Span::styled(
            utils::truncate_ellipsis(title, width),
            CellStyle::new().fg(p.primary).bold(),
        )],
        width,
        CellStyle::new().bg(p.surface),
    ));
    rows.extend(body.iter().take(height.saturating_sub(1)).map(|line| {
        Row::padded(
            vec![Span::plain(utils::truncate_ellipsis(line, width))],
            width,
            CellStyle::new().bg(p.surface),
        )
    }));
    rows
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
        hints: vec![
            KeyHint::new("↑↓", "navigate"),
            KeyHint::new("enter", "select"),
            KeyHint::new("esc", "close"),
        ],
        border: ThemeRole::Focus,
    }
}

fn quiet_picker_rows(picker: &PickerView, width: usize, height: usize) -> Vec<Row> {
    if width == 0 || height == 0 {
        return Vec::new();
    }
    let p = style::palette();
    let content = picker_content(picker, width);
    let mut header_spans = vec![
        Span::plain("  "),
        Span::styled(content.title.to_uppercase(), CellStyle::new().fg(p.warning).bold()),
    ];
    let used = super::layout::spans_width(&header_spans);
    let status = content.status.strip_prefix("focus: ").unwrap_or(&content.status);
    let status_width = utils::text_width(status);
    let body_width = super::layout::content_width(width);
    if used + status_width + 2 <= body_width {
        header_spans.push(Span::plain(" ".repeat(body_width - used - status_width)));
        header_spans.push(Span::styled(status, CellStyle::new().fg(p.secondary)));
    }

    let mut rows = vec![Row::padded(header_spans, width, CellStyle::new())];
    let body = layout_surface_body(&content, height.saturating_sub(2));
    for line in body {
        let text_style = match line.role {
            ThemeRole::Selection => CellStyle::new().fg(p.primary).bold(),
            ThemeRole::Secondary => CellStyle::new().fg(p.secondary),
            role => theme_role_style(role),
        };
        rows.push(Row::padded(
            vec![Span::plain("  "), Span::styled(line.text, text_style)],
            width,
            CellStyle::new(),
        ));
    }
    if rows.len() < height {
        rows.push(render_key_hints(&content.hints, width));
    }
    rows.truncate(height);
    rows
}

fn help_rows(help: &HelpView, width: usize, height: usize, theme: &SurfaceThemeView) -> Vec<Row> {
    let body = help
        .bindings
        .iter()
        .enumerate()
        .map(|(index, binding)| {
            let line = if binding.description.is_empty() {
                SurfaceLine::new(binding.key.clone(), ThemeRole::Accent)
            } else {
                SurfaceLine::text(format!("{:<16}{}", binding.key, binding.description))
            };
            if index == help.scroll { SurfaceLine::selected(line.text) } else { line }
        })
        .collect();
    render_bounded_view_with_background(
        &ViewContent {
            title: "help".to_string(),
            status: "focus: keyboard".to_string(),
            body,
            focus: Some(help.scroll),
            hints: vec![KeyHint::new("↑↓", "scroll"), KeyHint::new("esc", "close")],
            border: ThemeRole::Focus,
        },
        width,
        height,
        theme,
        style::Color::Reset,
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
            hints: vec![
                KeyHint::new("↑↓", "navigate"),
                KeyHint::new("enter", "choose"),
                KeyHint::new("esc", "cancel"),
            ],
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
        .map(|line| super::tool_output::sanitize_terminal_text(line))
        .flat_map(|line| super::layout::wrap_text(&line, width.saturating_sub(2).max(1)))
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
        ToolStatus::Running => ThemeRole::Active,
        ToolStatus::Ok => ThemeRole::Success,
        ToolStatus::Failed => ThemeRole::Failure,
        ToolStatus::Cancelled => ThemeRole::Warning,
    };
    render_bounded_view(
        &ViewContent {
            title: detail.title.clone(),
            status: format!("{} · focus: output", detail.status.label()),
            body,
            focus: Some(1),
            hints: vec![KeyHint::new("↑↓", "scroll"), KeyHint::new("esc", "close")],
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
            hints: vec![KeyHint::new("↑↓", "scroll"), KeyHint::new("esc", "close")],
            border: ThemeRole::Focus,
        },
        width,
        height,
        theme,
    )
}

fn mcp_trust_rows(trust: &McpTrustView, width: usize, height: usize, theme: &SurfaceThemeView) -> Vec<Row> {
    let mut body = vec![
        SurfaceLine::muted(format!("workspace: {}", trust.workspace)),
        SurfaceLine::muted(format!("config: {}", trust.config_path)),
        SurfaceLine::muted(format!("sha256: {}", trust.hash)),
    ];
    if trust.servers.is_empty() {
        body.push(SurfaceLine::muted("project configuration has no servers"));
    } else {
        body.push(SurfaceLine::title("servers:"));
        body.extend(trust.servers.iter().map(|server| {
            let replaces = if server.replaces_global { " · replaces global" } else { "" };
            SurfaceLine::text(format!("  {} · {}{}", server.name, server.transport, replaces))
        }));
    }
    if matches!(trust.action, crate::app::McpTrustAction::Trust) {
        body.push(SurfaceLine::new(
            "warning: not sandboxed; local stdio servers run with thndrs process permissions",
            theme.warning,
        ));
    } else {
        body.push(SurfaceLine::new(
            "warning: revoking trust deactivates the listed project definitions",
            theme.warning,
        ));
    }
    let actions_start = body.len();
    for (index, label) in [trust.action.label(), "cancel"].into_iter().enumerate() {
        let selected = index == trust.selected;
        let text = format!("{} {}", if selected { "❯" } else { " " }, label);
        body.push(if selected { SurfaceLine::selected(text) } else { SurfaceLine::text(text) });
    }
    render_bounded_view(
        &ViewContent {
            title: match trust.action {
                crate::app::McpTrustAction::Trust => "trust project MCP".to_string(),
                crate::app::McpTrustAction::Revoke => "revoke project MCP trust".to_string(),
            },
            status: "review required · focused".to_string(),
            body,
            focus: Some(actions_start + trust.selected),
            hints: vec![
                KeyHint::new("↑↓", "choose"),
                KeyHint::new("enter", "confirm"),
                KeyHint::new("esc", "cancel"),
            ],
            border: ThemeRole::Warning,
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
        .map(|error| SurfaceLine::new(format!("! {error}"), theme.failure))
        .collect::<Vec<_>>();
    let detail_width = width.saturating_sub(4).max(1);
    body.extend(
        form.details
            .iter()
            .flat_map(|detail| super::layout::wrap_text(detail, detail_width))
            .map(SurfaceLine::text),
    );

    let field_focus = if form.actions.is_empty() { Some(form.focus_index) } else { None };
    body.extend(form.fields.iter().enumerate().map(|(index, field)| {
        let focused = form.actions.is_empty() && (index == form.focus_index || field.focused);
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
            status: form.status.clone(),
            body,
            focus: form
                .actions
                .is_empty()
                .then_some(field_focus.unwrap_or_default())
                .or_else(|| (!form.actions.is_empty()).then_some(action_offset + form.selected)),
            hints: if form.actions.is_empty() {
                vec![
                    KeyHint::new("enter", form.submit_label.clone()),
                    KeyHint::new("esc", form.cancel_label.clone()),
                ]
            } else {
                vec![
                    KeyHint::new("↑↓", "choose"),
                    KeyHint::new("enter", "confirm"),
                    KeyHint::new("esc", form.cancel_label.clone()),
                ]
            },
            border: if form.attention { ThemeRole::Warning } else { ThemeRole::Focus },
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
                hints: vec![KeyHint::new("esc", "close")],
                border: ThemeRole::Focus,
            },
            width,
            height,
            theme,
        );
    }

    let widths = table_column_widths(table, body_width);
    let mut body = vec![SurfaceLine::title(table_line(&table.header, &widths))];
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
            focus: table.selected_row.map(|row| row + 1),
            hints: vec![KeyHint::new("↑↓", "inspect"), KeyHint::new("esc", "close")],
            border: ThemeRole::Focus,
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
            hints: vec![KeyHint::new("↑↓", "scroll"), KeyHint::new("esc", "close")],
            border: ThemeRole::Focus,
        },
        width,
        height,
        theme,
    )
}

fn render_bounded_view(content: &ViewContent, width: usize, height: usize, theme: &SurfaceThemeView) -> Vec<Row> {
    render_bounded_view_with_background(content, width, height, theme, style::palette().surface)
}

fn render_bounded_view_with_background(
    content: &ViewContent, width: usize, height: usize, theme: &SurfaceThemeView, background: style::Color,
) -> Vec<Row> {
    if width == 0 || height == 0 {
        return Vec::new();
    }

    let header = format!("{} · {}", content.title, content.status);
    let hint_height = usize::from(!content.hints.is_empty() && height > 1);
    let max_body_height = height.saturating_sub(1 + hint_height);
    let body_rows = layout_surface_body(content, max_body_height);
    let mut lines = Vec::with_capacity(body_rows.len() + 1);
    lines.push(SurfaceLine::new(header, content.border));
    lines.extend(body_rows);
    let mut rows = render_lines(&lines, width, height.saturating_sub(hint_height), theme, background);
    if hint_height > 0 {
        rows.push(render_key_hints(&content.hints, width));
    }
    rows
}

fn layout_surface_body(content: &ViewContent, max_lines: usize) -> Vec<SurfaceLine> {
    if max_lines == 0 {
        return Vec::new();
    }

    let body_budget = max_lines;
    if content.body.len() <= body_budget {
        return content.body.clone();
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

fn render_lines(
    lines: &[SurfaceLine], width: usize, height: usize, theme: &SurfaceThemeView, background: style::Color,
) -> Vec<Row> {
    if width == 0 || height == 0 {
        return Vec::new();
    }

    let p = style::palette();
    lines
        .iter()
        .take(height)
        .map(|line| {
            let row_background = if line.role == theme.selection { p.selection } else { background };
            let row_style = theme_role_style(line.role).bg(row_background);
            Row::padded(
                vec![Span::styled(utils::truncate_ellipsis(&line.text, width), row_style)],
                width,
                CellStyle::default().bg(row_background),
            )
        })
        .collect()
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
        ThemeRole::Selection => CellStyle::new().fg(p.primary).bold(),
        _ => CellStyle::new().fg(p.color(role)),
    }
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::renderer::row::Frame;
    use crate::renderer::view::{
        DiffSummaryView, McpTrustServerView, McpTrustView, PermissionOptionView, PickerItemView, SetupFieldView,
        SurfaceThemeView, TableCellView, ThemeRole,
    };

    #[test]
    fn transcript_lens_preserves_bounded_row_contract() {
        let rows = transcript_lens_rows(
            "details",
            &["one".to_string(), "two".to_string(), "three".to_string()],
            24,
            3,
        );
        let text = rows.iter().map(Row::text).collect::<Vec<_>>().join("\n");

        assert!(text.contains("details"), "title should remain visible:\n{text}");
        assert!(text.contains("one"), "body should remain visible:\n{text}");
        assert!(
            rows.iter().all(|row| row.width == 24),
            "surface rows should preserve the existing row contract"
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
        let rows =
            render_surface(&SurfaceRenderInput { surface: &surface, theme: &test_theme(), width: 32, height: 4 });
        let text = rows.iter().map(Row::text).collect::<Vec<_>>().join("\n");

        assert!(text.contains("COMMANDS"));
        assert!(text.contains("❯ health"));
        assert!(
            !text.contains('╭'),
            "command picker should use a quiet rail instead of a box"
        );
        assert!(rows.iter().all(|row| row.spans.iter().all(|span| {
            matches!(span.style.bg, style::Color::Reset) || span.style.bg == style::palette().selection
        })));
        assert!(rows.iter().any(|row| row.text().contains("↑↓  navigate")));
        assert!(rows.iter().all(|row| row.width == 32));
    }

    #[test]
    fn tool_detail_strips_terminal_controls_before_rendering() {
        let surface = FocusedSurfaceView::ToolDetail(ToolDetailView {
            entry_index: 0,
            title: "run_shell".to_string(),
            status: ToolStatus::Failed,
            scroll: 0,
            output: vec!["\u{1b}[31mfailed\u{1b}[0m \u{1b}]8;;https://example.com\u{7}link".to_string()],
        });

        let rows =
            render_surface(&SurfaceRenderInput { surface: &surface, theme: &test_theme(), width: 40, height: 8 });
        let text = rows.iter().map(Row::text).collect::<Vec<_>>().join("\n");

        assert!(text.contains("failed link"), "{text}");
        assert!(!text.contains('\u{1b}'), "{text:?}");
    }

    #[test]
    fn help_scroll_reveals_clipped_shortcuts() {
        let bindings = crate::app::Keymap::default().help_bindings(false);
        let first_key = bindings[0].key.clone();
        let last_key = bindings.last().unwrap().key.clone();
        let surface = FocusedSurfaceView::Help(HelpView { scroll: bindings.len().saturating_sub(1), bindings });
        let rows =
            render_surface(&SurfaceRenderInput { surface: &surface, theme: &test_theme(), width: 48, height: 8 });
        let text = rows.iter().map(Row::text).collect::<Vec<_>>().join("\n");

        assert!(text.contains(&last_key));
        assert!(!text.contains(&first_key));
        assert!(text.contains("rows above"));
        assert!(text.contains("↑↓  scroll"));
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
    fn file_picker_uses_borderless_rows_and_preserves_content() {
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
        assert!(rows.iter().all(|row| row.spans.iter().all(|span| {
            matches!(span.style.bg, style::Color::Reset) || span.style.bg == style::palette().selection
        })));
        assert!(rows.iter().any(|row| row.text().contains("no matches")));
    }

    #[test]
    fn mcp_trust_surface_keeps_warning_and_selected_action_at_constrained_widths() {
        let surface = FocusedSurfaceView::McpTrust(McpTrustView {
            action: crate::app::McpTrustAction::Trust,
            workspace: "/workspaces/example".to_string(),
            config_path: ".thndrs/mcp.toml".to_string(),
            hash: "0123456789abcdef0123456789abcdef".to_string(),
            servers: vec![
                McpTrustServerView { name: "docs".to_string(), transport: "Stdio".to_string(), replaces_global: true },
                McpTrustServerView {
                    name: "web".to_string(),
                    transport: "StreamableHttp".to_string(),
                    replaces_global: false,
                },
            ],
            selected: 0,
        });
        for (width, height) in [(80, 12), (30, 8), (16, 4)] {
            let rows = render_surface(&SurfaceRenderInput { surface: &surface, theme: &test_theme(), width, height });
            let text = rows.iter().map(Row::text).collect::<Vec<_>>().join("\n");
            assert!(rows.iter().all(|row| row.width == width));
            assert!(
                text.contains("❯ trust"),
                "selected action missing at {width}x{height}:\n{text}"
            );
            assert!(
                text.contains("↑↓  choose"),
                "keyboard hint missing at {width}x{height}:\n{text}"
            );
            if width == 80 {
                assert!(text.contains("workspace:"), "workspace missing:\n{text}");
                assert!(text.contains("config:"), "config path missing:\n{text}");
                assert!(text.contains("sha256:"), "hash missing:\n{text}");
                assert!(
                    text.contains("docs · Stdio · replaces global"),
                    "server replacement missing:\n{text}"
                );
                assert!(text.contains("not sandboxed"), "containment warning missing:\n{text}");
            }
        }
    }

    #[test]
    fn selected_surface_rows_keep_text_distinct_from_the_selection_fill() {
        let rows = render_lines(
            &[SurfaceLine::selected("selected")],
            20,
            1,
            &test_theme(),
            style::palette().surface,
        );
        let selected = rows[0]
            .spans
            .iter()
            .find(|span| span.text.contains("selected"))
            .expect("selected row text");

        assert_ne!(selected.style.fg, selected.style.bg);
        assert!(selected.style.bold);
    }

    #[test]
    fn snapshot_focused_surfaces() {
        let cases = vec![
            (
                "permission",
                FocusedSurfaceView::Permission(PermissionView {
                    title: "Run cargo test".to_string(),
                    scope: "local user · active tool only".to_string(),
                    selected: 1,
                    options: vec![
                        PermissionOptionView { label: "Allow once".to_string(), kind: "allow once".to_string() },
                        PermissionOptionView { label: "Reject".to_string(), kind: "reject once".to_string() },
                    ],
                }),
            ),
            (
                "MCP trust",
                FocusedSurfaceView::McpTrust(McpTrustView {
                    action: crate::app::McpTrustAction::Trust,
                    workspace: "/workspace/project".to_string(),
                    config_path: ".thndrs/mcp.toml".to_string(),
                    hash: "0123456789abcdef".to_string(),
                    servers: vec![McpTrustServerView {
                        name: "docs".to_string(),
                        transport: "Stdio".to_string(),
                        replaces_global: true,
                    }],
                    selected: 0,
                }),
            ),
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
                            label: "src/cli/renderer/surface.rs".to_string(),
                            detail: "focused UI".to_string(),
                        },
                    ],
                }),
            ),
            (
                "help",
                FocusedSurfaceView::Help(HelpView {
                    scroll: 0,
                    bindings: crate::app::Keymap::default().help_bindings(false),
                }),
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
                "transcript lens",
                FocusedSurfaceView::TranscriptLens { selected_entry: Some(12), scroll: 3 },
            ),
            (
                "setup form",
                FocusedSurfaceView::SetupForm(SetupFormView {
                    title: "setup".to_string(),
                    attention: false,
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
            assert!(
                rows.iter().all(|row| !row.text().contains(['╭', '╮', '╰', '╯', '│'])),
                "{label} should be borderless"
            );
            rendered.push_str(label);
            rendered.push_str(" buffer:\n");
            rendered.push_str(&ratatui_buffer_text(&rows, 48));
            rendered.push_str("styles:\n");
            rendered.push_str(&Frame { rows, width: 48, cursor: None, cursor_visible: true }.render_styled());
            rendered.push('\n');
        }

        assert!(!rendered.contains("sk-hidden"));
        insta::assert_snapshot!("focused_surfaces", rendered);
    }

    fn ratatui_buffer_text(rows: &[Row], width: usize) -> String {
        let height = rows.len().max(1);
        let backend = TestBackend::new(width as u16, height as u16);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let logical = Frame { rows: rows.to_vec(), width, cursor: None, cursor_visible: false };
        terminal
            .draw(|frame| super::super::ratatui::render_logical_frame(frame, &logical))
            .expect("render surface through Ratatui");
        let buffer = terminal.backend().buffer();
        let mut rendered = String::new();
        for y in 0..height as u16 {
            let line = (0..width as u16).map(|x| buffer[(x, y)].symbol()).collect::<String>();
            rendered.push_str(line.trim_end());
            rendered.push('\n');
        }
        rendered
    }

    fn test_theme() -> SurfaceThemeView {
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
