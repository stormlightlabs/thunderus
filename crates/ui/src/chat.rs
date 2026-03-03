//! Chat screen components for active conversation

use super::colors;
use super::components::{
    HintToken, SectionTone, ToolCallState, draw_hint_line, draw_input_container, draw_section_block, draw_tool_call,
    draw_top_bordered_container, single_line_top_bordered_row_height,
};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span, Text},
    widgets::{Block, Padding, Paragraph, Wrap},
};
use thunderus_core::{ResponseSections, estimate_token_cost_usd};

#[derive(Debug, Clone, PartialEq, Default)]
pub enum StreamingState {
    #[default]
    Idle,
    Streaming,
    Thinking,
    /// Tool name
    CallingTool(String),
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
    pub sections: Option<ResponseSections>,
    pub tool_calls: Vec<ToolCallDisplay>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MessageRole {
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone)]
pub struct ToolCallDisplay {
    pub id: String,
    pub name: String,
    pub arguments: String,
    pub status: ToolCallStatus,
    pub output: Option<String>,
    pub expanded: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ToolCallStatus {
    Pending,
    Running,
    Success,
    Error,
}

impl ToolCallDisplay {
    pub fn to_ui_state(&self) -> ToolCallState {
        match self.status {
            ToolCallStatus::Pending | ToolCallStatus::Running => ToolCallState::Running,
            ToolCallStatus::Success => ToolCallState::Success,
            ToolCallStatus::Error => ToolCallState::Error,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IncomingStreamEvent {
    Delta {
        content: Option<String>,
        reasoning_content: Option<String>,
    },
    Done {
        usage: Option<TokenUsage>,
        model: Option<String>,
    },
    Error(String),
    ToolCalling {
        name: String,
        arguments: String,
    },
    ToolCompleted {
        name: String,
        result: String,
    },
    Thinking(String),
}

impl ChatMessage {
    pub fn user(content: String) -> Self {
        Self { role: MessageRole::User, content, sections: None, tool_calls: Vec::new() }
    }

    pub fn assistant(content: String) -> Self {
        let sections = ResponseSections::parse(&content);
        let has_sections = sections.has_content();
        Self {
            role: MessageRole::Assistant,
            content,
            sections: if has_sections { Some(sections) } else { None },
            tool_calls: Vec::new(),
        }
    }

    pub fn assistant_streaming(content: String) -> Self {
        Self { role: MessageRole::Assistant, content, sections: None, tool_calls: Vec::new() }
    }

    pub fn tool(_name: String, output: String) -> Self {
        Self { role: MessageRole::Tool, content: output, sections: None, tool_calls: Vec::new() }
    }

    pub fn finalize(&mut self) {
        if self.role == MessageRole::Assistant && self.sections.is_none() {
            let sections = ResponseSections::parse(&self.content);
            if sections.has_content() {
                self.sections = Some(sections);
            }
        }
    }

    pub fn add_tool_call(&mut self, name: String, arguments: String) -> usize {
        let id = format!("call_{}", self.tool_calls.len());
        self.tool_calls.push(ToolCallDisplay {
            id,
            name,
            arguments,
            status: ToolCallStatus::Running,
            output: None,
            expanded: false,
        });
        self.tool_calls.len() - 1
    }

    pub fn complete_tool_call(&mut self, index: usize, output: String, success: bool) {
        if let Some(tool_call) = self.tool_calls.get_mut(index) {
            tool_call.output = Some(output);
            tool_call.status = if success { ToolCallStatus::Success } else { ToolCallStatus::Error };
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
    pub last_usage: Option<TokenUsage>,
    pub last_model: Option<String>,
    pub running: bool,
    pending_user_message: Option<String>,
    /// Index of currently running tool call
    current_tool_call: Option<usize>,
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
                    self.submit_user_message(content);
                    self.input_buffer.clear();
                    self.cursor_position = 0;
                }
            }
            KeyCode::Up => {
                if self.scroll_offset > 0 {
                    self.scroll_offset -= 1;
                }
            }
            KeyCode::Down => self.scroll_offset += 1,
            KeyCode::Tab => {
                if let Some(last_msg) = self.messages.last_mut()
                    && let Some(last_tool) = last_msg.tool_calls.last_mut()
                {
                    last_tool.expanded = !last_tool.expanded;
                }
            }
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

    pub fn handle_stream_event(&mut self, event: IncomingStreamEvent) {
        match event {
            IncomingStreamEvent::Delta { content, reasoning_content } => {
                if let Some(delta) = content {
                    self.append_stream(&delta, reasoning_content.as_deref());
                }
                self.streaming_state = StreamingState::Streaming;
            }
            IncomingStreamEvent::Done { usage, model } => {
                self.last_usage = usage;
                if let Some(model_id) = model {
                    self.last_model = Some(model_id);
                }
                self.finish_current_stream();
            }
            IncomingStreamEvent::Error(message) => {
                self.append_stream(&format!("\n\n[provider error] {message}"), None);
                self.finish_current_stream();
            }
            IncomingStreamEvent::ToolCalling { name, arguments } => {
                self.streaming_state = StreamingState::CallingTool(name.clone());

                if let Some(last) = self.messages.last_mut()
                    && last.role == MessageRole::Assistant
                {
                    let idx = last.add_tool_call(name, arguments);
                    self.current_tool_call = Some(idx);
                }
            }
            IncomingStreamEvent::ToolCompleted { name: _, result } => {
                if let Some(last) = self.messages.last_mut()
                    && last.role == MessageRole::Assistant
                    && let Some(idx) = self.current_tool_call
                {
                    last.complete_tool_call(idx, result, true);
                }
                self.current_tool_call = None;
                self.streaming_state = StreamingState::Streaming;
            }
            IncomingStreamEvent::Thinking(_content) => {
                self.streaming_state = StreamingState::Thinking;
                // TODO: display thinking content
            }
        }
    }

    pub fn finish_stream(&mut self, full_content: String) {
        if let Some(last) = self.messages.last_mut()
            && last.role == MessageRole::Assistant
        {
            last.content = full_content;
            last.finalize();
        }
        self.streaming_state = StreamingState::Idle;
        self.current_tool_call = None;
    }

    fn finish_current_stream(&mut self) {
        if let Some(last) = self.messages.last_mut()
            && last.role == MessageRole::Assistant
        {
            last.finalize();
        }
        self.streaming_state = StreamingState::Idle;
        self.current_tool_call = None;
    }

    pub fn reset_streaming(&mut self) {
        self.streaming_state = StreamingState::Idle;
        self.pending_user_message = None;
        self.current_tool_call = None;
    }

    pub fn submit_user_message(&mut self, content: String) {
        self.pending_user_message = Some(content.clone());
        self.messages.push(ChatMessage::user(content));
        self.messages.push(ChatMessage::assistant_streaming(String::new()));
        self.streaming_state = StreamingState::Streaming;
    }

    pub fn take_pending_user_message(&mut self) -> Option<String> {
        self.pending_user_message.take()
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
            Constraint::Length(single_line_top_bordered_row_height()),
            Constraint::Length(single_line_top_bordered_row_height()),
        ])
        .split(size);

    draw_messages(frame, main_layout[0], app);
    draw_hints(frame, main_layout[1]);
    draw_token_usage_row(frame, main_layout[2], app);
    draw_input_area(frame, main_layout[3], app);
}

fn draw_messages(frame: &mut Frame, area: Rect, app: &ChatApp) {
    let container = Block::default()
        .style(Style::default().bg(colors::BG_TERMINAL))
        .padding(Padding::new(1, 1, 0, 1));
    frame.render_widget(container.clone(), area);

    let inner = container.inner(area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    if app.messages.is_empty() {
        draw_empty_state(frame, inner);
        return;
    }

    let mut constraints = Vec::new();
    for msg in &app.messages {
        let height = estimate_message_height(msg, inner.width);
        constraints.push(Constraint::Length(height));
    }
    constraints.push(Constraint::Min(0));

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    for (idx, msg) in app.messages.iter().enumerate() {
        if idx + 1 < layout.len() {
            draw_message(frame, layout[idx], msg, &app.streaming_state);
        }
    }
}

fn estimate_message_height(msg: &ChatMessage, width: u16) -> u16 {
    let mut height = 0u16;

    let line_count = msg.content.lines().count() as u16;
    let wrap_factor = if width > 0 { 1 + msg.content.len() as u16 / width } else { 1 };
    height += line_count.max(wrap_factor) + 1;

    for tool_call in &msg.tool_calls {
        height += 3;
        if tool_call.expanded
            && let Some(output) = &tool_call.output
        {
            let output_lines = output.lines().count() as u16;
            height += output_lines.min(10) + 1;
        }
    }

    if msg.sections.is_some() {
        height += 8;
    }

    height
}

fn draw_message(frame: &mut Frame, area: Rect, msg: &ChatMessage, streaming_state: &StreamingState) {
    match msg.role {
        MessageRole::User => draw_user_message(frame, area, &msg.content),
        MessageRole::Assistant => {
            if !msg.tool_calls.is_empty() {
                let tool_height: u16 = msg.tool_calls.iter().map(|_| 3u16).sum();
                let tool_area = Rect::new(area.x, area.y, area.width, tool_height);
                let tool_layout = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints(vec![Constraint::Length(3); msg.tool_calls.len()])
                    .split(tool_area);

                for (idx, tool_call) in msg.tool_calls.iter().enumerate() {
                    if idx < tool_layout.len() {
                        draw_tool_call_widget(frame, tool_layout[idx], tool_call);
                    }
                }
            }

            let content_offset = msg.tool_calls.len() as u16 * 3;
            if area.height > content_offset {
                let content_area = Rect::new(
                    area.x,
                    area.y + content_offset,
                    area.width,
                    area.height - content_offset,
                );

                if let Some(ref sections) = msg.sections {
                    draw_assistant_sections(frame, content_area, sections);
                } else {
                    let is_streaming = *streaming_state == StreamingState::Streaming;
                    draw_assistant_raw(frame, content_area, &msg.content, is_streaming);
                }
            }
        }
        MessageRole::Tool => draw_tool_output(frame, area, &msg.content),
    }
}

fn draw_user_message(frame: &mut Frame, area: Rect, content: &str) {
    let line = Line::from(vec![
        Span::styled("❯ ", Style::default().fg(colors::ACCENT_CYAN)),
        Span::styled(content, Style::default().fg(colors::TEXT_PRIMARY)),
    ]);
    let paragraph = Paragraph::new(line)
        .style(Style::default().bg(colors::BG_TERMINAL))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn draw_tool_output(frame: &mut Frame, area: Rect, content: &str) {
    let text = Text::from(vec![Line::from(vec![
        Span::styled("  ", Style::default()),
        Span::styled(content, Style::default().fg(colors::TEXT_MUTED)),
    ])]);
    let paragraph = Paragraph::new(text)
        .style(Style::default().bg(colors::BG_TERMINAL))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn draw_tool_call_widget(frame: &mut Frame, area: Rect, tool_call: &ToolCallDisplay) {
    let state = tool_call.to_ui_state();
    let details = if tool_call.expanded && tool_call.output.is_some() {
        Text::from(tool_call.output.as_ref().unwrap().clone())
    } else {
        Text::from(format!("Args: {}", tool_call.arguments))
    };

    draw_tool_call(frame, area, &tool_call.name, &tool_call.arguments, state, details);
}

fn draw_assistant_raw(frame: &mut Frame, area: Rect, content: &str, is_streaming: bool) {
    let mut spans = vec![Span::styled("◉ ", Style::default().fg(colors::ACCENT_PURPLE))];

    if is_streaming && content.is_empty() {
        spans.push(Span::styled("Thinking...", Style::default().fg(colors::TEXT_MUTED)));
    } else {
        spans.push(Span::styled(content, Style::default().fg(colors::TEXT_SECONDARY)));
    }

    if is_streaming {
        spans.push(Span::styled(" █", Style::default().fg(colors::ACCENT_CYAN)));
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
            "◉",
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
            "⚡",
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
            "✓",
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
            "→",
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
        HintToken::Key("Tab"),
        HintToken::Text(" to expand tools, "),
        HintToken::Key("Up/Down"),
        HintToken::Text(" to scroll, "),
        HintToken::Key("ctrl+d"),
        HintToken::Text(" to quit"),
    ];
    draw_hint_line(frame, area, &tokens);
}

fn draw_token_usage_row(frame: &mut Frame, area: Rect, app: &ChatApp) {
    let inner = draw_top_bordered_container(frame, area);
    let row_text = build_token_usage_text(app);
    let paragraph = Paragraph::new(row_text)
        .style(Style::default().fg(colors::TEXT_MUTED).bg(colors::BG_TERMINAL))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, inner);
}

fn build_token_usage_text(app: &ChatApp) -> String {
    let Some(usage) = app.last_usage else {
        return "Token usage appears here after the first response.".to_string();
    };

    let mut parts = Vec::with_capacity(5);

    if let Some(model) = app.last_model.as_deref() {
        parts.push(format!("model: {model}"));
    }

    parts.push(format!("prompt: {}", format_u32_with_grouping(usage.prompt_tokens)));
    parts.push(format!(
        "completion: {}",
        format_u32_with_grouping(usage.completion_tokens)
    ));
    parts.push(format!("total: {}", format_u32_with_grouping(usage.total_tokens)));

    if let Some(model) = app.last_model.as_deref()
        && let Some(cost) = estimate_token_cost_usd(model, usage.prompt_tokens, usage.completion_tokens)
    {
        parts.push(format!("est: {}", format_usd(cost)));
    }

    parts.join("  |  ")
}

fn format_u32_with_grouping(value: u32) -> String {
    let mut out = String::new();
    let digits = value.to_string();

    for (idx, ch) in digits.chars().rev().enumerate() {
        if idx > 0 && idx % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }

    out.chars().rev().collect()
}

fn format_usd(value: f64) -> String {
    if value < 0.0001 { format!("${value:.6}") } else { format!("${value:.4}") }
}

fn draw_input_area(frame: &mut Frame, area: Rect, app: &ChatApp) {
    let show_cursor = app.streaming_state == StreamingState::Idle;
    draw_input_container(frame, area, &app.input_buffer, show_cursor);
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
        assert!(app.pending_user_message.is_none());
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
    fn test_submit_user_message_marks_pending() {
        let mut app = ChatApp::new();
        app.submit_user_message("Hello".to_string());

        assert_eq!(app.take_pending_user_message(), Some("Hello".to_string()));
        assert_eq!(app.take_pending_user_message(), None);
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

        assert_eq!(app.streaming_state, StreamingState::Idle);
        assert!(app.messages[0].sections.is_some());
    }

    #[test]
    fn test_handle_stream_event_finalizes_message() {
        let mut app = ChatApp::new();
        app.submit_user_message("Question".to_string());
        app.handle_stream_event(IncomingStreamEvent::Delta {
            content: Some("Intent\n\nDo it\n\nResult\n\nDone".to_string()),
            reasoning_content: None,
        });
        app.handle_stream_event(IncomingStreamEvent::Done { usage: None, model: None });

        assert_eq!(app.streaming_state, StreamingState::Idle);
        assert!(app.messages[1].sections.is_some());
    }

    #[test]
    fn test_done_event_sets_usage_and_model() {
        let mut app = ChatApp::new();
        app.handle_stream_event(IncomingStreamEvent::Done {
            usage: Some(TokenUsage { prompt_tokens: 1200, completion_tokens: 300, total_tokens: 1500 }),
            model: Some("glm-5".to_string()),
        });

        assert_eq!(
            app.last_usage,
            Some(TokenUsage { prompt_tokens: 1200, completion_tokens: 300, total_tokens: 1500 })
        );
        assert_eq!(app.last_model, Some("glm-5".to_string()));
    }

    #[test]
    fn test_tool_call_events() {
        let mut app = ChatApp::new();
        app.submit_user_message("Read a file".to_string());

        app.handle_stream_event(IncomingStreamEvent::ToolCalling {
            name: "read".to_string(),
            arguments: r#"{"path": "/test.txt"}"#.to_string(),
        });

        assert!(matches!(app.streaming_state, StreamingState::CallingTool(_)));
        assert_eq!(app.messages[1].tool_calls.len(), 1);
        assert_eq!(app.messages[1].tool_calls[0].name, "read");

        app.handle_stream_event(IncomingStreamEvent::ToolCompleted {
            name: "read".to_string(),
            result: "File contents".to_string(),
        });

        assert_eq!(app.messages[1].tool_calls[0].status, ToolCallStatus::Success);
        assert_eq!(app.messages[1].tool_calls[0].output, Some("File contents".to_string()));
    }

    #[test]
    fn test_tool_call_expansion() {
        let mut msg = ChatMessage::assistant_streaming(String::new());
        let idx = msg.add_tool_call("read".to_string(), r#"{"path": "/test.txt"}"#.to_string());

        assert!(!msg.tool_calls[idx].expanded);

        msg.tool_calls[idx].expanded = true;
        assert!(msg.tool_calls[idx].expanded);
    }
}
