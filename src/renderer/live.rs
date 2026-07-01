//! Live region row builders for the direct renderer.
//!
//! The live chrome is rebuilt each tick and composed into the full viewport by
//! [`super::region::LiveRegion`].

use crate::app::{App, Mode, PromptAccessory, PromptState, RunState};
use crate::renderer::cursor::{prompt_cursor, prompt_rows};
use crate::renderer::row::{CursorCoord, Row};
use crate::renderer::style as renderer_style;
use crate::renderer::style::{CellStyle, Color, Span};

/// Build the dynamic status row: session id + status icon + queue info.
///
/// Sits above the prompt input in the live region.
pub fn dynamic_status_row(app: &App, width: usize) -> Row {
    let p = renderer_style::palette();
    let bg = p.surface0;

    let label = app.status_label();
    let status_color = renderer_style::status_color(label);
    let icon = renderer_style::status_icon(label, app.ui_tick);
    let session = if app.session_id.is_empty() { "thndrs" } else { &app.session_id };

    let mut spans = vec![
        Span::styled(session.to_string(), CellStyle::new().fg(p.accent).bg(bg).bold()),
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
            CellStyle::new().fg(p.subtext0).bg(bg),
        ));
    }

    Row::padded(spans, width, bg_style(bg))
}

/// Build prompt input rows from app state.
///
/// Returns the rows and the cursor coordinate (relative to the first row).
pub fn prompt_rows_for(app: &App, width: usize) -> (Vec<Row>, Option<CursorCoord>) {
    let p = renderer_style::palette();
    let surface = p.surface0;
    let prompt_state = app.prompt_state();

    let (prompt_color, show_input, icon) = match prompt_state {
        PromptState::Editable => (p.yellow, true, "›"),
        PromptState::Submitted => (p.yellow, false, "·"),
        PromptState::Streaming | PromptState::RunningTool => (p.teal, true, "›"),
        PromptState::Stopped => (p.teal, true, "○"),
        PromptState::Errored => (p.red, true, "✕"),
    };

    let indent = prompt_prefix_width(app);
    let body_width = width.saturating_sub(indent).max(1);
    let input_text = app.input.as_str();
    let cursor_pos = app.input.cursor();

    if !show_input {
        let spans = vec![
            Span::styled(icon, CellStyle::new().fg(prompt_color).bg(surface)),
            Span::styled("  submitted", CellStyle::new().fg(p.overlay0).bg(surface)),
        ];
        return (
            vec![Row::padded(spans, width, bg_style(surface))],
            Some(CursorCoord::new(0, 0)),
        );
    }

    let visual_rows = prompt_rows(input_text, body_width);
    let cursor = prompt_cursor(input_text, cursor_pos, body_width, indent);

    let text_style = CellStyle::new().fg(p.text).bg(surface);
    let mention_style = CellStyle::new().fg(p.accent).bg(surface).bold();

    let mut rows = Vec::with_capacity(visual_rows.len());
    for (idx, line) in visual_rows.into_iter().enumerate() {
        let mut spans: Vec<Span> = if idx == 0 {
            let mut s = vec![
                Span::styled(icon, CellStyle::new().fg(prompt_color).bg(surface)),
                Span::styled("  ", CellStyle::new().bg(surface)),
            ];
            if app.mode == Mode::Command {
                s.push(Span::styled(":", CellStyle::new().fg(p.accent).bg(surface)));
            }
            s
        } else {
            vec![Span::styled(" ".repeat(indent), CellStyle::new().bg(surface))]
        };

        if !line.is_empty() {
            spans.extend(mention_styled_spans(&line, text_style, mention_style, surface));
        }
        rows.push(Row::padded(spans, width, bg_style(surface)));
    }

    if rows.is_empty() {
        let mut spans = vec![
            Span::styled(icon, CellStyle::new().fg(prompt_color).bg(surface)),
            Span::styled("  ", CellStyle::new().bg(surface)),
        ];
        if app.mode == Mode::Command {
            spans.push(Span::styled(":", CellStyle::new().fg(p.accent).bg(surface)));
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
        PromptAccessory::Files(_) => file_picker_rows(app, width, max_height),
    }
}

/// Build the static status row (model/search/tokens/cwd) below the prompt.
///
/// Width-aware clipping hides segments that don't fit.
pub fn static_status_row(app: &App, width: usize) -> Row {
    let p = renderer_style::palette();
    let bg = p.surface0;
    let subtext = CellStyle::new().fg(p.subtext0).bg(bg);
    let muted = CellStyle::new().fg(p.overlay0).bg(bg);
    let model_label = format!("model: {}", app.model);
    let search_label = app.websearch.label();
    let search_text = format!("search: {search_label}");
    let token_text = format!("tok: ↑{} ↓{}", app.session_tokens_in, app.session_tokens_out);
    let token_style = CellStyle::new().fg(p.peach).bg(bg);

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
        spans.push(Span::styled(token_text, token_style));
    }
    if show_cwd {
        spans.push(Span::styled("   ", CellStyle::new().bg(bg)));
        let used =
            model_len + if show_search { search_len + 3 } else { 0 } + if show_tokens { token_len + 3 } else { 0 } + 6;
        let cwd_display = crate::renderer::path_display::footer_segment(&app.cwd, width, used);
        spans.push(Span::styled(cwd_display, muted));
    }

    Row::padded(spans, width, bg_style(bg))
}

/// Build a [`CellStyle`] with only a background color (for padding/fill).
fn bg_style(color: Color) -> CellStyle {
    CellStyle::new().bg(color)
}

fn prompt_prefix_width(app: &App) -> usize {
    if app.mode == Mode::Command { 4 } else { 3 }
}

fn command_rows(app: &App, selected: usize, width: usize, max_height: usize) -> Vec<Row> {
    let p = renderer_style::palette();
    let bg = p.surface0;
    let commands = crate::app::command_suggestions_for_app(app);

    if commands.is_empty() {
        return vec![Row::padded(
            vec![Span::styled(
                "no matching commands",
                CellStyle::new().fg(p.overlay0).bg(bg),
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
            let row_bg = if is_selected { p.surface1 } else { bg };
            let marker = if is_selected { "›" } else { " " };
            let marker_style =
                if is_selected { CellStyle::new().fg(p.peach).bg(row_bg).bold() } else { CellStyle::new().bg(bg) };
            let cmd_style = if is_selected {
                CellStyle::new().fg(p.text).bg(row_bg).bold()
            } else {
                CellStyle::new().fg(p.subtext0).bg(bg)
            };
            let desc_style = CellStyle::new().fg(p.overlay0).bg(row_bg);
            let spans = vec![
                Span::styled(marker, marker_style),
                Span::styled(" ", CellStyle::new().bg(row_bg)),
                Span::styled(cmd.to_string(), cmd_style),
                Span::styled(format!("  {desc}"), desc_style),
            ];
            Row::padded(spans, width, bg_style(row_bg))
        })
        .collect()
}

/// Build styled spans for a prompt line, highlighting `@path` mentions.
///
/// Mention patterns are `@` followed by path-like characters (word chars,
/// `/`, `.`, `-`, `_`). The `@` and path are styled with `mention_style`;
/// all other text uses `text_style`.
fn mention_styled_spans(line: &str, text_style: CellStyle, mention_style: CellStyle, _bg: Color) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '@' {
            if !current.is_empty() {
                spans.push(Span::styled(std::mem::take(&mut current), text_style));
            }

            let mut mention = String::from('@');
            i += 1;
            while i < chars.len() && is_mention_char(chars[i]) {
                mention.push(chars[i]);
                i += 1;
            }

            if mention.len() > 1 {
                spans.push(Span::styled(mention, mention_style));
            } else {
                spans.push(Span::styled("@", text_style));
            }
        } else {
            current.push(chars[i]);
            i += 1;
        }
    }

    if !current.is_empty() {
        spans.push(Span::styled(current, text_style));
    }

    if spans.is_empty() {
        spans.push(Span::styled(line.to_string(), text_style));
    }

    spans
}

/// Whether a character is valid in a file mention path after `@`.
fn is_mention_char(ch: char) -> bool {
    ch.is_alphanumeric() || matches!(ch, '/' | '.' | '-' | '_' | '~')
}

/// Build file picker rows for the live region.
///
/// Renders: query header, match list with selection marker + fuzzy highlight
/// indices + long path clipping, "no matches" row, and footer hints.
fn file_picker_rows(app: &App, width: usize, max_height: usize) -> Vec<Row> {
    let p = renderer_style::palette();
    let bg = p.surface0;
    let surface1 = p.surface1;
    let label_style = CellStyle::new().fg(p.accent).bg(bg).bold();
    let muted_style = CellStyle::new().fg(p.overlay0).bg(bg);
    let text_style = CellStyle::new().fg(p.text).bg(bg);
    let highlight_style = CellStyle::new().fg(p.accent).bg(bg).bold();
    let selected_style = CellStyle::new().fg(p.text).bg(surface1).bold();
    let selected_marker_style = CellStyle::new().fg(p.peach).bg(surface1).bold();

    let Some(picker) = app.file_picker.as_ref() else {
        return vec![Row::padded(
            vec![Span::styled("files loading", muted_style)],
            width,
            bg_style(bg),
        )];
    };

    let mut rows = Vec::new();

    let query_display = if picker.query.is_empty() { "type to filter".to_string() } else { picker.query.clone() };
    rows.push(Row::padded(
        vec![
            Span::styled("files", label_style),
            Span::styled("  ", CellStyle::new().bg(bg)),
            Span::styled(query_display, muted_style),
        ],
        width,
        bg_style(bg),
    ));

    if picker.matches.is_empty() {
        rows.push(Row::padded(
            vec![Span::styled("no matches", muted_style)],
            width,
            bg_style(bg),
        ));
    } else {
        let visible_rows = picker.matches.len().clamp(1, crate::app::FILE_PICKER_VISIBLE_ROWS);
        let end = (picker.scroll + visible_rows).min(picker.matches.len());
        let available = width.saturating_sub(6); // marker + indent

        for (idx, path) in picker.matches[picker.scroll..end].iter().enumerate() {
            let absolute_idx = picker.scroll + idx;
            let is_selected = absolute_idx == picker.selected;
            let row_bg = if is_selected { surface1 } else { bg };
            let marker = if is_selected { "›" } else { " " };
            let marker_style = if is_selected { selected_marker_style } else { CellStyle::new().bg(bg) };

            let truncated = crate::utils::truncate_ellipsis(path, available);
            let indices = picker.match_indices.get(absolute_idx).cloned().unwrap_or_default();

            let path_spans = build_fuzzy_highlight_spans(
                &truncated,
                &indices,
                if is_selected { selected_style } else { text_style },
                highlight_style.with_bg(row_bg),
            );

            let mut spans = vec![
                Span::styled(marker, marker_style),
                Span::styled("  ", CellStyle::new().bg(row_bg)),
            ];
            spans.extend(path_spans);
            rows.push(Row::padded(spans, width, bg_style(row_bg)));
        }
    }

    rows.push(Row::padded(
        vec![
            Span::styled("Enter", label_style),
            Span::styled(" select   ", muted_style),
            Span::styled("Esc", label_style),
            Span::styled(" close", muted_style),
        ],
        width,
        bg_style(bg),
    ));

    rows.truncate(max_height.max(1));
    rows
}

/// Build styled spans for a path with fuzzy match indices highlighted.
///
/// Characters at the given indices are rendered with `highlight_style`; all
/// others use `base_style`. The indices refer to char positions in the
/// original path, but since we truncate with ellipsis, indices beyond the
/// truncated length are skipped.
fn build_fuzzy_highlight_spans(
    text: &str, indices: &[usize], base_style: CellStyle, highlight_style: CellStyle,
) -> Vec<Span> {
    if indices.is_empty() {
        return vec![Span::styled(text.to_string(), base_style)];
    }

    let chars: Vec<char> = text.chars().collect();
    let index_set: std::collections::HashSet<usize> = indices.iter().copied().collect();

    let mut spans = Vec::new();
    let mut current = String::new();
    let mut current_is_highlight = None;

    for (i, ch) in chars.iter().enumerate() {
        let is_highlight = index_set.contains(&i);
        match current_is_highlight {
            None => {
                current.push(*ch);
                current_is_highlight = Some(is_highlight);
            }
            Some(prev) if prev == is_highlight => {
                current.push(*ch);
            }
            Some(_) => {
                let style = if current_is_highlight.unwrap() { highlight_style } else { base_style };
                spans.push(Span::styled(std::mem::take(&mut current), style));
                current.push(*ch);
                current_is_highlight = Some(is_highlight);
            }
        }
    }

    if !current.is_empty() {
        let style = if current_is_highlight.unwrap_or(false) { highlight_style } else { base_style };
        spans.push(Span::styled(current, style));
    }

    spans
}

fn help_rows(width: usize, max_height: usize) -> Vec<Row> {
    let p = renderer_style::palette();
    let bg = p.surface0;
    let label_style = CellStyle::new().fg(p.accent).bg(bg).bold();
    let desc_style = CellStyle::new().fg(p.subtext0).bg(bg);
    let section_style = CellStyle::new().fg(p.overlay1).bg(bg).bold();

    let entries: &[(&str, &str)] = &[
        ("── Navigation ──", ""),
        ("Up/Down", "select item or recall history"),
        ("PageUp/PageDn", "scroll picker faster"),
        ("Enter", "accept highlighted item"),
        ("Escape", "close help, files, or commands"),
        ("── Editing ──", ""),
        ("Shift+Enter", "insert newline"),
        ("Ctrl+A/E", "move to start/end"),
        ("── Files ──", ""),
        ("Ctrl+P", "pick a file"),
        ("@path", "mention a file from fuzzy search"),
        ("── App ──", ""),
        ("Ctrl+D", "quit after double-press"),
    ];

    entries
        .iter()
        .take(max_height)
        .map(|&(key, desc)| {
            if desc.is_empty() {
                Row::padded(vec![Span::styled(key.to_string(), section_style)], width, bg_style(bg))
            } else {
                Row::padded(
                    vec![
                        Span::styled(format!("{key:<16}"), label_style),
                        Span::styled(desc.to_string(), desc_style),
                    ],
                    width,
                    bg_style(bg),
                )
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, Mode, RunState};
    use crate::cli::{Cli, Theme, WebSearchMode};
    use crate::renderer::layout::truncate_spans;
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

        let rows = accessory_rows(&app, 80, 16);
        assert!(!rows.is_empty(), "help should produce rows");
        let combined: String = rows.iter().map(|r| r.text()).collect();
        assert!(combined.contains("Navigation"), "help should have Navigation section");
        assert!(combined.contains("Enter"), "help should include Enter key");
        assert!(combined.contains("Escape"), "help should include Escape key");
    }

    #[test]
    fn truncate_row_helper_works() {
        let spans = vec![Span::plain("hello world")];
        let out = truncate_spans(&spans, 5, CellStyle::default());
        assert_eq!(out.iter().map(|s| s.text.chars().count()).sum::<usize>(), 5);
    }

    fn picker_app(files: Vec<String>) -> App {
        let mut app = test_app();
        app.file_picker = Some(crate::app::FilePickerState {
            query: String::new(),
            all_files: files.clone(),
            matches: files,
            match_indices: Vec::new(),
            selected: 0,
            scroll: 0,
        });
        app.prompt_accessory = PromptAccessory::Files(crate::app::FilePickerSource::Forced);
        app
    }

    #[test]
    fn snapshot_file_picker_empty_query() {
        let app = picker_app(vec![
            "src/main.rs".to_string(),
            "src/lib.rs".to_string(),
            "Cargo.toml".to_string(),
        ]);
        let rows = accessory_rows(&app, 80, 12);
        let frame = crate::renderer::row::Frame { rows, width: 80, cursor: None };
        insta::assert_snapshot!("file_picker_empty_query", frame.render_styled());
    }

    #[test]
    fn snapshot_file_picker_filtered_results() {
        let mut app = picker_app(vec![
            "src/main.rs".to_string(),
            "src/lib.rs".to_string(),
            "Cargo.toml".to_string(),
        ]);
        if let Some(picker) = app.file_picker.as_mut() {
            picker.query = "main".to_string();
            picker.matches = vec!["src/main.rs".to_string()];
            picker.match_indices = vec![vec![4, 5, 6, 7]];
        }
        let rows = accessory_rows(&app, 80, 12);
        let frame = crate::renderer::row::Frame { rows, width: 80, cursor: None };
        insta::assert_snapshot!("file_picker_filtered", frame.render_styled());
    }

    #[test]
    fn snapshot_file_picker_no_matches() {
        let mut app = picker_app(vec!["src/main.rs".to_string()]);
        if let Some(picker) = app.file_picker.as_mut() {
            picker.query = "xyz".to_string();
            picker.matches = Vec::new();
            picker.match_indices = Vec::new();
        }
        let rows = accessory_rows(&app, 80, 12);
        let frame = crate::renderer::row::Frame { rows, width: 80, cursor: None };
        insta::assert_snapshot!("file_picker_no_matches", frame.render_styled());
    }

    #[test]
    fn snapshot_file_picker_long_path_clipping() {
        let app = picker_app(vec!["src/very/deeply/nested/path/to/some/module/file.rs".to_string()]);
        let rows = accessory_rows(&app, 30, 12);
        let frame = crate::renderer::row::Frame { rows, width: 30, cursor: None };
        insta::assert_snapshot!("file_picker_long_path", frame.render_styled());
    }

    #[test]
    fn snapshot_file_picker_scrolled_selection() {
        let files: Vec<String> = (0..15).map(|i| format!("src/file_{i:02}.rs")).collect();
        let mut app = picker_app(files);
        if let Some(picker) = app.file_picker.as_mut() {
            picker.selected = 5;
            picker.scroll = 3;
        }
        let rows = accessory_rows(&app, 80, 12);
        let frame = crate::renderer::row::Frame { rows, width: 80, cursor: None };
        insta::assert_snapshot!("file_picker_scrolled", frame.render_styled());
    }

    #[test]
    fn snapshot_mention_styling_in_prompt() {
        let mut app = test_app();
        app.input.set_text("check @src/main.rs for details");
        let (rows, _) = prompt_rows_for(&app, 80);
        let frame = crate::renderer::row::Frame { rows, width: 80, cursor: None };
        insta::assert_snapshot!("mention_styling", frame.render_styled());
    }

    #[test]
    fn snapshot_help_rows() {
        let mut app = test_app();
        app.prompt_accessory = PromptAccessory::Help;
        let rows = accessory_rows(&app, 80, 16);
        let frame = crate::renderer::row::Frame { rows, width: 80, cursor: None };
        insta::assert_snapshot!("help_rows", frame.render_styled());
    }

    #[test]
    fn snapshot_command_suggestions() {
        let mut app = test_app();
        app.input.set_text("/c");
        app.mode = Mode::Command;
        app.prompt_accessory = PromptAccessory::Commands { selected: 0 };
        let rows = accessory_rows(&app, 80, 8);
        let frame = crate::renderer::row::Frame { rows, width: 80, cursor: None };
        insta::assert_snapshot!("command_suggestions", frame.render_styled());
    }
}
