//! Reusable terminal UI components aligned with designs/templates and designs/static/styles.css.

use super::colors;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Padding, Paragraph, Wrap},
};

/// Semantic section styles used across chat-active, tools, and loading pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionTone {
    Intent,
    Actions,
    Result,
    Next,
}

impl SectionTone {
    pub fn accent_color(self) -> ratatui::style::Color {
        match self {
            Self::Intent => colors::ACCENT_PURPLE,
            Self::Actions => colors::ACCENT_YELLOW,
            Self::Result => colors::ACCENT_GREEN,
            Self::Next => colors::ACCENT_CYAN,
        }
    }
}

/// Tool call status style used by tool cards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCallState {
    Success,
    Error,
    Running,
}

impl ToolCallState {
    fn color(self) -> ratatui::style::Color {
        match self {
            Self::Success => colors::ACCENT_GREEN,
            Self::Error => colors::ACCENT_RED,
            Self::Running => colors::ACCENT_YELLOW,
        }
    }

    fn glyph(self) -> &'static str {
        match self {
            Self::Success => "✓",
            Self::Error => "✕",
            Self::Running => "◌",
        }
    }
}

/// Tokenized text style for hint lines where keys and body text are mixed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HintToken<'a> {
    Text(&'a str),
    Key(&'a str),
}

pub const TOP_BORDER_HEIGHT: u16 = 1;
pub const SINGLE_LINE_CONTENT_HEIGHT: u16 = 1;
pub const TOP_BORDERED_ROW_PADDING: Padding = Padding::uniform(1);

pub const fn top_bordered_row_height(content_lines: u16) -> u16 {
    TOP_BORDER_HEIGHT + TOP_BORDERED_ROW_PADDING.top + TOP_BORDERED_ROW_PADDING.bottom + content_lines
}

pub const fn single_line_top_bordered_row_height() -> u16 {
    top_bordered_row_height(SINGLE_LINE_CONTENT_HEIGHT)
}

/// Draws the ASCII logo block.
pub fn draw_ascii_logo(frame: &mut Frame, area: Rect, logo: &str) {
    let logo_text = Text::from(logo.trim_end_matches('\n')).style(Style::default().fg(colors::ACCENT_CYAN));

    let logo_paragraph = Paragraph::new(logo_text)
        .block(Block::default())
        .wrap(Wrap { trim: false })
        .style(Style::default());
    frame.render_widget(logo_paragraph, area);
}

/// Draws the "What can I help you build?" heading pattern.
pub fn draw_brand_greeting(frame: &mut Frame, area: Rect, content: &str) {
    let line = Line::from(vec![Span::styled(
        content,
        Style::default().fg(colors::TEXT_PRIMARY).add_modifier(Modifier::BOLD),
    )]);
    let paragraph = Paragraph::new(line).alignment(Alignment::Center);
    frame.render_widget(paragraph, area);
}

/// Draws uppercase muted section title
pub fn draw_section_title_muted(frame: &mut Frame, area: Rect, title: &str) {
    let uppercase = title.to_ascii_uppercase();
    let paragraph =
        Paragraph::new(Span::styled(uppercase, Style::default().fg(colors::TEXT_MUTED))).alignment(Alignment::Center);
    frame.render_widget(paragraph, area);
}

/// Draws a reusable card-item with optional selected state.
pub fn draw_card_item(frame: &mut Frame, area: Rect, label: &str, is_selected: bool) {
    let border_color = if is_selected { colors::ACCENT_CYAN } else { colors::BORDER_COLOR };

    let outer = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(colors::BG_TERMINAL));
    frame.render_widget(outer.clone(), area);

    let inner = outer.inner(area);
    let fill = Block::default().style(Style::default());
    frame.render_widget(fill, inner);

    let content = Line::from(vec![
        Span::styled("  \u{203a}   ", Style::default().fg(colors::ACCENT_CYAN)),
        Span::styled(
            label,
            Style::default().fg(if is_selected { colors::TEXT_PRIMARY } else { colors::TEXT_SECONDARY }),
        ),
    ]);

    let card = Paragraph::new(content)
        .style(Style::default())
        .wrap(Wrap { trim: true });
    frame.render_widget(card, inner);
}

/// Draws a hint/footer line with colorized key segments.
pub fn draw_hint_line(frame: &mut Frame, area: Rect, tokens: &[HintToken<'_>]) {
    let paragraph = Paragraph::new(hint_line(tokens)).alignment(Alignment::Center);
    frame.render_widget(paragraph, area);
}

fn hint_line<'a>(tokens: &[HintToken<'a>]) -> Line<'a> {
    let spans = tokens
        .iter()
        .map(|token| match token {
            HintToken::Text(value) => Span::styled(*value, Style::default().fg(colors::TEXT_MUTED)),
            HintToken::Key(value) => Span::styled(*value, Style::default().fg(colors::ACCENT_CYAN)),
        })
        .collect::<Vec<_>>();

    Line::from(spans)
}

/// Draws the horizontal divider above the input area.
pub fn draw_input_separator(frame: &mut Frame, area: Rect) {
    let separator = Paragraph::new(Line::from("\u{2500}".repeat(area.width as usize)))
        .style(Style::default().fg(colors::BORDER_COLOR).bg(colors::BG_TERMINAL));
    frame.render_widget(separator, area);
}

/// Draws the shared input container with a top border and horizontal padding.
pub fn draw_input_container(frame: &mut Frame, area: Rect, input: &str, show_cursor: bool) {
    let inner = draw_top_bordered_container(frame, area);
    draw_input_line(frame, inner, input, show_cursor);
}

pub fn draw_top_bordered_container(frame: &mut Frame, area: Rect) -> Rect {
    let container = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(colors::BORDER_COLOR))
        .style(Style::default().bg(colors::BG_TERMINAL))
        .padding(TOP_BORDERED_ROW_PADDING);
    frame.render_widget(container.clone(), area);

    container.inner(area)
}

/// Draws a REPL-style input line with prompt and cursor.
pub fn draw_input_line(frame: &mut Frame, area: Rect, input: &str, show_cursor: bool) {
    let cursor_char = if show_cursor { "\u{2588}" } else { " " };
    let input_line = Line::from(vec![
        Span::styled("\u{276f} ", Style::default().fg(colors::ACCENT_CYAN)),
        Span::styled(input, Style::default().fg(colors::TEXT_PRIMARY)),
        Span::styled(cursor_char, Style::default().fg(colors::ACCENT_CYAN)),
    ]);

    let paragraph = Paragraph::new(input_line).style(Style::default().bg(colors::BG_TERMINAL));
    frame.render_widget(paragraph, area);
}

/// Draws a conversation section (`intent/actions/result/next`) with left accent bar.
pub fn draw_section_block(frame: &mut Frame, area: Rect, tone: SectionTone, icon: &str, title: &str, body: Text<'_>) {
    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(tone.accent_color()))
        .style(Style::default().bg(colors::BG_TERMINAL));
    frame.render_widget(block.clone(), area);

    let inner = block.inner(area);
    let content_area = Rect::new(
        inner.x.saturating_add(1),
        inner.y,
        inner.width.saturating_sub(1),
        inner.height,
    );
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1), Constraint::Min(0)])
        .split(content_area);

    let title_upper = title.to_ascii_uppercase();
    let header = Line::from(vec![
        Span::styled(icon, Style::default().fg(tone.accent_color())),
        Span::raw(" "),
        Span::styled(
            title_upper,
            Style::default().fg(tone.accent_color()).add_modifier(Modifier::BOLD),
        ),
    ]);
    let header_paragraph = Paragraph::new(header);
    frame.render_widget(header_paragraph, layout[0]);

    let body_paragraph = Paragraph::new(body)
        .style(Style::default().fg(colors::TEXT_SECONDARY))
        .wrap(Wrap { trim: false });
    frame.render_widget(body_paragraph, layout[2]);
}

/// Draws a tool call container with a status glyph and optional details content.
pub fn draw_tool_call(frame: &mut Frame, area: Rect, name: &str, args: &str, state: ToolCallState, details: Text<'_>) {
    let outer = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(colors::BORDER_COLOR))
        .style(Style::default().bg(colors::BG_SECONDARY));
    frame.render_widget(outer.clone(), area);

    let inner = outer.inner(area);
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(inner);

    let header_fill = Block::default().style(Style::default().bg(colors::BG_TERTIARY));
    frame.render_widget(header_fill, layout[0]);

    let header = Line::from(vec![
        Span::styled(
            name,
            Style::default().fg(colors::ACCENT_CYAN).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(args, Style::default().fg(colors::TEXT_MUTED)),
        Span::raw("  "),
        Span::styled(
            state.glyph(),
            Style::default().fg(state.color()).add_modifier(Modifier::BOLD),
        ),
    ]);
    let header_paragraph = Paragraph::new(header);
    frame.render_widget(header_paragraph, layout[0]);

    let details_fill = Block::default().style(Style::default().bg(colors::BG_SECONDARY));
    frame.render_widget(details_fill, layout[1]);
    let details_paragraph = Paragraph::new(details)
        .style(Style::default().fg(colors::TEXT_SECONDARY))
        .wrap(Wrap { trim: false });
    frame.render_widget(details_paragraph, layout[1]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_section_tone_accent_mapping() {
        assert_eq!(SectionTone::Intent.accent_color(), colors::ACCENT_PURPLE);
        assert_eq!(SectionTone::Actions.accent_color(), colors::ACCENT_YELLOW);
        assert_eq!(SectionTone::Result.accent_color(), colors::ACCENT_GREEN);
        assert_eq!(SectionTone::Next.accent_color(), colors::ACCENT_CYAN);
    }

    #[test]
    fn test_tool_call_state_glyphs() {
        assert_eq!(ToolCallState::Success.glyph(), "✓");
        assert_eq!(ToolCallState::Error.glyph(), "✕");
        assert_eq!(ToolCallState::Running.glyph(), "◌");
    }
}
