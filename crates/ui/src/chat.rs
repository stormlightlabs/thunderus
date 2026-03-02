//! Chat screen components for active conversation

use super::colors;
use super::components::{
    HintToken, SectionTone, draw_hint_line, draw_input_line, draw_input_separator, draw_section_block,
};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span, Text},
    widgets::{Block, Paragraph, Wrap},
};
use thunderus_core::ResponseSections;

#[derive(Debug, Clone, PartialEq, Default)]
pub enum StreamingState {
    #[default]
    Idle,
    Streaming,
    Done,
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
    pub sections: Option<ResponseSections>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MessageRole {
    User,
    Assistant,
}

impl ChatMessage {
    pub fn user(content: String) -> Self {
        Self { role: MessageRole::User, content, sections: None }
    }

    pub fn assistant(content: String) -> Self {
        let sections = ResponseSections::parse(&content);
        let has_sections = sections.has_content();
        Self { role: MessageRole::Assistant, content, sections: if has_sections { Some(sections) } else { None } }
    }

    pub fn assistant_streaming(content: String) -> Self {
        Self { role: MessageRole::Assistant, content, sections: None }
    }

    pub fn finalize(&mut self) {
        if self.role == MessageRole::Assistant && self.sections.is_none() {
            let sections = ResponseSections::parse(&self.content);
            if sections.has_content() {
                self.sections = Some(sections);
            }
        }
    }
}

#[derive(Default)]
pub struct ChatApp {
    pub messages: Vec<ChatMessage>,
    pub input_buffer: String,
    pub cursor_position: usize,
    pub streaming_state: StreamingState,
    pub scroll_offset: u16,
    pub running: bool,
}

impl ChatApp {
    pub fn new() -> Self {
        Self { running: true, ..Default::default() }
    }

    pub fn handle_input(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }

        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => self.running = false,
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => self.running = false,
            KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => self.running = false,
            KeyCode::Char(c) => {
                self.input_buffer.insert(self.cursor_position, c);
                self.cursor_position += 1;
            }
            KeyCode::Backspace => {
                if self.cursor_position > 0 {
                    self.cursor_position -= 1;
                    self.input_buffer.remove(self.cursor_position);
                }
            }
            KeyCode::Left => {
                if self.cursor_position > 0 {
                    self.cursor_position -= 1;
                }
            }
            KeyCode::Right => {
                if self.cursor_position < self.input_buffer.len() {
                    self.cursor_position += 1;
                }
            }
            KeyCode::Enter => {
                if !self.input_buffer.is_empty() && self.streaming_state == StreamingState::Idle {
                    let content = self.input_buffer.clone();
                    self.messages.push(ChatMessage::user(content));
                    self.input_buffer.clear();
                    self.cursor_position = 0;
                    self.streaming_state = StreamingState::Streaming;
                    self.messages.push(ChatMessage::assistant_streaming(String::new()));
                }
            }
            KeyCode::Up => {
                if self.scroll_offset > 0 {
                    self.scroll_offset -= 1;
                }
            }
            KeyCode::Down => self.scroll_offset += 1,
            _ => {}
        }
    }

    pub fn append_stream(&mut self, content: &str, _reasoning: Option<&str>) {
        if let Some(last) = self.messages.last_mut()
            && last.role == MessageRole::Assistant
        {
            last.content.push_str(content);
        }
    }

    pub fn finish_stream(&mut self, full_content: String) {
        if let Some(last) = self.messages.last_mut()
            && last.role == MessageRole::Assistant
        {
            last.content = full_content;
            last.finalize();
        }
        self.streaming_state = StreamingState::Done;
    }

    pub fn reset_streaming(&mut self) {
        self.streaming_state = StreamingState::Idle;
    }

    pub fn has_messages(&self) -> bool {
        !self.messages.is_empty()
    }
}

pub fn draw_chat_screen(frame: &mut Frame, app: &ChatApp) {
    let size = frame.area();

    let clear = Block::default().style(Style::default().bg(colors::BG_TERMINAL));
    frame.render_widget(clear, size);

    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(3),
        ])
        .split(size);

    draw_messages(frame, main_layout[0], app);
    draw_hints(frame, main_layout[1]);
    draw_input_separator(frame, main_layout[2]);
    draw_input_area(frame, main_layout[3], app);
}

fn draw_messages(frame: &mut Frame, area: Rect, app: &ChatApp) {
    if app.messages.is_empty() {
        draw_empty_state(frame, area);
        return;
    }

    let mut constraints = Vec::new();
    for msg in &app.messages {
        let height = estimate_message_height(msg, area.width);
        constraints.push(Constraint::Length(height));
    }
    constraints.push(Constraint::Min(0));

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    for (idx, msg) in app.messages.iter().enumerate() {
        if idx + 1 < layout.len() {
            draw_message(frame, layout[idx], msg, &app.streaming_state);
        }
    }
}

fn estimate_message_height(msg: &ChatMessage, width: u16) -> u16 {
    let line_count = msg.content.lines().count() as u16;
    let wrap_factor = if width > 0 { 1 + msg.content.len() as u16 / width } else { 1 };
    let base = line_count.max(wrap_factor);
    if msg.sections.is_some() { base + 8 } else { base + 2 }
}

fn draw_message(frame: &mut Frame, area: Rect, msg: &ChatMessage, streaming_state: &StreamingState) {
    match msg.role {
        MessageRole::User => draw_user_message(frame, area, &msg.content),
        MessageRole::Assistant => {
            if let Some(ref sections) = msg.sections {
                draw_assistant_sections(frame, area, sections);
            } else {
                let is_streaming = *streaming_state == StreamingState::Streaming;
                draw_assistant_raw(frame, area, &msg.content, is_streaming);
            }
        }
    }
}

fn draw_user_message(frame: &mut Frame, area: Rect, content: &str) {
    let line = Line::from(vec![
        Span::styled(" \u{276f} ", Style::default().fg(colors::ACCENT_CYAN)),
        Span::styled(content, Style::default().fg(colors::TEXT_PRIMARY)),
    ]);
    let paragraph = Paragraph::new(line)
        .style(Style::default().bg(colors::BG_TERMINAL))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn draw_assistant_raw(frame: &mut Frame, area: Rect, content: &str, is_streaming: bool) {
    let mut spans = vec![Span::styled(" \u{25c9} ", Style::default().fg(colors::ACCENT_PURPLE))];

    if is_streaming && content.is_empty() {
        spans.push(Span::styled("Thinking...", Style::default().fg(colors::TEXT_MUTED)));
    } else {
        spans.push(Span::styled(content, Style::default().fg(colors::TEXT_SECONDARY)));
    }

    if is_streaming {
        spans.push(Span::styled(" \u{2588}", Style::default().fg(colors::ACCENT_CYAN)));
    }

    let line = Line::from(spans);
    let paragraph = Paragraph::new(line)
        .style(Style::default().bg(colors::BG_TERMINAL))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn draw_assistant_sections(frame: &mut Frame, area: Rect, sections: &ResponseSections) {
    let mut constraints = Vec::new();

    if sections.intent.is_some() {
        constraints.push(Constraint::Length(4));
    }
    if sections.actions.is_some() {
        constraints.push(Constraint::Length(5));
    }
    if sections.result.is_some() {
        constraints.push(Constraint::Length(6));
    }
    if sections.next.is_some() {
        constraints.push(Constraint::Length(4));
    }

    if constraints.is_empty() {
        return;
    }

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let mut slot = 0;

    if let Some(ref intent) = sections.intent
        && slot < layout.len()
    {
        draw_section_block(
            frame,
            layout[slot],
            SectionTone::Intent,
            "\u{25c9}",
            "Intent",
            Text::from(intent.clone()),
        );
        slot += 1;
    }

    if let Some(ref actions) = sections.actions
        && slot < layout.len()
    {
        draw_section_block(
            frame,
            layout[slot],
            SectionTone::Actions,
            "\u{26a1}",
            "Actions",
            Text::from(actions.clone()),
        );
        slot += 1;
    }

    if let Some(ref result) = sections.result
        && slot < layout.len()
    {
        draw_section_block(
            frame,
            layout[slot],
            SectionTone::Result,
            "\u{2713}",
            "Result",
            Text::from(result.clone()),
        );
        slot += 1;
    }

    if let Some(ref next) = sections.next
        && slot < layout.len()
    {
        draw_section_block(
            frame,
            layout[slot],
            SectionTone::Next,
            "\u{2192}",
            "Next",
            Text::from(next.clone()),
        );
    }
}

fn draw_empty_state(frame: &mut Frame, area: Rect) {
    let text =
        Text::from("Start a conversation by typing a message below.").style(Style::default().fg(colors::TEXT_MUTED));
    let paragraph = Paragraph::new(text)
        .alignment(Alignment::Center)
        .style(Style::default().bg(colors::BG_TERMINAL));

    let centered_y = area.y + area.height.saturating_sub(1) / 2;
    let centered_area = Rect::new(area.x, centered_y, area.width, 1);
    frame.render_widget(paragraph, centered_area);
}

fn draw_hints(frame: &mut Frame, area: Rect) {
    let tokens = [
        HintToken::Text("Press "),
        HintToken::Key("Enter"),
        HintToken::Text(" to send, "),
        HintToken::Key("Up/Down"),
        HintToken::Text(" to scroll, "),
        HintToken::Key("ctrl+d"),
        HintToken::Text(" to quit"),
    ];
    draw_hint_line(frame, area, &tokens);
}

fn draw_input_area(frame: &mut Frame, area: Rect, app: &ChatApp) {
    let input_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1), Constraint::Length(1)])
        .split(area);

    let show_cursor = app.streaming_state != StreamingState::Streaming;
    draw_input_line(frame, input_layout[1], &app.input_buffer, show_cursor);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_app_default() {
        let app = ChatApp::new();
        assert!(app.messages.is_empty());
        assert!(app.input_buffer.is_empty());
        assert_eq!(app.cursor_position, 0);
        assert_eq!(app.streaming_state, StreamingState::Idle);
    }

    #[test]
    fn test_chat_message_user() {
        let msg = ChatMessage::user("Hello".to_string());
        assert_eq!(msg.role, MessageRole::User);
        assert_eq!(msg.content, "Hello");
        assert!(msg.sections.is_none());
    }

    #[test]
    fn test_chat_message_assistant_with_sections() {
        let content = "Intent\n\nDo something\n\nActions\n\n- action1\n\nResult\n\nDone\n\nNext\n\nDone";
        let msg = ChatMessage::assistant(content.to_string());
        assert_eq!(msg.role, MessageRole::Assistant);
        assert!(msg.sections.is_some());
        let sections = msg.sections.unwrap();
        assert_eq!(sections.intent, Some("Do something".to_string()));
    }

    #[test]
    fn test_streaming_state() {
        let mut app = ChatApp::new();

        app.handle_input(KeyEvent::from(KeyCode::Char('h')));
        app.handle_input(KeyEvent::from(KeyCode::Char('i')));
        app.handle_input(KeyEvent::from(KeyCode::Enter));

        assert_eq!(app.streaming_state, StreamingState::Streaming);
        assert_eq!(app.messages.len(), 2);
        assert_eq!(app.messages[0].role, MessageRole::User);
        assert_eq!(app.messages[1].role, MessageRole::Assistant);
    }

    #[test]
    fn test_append_stream() {
        let mut app = ChatApp::new();
        app.messages.push(ChatMessage::user("Hello".to_string()));
        app.messages.push(ChatMessage::assistant_streaming(String::new()));

        app.append_stream("Hi", None);
        app.append_stream(" there", None);

        assert_eq!(app.messages[1].content, "Hi there");
    }

    #[test]
    fn test_finish_stream() {
        let mut app = ChatApp::new();
        app.streaming_state = StreamingState::Streaming;
        app.messages
            .push(ChatMessage::assistant_streaming("Partial".to_string()));

        app.finish_stream("Full response\n\nIntent\n\nDo x\n\nResult\n\nDone".to_string());

        assert_eq!(app.streaming_state, StreamingState::Done);
        assert!(app.messages[0].sections.is_some());
    }
}
