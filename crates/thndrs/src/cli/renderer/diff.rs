//! Unified-diff projection and transcript row rendering.

use crate::renderer::row::Row;
use crate::renderer::style::{CellStyle, Color, Span};
use crate::utils;

use super::transcript::ACTIVITY_RAIL;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiffLineKind {
    Context,
    Addition,
    Deletion,
    Marker,
}

/// One source line with coordinates on both sides of a hunk.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub old_line: Option<usize>,
    pub new_line: Option<usize>,
    pub content: String,
}

/// A validated unified-diff hunk.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffHunk {
    pub header: String,
    pub lines: Vec<DiffLine>,
}

/// A file-level diff and its display metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffFile {
    pub old_path: Option<String>,
    pub new_path: Option<String>,
    pub hunks: Vec<DiffHunk>,
    pub binary: bool,
}

impl DiffFile {
    fn path(&self) -> &str {
        self.new_path
            .as_deref()
            .filter(|path| *path != "/dev/null")
            .or(self.old_path.as_deref())
            .unwrap_or("unknown file")
    }

    fn counts(&self) -> (usize, usize) {
        self.hunks
            .iter()
            .flat_map(|hunk| &hunk.lines)
            .fold((0, 0), |counts, line| match line.kind {
                DiffLineKind::Addition => (counts.0 + 1, counts.1),
                DiffLineKind::Deletion => (counts.0, counts.1 + 1),
                DiffLineKind::Context | DiffLineKind::Marker => counts,
            })
    }
}

/// A confidently parsed unified diff.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnifiedDiff {
    pub files: Vec<DiffFile>,
}

impl UnifiedDiff {
    pub fn parse(input: &[String]) -> Option<Self> {
        let mut files = Vec::new();
        let mut current: Option<DiffFile> = None;
        let mut index = 0;

        while index < input.len() {
            let line = &input[index];
            if let Some(paths) = line.strip_prefix("diff --git ") {
                push_file(&mut files, current.take());
                let (old_path, new_path) = parse_git_paths(paths)?;
                current = Some(DiffFile {
                    old_path: Some(old_path),
                    new_path: Some(new_path),
                    hunks: Vec::new(),
                    binary: false,
                });
                index += 1;
                continue;
            }
            if let Some(path) = line.strip_prefix("--- ") {
                if current
                    .as_ref()
                    .is_some_and(|file| !file.hunks.is_empty() || file.binary)
                {
                    push_file(&mut files, current.take());
                }
                current.get_or_insert_with(empty_file).old_path = Some(parse_header_path(path));
                index += 1;
                continue;
            }
            if let Some(path) = line.strip_prefix("+++ ") {
                current.get_or_insert_with(empty_file).new_path = Some(parse_header_path(path));
                index += 1;
                continue;
            }
            if line.starts_with("Binary files ") || line == "GIT binary patch" {
                let file = current.get_or_insert_with(empty_file);
                if let Some((old_path, new_path)) = parse_binary_paths(line) {
                    file.old_path.get_or_insert(old_path);
                    file.new_path.get_or_insert(new_path);
                }
                file.binary = true;
                index += 1;
                continue;
            }
            if line.starts_with("@@ ") {
                let file = current.as_mut()?;
                if file.old_path.is_none() && file.new_path.is_none() {
                    return None;
                }
                let ((old_start, old_count), (new_start, new_count)) = parse_hunk_header(line)?;
                let old_line = old_start;
                let new_line = new_start;
                let mut old_seen = 0;
                let mut new_seen = 0;
                let mut lines = Vec::new();
                index += 1;

                while index < input.len() && (old_seen < old_count || new_seen < new_count) {
                    let source = &input[index];
                    let projected = match source.as_bytes().first().copied() {
                        Some(b' ') => {
                            let line = DiffLine {
                                kind: DiffLineKind::Context,
                                old_line: Some(old_line.checked_add(old_seen)?),
                                new_line: Some(new_line.checked_add(new_seen)?),
                                content: source[1..].to_string(),
                            };
                            old_seen += 1;
                            new_seen += 1;
                            line
                        }
                        Some(b'-') => {
                            let line = DiffLine {
                                kind: DiffLineKind::Deletion,
                                old_line: Some(old_line.checked_add(old_seen)?),
                                new_line: None,
                                content: source[1..].to_string(),
                            };
                            old_seen += 1;
                            line
                        }
                        Some(b'+') => {
                            let line = DiffLine {
                                kind: DiffLineKind::Addition,
                                old_line: None,
                                new_line: Some(new_line.checked_add(new_seen)?),
                                content: source[1..].to_string(),
                            };
                            new_seen += 1;
                            line
                        }
                        Some(b'\\') if source == "\\ No newline at end of file" => DiffLine {
                            kind: DiffLineKind::Marker,
                            old_line: None,
                            new_line: None,
                            content: source.to_string(),
                        },
                        _ => return None,
                    };
                    lines.push(projected);
                    index += 1;
                }
                if old_seen != old_count || new_seen != new_count {
                    return None;
                }
                while let Some(source) = input
                    .get(index)
                    .filter(|line| line.as_str() == "\\ No newline at end of file")
                {
                    lines.push(DiffLine {
                        kind: DiffLineKind::Marker,
                        old_line: None,
                        new_line: None,
                        content: source.clone(),
                    });
                    index += 1;
                }
                file.hunks.push(DiffHunk { header: line.clone(), lines });
                continue;
            }
            if is_diff_metadata(line) {
                index += 1;
                continue;
            }
            return None;
        }
        push_file(&mut files, current);

        (!files.is_empty() && files.iter().all(|file| file.binary || !file.hunks.is_empty())).then_some(Self { files })
    }

    pub(crate) fn summary(&self) -> (Vec<String>, usize, usize) {
        let mut files = self
            .files
            .iter()
            .map(|file| file.path().to_string())
            .collect::<Vec<_>>();
        files.sort();
        files.dedup();
        let (added, removed) = self.files.iter().fold((0, 0), |total, file| {
            let counts = file.counts();
            (total.0 + counts.0, total.1 + counts.1)
        });
        (files, added, removed)
    }
}

fn empty_file() -> DiffFile {
    DiffFile { old_path: None, new_path: None, hunks: Vec::new(), binary: false }
}

fn push_file(files: &mut Vec<DiffFile>, file: Option<DiffFile>) {
    if let Some(file) = file
        && (file.binary || !file.hunks.is_empty())
    {
        files.push(file);
    }
}

fn parse_git_paths(paths: &str) -> Option<(String, String)> {
    let (old, rest) = parse_path_token(paths)?;
    let (new, rest) = parse_path_token(rest.trim_start())?;
    rest.trim()
        .is_empty()
        .then_some((strip_side_prefix(&old, "a/"), strip_side_prefix(&new, "b/")))
}

fn parse_header_path(path: &str) -> String {
    let path = parse_path_token(path)
        .map(|(path, _)| path)
        .unwrap_or_else(|| path.split('\t').next().unwrap_or(path).to_string());
    let prefix = if path.starts_with("a/") { "a/" } else { "b/" };
    strip_side_prefix(&path, prefix)
}

fn strip_side_prefix(path: &str, prefix: &str) -> String {
    path.strip_prefix(prefix).unwrap_or(path).to_string()
}

fn parse_path_token(input: &str) -> Option<(String, &str)> {
    if !input.starts_with('"') {
        let end = input.find(['\t', ' ']).unwrap_or(input.len());
        return (end > 0).then(|| (input[..end].to_string(), &input[end..]));
    }

    let mut path = String::new();
    let mut chars = input[1..].char_indices();
    while let Some((offset, ch)) = chars.next() {
        match ch {
            '"' => return Some((path, &input[offset + 2..])),
            '\\' => {
                let (_, escaped) = chars.next()?;
                path.push(match escaped {
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    other => other,
                });
            }
            other => path.push(other),
        }
    }
    None
}

fn parse_binary_paths(line: &str) -> Option<(String, String)> {
    let paths = line.strip_prefix("Binary files ")?.strip_suffix(" differ")?;
    let (old, rest) = parse_path_token(paths)?;
    let rest = rest.trim_start().strip_prefix("and ")?;
    let (new, rest) = parse_path_token(rest)?;
    rest.trim()
        .is_empty()
        .then_some((strip_side_prefix(&old, "a/"), strip_side_prefix(&new, "b/")))
}

fn is_diff_metadata(line: &str) -> bool {
    [
        "index ",
        "old mode ",
        "new mode ",
        "new file mode ",
        "deleted file mode ",
        "similarity index ",
        "dissimilarity index ",
        "rename from ",
        "rename to ",
        "copy from ",
        "copy to ",
    ]
    .iter()
    .any(|prefix| line.starts_with(prefix))
}

fn parse_hunk_header(header: &str) -> Option<((usize, usize), (usize, usize))> {
    let mut fields = header.split_whitespace();
    (fields.next()? == "@@").then_some(())?;
    let old = parse_range(fields.next()?.strip_prefix('-')?)?;
    let new = parse_range(fields.next()?.strip_prefix('+')?)?;
    (fields.next()? == "@@").then_some((old, new))
}

fn parse_range(range: &str) -> Option<(usize, usize)> {
    let (start, count) = range.split_once(',').unwrap_or((range, "1"));
    Some((start.parse().ok()?, count.parse().ok()?))
}

/// Render semantic diff rows inside an activity block.
pub fn rows(
    diff: &UnifiedDiff, width: usize, body_width: usize, bg: Color, max_source_lines: Option<usize>,
) -> Vec<Row> {
    let p = super::style::palette();
    let rail_style = CellStyle::new().fg(p.warning).bg(bg);
    let muted_style = CellStyle::new().fg(p.border).bg(bg);
    let mut rows = Vec::new();
    let mut remaining = max_source_lines.unwrap_or(usize::MAX);
    let total_source_lines = diff
        .files
        .iter()
        .flat_map(|file| &file.hunks)
        .map(|hunk| hunk.lines.len())
        .sum::<usize>();
    let mut rendered_source_lines = 0;

    for file in &diff.files {
        if remaining == 0 && !file.hunks.is_empty() {
            break;
        }
        let (additions, deletions) = file.counts();
        push_clipped_row(
            &mut rows,
            &[
                Span::styled(ACTIVITY_RAIL, rail_style),
                Span::styled("   ", CellStyle::new().bg(bg)),
                Span::styled(file.path().to_string(), CellStyle::new().fg(p.primary).bg(bg).bold()),
                Span::styled(
                    format!("  +{additions} −{deletions}"),
                    CellStyle::new().fg(p.secondary).bg(bg),
                ),
            ],
            width,
            bg,
        );

        if file.binary {
            push_clipped_row(
                &mut rows,
                &[
                    Span::styled(ACTIVITY_RAIL, rail_style),
                    Span::styled("   Binary files differ", CellStyle::new().fg(p.warning).bg(bg)),
                ],
                width,
                bg,
            );
        }

        let language = super::highlight::path_extension_language(file.path());
        let number_width = file
            .hunks
            .iter()
            .flat_map(|hunk| &hunk.lines)
            .flat_map(|line| [line.old_line, line.new_line])
            .flatten()
            .max()
            .map_or(1, |number| number.to_string().len());

        for hunk in &file.hunks {
            if remaining == 0 {
                break;
            }
            push_clipped_row(
                &mut rows,
                &[
                    Span::styled(ACTIVITY_RAIL, rail_style),
                    Span::styled(format!("   {}", hunk.header), CellStyle::new().fg(p.link).bg(bg)),
                ],
                width,
                bg,
            );
            let visible_lines = hunk.lines.iter().take(remaining).collect::<Vec<_>>();
            remaining = remaining.saturating_sub(visible_lines.len());
            rendered_source_lines += visible_lines.len();
            let source = visible_lines
                .iter()
                .map(|line| line.content.as_str())
                .collect::<Vec<_>>()
                .join("\n")
                + "\n";
            let highlighted = super::highlight::highlight_lines(&source, language);
            for (line, syntax) in visible_lines.into_iter().zip(highlighted) {
                render_source_line(&mut rows, line, syntax, number_width, width, body_width, bg);
            }
        }
    }

    if rendered_source_lines < total_source_lines {
        push_clipped_row(
            &mut rows,
            &[
                Span::styled(ACTIVITY_RAIL, rail_style),
                Span::styled("   … diff preview · Ctrl+O details", muted_style),
            ],
            width,
            bg,
        );
    }

    if rows.is_empty() {
        rows.push(Row::padded(
            vec![
                Span::styled(ACTIVITY_RAIL, rail_style),
                Span::styled("   Unsupported diff", muted_style),
            ],
            width,
            CellStyle::new().bg(bg),
        ));
    }
    rows
}

fn push_clipped_row(rows: &mut Vec<Row>, spans: &[Span], width: usize, bg: Color) {
    let style = CellStyle::new().fg(super::style::palette().border).bg(bg);
    let content_width = padded_content_width(width);
    rows.push(Row::padded(
        super::layout::truncate_spans(spans, content_width, style),
        width,
        CellStyle::new().bg(bg),
    ));
}

fn padded_content_width(width: usize) -> usize {
    let left_padding = width.min(2);
    let right_padding = width.saturating_sub(left_padding).min(2);
    width.saturating_sub(left_padding + right_padding)
}

fn render_source_line(
    rows: &mut Vec<Row>, line: &DiffLine, syntax: Vec<Span>, number_width: usize, width: usize, body_width: usize,
    bg: Color,
) {
    let p = super::style::palette();
    let (marker, source_color, source_bg) = match line.kind {
        DiffLineKind::Context => (' ', p.border, bg),
        DiffLineKind::Addition => ('+', p.success, p.surface),
        DiffLineKind::Deletion => ('-', p.failure, p.surface),
        DiffLineKind::Marker => (' ', p.warning, bg),
    };
    let old = line.old_line.map_or_else(String::new, |number| number.to_string());
    let new = line.new_line.map_or_else(String::new, |number| number.to_string());
    let full_gutter = format!("{marker} {old:>number_width$} {new:>number_width$} │ ");
    let compact = body_width < utils::text_width(ACTIVITY_RAIL) + utils::text_width(&full_gutter) + 1;
    let gutter = if compact { format!("{marker}│") } else { full_gutter };
    let content_width = body_width
        .saturating_sub(utils::text_width(ACTIVITY_RAIL))
        .saturating_sub(utils::text_width(&gutter))
        .max(1);
    let source_style = CellStyle::new().fg(source_color).bg(source_bg);
    let syntax = syntax
        .into_iter()
        .map(|span| {
            let mut style = span.style.with_bg(source_bg);
            if style.fg == Color::Reset {
                style.fg = source_color;
            }
            Span { text: span.text, style }
        })
        .collect::<Vec<_>>();

    for (index, wrapped) in super::layout::wrap_spans(&syntax, content_width)
        .into_iter()
        .enumerate()
    {
        let line_gutter = if index == 0 { gutter.clone() } else { "↪ ".to_string() };
        let mut spans = vec![
            Span::styled(ACTIVITY_RAIL, CellStyle::new().fg(p.warning).bg(bg)),
            Span::styled(line_gutter, source_style),
        ];
        spans.extend(if wrapped.is_empty() { vec![Span::styled("", source_style)] } else { wrapped });
        rows.push(Row::padded(
            super::layout::truncate_spans(&spans, padded_content_width(width), source_style),
            width,
            CellStyle::new().bg(bg),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<String> {
        [
            "diff --git a/src/lib.rs b/src/lib.rs",
            "--- a/src/lib.rs",
            "+++ b/src/lib.rs",
            "@@ -41,2 +41,3 @@ fn render()",
            " let state = 1;",
            "-old_call();",
            "+new_call();",
            "+another_call();",
            "\\ No newline at end of file",
        ]
        .into_iter()
        .map(str::to_string)
        .collect()
    }

    #[test]
    fn parses_files_hunks_kinds_and_coordinates() {
        let diff = UnifiedDiff::parse(&sample()).expect("valid diff");
        let file = &diff.files[0];
        assert_eq!(file.path(), "src/lib.rs");
        assert_eq!(file.counts(), (2, 1));
        assert_eq!(file.hunks[0].lines[0].old_line, Some(41));
        assert_eq!(file.hunks[0].lines[1].new_line, None);
        assert_eq!(file.hunks[0].lines[2].new_line, Some(42));
        assert_eq!(file.hunks[0].lines[4].kind, DiffLineKind::Marker);
    }

    #[test]
    fn rejects_arbitrary_output_and_incomplete_hunks() {
        assert!(UnifiedDiff::parse(&["cargo test".to_string()]).is_none());
        assert!(UnifiedDiff::parse(&sample()[..6]).is_none());
    }

    #[test]
    fn renders_semantic_gutters_and_layers_syntax_over_change_background() {
        let diff = UnifiedDiff::parse(&sample()).expect("valid diff");
        let rendered = rows(&diff, 80, 76, Color::Reset, None);
        let text = rendered.iter().map(Row::text).collect::<Vec<_>>().join("\n");
        assert!(text.contains("src/lib.rs  +2 −1"));
        assert!(text.contains("- 42    │ old_call();"));
        assert!(text.contains("+    42 │ new_call();"));
        assert!(
            rendered.iter().flat_map(|row| &row.spans).any(|span| {
                span.text.contains("new_call") && span.style.bg == super::super::style::palette().surface
            })
        );
    }

    #[test]
    fn wrapped_change_fragments_keep_their_semantic_color() {
        let p = super::super::style::palette();
        for (kind, color) in [(DiffLineKind::Addition, p.success), (DiffLineKind::Deletion, p.failure)] {
            let mut rendered = Vec::new();
            let line = DiffLine { kind, old_line: Some(1), new_line: Some(1), content: "abcdefghijk".to_string() };
            render_source_line(
                &mut rendered,
                &line,
                vec![Span::plain(line.content.clone())],
                1,
                12,
                8,
                Color::Reset,
            );

            let fragments = rendered
                .iter()
                .flat_map(|row| &row.spans)
                .filter(|span| span.text.chars().all(char::is_alphabetic))
                .collect::<Vec<_>>();
            assert!(fragments.len() > 1, "source should wrap");
            assert!(fragments.iter().all(|span| span.style.fg == color));
        }
    }

    #[test]
    fn narrow_rendering_keeps_change_rails_and_wraps() {
        let diff = UnifiedDiff::parse(&sample()).expect("valid diff");
        let rendered = rows(&diff, 20, 10, Color::Reset, None);
        assert!(
            rendered.iter().all(|row| row.text_width() == 20),
            "unexpected row widths: {:?}",
            rendered
                .iter()
                .map(|row| (row.text_width(), row.text()))
                .collect::<Vec<_>>()
        );
        assert!(rendered.iter().any(|row| row.text().contains("↪")));
        assert!(rendered.iter().any(|row| row.text().contains("+│")));
    }

    #[test]
    fn parses_binary_file_markers() {
        let input = [
            "diff --git a/image.bin b/image.bin".to_string(),
            "Binary files a/image.bin and b/image.bin differ".to_string(),
        ];
        let diff = UnifiedDiff::parse(&input).expect("binary diff");
        assert!(diff.files[0].binary);
    }

    #[test]
    fn rejects_mixed_output_and_unknown_markers() {
        let mut mixed = vec!["warning: partial output".to_string()];
        mixed.extend(sample());
        assert!(UnifiedDiff::parse(&mixed).is_none());

        let mut marker = sample();
        *marker.last_mut().expect("marker") = "\\ unexpected marker".to_string();
        assert!(UnifiedDiff::parse(&marker).is_none());
    }

    #[test]
    fn rejects_hunk_coordinates_that_overflow() {
        let maximum = usize::MAX;
        let one_line = [
            "--- a/file".to_string(),
            "+++ b/file".to_string(),
            format!("@@ -{maximum} +{maximum} @@"),
            " line".to_string(),
        ];
        assert!(UnifiedDiff::parse(&one_line).is_some());

        let overflow = [
            "--- a/file".to_string(),
            "+++ b/file".to_string(),
            format!("@@ -{maximum},2 +{maximum},2 @@"),
            " first".to_string(),
            " second".to_string(),
        ];
        assert!(UnifiedDiff::parse(&overflow).is_none());
    }

    #[test]
    fn parses_quoted_and_binary_paths_with_spaces() {
        let quoted = [
            "diff --git \"a/a file.rs\" \"b/a file.rs\"".to_string(),
            "--- \"a/a file.rs\"".to_string(),
            "+++ \"b/a file.rs\"".to_string(),
            "@@ -1 +1 @@".to_string(),
            "-old".to_string(),
            "+new".to_string(),
        ];
        assert_eq!(
            UnifiedDiff::parse(&quoted).expect("quoted diff").files[0].path(),
            "a file.rs"
        );

        let binary = ["Binary files \"a/an image.bin\" and \"b/an image.bin\" differ".to_string()];
        assert_eq!(
            UnifiedDiff::parse(&binary).expect("binary diff").files[0].path(),
            "an image.bin"
        );
    }

    #[test]
    fn bounds_preview_before_highlighting_and_handles_tiny_widths() {
        let diff = UnifiedDiff::parse(&sample()).expect("valid diff");
        let preview = rows(&diff, 30, 26, Color::Reset, Some(2));
        let text = preview.iter().map(Row::text).collect::<Vec<_>>().join("\n");
        assert!(text.contains("diff preview"));
        assert!(!text.contains("new_call"));

        for width in 0..=2 {
            assert!(
                rows(&diff, width, width, Color::Reset, None)
                    .iter()
                    .all(|row| row.text_width() == width)
            );
        }
    }
}
