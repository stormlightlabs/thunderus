//! Reusable terminal UI components aligned with designs/templates and designs/static/styles.css.

use super::colors;
use super::layout::{AreaSpec, ConstraintSpec};
use crate::ToolCallStatus;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Padding, Paragraph, Wrap},
};
use thndrs_ui_macros::AreaSpec;

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

impl From<ToolCallStatus> for ToolCallState {
    fn from(val: ToolCallStatus) -> Self {
        match val {
            ToolCallStatus::Pending | ToolCallStatus::Running => ToolCallState::Running,
            ToolCallStatus::Success => ToolCallState::Success,
            ToolCallStatus::Error => ToolCallState::Error,
        }
    }
}

impl ToolCallState {
    pub fn color(self) -> ratatui::style::Color {
        match self {
            Self::Success => colors::ACCENT_GREEN,
            Self::Error => colors::ACCENT_RED,
            Self::Running => colors::ACCENT_YELLOW,
        }
    }

    pub fn glyph(self) -> &'static str {
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

#[derive(AreaSpec)]
pub struct AsciiLogo;

impl AsciiLogo {
    pub fn render(self, frame: &mut Frame, area: Rect, logo: &str) {
        let logo_text = Text::from(logo.trim_end_matches('\n')).style(Style::default().fg(colors::ACCENT_CYAN));

        let logo_paragraph = Paragraph::new(logo_text)
            .block(Block::default())
            .wrap(Wrap { trim: false })
            .style(Style::default());
        frame.render_widget(logo_paragraph, self.area(area));
    }
}

#[derive(AreaSpec)]
pub struct BrandGreeting;

impl BrandGreeting {
    pub fn render(self, frame: &mut Frame, area: Rect, content: &str) {
        let line = Line::from(vec![Span::styled(
            content,
            Style::default().fg(colors::TEXT_PRIMARY).add_modifier(Modifier::BOLD),
        )]);
        let paragraph = Paragraph::new(line).alignment(Alignment::Center);
        frame.render_widget(paragraph, self.area(area));
    }
}

#[derive(AreaSpec)]
pub struct MutedSectionTitle;

impl MutedSectionTitle {
    pub fn render(self, frame: &mut Frame, area: Rect, title: &str) {
        let uppercase = title.to_ascii_uppercase();
        let paragraph = Paragraph::new(Span::styled(uppercase, Style::default().fg(colors::TEXT_MUTED)))
            .alignment(Alignment::Center);
        frame.render_widget(paragraph, self.area(area));
    }
}

#[derive(AreaSpec)]
pub struct CardItem;

impl CardItem {
    pub fn render(self, frame: &mut Frame, area: Rect, label: &str, is_selected: bool) {
        let border_color = if is_selected { colors::ACCENT_CYAN } else { colors::BORDER_COLOR };

        let outer = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border_color))
            .style(Style::default().bg(colors::BG_TERMINAL));
        frame.render_widget(outer.clone(), self.area(area));

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

    pub fn required_height(label: &str, area_width: u16) -> u16 {
        const MIN_CARD_HEIGHT: u16 = 3;
        const LABEL_PREFIX_WIDTH: usize = 6; // "  ›   "

        let inner_width = area_width.saturating_sub(2) as usize;
        if inner_width == 0 {
            return MIN_CARD_HEIGHT;
        }

        if inner_width <= LABEL_PREFIX_WIDTH {
            return MIN_CARD_HEIGHT;
        }

        let label_width = label.chars().count();
        let first_line_capacity = inner_width - LABEL_PREFIX_WIDTH;
        let remaining = label_width.saturating_sub(first_line_capacity);
        let extra_lines = if remaining == 0 { 0 } else { remaining.div_ceil(inner_width) };
        let content_lines = 1 + extra_lines as u16;
        (content_lines + 2).max(MIN_CARD_HEIGHT)
    }
}

#[derive(AreaSpec)]
pub struct HintFooter;

impl HintFooter {
    pub fn render(self, frame: &mut Frame, area: Rect, tokens: &[HintToken<'_>]) {
        let paragraph = Paragraph::new(hint_line(tokens)).alignment(Alignment::Center);
        frame.render_widget(paragraph, self.area(area));
    }
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

#[derive(AreaSpec)]
pub struct InputSeparator;

impl InputSeparator {
    pub fn render(self, frame: &mut Frame, area: Rect) {
        let separator = Paragraph::new(Line::from("\u{2500}".repeat(area.width as usize)))
            .style(Style::default().fg(colors::BORDER_COLOR).bg(colors::BG_TERMINAL));
        frame.render_widget(separator, self.area(area));
    }
}

#[derive(AreaSpec)]
pub struct TopBorderedInputRow;

impl TopBorderedInputRow {
    pub fn render(&self, frame: &mut Frame, area: Rect, input: &str, show_cursor: bool) {
        let inner = self.render_container(frame, area);
        self.render_input_line(frame, inner, input, show_cursor);
    }

    pub fn render_container(&self, frame: &mut Frame, area: Rect) -> Rect {
        let container = Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(colors::BORDER_COLOR))
            .style(Style::default().bg(colors::BG_TERMINAL))
            .padding(TOP_BORDERED_ROW_PADDING);
        frame.render_widget(container.clone(), self.area(area));

        container.inner(area)
    }

    pub fn render_input_line(&self, frame: &mut Frame, area: Rect, input: &str, show_cursor: bool) {
        let cursor_char = if show_cursor { "\u{2588}" } else { " " };
        let input_line = Line::from(vec![
            Span::styled("\u{276f} ", Style::default().fg(colors::ACCENT_CYAN)),
            Span::styled(input, Style::default().fg(colors::TEXT_PRIMARY)),
            Span::styled(cursor_char, Style::default().fg(colors::ACCENT_CYAN)),
        ]);

        let paragraph = Paragraph::new(input_line).style(Style::default().bg(colors::BG_TERMINAL));
        frame.render_widget(paragraph, area);
    }
}

#[derive(AreaSpec)]
pub struct SectionBlock;

impl ConstraintSpec for SectionBlock {
    fn direction(&self) -> Direction {
        Direction::Vertical
    }

    fn constraints(&self, _area: Rect) -> Vec<Constraint> {
        vec![Constraint::Length(1), Constraint::Length(1), Constraint::Min(0)]
    }
}

impl SectionBlock {
    pub fn render(self, frame: &mut Frame, area: Rect, tone: SectionTone, icon: &str, title: &str, body: Text<'_>) {
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
        let layout = self.split(content_area);
        if layout.len() < 3 {
            return;
        }

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

    pub fn estimate_height(content: &str, width: u16, min_height: u16) -> u16 {
        let wrapped_lines = wrapped_line_count(content, width.saturating_sub(2));
        (wrapped_lines as u16 + 2).max(min_height)
    }
}

#[derive(AreaSpec)]
pub struct ToolCallCard;

impl ConstraintSpec for ToolCallCard {
    fn direction(&self) -> Direction {
        Direction::Vertical
    }

    fn constraints(&self, _area: Rect) -> Vec<Constraint> {
        vec![Constraint::Length(1), Constraint::Min(0)]
    }
}

impl ToolCallCard {
    pub fn render(
        self, frame: &mut Frame, area: Rect, name: &str, args: &str, state: ToolCallState, details: Text<'_>,
    ) {
        let outer = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(colors::BORDER_COLOR))
            .style(Style::default().bg(colors::BG_SECONDARY));
        frame.render_widget(outer.clone(), area);

        let inner = outer.inner(area);
        let layout = self.split(inner);
        if layout.len() < 2 {
            return;
        }

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

    pub fn collapsed_height() -> u16 {
        3
    }

    pub fn expanded_height(output: &str, width: u16, max_output_lines: u16) -> u16 {
        let lines = wrapped_line_count(output, width.saturating_sub(4)) as u16;
        Self::collapsed_height() + lines.min(max_output_lines) + 1
    }
}

pub fn wrapped_line_count(content: &str, width: u16) -> usize {
    if width == 0 {
        return 1;
    }

    let width = width as usize;
    let mut total = 0usize;

    for line in content.lines() {
        let chars = line.chars().count();
        total += chars.div_ceil(width).max(1);
    }

    if content.is_empty() { 1 } else { total.max(1) }
}

/// Backward-compatible wrappers.
pub fn draw_ascii_logo(frame: &mut Frame, area: Rect, logo: &str) {
    AsciiLogo.render(frame, area, logo);
}

pub fn draw_brand_greeting(frame: &mut Frame, area: Rect, content: &str) {
    BrandGreeting.render(frame, area, content);
}

pub fn draw_section_title_muted(frame: &mut Frame, area: Rect, title: &str) {
    MutedSectionTitle.render(frame, area, title);
}

pub fn draw_card_item(frame: &mut Frame, area: Rect, label: &str, is_selected: bool) {
    CardItem.render(frame, area, label, is_selected);
}

pub fn draw_hint_line(frame: &mut Frame, area: Rect, tokens: &[HintToken<'_>]) {
    HintFooter.render(frame, area, tokens);
}

pub fn draw_input_separator(frame: &mut Frame, area: Rect) {
    InputSeparator.render(frame, area);
}

pub fn draw_input_container(frame: &mut Frame, area: Rect, input: &str, show_cursor: bool) {
    TopBorderedInputRow.render(frame, area, input, show_cursor);
}

pub fn draw_top_bordered_container(frame: &mut Frame, area: Rect) -> Rect {
    TopBorderedInputRow.render_container(frame, area)
}

pub fn draw_input_line(frame: &mut Frame, area: Rect, input: &str, show_cursor: bool) {
    TopBorderedInputRow.render_input_line(frame, area, input, show_cursor);
}

pub fn draw_section_block(frame: &mut Frame, area: Rect, tone: SectionTone, icon: &str, title: &str, body: Text<'_>) {
    SectionBlock.render(frame, area, tone, icon, title, body);
}

pub fn draw_tool_call(frame: &mut Frame, area: Rect, name: &str, args: &str, state: ToolCallState, details: Text<'_>) {
    ToolCallCard.render(frame, area, name, args, state, details);
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

    #[test]
    fn test_card_item_required_height_minimum() {
        assert_eq!(CardItem::required_height("hello", 0), 3);
        assert_eq!(CardItem::required_height("hello", 6), 3);
    }

    #[test]
    fn test_wrapped_line_count_handles_empty_text() {
        assert_eq!(wrapped_line_count("", 10), 1);
    }
}
