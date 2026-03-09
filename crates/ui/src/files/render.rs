use super::FileBrowserApp;
use crate::colors;
use crate::components::{HintFooter, HintToken, TopBorderedInputRow};
use crate::layout::ConstraintSpec;
use crate::layout::split as split_rects;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

pub fn draw_file_browser_screen(frame: &mut Frame, app: &FileBrowserApp) {
    let size = frame.area();
    let clear = Block::default().style(Style::default().bg(colors::BG_TERMINAL));
    frame.render_widget(clear, size);

    let shell = crate::files::FileBrowserShell.split(size);
    if shell.len() < 3 {
        return;
    }

    draw_file_browser_main(frame, shell[0], app);
    draw_file_browser_hints(frame, shell[1]);
    draw_file_browser_status(frame, shell[2], app);

    if app.finder.active {
        draw_fuzzy_overlay(frame, size, app);
    }
}

fn draw_file_browser_main(frame: &mut Frame, area: Rect, app: &FileBrowserApp) {
    let sidebar_width = area.width.clamp(24, 38);
    let layout = split_rects(
        area,
        Direction::Horizontal,
        vec![Constraint::Length(sidebar_width), Constraint::Min(0)],
    );

    if layout.len() < 2 {
        return;
    }

    draw_tree_pane(frame, layout[0], app);
    draw_content_pane(frame, layout[1], app);
}

fn draw_tree_pane(frame: &mut Frame, area: Rect, app: &FileBrowserApp) {
    let tree_title = format!(" {} ", app.workspace_root.display());
    let block = Block::default()
        .title(tree_title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(colors::BORDER_COLOR))
        .style(Style::default().bg(colors::BG_TERMINAL));
    frame.render_widget(block.clone(), area);

    let inner = block.inner(area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let lines = app
        .visible_entries
        .iter()
        .enumerate()
        .map(|(idx, entry)| {
            let indent = "  ".repeat(entry.depth as usize);
            let icon = if entry.is_dir { if entry.expanded { "v" } else { ">" } } else { "-" };

            let mut name_style = Style::default().fg(colors::TEXT_SECONDARY);
            if app.active_file.as_deref() == Some(entry.path.as_path()) {
                name_style = name_style.fg(colors::ACCENT_CYAN).add_modifier(Modifier::BOLD);
            }

            let mut row_style = Style::default().bg(colors::BG_TERMINAL);
            if idx == app.selected_index {
                row_style = row_style.fg(colors::TEXT_PRIMARY).add_modifier(Modifier::BOLD);
            }

            Line::from(vec![
                Span::styled(indent, row_style),
                Span::styled(
                    icon,
                    row_style.fg(if entry.is_dir { colors::ACCENT_YELLOW } else { colors::TEXT_MUTED }),
                ),
                Span::styled(" ", row_style),
                Span::styled(entry.name.clone(), name_style.patch(row_style)),
            ])
        })
        .collect::<Vec<_>>();

    let tree_text = Text::from(lines);
    let paragraph = Paragraph::new(tree_text)
        .style(Style::default().bg(colors::BG_TERMINAL))
        .scroll((app.tree_scroll.offset.min(u16::MAX as usize) as u16, 0));
    frame.render_widget(paragraph, inner);
}

fn draw_content_pane(frame: &mut Frame, area: Rect, app: &FileBrowserApp) {
    let layout = split_rects(
        area,
        Direction::Vertical,
        vec![Constraint::Length(1), Constraint::Min(0)],
    );
    if layout.len() < 2 {
        return;
    }

    let breadcrumb = build_breadcrumb(app);
    let crumb_paragraph = Paragraph::new(Line::from(vec![
        Span::styled(" ", Style::default()),
        Span::styled(breadcrumb, Style::default().fg(colors::ACCENT_CYAN)),
    ]))
    .style(Style::default().bg(colors::BG_TERMINAL));
    frame.render_widget(crumb_paragraph, layout[0]);

    let content_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(colors::BORDER_COLOR))
        .style(Style::default().bg(colors::BG_TERMINAL));
    frame.render_widget(content_block.clone(), layout[1]);
    let inner = content_block.inner(layout[1]);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    if app.highlighted_lines.is_empty() {
        let placeholder = Paragraph::new("Select a file from the tree or open fuzzy finder with @")
            .style(Style::default().fg(colors::TEXT_MUTED).bg(colors::BG_TERMINAL))
            .wrap(Wrap { trim: true });
        frame.render_widget(placeholder, inner);
        return;
    }

    let line_width = app.highlighted_lines.len().to_string().len().max(3);
    let lines = app
        .highlighted_lines
        .iter()
        .map(|line| {
            let mut spans = Vec::with_capacity(line.segments.len() + 2);
            spans.push(Span::styled(
                format!("{:>line_width$} ", line.line_number, line_width = line_width),
                Style::default().fg(colors::TEXT_MUTED),
            ));

            for segment in &line.segments {
                let mut style = Style::default().fg(segment.fg);
                if segment.bold {
                    style = style.add_modifier(Modifier::BOLD);
                }
                if segment.italic {
                    style = style.add_modifier(Modifier::ITALIC);
                }
                spans.push(Span::styled(segment.text.clone(), style));
            }

            if line.segments.is_empty() {
                spans.push(Span::raw(" "));
            }

            Line::from(spans)
        })
        .collect::<Vec<_>>();

    let paragraph = Paragraph::new(Text::from(lines))
        .style(Style::default().bg(colors::BG_TERMINAL))
        .scroll((app.content_scroll.offset.min(u16::MAX as usize) as u16, 0))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, inner);
}

fn draw_file_browser_hints(frame: &mut Frame, area: Rect) {
    let tokens = [
        HintToken::Text("Press "),
        HintToken::Key("@"),
        HintToken::Text(" for finder, "),
        HintToken::Key("Enter"),
        HintToken::Text(" to open/toggle, "),
        HintToken::Key("Esc"),
        HintToken::Text(" to return to chat"),
    ];
    HintFooter.render(frame, area, &tokens);
}

fn draw_file_browser_status(frame: &mut Frame, area: Rect, app: &FileBrowserApp) {
    TopBorderedInputRow.render(frame, area, &app.status_line, false);
}

fn draw_fuzzy_overlay(frame: &mut Frame, area: Rect, app: &FileBrowserApp) {
    let rows = split_rects(
        area,
        Direction::Vertical,
        vec![Constraint::Fill(1), Constraint::Length(12), Constraint::Fill(1)],
    );
    if rows.len() < 2 {
        return;
    }

    let cols = split_rects(
        rows[1],
        Direction::Horizontal,
        vec![
            Constraint::Fill(1),
            Constraint::Length(72.min(area.width.saturating_sub(2))),
            Constraint::Fill(1),
        ],
    );
    if cols.len() < 2 {
        return;
    }

    let panel = cols[1];
    frame.render_widget(Clear, panel);
    let overlay = Block::default()
        .title(" open file ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(colors::ACCENT_CYAN))
        .style(Style::default().bg(colors::BG_TERMINAL));
    frame.render_widget(overlay.clone(), panel);
    let inner = overlay.inner(panel);
    frame.render_widget(Block::default().style(Style::default().bg(colors::BG_TERMINAL)), inner);

    let mut rows = Vec::with_capacity(app.finder.filtered_len() + 2);
    rows.push(Constraint::Length(1));
    rows.extend((0..app.finder.filtered_len()).map(|_| Constraint::Length(1)));
    rows.push(Constraint::Min(0));

    let layout = split_rects(inner, Direction::Vertical, rows);
    if layout.is_empty() {
        return;
    }

    let input = Paragraph::new(Line::from(vec![
        Span::styled(
            "@",
            Style::default().fg(colors::ACCENT_CYAN).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            &app.finder.query,
            Style::default().fg(colors::TEXT_PRIMARY).add_modifier(Modifier::BOLD),
        ),
    ]))
    .style(Style::default().bg(colors::BG_TERMINAL));
    frame.render_widget(input, layout[0]);

    for (idx, path) in app.finder.filtered_items().enumerate() {
        if let Some(slot) = layout.get(idx + 1).copied() {
            let selected = idx == app.finder.selected;
            let row_style = if selected {
                Style::default()
                    .bg(colors::BG_TERMINAL)
                    .fg(colors::TEXT_PRIMARY)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().bg(colors::BG_TERMINAL)
            };

            let line = Line::from(vec![
                Span::styled(if selected { "> " } else { "  " }, row_style.fg(colors::ACCENT_CYAN)),
                Span::styled(path.display().to_string(), row_style.fg(colors::TEXT_SECONDARY)),
            ]);
            let para = Paragraph::new(line).style(row_style);
            frame.render_widget(para, slot);
        }
    }
}

fn build_breadcrumb(app: &FileBrowserApp) -> String {
    let root = app
        .workspace_root
        .file_name()
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| app.workspace_root.display().to_string());

    let Some(active) = app.active_file.as_ref() else {
        return format!("{root} > (no file selected)");
    };

    let mut parts = vec![root];
    parts.extend(
        active
            .components()
            .map(|component| component.as_os_str().to_string_lossy().into_owned()),
    );

    parts.join(" > ")
}
