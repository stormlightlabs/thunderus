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
    ColumnAlignment, ColumnWidthPolicy, DiffDetailView, FocusedSurfaceView, PickerSurfaceView, SetupFormView,
    SurfaceRenderInput, SurfaceRenderer, SurfaceThemeView, TableCellView, TableView, ThemeRole, ToolDetailView,
};
use crate::utils;

const HELP_ROWS: &[(&str, &str)] = &[
    ("Enter", "submit prompt or accept focused selection"),
    ("Esc", "close focused surface and return to prompt"),
    ("Ctrl+C", "stop a running turn"),
    ("Ctrl+O", "open output, diff, warning, or error detail"),
    ("Ctrl+P", "open workspace file picker"),
    ("Tab", "accept a command or file suggestion"),
    ("Ctrl+T", "toggle running input target"),
];

/// iocraft-backed renderer for bounded focused surfaces.
#[derive(Default)]
pub struct IocraftSurfaceRenderer;

impl SurfaceRenderer for IocraftSurfaceRenderer {
    fn render_surface(&mut self, input: SurfaceRenderInput<'_>) -> Vec<Row> {
        render_surface(&input)
    }
}

/// Render a semantic focused surface through iocraft.
pub fn render_surface(input: &SurfaceRenderInput<'_>) -> Vec<Row> {
    if input.width == 0 || input.height == 0 {
        return Vec::new();
    }

    match input.surface {
        FocusedSurfaceView::None => Vec::new(),
        FocusedSurfaceView::CommandPicker(picker) => picker_rows(picker, input.width, input.height, input.theme),
        FocusedSurfaceView::FilePicker(picker) => picker_rows(picker, input.width, input.height, input.theme),
        FocusedSurfaceView::Help => help_rows(input.width, input.height, input.theme),
        FocusedSurfaceView::ToolDetail(detail) => tool_detail_rows(detail, input.width, input.height, input.theme),
        FocusedSurfaceView::DiffDetail(detail) => diff_detail_rows(detail, input.width, input.height, input.theme),
        FocusedSurfaceView::TranscriptLens { selected_entry, scroll } => {
            let body = vec![
                format!(
                    "entry: {}",
                    selected_entry.map_or_else(|| "latest".to_string(), |entry| entry.to_string())
                ),
                format!("scroll: {scroll}"),
            ];
            transcript_lens_rows("transcript", &body, input.width, input.height)
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

fn picker_rows(picker: &PickerSurfaceView, width: usize, height: usize, theme: &SurfaceThemeView) -> Vec<Row> {
    let lines = if picker.items.is_empty() {
        vec![
            SurfaceLine::title(format!("{}  {}", picker.title, query_label(&picker.query))),
            SurfaceLine::muted("no matches"),
        ]
    } else {
        let visible = height.saturating_sub(2);
        let start = picker.selected.saturating_add(1).saturating_sub(visible.max(1));
        let mut lines = vec![SurfaceLine::title(format!(
            "{}  {}",
            picker.title,
            query_label(&picker.query)
        ))];
        lines.extend(
            picker
                .items
                .iter()
                .enumerate()
                .skip(start)
                .take(visible)
                .map(|(index, item)| {
                    let selected = index == picker.selected;
                    let marker = if selected { "›" } else { " " };
                    let detail_budget = width.saturating_sub(32).min(28);
                    let detail = if item.detail.is_empty() {
                        String::new()
                    } else {
                        format!("  {}", utils::truncate_ellipsis(&item.detail, detail_budget))
                    };
                    let label_budget = width.saturating_sub(4 + utils::text_width(&detail)).max(1);
                    let text = format!(
                        "{marker} {}{detail}",
                        utils::truncate_ellipsis_start(&item.label, label_budget)
                    );
                    if selected { SurfaceLine::selected(text) } else { SurfaceLine::text(text) }
                }),
        );
        lines.push(SurfaceLine::muted("Enter select   Esc close"));
        lines
    };
    render_lines(&lines, width, height, theme)
}

fn help_rows(width: usize, height: usize, theme: &SurfaceThemeView) -> Vec<Row> {
    let mut lines = vec![SurfaceLine::title("help")];
    lines.extend(
        HELP_ROWS
            .iter()
            .map(|(key, description)| SurfaceLine::text(format!("{key:<8} {description}"))),
    );
    render_lines(&lines, width, height, theme)
}

fn tool_detail_rows(detail: &ToolDetailView, width: usize, height: usize, theme: &SurfaceThemeView) -> Vec<Row> {
    let status_role = match detail.status {
        ToolStatus::Failed => theme.error,
        ToolStatus::Cancelled => theme.warning,
        _ => theme.selected,
    };
    let mut lines = vec![SurfaceLine::new(
        format!("{} [{}]", detail.title, detail.status.label()),
        status_role,
    )];
    let body_budget = height.saturating_sub(1);
    lines.extend(
        detail
            .output
            .iter()
            .skip(detail.scroll)
            .take(body_budget)
            .cloned()
            .map(SurfaceLine::text),
    );
    render_lines(&lines, width, height, theme)
}

fn diff_detail_rows(detail: &DiffDetailView, width: usize, height: usize, theme: &SurfaceThemeView) -> Vec<Row> {
    let files = if detail.summary.files.is_empty() { "diff".to_string() } else { detail.summary.files.join(", ") };
    let mut lines = vec![SurfaceLine::title(format!(
        "{files} +{} -{}",
        detail.summary.added, detail.summary.removed
    ))];
    lines.extend(detail.lines.iter().take(height.saturating_sub(1)).map(|line| {
        if line.starts_with('+') && !line.starts_with("+++") {
            SurfaceLine::new(line.clone(), theme.diff_added)
        } else if line.starts_with('-') && !line.starts_with("---") {
            SurfaceLine::new(line.clone(), theme.diff_removed)
        } else {
            SurfaceLine::text(line.clone())
        }
    }));
    render_lines(&lines, width, height, theme)
}

fn setup_form_rows(form: &SetupFormView, width: usize, height: usize, theme: &SurfaceThemeView) -> Vec<Row> {
    let mut lines = vec![SurfaceLine::title("setup")];
    lines.extend(
        form.validation_errors
            .iter()
            .map(|error| SurfaceLine::new(format!("! {error}"), theme.error)),
    );
    lines.extend(form.fields.iter().enumerate().map(|(index, field)| {
        let focused = index == form.focus_index || field.focused;
        let marker = if focused { "›" } else { " " };
        let value = if field.secret && !field.value.is_empty() { "[hidden]".to_string() } else { field.value.clone() };
        let multiline = if field.multiline { " multiline" } else { "" };
        let error = field
            .error
            .as_ref()
            .map_or(String::new(), |error| format!("  ! {error}"));
        let text = format!("{marker} {}: {value}{multiline}{error}", field.label);
        if focused { SurfaceLine::selected(text) } else { SurfaceLine::text(text) }
    }));
    lines.push(SurfaceLine::muted(format!(
        "{}   {}",
        form.submit_label, form.cancel_label
    )));
    render_lines(&lines, width, height, theme)
}

fn table_rows(table: &TableView, width: usize, height: usize, theme: &SurfaceThemeView) -> Vec<Row> {
    if width < 24 && !table.narrow_fallback.is_empty() {
        let lines = table
            .narrow_fallback
            .iter()
            .cloned()
            .map(SurfaceLine::text)
            .collect::<Vec<_>>();
        return render_lines(&lines, width, height, theme);
    }

    let widths = table_column_widths(table, width);
    let mut lines = vec![SurfaceLine::title(table_line(&table.header, &widths))];
    lines.push(SurfaceLine::muted(
        widths
            .iter()
            .map(|width| "-".repeat(*width))
            .collect::<Vec<_>>()
            .join(" "),
    ));
    lines.extend(table.rows.iter().enumerate().map(|(index, row)| {
        let selected = table.selected_row == Some(index);
        let marker = if selected { "›" } else { " " };
        let text = format!("{marker} {}", table_line(row, &widths));
        if selected { SurfaceLine::selected(text) } else { SurfaceLine::text(text) }
    }));
    render_lines(&lines, width, height, theme)
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
    let text = row_text(row);
    *row = Row::padded(vec![Span::styled(text, style)], row.width, CellStyle::default().bg(bg));
}

fn row_text(row: &Row) -> String {
    let mut out = String::new();
    for span in &row.spans {
        out.push_str(&span.text);
    }
    out
}

fn canvas_to_rows(canvas: &Canvas, width: usize, pad_style: CellStyle) -> Vec<Row> {
    (0..canvas.height())
        .map(|y| {
            let mut spans = Vec::new();
            let mut current_text = String::new();
            let mut current_style: Option<CellStyle> = None;
            for x in 0..canvas.width().min(width) {
                let cell = canvas.cell(x, y);
                let style = cell.map_or(pad_style, cell_style);
                let text = cell.and_then(CanvasCell::text).unwrap_or(" ");
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
        let surface = FocusedSurfaceView::CommandPicker(PickerSurfaceView {
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

        assert!(text.contains("commands"));
        assert!(text.contains("› health"));
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
    fn iocraft_picker_returns_only_content_rows() {
        let surface = FocusedSurfaceView::FilePicker(PickerSurfaceView {
            title: "files".to_string(),
            query: "missing".to_string(),
            selected: 0,
            items: Vec::new(),
        });

        let rows =
            render_surface(&SurfaceRenderInput { surface: &surface, theme: &test_theme(), width: 40, height: 8 });

        assert_eq!(rows.len(), 2);
        assert!(rows[0].text().contains("files"));
        assert!(rows[1].text().contains("no matches"));
    }

    #[test]
    fn snapshot_iocraft_focused_surfaces() {
        let cases = vec![
            (
                "command picker",
                FocusedSurfaceView::CommandPicker(PickerSurfaceView {
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
                FocusedSurfaceView::FilePicker(PickerSurfaceView {
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
            ("help", FocusedSurfaceView::Help),
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
                    fields: vec![SetupFieldView {
                        label: "credential".to_string(),
                        value: "sk-hidden".to_string(),
                        focused: true,
                        secret: true,
                        multiline: false,
                        error: None,
                    }],
                    focus_index: 0,
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
