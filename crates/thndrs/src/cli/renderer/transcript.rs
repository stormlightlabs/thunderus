//! Semantic transcript row construction for the direct renderer.
//!
//! This module owns turning transcript [`Entry`] values into [`Row`] blocks.

use std::path::Path;

use crate::app::{App, Entry, ToolStatus};
use crate::internals::{SelfKnowledgeSnapshot, StartupSection};
use crate::providers::codex;
use crate::renderer::row::Row;
use crate::renderer::style::{CellStyle, Color, Span};
use crate::{renderer, utils};

/// Maximum tool output lines rendered before a truncation marker is shown.
const MAX_TOOL_OUTPUT_LINES: usize = 6;

/// Minimum content width where markdown tables remain legible.
const MIN_TABLE_RENDER_WIDTH: usize = 24;

/// Maximum skill-list rows shown under the startup workbench loaded heading.
const MAX_STARTUP_LOADED_SKILL_ROWS: usize = 4;

/// Gutter prefix for tool output lines.
pub const GUTTER: &str = "   │ ";

/// Role rail shown on transcript entry rows.
const ENTRY_RAIL: &str = "│ ";

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
    /// [`RowGroupId`] so native scrollback navigation can correlate rows to the
    /// originating entry.
    pub entry_index: Option<usize>,
}

impl TranscriptRowContext<'_> {
    /// Build all rows for a single transcript entry.
    pub fn rows_for_entry(&self, entry: &Entry) -> Vec<Row> {
        let mut rows = entry_to_rows(entry, self.user_label, self.width, self.cwd);
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
        Self { user_label, cwd, width, entry_index: None }
    }
}

#[derive(Clone, Copy)]
struct StartupBannerTheme {
    width: usize,
    bg: Color,
    brand_style: CellStyle,
    version_style: CellStyle,
    key_style: CellStyle,
    value_style: CellStyle,
    heading_style: CellStyle,
    search_style: CellStyle,
    attention_style: CellStyle,
    muted_style: CellStyle,
    hint_style: CellStyle,
    separator_style: CellStyle,
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
}

impl ToolBlockView<'_> {
    fn rows(&self) -> Vec<Row> {
        let p = super::style::palette();
        let (status_label, status_color, icon) = match self.status {
            ToolStatus::Running => ("running", p.peach, "·"),
            ToolStatus::Ok => ("ok", p.green, "✓"),
            ToolStatus::Failed => ("failed", p.red, "✕"),
            ToolStatus::Cancelled => ("cancelled", p.peach, "○"),
        };
        let header_style = CellStyle::new().fg(p.text).bg(self.bg).bold();
        let status_style = CellStyle::new().fg(status_color).bg(self.bg);
        let muted_style = CellStyle::new().fg(p.subtext0).bg(self.bg);
        let gutter_style = CellStyle::new().fg(p.overlay0).bg(self.bg);
        let rail_style = CellStyle::new().fg(status_color).bg(self.bg);
        let content_width = self.body_width.saturating_sub(utils::text_width(ENTRY_RAIL));
        let tool_content_width = content_width.saturating_sub(utils::text_width(GUTTER));

        let args_summary = summarize_tool_args(self.args, self.cwd);
        let base_name = self.name.split('#').next().unwrap_or(self.name);
        let lang = super::highlight::tool_output_language(base_name, self.args);

        let mut rows = vec![Row::blank(self.width, CellStyle::new().bg(self.bg))];

        let mut header_spans = vec![
            Span::styled(ENTRY_RAIL, rail_style),
            Span::styled(format!("{icon} "), status_style),
            Span::styled(self.name.to_string(), header_style),
            Span::styled(format!(" [{status_label}]"), status_style),
        ];

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
        if self.width >= 90 && !self.output.is_empty() && !header_spans.is_empty() {
            let metadata = format!("  lines: {}", self.output.len());
            let header_width: usize = header_spans.iter().map(|s| utils::text_width(&s.text)).sum();
            if header_width + utils::text_width(&metadata) <= self.body_width {
                header_spans.push(Span::styled(metadata, CellStyle::new().fg(p.overlay0).bg(self.bg)));
            }
        }
        if !header_spans.is_empty() {
            rows.push(Row::padded(header_spans, self.width, CellStyle::new().bg(self.bg)));
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
                            "   │ … ({} lines stored, {} shown here)",
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
        rows.push(Row::padded(
            vec![
                Span::styled(ENTRY_RAIL, rail_style),
                Span::styled(Self::separator(&column_widths), separator_style),
            ],
            width,
            CellStyle::new().bg(bg),
        ));
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

    fn separator(widths: &[usize]) -> String {
        widths
            .iter()
            .map(|width| "─".repeat((*width).max(1)))
            .collect::<Vec<_>>()
            .join("  ")
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
        let bg = p.panel_bg;
        let theme = StartupBannerTheme {
            width,
            bg,
            brand_style: CellStyle::new().fg(p.accent).bg(bg).bold(),
            version_style: CellStyle::new().fg(p.overlay0).bg(bg),
            key_style: CellStyle::new().fg(p.mauve).bg(bg),
            value_style: CellStyle::new().fg(p.text).bg(bg),
            heading_style: CellStyle::new().fg(p.teal).bg(bg).bold(),
            search_style: CellStyle::new().fg(p.pink).bg(bg).bold(),
            attention_style: CellStyle::new().fg(p.peach).bg(bg).bold(),
            muted_style: CellStyle::new().fg(p.subtext0).bg(bg),
            hint_style: CellStyle::new().fg(p.teal).bg(bg),
            separator_style: CellStyle::new().fg(p.overlay0).bg(bg),
        };
        let snapshot = self.self_knowledge_snapshot();
        let sections = snapshot.startup_sections();

        let mut rows = vec![Row::blank(width, CellStyle::new().bg(bg))];
        push_banner_brand_row(&mut rows, &snapshot, theme);
        push_banner_separator(&mut rows, theme);
        push_banner_key_value_row(
            &mut rows,
            "model",
            &format!(
                "{} · {}",
                codex::display_model_id(&snapshot.runtime.provider.model),
                snapshot.runtime.provider.provider
            ),
            theme,
        );
        if !self.cwd.as_os_str().is_empty() {
            let cwd_line = super::path_display::cwd_segment(&self.cwd, theme.body_width());
            let cwd_line = cwd_line.strip_prefix("cwd: ").unwrap_or(&cwd_line);
            push_banner_key_value_row(&mut rows, "cwd", cwd_line, theme);
        }
        push_banner_key_value_row(&mut rows, "search", &snapshot.runtime.provider.search.mode, theme);

        push_banner_separator(&mut rows, theme);
        push_banner_heading(&mut rows, "context", theme);
        for line in self.startup_section_lines(&sections, "Context") {
            push_wrapped_banner_text(&mut rows, &line, BannerIndent(2, 2), theme, theme.value_style);
        }

        push_banner_separator(&mut rows, theme);
        push_banner_heading(&mut rows, "skills", theme);
        for line in &snapshot.loaded_skill_lines(theme.body_width().saturating_sub(2)) {
            push_wrapped_banner_text(&mut rows, line, BannerIndent(2, 2), theme, theme.muted_style);
        }

        push_banner_separator(&mut rows, theme);
        push_banner_heading(&mut rows, "search", theme);
        push_wrapped_banner_text(
            &mut rows,
            &snapshot.search_line(),
            BannerIndent(2, 2),
            theme,
            theme.muted_style,
        );

        let diagnostics = self.startup_section_lines(&sections, "Diagnostics");
        if diagnostics.iter().any(|line| line != "(none)") {
            push_banner_separator(&mut rows, theme);
            push_banner_heading(&mut rows, "attention", theme);
            for line in diagnostics {
                push_wrapped_banner_text(&mut rows, &line, BannerIndent(2, 2), theme, theme.muted_style);
            }
        }

        push_banner_separator(&mut rows, theme);
        push_wrapped_banner_text(
            &mut rows,
            "Ask for a change, run a command, or inspect this repo.",
            BannerIndent(0, 0),
            theme,
            theme.muted_style,
        );
        rows.push(Row::blank(width, CellStyle::new().bg(bg)));
        push_wrapped_banner_text(
            &mut rows,
            "? help /model /search",
            BannerIndent(0, 0),
            theme,
            theme.hint_style,
        );

        rows.push(Row::blank(width, CellStyle::new().bg(bg)));
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
                        "Context" | "Diagnostics" => super::path_display::transcript_line(line, &self.cwd),
                        _ => line.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl SelfKnowledgeSnapshot {
    fn loaded_skill_lines(&self, width: usize) -> Vec<String> {
        let skill_names = self
            .inventory
            .references
            .skills
            .iter()
            .map(|skill| skill.name.as_str())
            .collect::<Vec<_>>();
        if skill_names.is_empty() {
            return vec!["(none)".to_string()];
        }

        let content_width = width.max(1);
        let mut shown = 0usize;
        let mut rows = Vec::new();
        for count in 1..=skill_names.len() {
            let hidden = skill_names.len() - count;
            let candidate = loaded_skill_rows(&skill_names[..count], hidden > 0, content_width);
            let available_rows = if hidden > 0 {
                MAX_STARTUP_LOADED_SKILL_ROWS.saturating_sub(1)
            } else {
                MAX_STARTUP_LOADED_SKILL_ROWS
            };
            if candidate.len() > available_rows {
                break;
            }
            shown = count;
            rows = candidate;
        }

        let hidden = skill_names.len() - shown;
        if hidden > 0 {
            rows.push(format!("...{hidden} skills hidden"));
        }

        rows
    }

    fn search_line(&self) -> String {
        format!(
            "{}; {}",
            self.runtime.provider.search.provider_native_search, self.runtime.provider.search.local_search
        )
    }
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

fn loaded_skill_rows(skill_names: &[&str], has_hidden: bool, width: usize) -> Vec<String> {
    let mut rows = vec![String::new()];
    for (index, name) in skill_names.iter().enumerate() {
        let suffix = if has_hidden || index + 1 < skill_names.len() { "," } else { "" };
        add_loaded_skill(&mut rows, &format!("{name}{suffix}"), width);
    }
    rows.into_iter().filter(|line| !line.is_empty()).collect()
}

fn add_loaded_skill(rows: &mut Vec<String>, token: &str, width: usize) {
    for part in split_hyphenated_token(token, width) {
        let current = rows.last_mut().expect("loaded skill rows should keep one active row");
        let separator_width = usize::from(!current.is_empty());
        if current.is_empty() {
            current.push_str(&part);
        } else if utils::text_width(current) + separator_width + utils::text_width(&part) <= width {
            current.push(' ');
            current.push_str(&part);
        } else {
            rows.push(part);
        }
    }
}

fn split_hyphenated_token(token: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut remaining = token;
    let mut parts = Vec::new();
    while utils::text_width(remaining) > width {
        let split = hyphen_split_index(remaining, width);
        parts.push(remaining[..split].to_string());
        remaining = &remaining[split..];
    }
    if !remaining.is_empty() {
        parts.push(remaining.to_string());
    }
    parts
}

fn hyphen_split_index(text: &str, width: usize) -> usize {
    let mut fallback = text.len();
    for (index, _) in text.char_indices() {
        if index > 0 && utils::text_width(&text[..index]) > width {
            fallback = index;
            break;
        }
    }

    text.char_indices()
        .filter_map(|(index, ch)| {
            let split = index + ch.len_utf8();
            (ch == '-' && split <= fallback && utils::text_width(&text[..split]) <= width).then_some(split)
        })
        .next_back()
        .unwrap_or_else(|| fallback.max(1))
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

fn push_banner_brand_row(rows: &mut Vec<Row>, snapshot: &SelfKnowledgeSnapshot, theme: StartupBannerTheme) {
    rows.push(Row::padded(
        vec![
            Span::styled(snapshot.identity.app_name.to_string(), theme.brand_style),
            Span::styled(format!("  v{}", snapshot.identity.app_version), theme.version_style),
        ],
        theme.width,
        CellStyle::new().bg(theme.bg),
    ));
}

fn push_banner_separator(rows: &mut Vec<Row>, theme: StartupBannerTheme) {
    let width = theme.body_width().max(1);
    rows.push(Row::padded(
        vec![Span::styled("─".repeat(width), theme.separator_style)],
        theme.width,
        CellStyle::new().bg(theme.bg),
    ));
}

fn push_banner_key_value_row(rows: &mut Vec<Row>, key: &str, value: &str, theme: StartupBannerTheme) {
    let key_width = 7usize.min(theme.body_width());
    let prefix = format!("{key:<key_width$}");
    let value_width = theme
        .body_width()
        .saturating_sub(utils::text_width(&prefix))
        .saturating_sub(1)
        .max(1);
    for (index, line) in super::layout::wrap_text(value, value_width).into_iter().enumerate() {
        let key_text = if index == 0 { prefix.clone() } else { " ".repeat(key_width) };
        rows.push(Row::padded(
            vec![
                Span::styled(key_text, theme.key_style),
                Span::styled(" ".to_string(), theme.muted_style),
                Span::styled(line, theme.value_style),
            ],
            theme.width,
            CellStyle::new().bg(theme.bg),
        ));
    }
}

fn push_banner_heading(rows: &mut Vec<Row>, label: &str, theme: StartupBannerTheme) {
    let style = match label {
        "search" => theme.search_style,
        "attention" => theme.attention_style,
        _ => theme.heading_style,
    };
    rows.push(Row::padded(
        vec![Span::styled(label.to_ascii_uppercase(), style)],
        theme.width,
        CellStyle::new().bg(theme.bg),
    ));
}

fn push_wrapped_banner_text(
    rows: &mut Vec<Row>, text: &str, indent: BannerIndent, theme: StartupBannerTheme, text_style: CellStyle,
) {
    let wrapped = wrap_with_indent(text, indent, theme.body_width());
    for line in wrapped {
        rows.push(Row::padded(
            vec![Span::styled(line, text_style)],
            theme.width,
            CellStyle::new().bg(theme.bg),
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
fn entry_to_rows(entry: &Entry, user_label: &str, width: usize, cwd: &Path) -> Vec<Row> {
    let p = super::style::palette();
    let bg = p.surface_dim;
    let body_width = super::layout::content_width(width);
    let railed_body_width = body_width.saturating_sub(utils::text_width(ENTRY_RAIL));

    match entry {
        Entry::User { text } => {
            let surface1 = p.surface1;
            let rail_style = CellStyle::new().fg(p.blue).bg(surface1).bold();
            let label_style = CellStyle::new().fg(p.blue).bg(surface1).bold();
            let text_style = CellStyle::new().fg(p.text).bg(surface1);
            let mut rows = LabeledBlock::new(rail_style, label_style, text_style, surface1, width, railed_body_width)
                .build(user_label, text);
            rows.push(Row::blank(width, CellStyle::new().bg(surface1)));
            rows
        }
        Entry::Agent { text, .. } => {
            let rail_style = CellStyle::new().fg(p.green).bg(bg).bold();
            let label_style = CellStyle::new().fg(p.green).bg(bg).bold();
            assistant_block_rows(text, rail_style, label_style, bg, width, railed_body_width)
        }
        Entry::Reasoning { text, streaming } => {
            let rail_style = CellStyle::new().fg(p.mauve).bg(bg).bold();
            let label_style = CellStyle::new().fg(p.mauve).bg(bg).bold();
            let text_style = CellStyle::new().fg(p.subtext0).bg(bg).italic();
            let label = if *streaming { "Thinking ·" } else { "Thinking ✓" };
            LabeledBlock::new(rail_style, label_style, text_style, bg, width, railed_body_width).build(label, text)
        }
        Entry::Tool { name, arguments, status, output } => {
            ToolBlockView { name, args: arguments, status: *status, output, width, body_width, bg, cwd }.rows()
        }
        Entry::Status { text } => {
            let rail_style = CellStyle::new().fg(p.overlay1).bg(bg);
            let label_style = CellStyle::new().fg(p.overlay1).bg(bg).bold();
            let text_style = CellStyle::new().fg(p.text).bg(bg);
            LabeledBlock::new(rail_style, label_style, text_style, bg, width, railed_body_width)
                .build(status_label_for(text), text)
        }
        Entry::Error { text } => {
            let rail_style = CellStyle::new().fg(p.red).bg(bg).bold();
            let label_style = CellStyle::new().fg(p.red).bg(bg).bold();
            let text_style = CellStyle::new().fg(p.text).bg(bg);
            LabeledBlock::new(rail_style, label_style, text_style, bg, width, railed_body_width).build("⚠ Error", text)
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
                Span::styled("Agent".to_string(), label_style),
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

/// Extract Markdown from either the internal four-tick wrapper or ordinary
/// fenced Markdown returned by a provider.
fn assistant_markdown_body(text: &str) -> Option<&str> {
    if let Some(rest) = text
        .strip_prefix("````md\n")
        .or_else(|| text.strip_prefix("````markdown\n"))
    {
        return Some(rest.strip_suffix("\n````").unwrap_or(rest));
    }

    text.contains("```").then_some(text)
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
    if text.starts_with("context  ") {
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
