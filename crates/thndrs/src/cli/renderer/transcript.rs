//! Semantic transcript row construction for the terminal renderer.
//!
//! This renderer module owns the terminal presentation of transcript content:
//! layout, styling, collapsing, and width-aware row construction. Semantic
//! identity and lifecycle state belong to `app::transcript_blocks`.

use std::path::Path;

use crate::app::{App, Entry, ToolStatus};
use crate::internals::StartupSection;
use crate::renderer::row::Row;
use crate::renderer::style::{CellStyle, Color, Span};
use crate::{renderer, utils};

/// Maximum tool output lines rendered before a truncation marker is shown.
const MAX_TOOL_OUTPUT_LINES: usize = 6;

/// Minimum content width where markdown tables remain legible.
const MIN_TABLE_RENDER_WIDTH: usize = 24;

/// Gutter prefix for tool output lines.
pub const GUTTER: &str = "   · ";

/// Role rail shown on transcript entry rows and partial-entry continuations.
pub const ENTRY_RAIL: &str = "  ";

#[derive(Clone, Copy)]
enum TableAlign {
    Left,
    Right,
    Center,
}

/// Context needed to render a single transcript entry into rows.
#[derive(Clone)]
pub struct TranscriptRowContext<'a> {
    pub user_label: &'a str,
    pub cwd: &'a Path,
    pub width: usize,
    /// Index of the entry in the transcript. When present, rows are tagged with a
    /// [`renderer::row::RowGroupId`] so viewport navigation can correlate rows to the
    /// originating entry.
    pub entry_index: Option<usize>,
    /// Whether this entry begins a consecutive group of tool activity.
    pub tool_group_start: bool,
}

impl TranscriptRowContext<'_> {
    /// Build all rows for a single transcript entry.
    pub fn rows_for_entry(&self, entry: &Entry) -> Vec<Row> {
        let mut rows = entry_to_rows(entry, self.user_label, self.width, self.cwd, self.tool_group_start);
        if let Some(index) = self.entry_index {
            let group_id = renderer::row::RowGroupId { entry_index: index };
            for row in &mut rows {
                row.group_id = Some(group_id);
            }
        }
        rows
    }

    /// Split a single entry into stable and live rows.
    ///
    /// Streaming assistant/reasoning blocks and running tools are entirely live
    /// until they finish. All other entries are fully stable.
    pub fn rows_for_entry_stable_and_live_rows(&self, entry: &Entry) -> (Vec<Row>, Vec<Row>) {
        let rows = self.rows_for_entry(entry);
        match entry {
            Entry::Agent { streaming: true, .. }
            | Entry::Reasoning { streaming: true, .. }
            | Entry::Tool { status: ToolStatus::Running, .. } => (Vec::new(), rows),
            _ => (rows, Vec::new()),
        }
    }
}

impl<'a> TranscriptRowContext<'a> {
    /// Build a context without entry grouping.
    #[cfg(test)]
    pub fn for_test(user_label: &'a str, cwd: &'a Path, width: usize) -> Self {
        Self { user_label, cwd, width, entry_index: None, tool_group_start: true }
    }
}

#[derive(Clone, Copy)]
struct StartupBannerTheme {
    width: usize,
    brand_style: CellStyle,
    attention_style: CellStyle,
    muted_style: CellStyle,
    hint_style: CellStyle,
    rail_style: CellStyle,
    success_style: CellStyle,
    meta_style: CellStyle,
}

impl StartupBannerTheme {
    fn body_width(self) -> usize {
        super::layout::content_width(self.width).saturating_sub(3)
    }
}

#[derive(Clone, Copy)]
struct LabeledBlock {
    rail_style: CellStyle,
    label_style: CellStyle,
    text_style: CellStyle,
    bg: Color,
    width: usize,
    body_width: usize,
}

impl LabeledBlock {
    fn new(
        rail_style: CellStyle, label_style: CellStyle, text_style: CellStyle, bg: Color, width: usize,
        body_width: usize,
    ) -> Self {
        Self { rail_style, label_style, text_style, bg, width, body_width }
    }

    /// Build a labeled text block with a single leading spacer row.
    fn build(self, label: &str, text: &str) -> Vec<Row> {
        let mut rows = vec![
            Row::blank(self.width, CellStyle::new().bg(self.bg)),
            Row::padded(
                vec![
                    Span::styled(ENTRY_RAIL, self.rail_style),
                    Span::styled(label.to_string(), self.label_style),
                ],
                self.width,
                CellStyle::new().bg(self.bg),
            ),
        ];

        for line in super::layout::wrap_text(text, self.body_width) {
            match line.is_empty() {
                true => rows.push(Row::blank(self.width, CellStyle::new().bg(self.bg))),
                false => rows.push(Row::padded(
                    vec![
                        Span::styled(ENTRY_RAIL, self.rail_style),
                        Span::styled(line, self.text_style),
                    ],
                    self.width,
                    CellStyle::new().bg(self.bg),
                )),
            }
        }
        rows
    }

    fn build_compact(self, label: &str, text: &str) -> Vec<Row> {
        let summary = text.lines().find(|line| !line.trim().is_empty()).unwrap_or_default();
        let label_width = utils::text_width(label) + 2;
        let summary = utils::truncate_ellipsis(summary, self.body_width.saturating_sub(label_width));
        vec![Row::padded(
            vec![
                Span::styled(ENTRY_RAIL, self.rail_style),
                Span::styled(label.to_string(), self.label_style),
                Span::styled("  ", CellStyle::new().bg(self.bg)),
                Span::styled(summary, self.text_style),
            ],
            self.width,
            CellStyle::new().bg(self.bg),
        )]
    }
}

#[derive(Copy, Clone)]
struct BannerIndent(usize, usize);

impl BannerIndent {
    fn first(&self) -> usize {
        self.0
    }

    fn continuation(&self) -> usize {
        self.1
    }
}

/// Build a tool block: header row + args summary + output lines + vertical padding.
struct ToolBlockView<'a> {
    name: &'a str,
    args: &'a str,
    status: ToolStatus,
    output: &'a [String],
    width: usize,
    body_width: usize,
    bg: Color,
    cwd: &'a Path,
    group_start: bool,
}

impl ToolBlockView<'_> {
    fn rows(&self) -> Vec<Row> {
        let p = super::style::palette();
        let (status_label, status_color, icon) = match self.status {
            ToolStatus::Running => ("Running", p.peach, "·"),
            ToolStatus::Ok => ("Done", p.green, "✓"),
            ToolStatus::Failed => ("Failed", p.red, "✕"),
            ToolStatus::Cancelled => ("Stopped", p.peach, "○"),
        };
        let header_style = CellStyle::new().fg(p.text).bg(self.bg).bold();
        let status_style = CellStyle::new().fg(status_color).bg(self.bg);
        let muted_style = CellStyle::new().fg(p.subtext0).bg(self.bg);
        let gutter_style = CellStyle::new().fg(p.overlay0).bg(self.bg);
        let rail_style = CellStyle::new().fg(p.yellow).bg(self.bg);
        let content_width = self.body_width.saturating_sub(utils::text_width(ENTRY_RAIL));
        let tool_content_width = content_width.saturating_sub(utils::text_width(GUTTER));

        let base_name = self.name.split('#').next().unwrap_or(self.name);
        let args_summary = summarize_tool_invocation(base_name, self.args, self.cwd);
        let lang = super::highlight::tool_output_language(base_name, self.args);

        let mut rows = Vec::new();
        if self.group_start {
            rows.push(Row::blank(self.width, CellStyle::new().bg(self.bg)));
            rows.push(Row::padded(
                vec![
                    Span::styled(ENTRY_RAIL, rail_style),
                    Span::styled("Activity", CellStyle::new().fg(p.yellow).bg(self.bg).bold()),
                ],
                self.width,
                CellStyle::new().bg(self.bg),
            ));
        }

        let mut header_spans = vec![
            Span::styled(ENTRY_RAIL, rail_style),
            Span::styled(format!("{icon} "), status_style),
            Span::styled(base_name.to_string(), header_style),
        ];
        header_spans.push(Span::styled(format!(" [{status_label}]"), status_style));

        if !args_summary.is_empty() {
            let header_width: usize = header_spans.iter().map(|s| utils::text_width(&s.text)).sum();
            if header_width + 2 + utils::text_width(&args_summary) <= self.body_width {
                header_spans.push(Span::styled("  ", CellStyle::new().bg(self.bg)));
                header_spans.push(Span::styled(args_summary, muted_style));
            } else {
                rows.push(Row::padded(header_spans, self.width, CellStyle::new().bg(self.bg)));
                for wrapped in super::layout::wrap_text(&args_summary, content_width.saturating_sub(2)) {
                    let spans = vec![
                        Span::styled(ENTRY_RAIL, rail_style),
                        Span::styled("  ", CellStyle::new().bg(self.bg)),
                        Span::styled(wrapped, muted_style),
                    ];
                    rows.push(Row::padded(spans, self.width, CellStyle::new().bg(self.bg)));
                }
                header_spans = Vec::new();
            }
        }
        let output_details = (self.status == ToolStatus::Ok && !self.output.is_empty())
            .then(|| format!("{} lines · Ctrl+O details", self.output.len()));
        if let Some(details) = output_details.as_ref()
            && !header_spans.is_empty()
        {
            header_spans.push(Span::styled(format!("  {details}"), muted_style));
        }
        if !header_spans.is_empty() {
            rows.push(Row::padded(header_spans, self.width, CellStyle::new().bg(self.bg)));
        } else if let Some(details) = output_details {
            rows.push(Row::padded(
                vec![
                    Span::styled(ENTRY_RAIL, rail_style),
                    Span::styled("  ", CellStyle::new().bg(self.bg)),
                    Span::styled(details, muted_style),
                ],
                self.width,
                CellStyle::new().bg(self.bg),
            ));
        }

        if let Some(summary) = edit_summary_line(self.name, self.output, self.status, self.cwd) {
            rows.push(Row::padded(
                vec![
                    Span::styled(ENTRY_RAIL, rail_style),
                    Span::styled("   edit  ", CellStyle::new().fg(p.overlay0).bg(self.bg).bold()),
                    Span::styled(summary, muted_style),
                ],
                self.width,
                CellStyle::new().bg(self.bg),
            ));
        }

        if let Some(summary) = diff_summary_line(self.output) {
            rows.push(Row::padded(
                vec![
                    Span::styled(ENTRY_RAIL, rail_style),
                    Span::styled("   diff  ", CellStyle::new().fg(p.overlay0).bg(self.bg).bold()),
                    Span::styled(summary, muted_style),
                ],
                self.width,
                CellStyle::new().bg(self.bg),
            ));
        }

        if self.status == ToolStatus::Ok {
            return rows;
        }

        match lang {
            Some(lang_str) => {
                let joined: String = self
                    .output
                    .iter()
                    .take(MAX_TOOL_OUTPUT_LINES)
                    .map(|line| {
                        let line = super::path_display::transcript_line(line, self.cwd);
                        format!("{line}\n")
                    })
                    .collect();
                let highlighted = super::highlight::highlight_lines(&joined, Some(lang_str));
                for hl_row in highlighted {
                    let mut spans = vec![Span::styled(ENTRY_RAIL, rail_style), Span::styled(GUTTER, gutter_style)];
                    let content_spans: Vec<_> = hl_row
                        .into_iter()
                        .map(|s| Span { text: s.text, style: s.style.bg(self.bg) })
                        .collect();
                    spans.extend(super::layout::truncate_spans(
                        &content_spans,
                        tool_content_width,
                        muted_style,
                    ));
                    rows.push(Row::padded(spans, self.width, CellStyle::new().bg(self.bg)));
                }
            }
            None => {
                for line in self.output.iter().take(MAX_TOOL_OUTPUT_LINES) {
                    let line = super::path_display::transcript_line(line, self.cwd);
                    let content_style = if is_section_header(&line) {
                        CellStyle::new().fg(p.overlay1).bg(self.bg).bold()
                    } else {
                        CellStyle::new().fg(p.subtext0).bg(self.bg)
                    };
                    for wrapped in super::layout::wrap_text_preserving_whitespace(&line, tool_content_width) {
                        let spans = vec![
                            Span::styled(ENTRY_RAIL, rail_style),
                            Span::styled(GUTTER, gutter_style),
                            Span::styled(wrapped, content_style),
                        ];
                        rows.push(Row::padded(spans, self.width, CellStyle::new().bg(self.bg)));
                    }
                }
            }
        }

        if self.output.len() > MAX_TOOL_OUTPUT_LINES {
            rows.push(Row::padded(
                vec![
                    Span::styled(ENTRY_RAIL, rail_style),
                    Span::styled(
                        format!(
                            "     … ({} lines stored, {} shown here) · Ctrl+O details",
                            self.output.len(),
                            MAX_TOOL_OUTPUT_LINES
                        ),
                        muted_style,
                    ),
                ],
                self.width,
                CellStyle::new().bg(self.bg),
            ));
        }

        rows
    }
}

#[derive(Clone, Copy)]
struct MarkdownTableTheme {
    cell_style: CellStyle,
    separator_style: CellStyle,
    rail_style: CellStyle,
    bg: Color,
    width: usize,
}

struct MarkdownTable {
    headers: Vec<String>,
    alignments: Vec<TableAlign>,
    rows: Vec<Vec<String>>,
}

impl MarkdownTable {
    fn new(header: &str, separator: &str) -> Self {
        Self { headers: Self::parse_cells(header), alignments: Self::parse_alignments(separator), rows: Vec::new() }
    }

    fn is_valid(&self) -> bool {
        self.headers.len() >= 2 && self.headers.len() == self.alignments.len()
    }

    fn push_row(&mut self, row: &str) {
        let mut cells = Self::parse_cells(row);
        cells.resize(self.headers.len(), String::new());
        cells.truncate(self.headers.len());
        self.rows.push(cells);
    }

    fn render(&self, rail_style: CellStyle, bg: Color, width: usize, body_width: usize) -> Vec<Row> {
        let p = super::style::palette();
        let text_style = CellStyle::new().fg(p.text).bg(bg);
        let separator_style = CellStyle::new().fg(p.overlay0).bg(bg);
        let header_theme = MarkdownTableTheme {
            cell_style: CellStyle::new().fg(p.text).bg(bg).bold().underlined(),
            separator_style,
            rail_style,
            bg,
            width,
        };
        let body_theme = MarkdownTableTheme { cell_style: text_style, separator_style, rail_style, bg, width };

        if body_width <= MIN_TABLE_RENDER_WIDTH {
            return self.render_narrow(rail_style, text_style, bg, width, body_width);
        }

        let column_widths = self.column_widths(body_width);
        if column_widths.contains(&0) {
            return self.render_narrow(rail_style, text_style, bg, width, body_width);
        }

        let mut rows = Vec::new();
        rows.push(Self::row(&self.headers, &self.alignments, &column_widths, header_theme));
        for cells in &self.rows {
            rows.push(Self::row(cells, &self.alignments, &column_widths, body_theme));
        }
        if self.rows.is_empty() {
            rows.push(Row::padded(
                vec![
                    Span::styled(ENTRY_RAIL, rail_style),
                    Span::styled("(no rows)", CellStyle::new().fg(p.subtext0).bg(bg)),
                ],
                width,
                CellStyle::new().bg(bg),
            ));
        }
        rows
    }

    fn render_narrow(
        &self, rail_style: CellStyle, text_style: CellStyle, bg: Color, width: usize, body_width: usize,
    ) -> Vec<Row> {
        let mut rows = Vec::new();
        let header = self.headers.join(" / ");
        for wrapped in super::layout::wrap_text(&header, body_width) {
            rows.push(Row::padded(
                vec![Span::styled(ENTRY_RAIL, rail_style), Span::styled(wrapped, text_style)],
                width,
                CellStyle::new().bg(bg),
            ));
        }
        for cells in &self.rows {
            let line = self
                .headers
                .iter()
                .zip(cells.iter())
                .map(|(header, cell)| format!("{header}: {cell}"))
                .collect::<Vec<_>>()
                .join("; ");
            for wrapped in super::layout::wrap_text(&line, body_width) {
                rows.push(Row::padded(
                    vec![Span::styled(ENTRY_RAIL, rail_style), Span::styled(wrapped, text_style)],
                    width,
                    CellStyle::new().bg(bg),
                ));
            }
        }
        rows
    }

    fn row(cells: &[String], alignments: &[TableAlign], widths: &[usize], theme: MarkdownTableTheme) -> Row {
        let mut spans = vec![Span::styled(ENTRY_RAIL, theme.rail_style)];
        for (index, cell) in cells.iter().enumerate() {
            if index > 0 {
                spans.push(Span::styled("  ", theme.separator_style));
            }
            let text = Self::align_cell(
                &utils::truncate_ellipsis(cell, widths[index]),
                widths[index],
                alignments[index],
            );
            spans.push(Span::styled(text, theme.cell_style));
        }
        Row::padded(spans, theme.width, CellStyle::new().bg(theme.bg))
    }

    fn column_widths(&self, body_width: usize) -> Vec<usize> {
        let columns = self.headers.len();
        let separators = columns.saturating_sub(1) * 2;
        let available = body_width.saturating_sub(separators);
        if available < columns {
            return vec![0; columns];
        }

        let mut desired = self
            .headers
            .iter()
            .enumerate()
            .map(|(index, header)| {
                std::iter::once(header)
                    .chain(self.rows.iter().filter_map(|row| row.get(index)))
                    .map(|cell| utils::text_width(cell))
                    .max()
                    .unwrap_or(1)
                    .max(3)
            })
            .collect::<Vec<_>>();
        let desired_total: usize = desired.iter().sum();
        if desired_total <= available {
            return desired;
        }

        let mut widths = desired
            .iter()
            .map(|desired_width| ((*desired_width * available) / desired_total).max(3))
            .collect::<Vec<_>>();
        while widths.iter().sum::<usize>() > available {
            if let Some((index, _)) = widths
                .iter()
                .enumerate()
                .filter(|(_, width)| **width > 3)
                .max_by_key(|(_, width)| **width)
            {
                widths[index] -= 1;
            } else {
                break;
            }
        }
        while widths.iter().sum::<usize>() < available {
            if let Some((index, _)) = desired
                .iter_mut()
                .enumerate()
                .max_by_key(|(index, desired_width)| desired_width.saturating_sub(widths[*index]))
            {
                widths[index] += 1;
            } else {
                break;
            }
        }
        widths
    }

    fn align_cell(text: &str, width: usize, align: TableAlign) -> String {
        let used = utils::text_width(text);
        if used >= width {
            return text.to_string();
        }
        let pad = width - used;
        match align {
            TableAlign::Left => format!("{text}{}", " ".repeat(pad)),
            TableAlign::Right => format!("{}{text}", " ".repeat(pad)),
            TableAlign::Center => {
                let left = pad / 2;
                let right = pad - left;
                format!("{}{text}{}", " ".repeat(left), " ".repeat(right))
            }
        }
    }

    fn parse_cells(line: &str) -> Vec<String> {
        line.trim()
            .trim_matches('|')
            .split('|')
            .map(|cell| cell.trim().to_string())
            .collect()
    }

    fn parse_alignments(line: &str) -> Vec<TableAlign> {
        Self::parse_cells(line)
            .into_iter()
            .map(|cell| {
                let left = cell.starts_with(':');
                let right = cell.ends_with(':');
                match (left, right) {
                    (true, true) => TableAlign::Center,
                    (false, true) => TableAlign::Right,
                    _ => TableAlign::Left,
                }
            })
            .collect()
    }

    fn is_row(line: &str) -> bool {
        line.trim().matches('|').count() >= 2
    }

    fn is_separator(line: &str) -> bool {
        let cells = Self::parse_cells(line);
        cells.len() >= 2
            && cells.iter().all(|cell| {
                let inner = cell.trim().trim_matches(':');
                inner.len() >= 3 && inner.chars().all(|ch| ch == '-')
            })
    }
}

impl App {
    /// Build startup banner rows from app state.
    pub fn render_banner_rows(&self, width: usize) -> Vec<Row> {
        let p = super::style::palette();
        let bg = Color::Reset;
        let theme = StartupBannerTheme {
            width,
            brand_style: CellStyle::new().fg(p.accent).bg(bg).bold(),
            attention_style: CellStyle::new().fg(p.peach).bg(bg).bold(),
            muted_style: CellStyle::new().fg(p.subtext0).bg(bg),
            hint_style: CellStyle::new().fg(p.yellow).bg(bg).bold(),
            rail_style: CellStyle::new().fg(p.overlay0).bg(bg),
            success_style: CellStyle::new().fg(p.green).bg(bg),
            meta_style: CellStyle::new().fg(p.overlay1).bg(bg),
        };
        let snapshot = self.self_knowledge_snapshot();
        let sections = snapshot.startup_sections();
        let context_lines = self.startup_section_lines(&sections, "Context");
        let diagnostics = self
            .startup_section_lines(&sections, "Diagnostics")
            .into_iter()
            .skip(self.transcript.skill_diagnostics.len())
            .collect::<Vec<_>>();

        let mut rows = Vec::new();
        push_banner_brand_row(&mut rows, theme);
        push_wrapped_banner_text(
            &mut rows,
            "Ask for change, run a command, or inspect the repo.",
            BannerIndent(0, 0),
            theme,
            theme.muted_style,
        );
        rows.push(Row::blank(width, CellStyle::new()));

        let context_ready = context_lines.iter().any(|line| line != "(none)");
        let context_text = readiness_context_text(&context_lines);
        let context_path = context_lines
            .iter()
            .find(|line| line.as_str() != "(none)")
            .map(|line| readiness_context_path(line))
            .unwrap_or_else(|| "—".to_string());
        push_banner_readiness_row(&mut rows, &context_text, &context_path, context_ready, theme);

        let skill_count = snapshot.inventory.references.skills.len();
        let mut skills_text = match skill_count {
            0 => "No skills available".to_string(),
            1 => "1 skill available".to_string(),
            count => format!("{count} skills available"),
        };
        if !self.transcript.skill_diagnostics.is_empty() {
            skills_text.push_str(&format!(" · {} skipped", self.transcript.skill_diagnostics.len()));
        }
        push_banner_readiness_row(&mut rows, &skills_text, "skills", skill_count > 0, theme);

        let search_mode = snapshot.runtime.provider.search.mode.as_str();
        push_banner_readiness_row(&mut rows, "Web Search", search_mode, true, theme);

        if diagnostics.iter().any(|line| line != "(none)") {
            rows.push(Row::blank(width, CellStyle::new()));
            push_banner_attention_heading(&mut rows, theme);
            for line in diagnostics {
                push_wrapped_banner_text(&mut rows, &line, BannerIndent(2, 2), theme, theme.muted_style);
            }
        }

        rows.push(Row::blank(width, CellStyle::new()));
        push_banner_help(&mut rows, theme);
        rows.push(Row::blank(width, CellStyle::new()));
        rows
    }

    fn startup_section_lines(&self, sections: &[StartupSection], heading: &str) -> Vec<String> {
        sections
            .iter()
            .find(|section| section.heading == heading)
            .map(|section| {
                section
                    .lines
                    .iter()
                    .map(|line| match section.heading {
                        "Context" | "Diagnostics" => super::path_display::transcript_line(line, &self.runtime.cwd),
                        _ => line.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Produce a compact, user-facing summary for a tool invocation.
pub fn summarize_tool_invocation(name: &str, args: &str, cwd: &Path) -> String {
    let summary = summarize_tool_args(args, cwd);
    if name != "run_shell" || summary.is_empty() {
        return summary;
    }

    let command = summary
        .strip_prefix("argv: ")
        .or_else(|| summary.strip_prefix("program: "))
        .unwrap_or(&summary);
    format!("$ {command}")
}

/// Produce a short summary of a tool's arguments for the transcript line.
pub fn summarize_tool_args(args: &str, cwd: &Path) -> String {
    let trimmed = args.trim();
    if trimmed.is_empty() || trimmed == "{}" {
        return String::new();
    }

    let v = match serde_json::from_str::<serde_json::Value>(trimmed) {
        Ok(v) => v,
        Err(_) => return utils::truncate_ellipsis(trimmed, 48),
    };

    match v.as_object() {
        Some(obj) => {
            if let Some(argv) = obj.get("argv").and_then(|value| value.as_array()) {
                let command = argv
                    .iter()
                    .filter_map(|value| value.as_str())
                    .map(|value| super::path_display::transcript_line(value, cwd))
                    .collect::<Vec<_>>()
                    .join(" ");
                if !command.is_empty() {
                    return format!("argv: {}", utils::truncate_ellipsis(&command, 72));
                }
            }
            for key in &["pattern", "path", "query", "root", "glob", "file", "program", "url"] {
                if let Some(val) = obj.get(*key).and_then(|f| f.as_str()) {
                    let val = super::path_display::transcript_line(val, cwd);
                    return format!("{}: {}", key, utils::truncate_ellipsis(&val, 40));
                }
            }
            for (k, val) in obj {
                if let Some(s) = val.as_str() {
                    let s = super::path_display::transcript_line(s, cwd);
                    return format!("{k}: {}", utils::truncate_ellipsis(&s, 40));
                }
            }
            utils::truncate_ellipsis(trimmed, 48)
        }
        None => utils::truncate_ellipsis(trimmed, 48),
    }
}

fn wrap_with_indent(text: &str, indent: BannerIndent, width: usize) -> Vec<String> {
    let first_width = width.saturating_sub(indent.first()).max(1);
    let continuation_width = width.saturating_sub(indent.continuation()).max(1);
    let mut out = Vec::new();
    for (index, line) in super::layout::wrap_text(text, first_width).into_iter().enumerate() {
        if index == 0 {
            out.push(format!("{}{}", " ".repeat(indent.first()), line));
        } else {
            for continued in super::layout::wrap_text(&line, continuation_width) {
                out.push(format!("{}{}", " ".repeat(indent.continuation()), continued));
            }
        }
    }
    out
}

fn readiness_context_text(lines: &[String]) -> String {
    let visible = lines
        .iter()
        .filter(|line| line.as_str() != "(none)")
        .collect::<Vec<_>>();
    let Some(first) = visible.first() else {
        return "No project instructions".to_string();
    };
    let path = first.split(" (truncated").next().unwrap_or(first);
    let name = Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path);
    format!("{name} loaded")
}

fn readiness_context_path(line: &str) -> String {
    let path = line.split(" (truncated").next().unwrap_or(line);
    if Path::new(path).components().count() == 1 { format!("./{path}") } else { path.to_string() }
}

fn push_banner_brand_row(rows: &mut Vec<Row>, theme: StartupBannerTheme) {
    rows.push(Row::padded(
        vec![
            Span::styled("thndrs", theme.brand_style),
            Span::styled(" / ready", theme.meta_style.bold()),
        ],
        theme.width,
        CellStyle::new(),
    ));
}

fn push_banner_readiness_row(rows: &mut Vec<Row>, text: &str, meta: &str, ready: bool, theme: StartupBannerTheme) {
    let prefix_width = utils::text_width("    ✓ ");
    let available = theme.body_width().saturating_sub(prefix_width).max(1);
    let wrapped = super::layout::wrap_text(text, available);
    for (index, line) in wrapped.into_iter().enumerate() {
        let marker = if index == 0 { if ready { "✓ " } else { "· " } } else { "  " };
        let marker_style = if ready { theme.success_style } else { theme.muted_style };
        let mut spans = vec![
            Span::styled("    ", theme.rail_style),
            Span::styled(marker, marker_style),
            Span::styled(line, theme.muted_style),
        ];
        if index == 0 {
            let used = super::layout::spans_width(&spans);
            let meta_width = utils::text_width(meta);
            if used + meta_width + 2 <= theme.body_width() {
                spans.push(Span::styled(
                    " ".repeat(theme.body_width() - used - meta_width),
                    theme.muted_style,
                ));
                spans.push(Span::styled(meta, theme.meta_style));
            }
        }
        rows.push(Row::padded(spans, theme.width, CellStyle::new()));
    }
}

fn push_banner_attention_heading(rows: &mut Vec<Row>, theme: StartupBannerTheme) {
    rows.push(Row::padded(
        vec![Span::styled("ATTENTION", theme.attention_style)],
        theme.width,
        CellStyle::new(),
    ));
}

fn push_banner_help(rows: &mut Vec<Row>, theme: StartupBannerTheme) {
    let spans = vec![
        Span::styled("?", theme.hint_style),
        Span::styled(" help   ", theme.muted_style),
        Span::styled("/model", theme.hint_style),
        Span::styled(" switch   ", theme.muted_style),
        Span::styled("/search", theme.hint_style),
        Span::styled(" configure", theme.muted_style),
    ];
    for line in super::layout::wrap_spans(&spans, theme.body_width()) {
        rows.push(Row::padded(line, theme.width, CellStyle::new()));
    }
}

fn push_wrapped_banner_text(
    rows: &mut Vec<Row>, text: &str, indent: BannerIndent, theme: StartupBannerTheme, text_style: CellStyle,
) {
    let wrapped = wrap_with_indent(text, indent, theme.body_width());
    for line in wrapped {
        rows.push(Row::padded(
            vec![Span::styled(line, text_style)],
            theme.width,
            CellStyle::new(),
        ));
    }
}

fn edit_summary_line(name: &str, output: &[String], status: ToolStatus, cwd: &Path) -> Option<String> {
    let operation = name.split('#').next().unwrap_or(name);
    let is_edit_tool = matches!(operation, "create_file" | "replace_range" | "write_patch");
    if !is_edit_tool
        && !output
            .iter()
            .any(|line| line.contains("wrote") || line.contains("replaced"))
    {
        return None;
    }
    let path = output
        .iter()
        .find_map(|line| path_like_suffix(line))
        .map(|path| super::path_display::transcript_line(&path, cwd))
        .unwrap_or_else(|| "(path unavailable)".to_string());
    Some(format!("{operation} {path} [{}]", status.label()))
}

fn diff_summary_line(output: &[String]) -> Option<String> {
    let mut added = 0usize;
    let mut removed = 0usize;
    let mut files = Vec::new();
    for line in output {
        if let Some(path) = line.strip_prefix("+++ ") {
            files.push(path.trim_start_matches("b/").to_string());
        } else if line.starts_with('+') && !line.starts_with("+++") {
            added += 1;
        } else if line.starts_with('-') && !line.starts_with("---") {
            removed += 1;
        }
    }
    if added == 0 && removed == 0 && files.is_empty() {
        return None;
    }
    files.sort();
    files.dedup();
    let file_label = match files.as_slice() {
        [] => "unknown file".to_string(),
        [file] => file.clone(),
        _ => format!("{} files", files.len()),
    };
    Some(format!("{file_label} +{added} -{removed}"))
}

fn path_like_suffix(line: &str) -> Option<String> {
    line.rsplit_once(": ").map(|(_, path)| path.to_string()).or_else(|| {
        line.split_whitespace()
            .last()
            .filter(|part| part.contains('/'))
            .map(str::to_string)
    })
}

/// Convert a single transcript entry to padded rows for scrollback.
fn entry_to_rows(entry: &Entry, user_label: &str, width: usize, cwd: &Path, tool_group_start: bool) -> Vec<Row> {
    let p = super::style::palette();
    let bg = Color::Reset;
    let body_width = super::layout::content_width(width);
    let railed_body_width = body_width.saturating_sub(utils::text_width(ENTRY_RAIL));

    match entry {
        Entry::User { text } => {
            let rail_style = CellStyle::new().fg(p.blue).bg(bg).bold();
            let label_style = CellStyle::new().fg(p.blue).bg(bg).bold();
            let text_style = CellStyle::new().fg(p.subtext0).bg(bg).italic();
            let mut rows = LabeledBlock::new(rail_style, label_style, text_style, bg, width, railed_body_width)
                .build(user_label, text);
            rows.push(Row::blank(width, CellStyle::new().bg(bg)));
            rows
        }
        Entry::Agent { text, .. } => {
            let rail_style = CellStyle::new().fg(p.green).bg(bg).bold();
            let label_style = CellStyle::new().fg(p.green).bg(bg).bold();
            assistant_block_rows(text, rail_style, label_style, bg, width, railed_body_width)
        }
        Entry::Reasoning { text, streaming } => {
            let rail_style = CellStyle::new().fg(p.mauve).bg(bg).bold();
            let label_style = CellStyle::new().fg(p.overlay1).bg(bg);
            let text_style = CellStyle::new().fg(p.subtext0).bg(bg).italic();
            let label = if *streaming { "Thinking ·" } else { "Thinking ✓" };
            LabeledBlock::new(rail_style, label_style, text_style, bg, width, railed_body_width)
                .build_compact(label, text)
        }
        Entry::Tool { name, arguments, status, output } => ToolBlockView {
            name,
            args: arguments,
            status: *status,
            output,
            width,
            body_width,
            bg,
            cwd,
            group_start: tool_group_start,
        }
        .rows(),
        Entry::Status { text } => {
            let rail_style = CellStyle::new().fg(p.overlay1).bg(bg);
            let label_style = CellStyle::new().fg(p.overlay1).bg(bg);
            let text_style = CellStyle::new().fg(p.subtext0).bg(bg);
            if text.contains('\n') {
                LabeledBlock::new(rail_style, label_style, text_style, bg, width, railed_body_width)
                    .build(status_label_for(text), text)
            } else {
                LabeledBlock::new(rail_style, label_style, text_style, bg, width, railed_body_width)
                    .build_compact(status_label_for(text), text)
            }
        }
        Entry::Error { text } => {
            let error_bg = p.surface_dim;
            let rail_style = CellStyle::new().fg(p.red).bg(error_bg).bold();
            let label_style = CellStyle::new().fg(p.red).bg(error_bg).bold();
            let text_style = CellStyle::new().fg(p.text).bg(error_bg);
            LabeledBlock::new(rail_style, label_style, text_style, error_bg, width, railed_body_width)
                .build("⚠ Error", text)
        }
    }
}

/// Build an assistant message block, detecting markdown code fences for syntax highlighting.
fn assistant_block_rows(
    text: &str, rail_style: CellStyle, label_style: CellStyle, bg: Color, width: usize, body_width: usize,
) -> Vec<Row> {
    let p = super::style::palette();
    let text_style = CellStyle::new().fg(p.text).bg(bg);
    let mut rows = vec![
        Row::blank(width, CellStyle::new().bg(bg)),
        Row::padded(
            vec![
                Span::styled(ENTRY_RAIL, rail_style),
                Span::styled("Response".to_string(), label_style),
            ],
            width,
            CellStyle::new().bg(bg),
        ),
    ];

    match assistant_markdown_body(text) {
        Some(markdown) => rows.extend(render_markdown_body(
            markdown, rail_style, text_style, bg, width, body_width,
        )),
        None => {
            for line in super::layout::wrap_text(text, body_width) {
                match line.is_empty() {
                    true => rows.push(Row::blank(width, CellStyle::new().bg(bg))),
                    false => rows.push(Row::padded(
                        vec![Span::styled(ENTRY_RAIL, rail_style), Span::styled(line, text_style)],
                        width,
                        CellStyle::new().bg(bg),
                    )),
                }
            }
        }
    }

    if rows.len() == 2 {
        rows.push(Row::blank(width, CellStyle::new().bg(bg)));
    }
    rows
}

/// Extract Markdown from an outer provider wrapper or ordinary fenced Markdown.
fn assistant_markdown_body(text: &str) -> Option<&str> {
    strip_outer_markdown_fence(text).or_else(|| text.contains("```").then_some(text))
}

/// Strip a complete or streaming outer Markdown fence without confusing its
/// contents with a code block.
fn strip_outer_markdown_fence(text: &str) -> Option<&str> {
    for (opening, closing) in [
        ("````markdown\r\n", "\r\n````"),
        ("````markdown\n", "\n````"),
        ("````md\r\n", "\r\n````"),
        ("````md\n", "\n````"),
        ("```markdown\r\n", "\r\n```"),
        ("```markdown\n", "\n```"),
        ("```md\r\n", "\r\n```"),
        ("```md\n", "\n```"),
    ] {
        if let Some(body) = text.strip_prefix(opening) {
            return Some(body.strip_suffix(closing).unwrap_or(body));
        }
    }
    None
}

/// Render markdown body with code fence detection and syntax highlighting.
fn render_markdown_body(
    markdown: &str, rail_style: CellStyle, text_style: CellStyle, bg: Color, width: usize, body_width: usize,
) -> Vec<Row> {
    let p = super::style::palette();
    let gutter_style = CellStyle::new().fg(p.overlay0).bg(bg);
    let code_width = body_width.saturating_sub(utils::text_width(GUTTER));
    let mut rows = Vec::new();
    let mut in_code_fence = false;
    let mut code_lang: Option<String> = None;
    let mut code_buf = String::new();
    let mut pending_plain = Vec::new();
    let mut lines = markdown.lines().peekable();

    while let Some(line) = lines.next() {
        if line.starts_with("```") {
            flush_plain_markdown_lines(
                &mut rows,
                &mut pending_plain,
                rail_style,
                text_style,
                bg,
                width,
                body_width,
            );
            if !in_code_fence {
                in_code_fence = true;
                let lang_str = line.trim_start_matches('`').trim();
                code_lang = if lang_str.is_empty() { None } else { Some(lang_str.to_string()) };
                code_buf.clear();
            } else {
                let lang = code_lang.as_deref();
                let highlighted = super::highlight::highlight_lines(&code_buf, lang);
                push_highlighted_code_rows(&mut rows, highlighted, rail_style, gutter_style, bg, width, code_width);
                in_code_fence = false;
                code_lang = None;
                code_buf.clear();
            }
            continue;
        }

        if in_code_fence {
            code_buf.push_str(line);
            code_buf.push('\n');
            continue;
        }

        if MarkdownTable::is_separator(line)
            && !pending_plain.is_empty()
            && let Some(header) = pending_plain.pop()
        {
            let mut table = MarkdownTable::new(&header, line);
            while let Some(peeked) = lines.peek() {
                if !MarkdownTable::is_row(peeked) {
                    break;
                }
                match lines.next() {
                    Some(row_line) => table.push_row(row_line),
                    None => break,
                }
            }
            if table.is_valid() {
                rows.extend(table.render(rail_style, bg, width, body_width));
                continue;
            }
            pending_plain.push(header);
            pending_plain.push(line.to_string());
        } else {
            pending_plain.push(line.to_string());
        }
    }

    flush_plain_markdown_lines(
        &mut rows,
        &mut pending_plain,
        rail_style,
        text_style,
        bg,
        width,
        body_width,
    );

    if in_code_fence && !code_buf.is_empty() {
        let lang = code_lang.as_deref();
        let highlighted = super::highlight::highlight_lines(&code_buf, lang);
        push_highlighted_code_rows(&mut rows, highlighted, rail_style, gutter_style, bg, width, code_width);
    }

    if rows.is_empty() {
        rows.push(Row::blank(width, CellStyle::new().bg(bg)));
    }

    rows
}

/// Append syntax-highlighted code rows, hard-wrapping oversized lines while
/// preserving the highlighter's spans and styles on each continuation row.
fn push_highlighted_code_rows(
    rows: &mut Vec<Row>, highlighted: Vec<Vec<Span>>, rail_style: CellStyle, gutter_style: CellStyle, bg: Color,
    width: usize, code_width: usize,
) {
    for highlighted_line in highlighted {
        let content_spans = highlighted_line
            .into_iter()
            .map(|span| Span { text: span.text, style: span.style.bg(bg) })
            .collect::<Vec<_>>();
        for wrapped in super::layout::wrap_spans(&content_spans, code_width) {
            let mut spans = vec![Span::styled(ENTRY_RAIL, rail_style), Span::styled(GUTTER, gutter_style)];
            spans.extend(wrapped);
            rows.push(Row::padded(spans, width, CellStyle::new().bg(bg)));
        }
    }
}

fn flush_plain_markdown_lines(
    rows: &mut Vec<Row>, pending: &mut Vec<String>, rail_style: CellStyle, text_style: CellStyle, bg: Color,
    width: usize, body_width: usize,
) {
    for line in pending.drain(..) {
        if line.is_empty() {
            rows.push(Row::blank(width, CellStyle::new().bg(bg)));
        } else {
            for wrapped in super::layout::wrap_text(&line, body_width) {
                rows.push(Row::padded(
                    vec![Span::styled(ENTRY_RAIL, rail_style), Span::styled(wrapped, text_style)],
                    width,
                    CellStyle::new().bg(bg),
                ));
            }
        }
    }
}

/// Detect whether a tool output line is a section header.
fn is_section_header(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("── ") || trimmed.starts_with("$ ")
}

/// Derive a label for status entries based on text content.
fn status_label_for(text: &str) -> &'static str {
    if text.starts_with("state:") {
        "Status"
    } else if text.starts_with("context  ") {
        "Context"
    } else if text.starts_with("logs  ") {
        "Session log"
    } else if text.starts_with("provider:") || text.starts_with("tool budget:") {
        "Diagnostic"
    } else if text.starts_with("queued ") {
        "Queued"
    } else if text.starts_with("queue target:") {
        "Queue"
    } else if text.starts_with("background ") || text == "no background processes" {
        "Background"
    } else if text == "cancelled" {
        "Cancelled"
    } else {
        "System"
    }
}

#[cfg(test)]
mod tests;
