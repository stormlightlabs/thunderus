//! Semantic transcript row construction for the terminal renderer.
//!
//! This renderer module owns the terminal presentation of transcript content:
//! layout, styling, collapsing, and width-aware row construction. Semantic
//! identity and lifecycle state belong to `app::transcript_blocks`.

use std::path::Path;

use markdown::mdast::Node;

use crate::app::{App, Entry, ToolStatus};
use crate::internals::StartupSection;
use crate::renderer::row::Row;
use crate::renderer::style::{CellStyle, Color, Span};
use crate::{renderer, utils};

use super::view::skill_activation_summary;

/// Maximum tool output lines rendered before a truncation marker is shown.
const MAX_TOOL_OUTPUT_LINES: usize = 6;

/// Minimum content width where markdown tables remain legible.
const MIN_TABLE_RENDER_WIDTH: usize = 24;

/// Gutter prefix for tool output lines.
pub const GUTTER: &str = "   · ";

/// Role rail shown on transcript entry rows and partial-entry continuations.
pub const ENTRY_RAIL: &str = "  ";

/// Stable rail shared by consecutive tool activity rows.
pub const ACTIVITY_RAIL: &str = "│ ";

/// Progressive-disclosure treatment for a semantic activity entry.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum ActivityProjection {
    /// Render the tool normally when no semantic projection is available.
    #[default]
    Regular,
    /// The entry is represented by the summary attached to the first entry in its group.
    Hidden,
    /// Render the group summary, optionally followed by this entry's disclosed tool row.
    Summary { summary: ActivitySummary, show_tool: bool },
    /// Render an individual tool row because its activity group is disclosed.
    DisclosedTool,
}

/// Semantic family displayed by an activity row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActivityKind {
    Explore,
    Command,
    Edit,
    Test,
}

/// Visual priority of an activity in the durable timeline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActivityImportance {
    Routine,
    Significant,
}

/// Semantic state displayed by one activity row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivitySummary {
    pub kind: ActivityKind,
    pub importance: ActivityImportance,
    pub calls: usize,
    pub reads: usize,
    pub searches: usize,
    pub running: bool,
    pub failed: bool,
    pub cancelled: bool,
    pub label: String,
    pub marker: String,
    pub details: Vec<String>,
    pub preview: Vec<String>,
    pub hidden_lines: usize,
    pub detail_target: bool,
    pub detail_open: bool,
}

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
    /// Whether Ctrl+O currently targets this tool entry.
    pub detail_target: bool,
    /// Whether this tool's output is expanded inline.
    pub detail_open: bool,
    /// First raw output line displayed for an expanded tool.
    pub detail_scroll: usize,
    /// Coalesced routine exploration state for this entry.
    pub activity: ActivityProjection,
}

impl TranscriptRowContext<'_> {
    /// Build all rows for a single transcript entry.
    pub fn rows_for_entry(&self, entry: &Entry) -> Vec<Row> {
        let mut rows = entry_to_rows(entry, self);
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
        let activity_is_live = match &self.activity {
            ActivityProjection::DisclosedTool => true,
            ActivityProjection::Summary { summary, .. } => summary.running || summary.detail_open,
            ActivityProjection::Regular | ActivityProjection::Hidden => false,
        };
        if self.detail_open || activity_is_live {
            return (Vec::new(), rows);
        }
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
        Self {
            user_label,
            cwd,
            width,
            entry_index: None,
            tool_group_start: true,
            detail_target: true,
            detail_open: false,
            detail_scroll: 0,
            activity: ActivityProjection::Regular,
        }
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
    meta_style: CellStyle,
}

impl StartupBannerTheme {
    fn body_width(self) -> usize {
        super::layout::UiGeometry::new(self.width)
            .prose_width()
            .saturating_sub(3)
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
    detail_target: bool,
    detail_open: bool,
    detail_scroll: usize,
}

impl ToolBlockView<'_> {
    fn rows(&self) -> Vec<Row> {
        let p = super::style::palette();
        let (status_label, status_color, icon) = match self.status {
            ToolStatus::Running => ("Running", p.active, "·"),
            ToolStatus::Ok => ("DONE", p.success, "✓"),
            ToolStatus::Failed => ("Failed", p.failure, "✕"),
            ToolStatus::Cancelled => ("Stopped", p.active, "○"),
        };
        let header_style = CellStyle::new().fg(p.primary).bg(self.bg).bold();
        let status_style = CellStyle::new().fg(status_color).bg(self.bg);
        let muted_style = CellStyle::new().fg(p.secondary).bg(self.bg);
        let gutter_style = CellStyle::new().fg(p.border).bg(self.bg);
        let rail_style = CellStyle::new().fg(p.warning).bg(self.bg);
        let content_width = self.body_width.saturating_sub(utils::text_width(ENTRY_RAIL));
        let tool_content_width = content_width.saturating_sub(utils::text_width(GUTTER));

        let base_name = self.name.split('#').next().unwrap_or(self.name);
        let args_summary = summarize_tool_invocation(base_name, self.args, self.cwd);
        let output = self
            .output
            .iter()
            .map(|line| super::tool_output::sanitize_terminal_text(line))
            .collect::<Vec<_>>();
        let content = super::tool_output::project(base_name, self.args, &output);

        let mut rows = Vec::new();
        if self.group_start {
            rows.push(Row::blank(self.width, CellStyle::new().bg(self.bg)));
        }

        let mut header_spans = vec![
            Span::styled(ACTIVITY_RAIL, rail_style),
            Span::styled(format!("{icon} [{status_label}] "), status_style),
            Span::styled(base_name.to_string(), header_style),
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
                        Span::styled(ACTIVITY_RAIL, rail_style),
                        Span::styled("  ", CellStyle::new().bg(self.bg)),
                        Span::styled(wrapped, muted_style),
                    ];
                    rows.push(Row::padded(spans, self.width, CellStyle::new().bg(self.bg)));
                }
                header_spans = Vec::new();
            }
        }
        let output_details = (!output.is_empty() && (self.detail_target || self.detail_open)).then(|| {
            if self.detail_open {
                format!("{} lines · Esc close", output.len())
            } else {
                format!("{} lines · Ctrl+O details", output.len())
            }
        });
        let details_in_header = output_details.as_ref().is_some_and(|details| {
            !header_spans.is_empty()
                && super::layout::spans_width(&header_spans) + 2 + utils::text_width(details) <= self.body_width
        });
        if details_in_header && let Some(details) = output_details.as_ref() {
            header_spans.push(Span::styled(format!("  {details}"), muted_style));
        }
        if !header_spans.is_empty() {
            rows.push(Row::padded(header_spans, self.width, CellStyle::new().bg(self.bg)));
        }
        if !details_in_header && let Some(details) = output_details {
            rows.push(Row::padded(
                vec![
                    Span::styled(ACTIVITY_RAIL, rail_style),
                    Span::styled("  ", CellStyle::new().bg(self.bg)),
                    Span::styled(details, muted_style),
                ],
                self.width,
                CellStyle::new().bg(self.bg),
            ));
        }

        if let Some(summary) = edit_summary_line(self.name, self.args, &output, self.status, self.cwd) {
            rows.push(Row::padded(
                vec![
                    Span::styled(ACTIVITY_RAIL, rail_style),
                    Span::styled("   edit  ", CellStyle::new().fg(p.border).bg(self.bg).bold()),
                    Span::styled(summary, muted_style),
                ],
                self.width,
                CellStyle::new().bg(self.bg),
            ));
        }

        if let Some(summary) = projected_diff_summary(&content) {
            rows.push(Row::padded(
                vec![
                    Span::styled(ACTIVITY_RAIL, rail_style),
                    Span::styled("   diff  ", CellStyle::new().fg(p.border).bg(self.bg).bold()),
                    Span::styled(summary, muted_style),
                ],
                self.width,
                CellStyle::new().bg(self.bg),
            ));
        }

        if self.status == ToolStatus::Ok && !self.detail_open {
            return rows;
        }

        if let super::tool_output::ContentKind::Diff(diff) = &content {
            let limit = (!self.detail_open).then_some(MAX_TOOL_OUTPUT_LINES);
            let diff_rows = super::diff::rows(diff, self.width, self.body_width, self.bg, limit);
            let start = if self.detail_open { self.detail_scroll.min(diff_rows.len()) } else { 0 };
            rows.extend(diff_rows.into_iter().skip(start));
            return rows;
        }

        let preview_start = if self.detail_open {
            self.detail_scroll.min(output.len())
        } else if self.status == ToolStatus::Running {
            output.len().saturating_sub(MAX_TOOL_OUTPUT_LINES)
        } else {
            0
        };
        let preview = &output[preview_start..];

        let mut output_rows = Vec::new();
        match content {
            super::tool_output::ContentKind::Code { language } => {
                let joined: String = preview
                    .iter()
                    .map(|line| {
                        let line = super::path_display::transcript_line(line, self.cwd);
                        format!("{line}\n")
                    })
                    .collect();
                let highlighted = super::highlight::highlight_lines(&joined, Some(language));
                for hl_row in highlighted {
                    let mut spans = vec![
                        Span::styled(ACTIVITY_RAIL, rail_style),
                        Span::styled(GUTTER, gutter_style),
                    ];
                    let content_spans: Vec<_> = hl_row
                        .into_iter()
                        .map(|s| Span { text: s.text, style: s.style.bg(self.bg) })
                        .collect();
                    spans.extend(super::layout::truncate_spans(
                        &content_spans,
                        tool_content_width,
                        muted_style,
                    ));
                    output_rows.push(Row::padded(spans, self.width, CellStyle::new().bg(self.bg)));
                }
            }
            super::tool_output::ContentKind::SearchResults => {
                for line in preview {
                    let line = super::path_display::transcript_line(line, self.cwd);
                    if let Some((path, number, content)) = search_result_parts(&line) {
                        let prefix = vec![
                            Span::styled(path, CellStyle::new().fg(p.link).bg(self.bg)),
                            Span::styled(format!(":{number}:"), CellStyle::new().fg(p.warning).bg(self.bg)),
                        ];
                        let prefix = super::layout::truncate_spans(&prefix, tool_content_width, muted_style);
                        let prefix_width = super::layout::spans_width(&prefix);
                        let first_width = tool_content_width.saturating_sub(prefix_width);
                        let wrapped = super::layout::wrap_text_preserving_whitespace(content, first_width.max(1));
                        for (index, part) in wrapped.into_iter().enumerate() {
                            let mut spans = vec![
                                Span::styled(ACTIVITY_RAIL, rail_style),
                                Span::styled(GUTTER, gutter_style),
                            ];
                            if index == 0 {
                                spans.extend(prefix.clone());
                            }
                            spans.push(Span::styled(part, CellStyle::new().fg(p.secondary).bg(self.bg)));
                            output_rows.push(Row::padded(
                                super::layout::truncate_spans(&spans, self.body_width, muted_style),
                                self.width,
                                CellStyle::new().bg(self.bg),
                            ));
                        }
                    } else {
                        push_plain_tool_line(
                            &mut output_rows,
                            &line,
                            self.width,
                            tool_content_width,
                            self.bg,
                            rail_style,
                            gutter_style,
                        );
                    }
                }
            }
            super::tool_output::ContentKind::Plain => {
                for line in preview {
                    let line = super::path_display::transcript_line(line, self.cwd);
                    push_plain_tool_line(
                        &mut output_rows,
                        &line,
                        self.width,
                        tool_content_width,
                        self.bg,
                        rail_style,
                        gutter_style,
                    );
                }
            }
            super::tool_output::ContentKind::Diff(_) => unreachable!("diff content returned above"),
        }

        if self.detail_open {
            rows.extend(output_rows);
        } else {
            rows.extend(bounded_tool_output_rows(
                output_rows,
                output.len(),
                self.status,
                self.detail_target,
                self.width,
                self.bg,
                rail_style,
                muted_style,
            ));
        }

        rows
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "The row projection needs the surrounding transcript styles."
)]
fn bounded_tool_output_rows(
    output_rows: Vec<Row>, stored_lines: usize, status: ToolStatus, detail_target: bool, width: usize, bg: Color,
    rail_style: CellStyle, muted_style: CellStyle,
) -> Vec<Row> {
    if output_rows.len() <= MAX_TOOL_OUTPUT_LINES && stored_lines <= MAX_TOOL_OUTPUT_LINES {
        return output_rows;
    }

    let hidden_lines = stored_lines
        .max(output_rows.len())
        .saturating_sub(MAX_TOOL_OUTPUT_LINES);
    let marker_text = format!(
        "     … +{hidden_lines} lines{}",
        if detail_target { " · Ctrl+O details" } else { "" }
    );
    let marker_spans = vec![
        Span::styled(ACTIVITY_RAIL, rail_style),
        Span::styled(marker_text, muted_style),
    ];
    let marker = Row::padded(
        super::layout::truncate_spans(&marker_spans, width, muted_style),
        width,
        CellStyle::new().bg(bg),
    );

    if status == ToolStatus::Running {
        let start = output_rows.len().saturating_sub(MAX_TOOL_OUTPUT_LINES);
        let mut bounded = Vec::with_capacity(MAX_TOOL_OUTPUT_LINES + 1);
        bounded.push(marker);
        bounded.extend(output_rows.into_iter().skip(start));
        return bounded;
    }

    const HEAD_ROWS: usize = 2;
    let tail_rows = MAX_TOOL_OUTPUT_LINES - HEAD_ROWS;
    let tail_start = output_rows.len().saturating_sub(tail_rows).max(HEAD_ROWS);
    let mut bounded = Vec::with_capacity(MAX_TOOL_OUTPUT_LINES + 1);
    bounded.extend(output_rows.iter().take(HEAD_ROWS).cloned());
    bounded.push(marker);
    bounded.extend(output_rows.into_iter().skip(tail_start));
    bounded
}

fn push_plain_tool_line(
    rows: &mut Vec<Row>, line: &str, width: usize, content_width: usize, bg: Color, rail_style: CellStyle,
    gutter_style: CellStyle,
) {
    let p = super::style::palette();
    let content_style = if is_section_header(line) {
        CellStyle::new().fg(p.secondary).bg(bg).bold()
    } else {
        CellStyle::new().fg(p.secondary).bg(bg)
    };
    for wrapped in super::layout::wrap_text_preserving_whitespace(line, content_width) {
        rows.push(Row::padded(
            vec![
                Span::styled(ACTIVITY_RAIL, rail_style),
                Span::styled(GUTTER, gutter_style),
                Span::styled(wrapped, content_style),
            ],
            width,
            CellStyle::new().bg(bg),
        ));
    }
}

fn search_result_parts(line: &str) -> Option<(String, &str, &str)> {
    let (path, tail) = line.split_once(':')?;
    let (number, content) = tail.split_once(':')?;
    number.parse::<usize>().ok()?;
    Some((path.to_string(), number, content))
}

fn activity_summary_rows(
    summary: &ActivitySummary, group_start: bool, width: usize, body_width: usize, bg: Color,
) -> Vec<Row> {
    let p = super::style::palette();
    let rail_style = CellStyle::new().fg(p.warning).bg(bg);
    let status_style = CellStyle::new()
        .fg(if summary.failed {
            p.failure
        } else if summary.running || summary.cancelled {
            p.active
        } else {
            p.success
        })
        .bg(bg);
    let muted_style = CellStyle::new().fg(p.secondary).bg(bg);
    let mut rows = Vec::new();
    if group_start {
        rows.push(Row::blank(width, CellStyle::new().bg(bg)));
    }

    let marker = format!("{} ", summary.marker);
    let label_width = body_width.saturating_sub(utils::text_width(ACTIVITY_RAIL) + utils::text_width(&marker));
    let status = format!("{marker}{}", utils::truncate_ellipsis(&summary.label, label_width));
    let available = body_width.saturating_sub(utils::text_width(ACTIVITY_RAIL) + utils::text_width(&status));
    let disclosure = if summary.detail_open {
        Some("Esc close")
    } else if summary.detail_target {
        Some("Ctrl+O details")
    } else {
        None
    };
    let full_details = disclosure.map_or_else(
        || match summary.details.is_empty() {
            true => String::new(),
            false => format!(" · {}", summary.details.join(" · ")),
        },
        |hint| match summary.details.is_empty() {
            true => format!(" · {hint}"),
            false => format!(" · {} · {hint}", summary.details.join(" · ")),
        },
    );
    let details = if utils::text_width(&full_details) <= available {
        full_details
    } else if let Some(hint) = disclosure {
        let compact = if summary.calls > 1 {
            format!(" · {} calls · {hint}", summary.calls)
        } else {
            format!(" · {hint}")
        };
        if utils::text_width(&compact) <= available {
            compact
        } else {
            utils::truncate_ellipsis(&format!(" · {hint}"), available)
        }
    } else {
        utils::truncate_ellipsis(&full_details, available)
    };
    rows.push(Row::padded(
        vec![
            Span::styled(ACTIVITY_RAIL, rail_style),
            Span::styled(
                status,
                if summary.importance == ActivityImportance::Significant {
                    status_style.bold()
                } else {
                    status_style
                },
            ),
            Span::styled(details, muted_style),
        ],
        width,
        CellStyle::new().bg(bg),
    ));
    if !summary.detail_open {
        let preview_style = CellStyle::new()
            .fg(if summary.failed { p.primary } else { p.secondary })
            .bg(bg);
        let preview_width = body_width.saturating_sub(utils::text_width(ACTIVITY_RAIL) + utils::text_width(GUTTER));
        for line in &summary.preview {
            for wrapped in super::layout::wrap_text_preserving_whitespace(line, preview_width) {
                rows.push(Row::padded(
                    vec![
                        Span::styled(ACTIVITY_RAIL, rail_style),
                        Span::styled(GUTTER, CellStyle::new().fg(p.border).bg(bg)),
                        Span::styled(wrapped, preview_style),
                    ],
                    width,
                    CellStyle::new().bg(bg),
                ));
            }
        }
        if summary.hidden_lines > 0 {
            let hint = if summary.detail_target { " · Ctrl+O details" } else { "" };
            rows.push(Row::padded(
                vec![
                    Span::styled(ACTIVITY_RAIL, rail_style),
                    Span::styled(format!("{GUTTER}… +{} lines{hint}", summary.hidden_lines), muted_style),
                ],
                width,
                CellStyle::new().bg(bg),
            ));
        }
    }
    rows
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
        let text_style = CellStyle::new().fg(p.primary).bg(bg);
        let separator_style = CellStyle::new().fg(p.border).bg(bg);
        let header_theme = MarkdownTableTheme {
            cell_style: CellStyle::new().fg(p.primary).bg(bg).bold().underlined(),
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
                    Span::styled("(no rows)", CellStyle::new().fg(p.secondary).bg(bg)),
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
            attention_style: CellStyle::new().fg(p.active).bg(bg).bold(),
            muted_style: CellStyle::new().fg(p.secondary).bg(bg),
            hint_style: CellStyle::new().fg(p.warning).bg(bg).bold(),
            rail_style: CellStyle::new().fg(p.border).bg(bg),
            meta_style: CellStyle::new().fg(p.secondary).bg(bg),
        };
        let snapshot = self.self_knowledge_snapshot();
        let sections = snapshot.startup_sections();
        let mut diagnostics = self
            .startup_section_lines(&sections, "Diagnostics")
            .into_iter()
            .skip(self.transcript.skill_diagnostics.len())
            .collect::<Vec<_>>();
        diagnostics.extend(
            self.transcript
                .context_diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.severity != crate::context::InstructionSeverity::Info)
                .map(|diagnostic| super::path_display::transcript_line(&diagnostic.summary(), &self.runtime.cwd)),
        );
        diagnostics.extend(self.transcript.skill_diagnostics.iter().map(|diagnostic| {
            let name = diagnostic
                .path
                .parent()
                .and_then(std::path::Path::file_name)
                .and_then(std::ffi::OsStr::to_str)
                .unwrap_or("unknown");
            format!("Skill skipped ({name}): {}", diagnostic.message)
        }));

        let mut rows = Vec::new();
        push_banner_brand_row(&mut rows, theme);
        push_wrapped_banner_text(
            &mut rows,
            "Ask for change, run a command, or inspect the repo.",
            BannerIndent(2, 2),
            theme,
            theme.muted_style,
        );

        if diagnostics.iter().any(|line| line != "(none)") {
            rows.push(Row::blank(width, CellStyle::new()));
            push_banner_attention_heading(&mut rows, theme);
            for line in diagnostics {
                push_wrapped_banner_text(&mut rows, &line, BannerIndent(2, 2), theme, theme.muted_style);
            }
        }

        rows.push(Row::blank(width, CellStyle::new()));
        if super::layout::UiGeometry::new(width).density() != super::layout::Density::Cramped {
            push_banner_help(&mut rows, theme);
        }
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

fn push_banner_brand_row(rows: &mut Vec<Row>, theme: StartupBannerTheme) {
    rows.push(Row::padded(
        vec![
            Span::styled("  ", theme.rail_style),
            Span::styled("thndrs", theme.brand_style),
            Span::styled(" / ready", theme.meta_style.bold()),
        ],
        theme.width,
        CellStyle::new(),
    ));
}

fn push_banner_attention_heading(rows: &mut Vec<Row>, theme: StartupBannerTheme) {
    rows.push(Row::padded(
        vec![
            Span::styled("  ", theme.rail_style),
            Span::styled("ATTENTION", theme.attention_style),
        ],
        theme.width,
        CellStyle::new(),
    ));
}

fn push_banner_help(rows: &mut Vec<Row>, theme: StartupBannerTheme) {
    let spans = vec![
        Span::styled("  ", theme.rail_style),
        Span::styled("?", theme.hint_style),
        Span::styled(" help", theme.muted_style),
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

fn edit_summary_line(name: &str, args: &str, output: &[String], status: ToolStatus, cwd: &Path) -> Option<String> {
    let operation = name.split('#').next().unwrap_or(name);
    let is_edit_tool = matches!(operation, "create_file" | "replace_range" | "write_patch");
    if !is_edit_tool
        && !output
            .iter()
            .any(|line| line.contains("wrote") || line.contains("replaced"))
    {
        return None;
    }
    let path = edit_path_from_args(args)
        .or_else(|| output.iter().find_map(|line| path_like_suffix(line)))
        .map(|path| super::path_display::transcript_line(&path, cwd))
        .unwrap_or_else(|| "files changed".to_string());
    Some(format!("{operation} {path} [{}]", status.label()))
}

pub(crate) fn edit_path_from_args(args: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(args).ok()?;
    value
        .get("path")
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            value
                .get("patches")
                .and_then(serde_json::Value::as_array)
                .and_then(|patches| patches.first())
                .and_then(|patch| patch.get("path"))
                .and_then(serde_json::Value::as_str)
        })
        .map(str::to_string)
}

fn diff_summary_line(diff: &super::diff::UnifiedDiff) -> Option<String> {
    let (files, added, removed) = diff.summary();
    if added == 0 && removed == 0 && files.is_empty() {
        return None;
    }
    let file_label = match files.as_slice() {
        [] => "unknown file".to_string(),
        [file] => file.clone(),
        _ => format!("{} files", files.len()),
    };
    Some(format!("{file_label} +{added} -{removed}"))
}

fn projected_diff_summary(content: &super::tool_output::ContentKind) -> Option<String> {
    match content {
        super::tool_output::ContentKind::Diff(diff) => diff_summary_line(diff),
        _ => None,
    }
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
fn entry_to_rows(entry: &Entry, context: &TranscriptRowContext<'_>) -> Vec<Row> {
    let p = super::style::palette();
    let bg = Color::Reset;
    let width = context.width;
    let geometry = super::layout::UiGeometry::new(width);
    let body_width = geometry.technical_width();
    let railed_body_width = geometry.prose_width().saturating_sub(utils::text_width(ENTRY_RAIL));
    let railed_technical_width = body_width.saturating_sub(utils::text_width(ENTRY_RAIL));

    match entry {
        Entry::User { text } => {
            let rail_style = CellStyle::new().fg(p.link).bg(bg).bold();
            let label_style = CellStyle::new().fg(p.link).bg(bg).bold();
            let text_style = CellStyle::new().fg(p.secondary).bg(bg);
            LabeledBlock::new(rail_style, label_style, text_style, bg, width, railed_body_width)
                .build(context.user_label, text)
        }
        Entry::Agent { text, .. } => {
            let rail_style = CellStyle::new().fg(p.success).bg(bg).bold();
            assistant_block_rows(text, rail_style, bg, width, railed_body_width, railed_technical_width)
        }
        Entry::Reasoning { text, streaming } => {
            let rail_style = CellStyle::new().fg(p.reasoning).bg(bg).bold();
            let label_style = CellStyle::new().fg(p.secondary).bg(bg);
            let text_style = CellStyle::new().fg(p.secondary).bg(bg).italic();
            let label = if *streaming { "Thinking ·" } else { "Thinking ✓" };
            LabeledBlock::new(rail_style, label_style, text_style, bg, width, railed_body_width)
                .build_compact(label, text)
        }
        Entry::Skill { name, path, token_estimate, context_percent, .. } => {
            let rail_style = CellStyle::new().fg(p.reasoning).bg(bg).bold();
            let label_style = CellStyle::new().fg(p.reasoning).bg(bg).bold();
            let text_style = CellStyle::new().fg(p.secondary).bg(bg);
            let summary = skill_activation_summary(name, path, *token_estimate, *context_percent);
            LabeledBlock::new(rail_style, label_style, text_style, bg, width, railed_body_width)
                .build_compact("◆ Skill", &summary)
        }
        Entry::Tool { name, arguments, status, output } => {
            let tool_rows = |group_start| {
                ToolBlockView {
                    name,
                    args: arguments,
                    status: *status,
                    output,
                    width,
                    body_width,
                    bg,
                    cwd: context.cwd,
                    group_start,
                    detail_target: context.detail_target,
                    detail_open: context.detail_open,
                    detail_scroll: context.detail_scroll,
                }
                .rows()
            };
            match &context.activity {
                ActivityProjection::Regular => tool_rows(context.tool_group_start),
                ActivityProjection::Hidden => Vec::new(),
                ActivityProjection::DisclosedTool => tool_rows(false),
                ActivityProjection::Summary { summary, show_tool } => {
                    let mut rows = activity_summary_rows(summary, context.tool_group_start, width, body_width, bg);
                    if *show_tool {
                        rows.extend(tool_rows(false));
                    }
                    rows
                }
            }
        }
        Entry::Status { text } => {
            if text.starts_with("context request ") {
                return context_request_rows(text, width, railed_body_width, bg);
            }
            let rail_style = CellStyle::new().fg(p.secondary).bg(bg);
            let label_style = CellStyle::new().fg(p.secondary).bg(bg);
            let text_style = CellStyle::new().fg(p.secondary).bg(bg);
            if text.contains('\n') {
                LabeledBlock::new(rail_style, label_style, text_style, bg, width, railed_body_width)
                    .build(status_label_for(text), text)
            } else {
                LabeledBlock::new(rail_style, label_style, text_style, bg, width, railed_body_width)
                    .build_compact(status_label_for(text), text)
            }
        }
        Entry::Error { text } => {
            let rail_style = CellStyle::new().fg(p.failure).bg(bg).bold();
            let label_style = CellStyle::new().fg(p.failure).bg(bg).bold();
            let text_style = CellStyle::new().fg(p.primary).bg(bg);
            let mut rows = LabeledBlock::new(rail_style, label_style, text_style, bg, width, railed_body_width)
                .build("⚠ Error", text);
            rows.push(Row::blank(width, CellStyle::new().bg(bg)));
            rows
        }
    }
}

fn context_request_rows(text: &str, width: usize, body_width: usize, bg: Color) -> Vec<Row> {
    let p = super::style::palette();
    let rail_style = CellStyle::new().fg(p.accent).bg(bg);
    let heading_style = CellStyle::new().fg(p.accent).bg(bg).bold();
    let label_style = CellStyle::new().fg(p.secondary).bg(bg);
    let value_style = CellStyle::new().fg(p.primary).bg(bg);
    let mut lines = text.lines();
    let summary = lines.next().unwrap_or_default().trim_start_matches("context request ");
    let mut rows = vec![
        Row::blank(width, CellStyle::new().bg(bg)),
        Row::padded(
            vec![
                Span::styled(ENTRY_RAIL, rail_style),
                Span::styled("◇ Request", heading_style),
                Span::styled("  ", CellStyle::new().bg(bg)),
                Span::styled(summary.to_string(), CellStyle::new().fg(p.primary).bg(bg).bold()),
            ],
            width,
            CellStyle::new().bg(bg),
        ),
    ];

    for line in lines {
        if line.is_empty() {
            rows.push(Row::blank(width, CellStyle::new().bg(bg)));
            continue;
        }
        if let Some(section) = line.strip_prefix("── ") {
            rows.push(Row::padded(
                vec![
                    Span::styled(ENTRY_RAIL, rail_style),
                    Span::styled(section.to_string(), heading_style),
                ],
                width,
                CellStyle::new().bg(bg),
            ));
            continue;
        }

        let (label, value) = line.split_once("  ").unwrap_or(("", line));
        let label = if label.is_empty() { String::new() } else { format!("{label:<18}  ") };
        let content = vec![
            Span::styled(label, label_style),
            Span::styled(value.to_string(), value_style),
        ];
        for wrapped in super::layout::wrap_spans(&content, body_width) {
            let mut spans = vec![Span::styled(ENTRY_RAIL, rail_style)];
            spans.extend(wrapped);
            rows.push(Row::padded(spans, width, CellStyle::new().bg(bg)));
        }
    }
    rows
}

/// Build an assistant message block, detecting markdown code fences for syntax highlighting.
fn assistant_block_rows(
    text: &str, rail_style: CellStyle, bg: Color, width: usize, prose_width: usize, technical_width: usize,
) -> Vec<Row> {
    let p = super::style::palette();
    let text_style = CellStyle::new().fg(p.primary).bg(bg);
    let mut rows = vec![Row::blank(width, CellStyle::new().bg(bg))];

    rows.extend(render_markdown_body(
        assistant_markdown_body(text),
        rail_style,
        text_style,
        bg,
        width,
        prose_width,
        technical_width,
    ));

    if rows.len() == 1 {
        rows.push(Row::blank(width, CellStyle::new().bg(bg)));
    }
    rows
}

/// Extract Markdown from an optional outer provider wrapper.
fn assistant_markdown_body(text: &str) -> &str {
    strip_outer_markdown_fence(text).unwrap_or(text)
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
    markdown: &str, rail_style: CellStyle, text_style: CellStyle, bg: Color, width: usize, prose_width: usize,
    technical_width: usize,
) -> Vec<Row> {
    let p = super::style::palette();
    let gutter_style = CellStyle::new().fg(p.border).bg(bg);
    let code_width = technical_width.saturating_sub(utils::text_width(GUTTER));
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
                prose_width,
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
                flush_plain_markdown_lines(
                    &mut rows,
                    &mut pending_plain,
                    rail_style,
                    text_style,
                    bg,
                    width,
                    prose_width,
                );
                rows.extend(table.render(rail_style, bg, width, technical_width));
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
        prose_width,
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
    if pending.is_empty() {
        return;
    }

    let trailing_blank = pending.last().is_some_and(String::is_empty);
    let source = std::mem::take(pending).join("\n");
    match markdown::to_mdast(&source, &markdown::ParseOptions::default()) {
        Ok(Node::Root(root)) => {
            render_markdown_blocks(rows, &root.children, rail_style, text_style, bg, width, body_width)
        }
        _ => push_plain_markdown_fallback(rows, &source, rail_style, text_style, bg, width, body_width),
    }
    if trailing_blank
        && rows
            .last()
            .is_some_and(|row| row.spans.iter().any(|span| !span.text.trim().is_empty()))
    {
        rows.push(Row::blank(width, CellStyle::new().bg(bg)));
    }
}

fn render_markdown_blocks(
    rows: &mut Vec<Row>, nodes: &[Node], rail_style: CellStyle, text_style: CellStyle, bg: Color, width: usize,
    body_width: usize,
) {
    for (index, node) in nodes.iter().enumerate() {
        if index > 0
            && rows
                .last()
                .is_some_and(|row| !row.spans.iter().all(|span| span.text.trim().is_empty()))
        {
            rows.push(Row::blank(width, CellStyle::new().bg(bg)));
        }
        render_markdown_block(rows, node, rail_style, text_style, bg, width, body_width);
    }
}

fn render_markdown_block(
    rows: &mut Vec<Row>, node: &Node, rail_style: CellStyle, text_style: CellStyle, bg: Color, width: usize,
    body_width: usize,
) {
    let p = super::style::palette();
    match node {
        Node::Paragraph(paragraph) => push_markdown_spans(
            rows,
            markdown_inline_spans(&paragraph.children, text_style),
            Vec::new(),
            rail_style,
            bg,
            width,
            body_width,
        ),
        Node::Heading(heading) => push_markdown_spans(
            rows,
            markdown_inline_spans(&heading.children, text_style.fg(p.accent).bold()),
            Vec::new(),
            rail_style,
            bg,
            width,
            body_width,
        ),
        Node::Blockquote(quote) => {
            let quote_style = text_style.fg(p.secondary).italic();
            let content = quote.children.iter().map(Node::to_string).collect::<Vec<_>>().join(" ");
            push_markdown_spans(
                rows,
                vec![Span::styled(content, quote_style)],
                vec![Span::styled("│ ", CellStyle::new().fg(p.border).bg(bg))],
                rail_style,
                bg,
                width,
                body_width,
            );
        }
        Node::List(list) => render_markdown_list(rows, list, rail_style, text_style, bg, width, body_width, 0),
        Node::Code(code) => {
            let highlighted = super::highlight::highlight_lines(&code.value, code.lang.as_deref());
            let gutter_style = CellStyle::new().fg(p.border).bg(bg);
            let code_width = body_width.saturating_sub(utils::text_width(GUTTER));
            push_highlighted_code_rows(rows, highlighted, rail_style, gutter_style, bg, width, code_width);
        }
        Node::ThematicBreak(_) => {
            let rule_width = body_width.min(24);
            push_markdown_spans(
                rows,
                vec![Span::styled(
                    "─".repeat(rule_width),
                    CellStyle::new().fg(p.border).bg(bg),
                )],
                Vec::new(),
                rail_style,
                bg,
                width,
                body_width,
            );
        }
        _ if node.children().is_some() => {
            if let Some(children) = node.children() {
                render_markdown_blocks(rows, children, rail_style, text_style, bg, width, body_width);
            }
        }
        _ => {
            let text = node.to_string();
            if !text.is_empty() {
                push_markdown_spans(
                    rows,
                    vec![Span::styled(text, text_style)],
                    Vec::new(),
                    rail_style,
                    bg,
                    width,
                    body_width,
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_markdown_list(
    rows: &mut Vec<Row>, list: &markdown::mdast::List, rail_style: CellStyle, text_style: CellStyle, bg: Color,
    width: usize, body_width: usize, depth: usize,
) {
    let start = list.start.unwrap_or(1);
    for (index, child) in list.children.iter().enumerate() {
        let Node::ListItem(item) = child else { continue };
        let marker = match item.checked {
            Some(true) => "[x] ".to_string(),
            Some(false) => "[ ] ".to_string(),
            None if list.ordered => format!("{}. ", start + index as u32),
            None => "• ".to_string(),
        };
        let indent = "  ".repeat(depth);
        let prefix = vec![Span::styled(
            format!("{indent}{marker}"),
            text_style.fg(super::style::palette().accent),
        )];
        let mut rendered_primary = false;
        for item_child in &item.children {
            match item_child {
                Node::Paragraph(paragraph) if !rendered_primary => {
                    push_markdown_spans(
                        rows,
                        markdown_inline_spans(&paragraph.children, text_style),
                        prefix.clone(),
                        rail_style,
                        bg,
                        width,
                        body_width,
                    );
                    rendered_primary = true;
                }
                Node::List(nested) => {
                    render_markdown_list(rows, nested, rail_style, text_style, bg, width, body_width, depth + 1);
                }
                other => render_markdown_block(rows, other, rail_style, text_style, bg, width, body_width),
            }
        }
        if !rendered_primary {
            push_markdown_spans(rows, Vec::new(), prefix, rail_style, bg, width, body_width);
        }
    }
}

fn markdown_inline_spans(nodes: &[Node], style: CellStyle) -> Vec<Span> {
    let p = super::style::palette();
    let mut spans = Vec::new();
    for node in nodes {
        match node {
            Node::Text(text) => spans.push(Span::styled(text.value.clone(), style)),
            Node::Strong(strong) => spans.extend(markdown_inline_spans(&strong.children, style.bold())),
            Node::Emphasis(emphasis) => spans.extend(markdown_inline_spans(&emphasis.children, style.italic())),
            Node::Delete(delete) => {
                spans.extend(markdown_inline_spans(&delete.children, style.fg(p.secondary)));
            }
            Node::InlineCode(code) => spans.push(Span::styled(
                code.value.clone(),
                CellStyle::new().fg(p.accent).bg(p.surface_muted),
            )),
            Node::Link(link) => {
                spans.extend(markdown_inline_spans(&link.children, style.fg(p.link).underlined()));
            }
            Node::LinkReference(link) => {
                spans.extend(markdown_inline_spans(&link.children, style.fg(p.link).underlined()));
            }
            Node::Image(image) => spans.push(Span::styled(image.alt.clone(), style.fg(p.link).italic())),
            Node::ImageReference(image) => spans.push(Span::styled(image.alt.clone(), style.fg(p.link).italic())),
            Node::Break(_) => spans.push(Span::styled("\n", style)),
            Node::InlineMath(math) => spans.push(Span::styled(math.value.clone(), style.italic())),
            Node::Html(html) => spans.push(Span::styled(html.value.clone(), style)),
            _ if node.children().is_some() => {
                if let Some(children) = node.children() {
                    spans.extend(markdown_inline_spans(children, style));
                }
            }
            _ => {
                let text = node.to_string();
                if !text.is_empty() {
                    spans.push(Span::styled(text, style));
                }
            }
        }
    }
    spans
}

#[allow(clippy::needless_pass_by_value)]
fn push_markdown_spans(
    rows: &mut Vec<Row>, spans: Vec<Span>, prefix: Vec<Span>, rail_style: CellStyle, bg: Color, width: usize,
    body_width: usize,
) {
    let prefix_width = super::layout::spans_width(&prefix);
    let content_width = body_width.saturating_sub(prefix_width).max(1);
    let wrapped_rows = match spans.as_slice() {
        [span] if !span.text.contains('\n') => super::layout::wrap_text(&span.text, content_width)
            .into_iter()
            .map(|text| vec![Span::styled(text, span.style)])
            .collect(),
        _ => super::layout::wrap_spans(&spans, content_width),
    };
    for (index, mut wrapped) in wrapped_rows.into_iter().enumerate() {
        if index > 0 {
            while wrapped.first().is_some_and(|span| span.text.trim().is_empty()) {
                wrapped.remove(0);
            }
            if let Some(first) = wrapped.first_mut() {
                first.text = first.text.trim_start().to_string();
            }
        }
        let mut row_spans = vec![Span::styled(ENTRY_RAIL, rail_style)];
        if index == 0 {
            row_spans.extend(prefix.clone());
        } else if prefix_width > 0 {
            row_spans.push(Span::styled(" ".repeat(prefix_width), CellStyle::new().bg(bg)));
        }
        row_spans.extend(wrapped);
        rows.push(Row::padded(row_spans, width, CellStyle::new().bg(bg)));
    }
}

fn push_plain_markdown_fallback(
    rows: &mut Vec<Row>, source: &str, rail_style: CellStyle, text_style: CellStyle, bg: Color, width: usize,
    body_width: usize,
) {
    for line in super::layout::wrap_text(source, body_width) {
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
