//! Chat screen components for active conversation

use super::colors;
use super::components::{
    HintFooter, HintToken, SectionBlock, SectionTone, ToolCallCard, ToolCallState, TopBorderedInputRow,
};
use super::components::{top_bordered_row_height, wrapped_line_count};
use super::files::{fuzzy_match_paths, read_file_for_prompt, workspace_files};
use super::layout::split as split_rects;
use super::tool::{TaskItem, draw_bash_output, draw_collapsible, draw_diff, draw_task_progress, parse_diff};
use chrono::{DateTime, Local, Utc};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Padding, Paragraph, Wrap};
use serde_json::Value;
use std::path::{Path, PathBuf};
use thndrs_core::{ResponseSections, estimate_token_cost_usd};

const INPUT_PROMPT_PREFIX: &str = "❯ ";

#[derive(Debug, Clone, PartialEq, Default)]
pub enum StreamingState {
    #[default]
    Idle,
    Streaming,
    Thinking,
    /// Tool name
    CallingTool(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MessageRole {
    User,
    Assistant,
    Tool,
}

impl MessageRole {
    pub fn label(self) -> &'static str {
        match self {
            MessageRole::User => "YOU",
            MessageRole::Assistant => "ASSISTANT",
            MessageRole::Tool => "TOOL",
        }
    }
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
        self.status.into()
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
        is_error: bool,
    },
    Thinking(String),
}

#[derive(Debug, Clone, Default)]
struct ChatFileFinder {
    active: bool,
    query: String,
    selected: usize,
    matches: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
    pub sections: Option<ResponseSections>,
    pub tool_calls: Vec<ToolCallDisplay>,
    pub created_at: DateTime<Utc>,
}

impl ChatMessage {
    pub fn user(content: String) -> Self {
        Self::user_at(content, Utc::now())
    }

    pub fn user_at(content: String, created_at: DateTime<Utc>) -> Self {
        Self { role: MessageRole::User, content, sections: None, tool_calls: Vec::new(), created_at }
    }

    pub fn assistant(content: String) -> Self {
        Self::assistant_at(content, Utc::now())
    }

    pub fn assistant_at(content: String, created_at: DateTime<Utc>) -> Self {
        let sections = ResponseSections::parse(&content);
        let has_sections = sections.has_content();
        Self {
            role: MessageRole::Assistant,
            content,
            sections: if has_sections { Some(sections) } else { None },
            tool_calls: Vec::new(),
            created_at,
        }
    }

    pub fn assistant_streaming(content: String) -> Self {
        Self {
            role: MessageRole::Assistant,
            content,
            sections: None,
            tool_calls: Vec::new(),
            created_at: Utc::now(),
        }
    }

    pub fn tool(name: String, output: String) -> Self {
        Self::tool_at(name, output, Utc::now())
    }

    pub fn tool_at(_name: String, output: String, created_at: DateTime<Utc>) -> Self {
        Self { role: MessageRole::Tool, content: output, sections: None, tool_calls: Vec::new(), created_at }
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
            if tool_call.name == "memory_recall" {
                tool_call.expanded = true;
            }
        }
    }
}

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
    pending_submission: Option<String>,
    pending_command: Option<String>,
    workspace_root: PathBuf,
    workspace_files: Vec<PathBuf>,
    file_finder: ChatFileFinder,
    pinned_files: Vec<PathBuf>,
    /// Index of currently running tool call
    current_tool_call: Option<usize>,
}

impl Default for ChatApp {
    fn default() -> Self {
        let workspace_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let workspace_files = workspace_files(&workspace_root);

        Self {
            messages: Vec::new(),
            input_buffer: String::new(),
            cursor_position: 0,
            streaming_state: StreamingState::Idle,
            scroll_offset: 0,
            last_usage: None,
            last_model: None,
            running: true,
            pending_user_message: None,
            pending_submission: None,
            pending_command: None,
            workspace_root,
            workspace_files,
            file_finder: ChatFileFinder::default(),
            pinned_files: Vec::new(),
            current_tool_call: None,
        }
    }
}

impl ChatApp {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn handle_input(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }

        if self.file_finder.active {
            self.handle_file_finder_input(key);
            return;
        }

        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => self.running = false,
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => self.running = false,
            KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => self.running = false,
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => self.clear_current_input_line(),
            KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => self.clear_input_buffer(),
            KeyCode::Char('@') => self.activate_file_finder(),
            KeyCode::Char(c) => {
                self.input_buffer.insert(self.cursor_position, c);
                self.cursor_position += 1;
            }
            KeyCode::Backspace => {
                if self.cursor_position > 0 {
                    self.cursor_position -= 1;
                    self.input_buffer.remove(self.cursor_position);
                } else {
                    self.unpin_last_file();
                }
            }
            KeyCode::Delete => {
                if self.cursor_position < self.input_buffer.len() {
                    self.input_buffer.remove(self.cursor_position);
                } else if self.cursor_position == 0 {
                    self.unpin_last_file();
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
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.input_buffer.insert(self.cursor_position, '\n');
                self.cursor_position += 1;
            }
            KeyCode::Enter => {
                if !self.input_buffer.is_empty() && self.streaming_state == StreamingState::Idle {
                    let content = self.input_buffer.clone();
                    if content.starts_with('/') {
                        self.pending_command = Some(content);
                    } else {
                        self.submit_user_message(content);
                    }
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

    fn handle_file_finder_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.file_finder = ChatFileFinder::default();
            }
            KeyCode::Char(c) => {
                self.file_finder.query.push(c);
                self.update_file_finder_matches();
            }
            KeyCode::Backspace => {
                self.file_finder.query.pop();
                self.update_file_finder_matches();
            }
            KeyCode::Up => {
                self.file_finder.selected = self.file_finder.selected.saturating_sub(1);
            }
            KeyCode::Down => {
                if self.file_finder.selected + 1 < self.file_finder.matches.len() {
                    self.file_finder.selected += 1;
                }
            }
            KeyCode::Enter => {
                if let Some(path) = self.file_finder.matches.get(self.file_finder.selected).cloned() {
                    self.toggle_pin(&path);
                }
                self.file_finder = ChatFileFinder::default();
            }
            _ => {}
        }
    }

    pub fn activate_file_finder(&mut self) {
        self.workspace_files = workspace_files(&self.workspace_root);
        self.file_finder.active = true;
        self.file_finder.query.clear();
        self.file_finder.selected = 0;
        self.file_finder.matches = self.workspace_files.iter().take(10).cloned().collect();
    }

    fn update_file_finder_matches(&mut self) {
        self.file_finder.matches = fuzzy_match_paths(&self.file_finder.query, &self.workspace_files, 10);
        self.file_finder.selected = self
            .file_finder
            .selected
            .min(self.file_finder.matches.len().saturating_sub(1));
    }

    fn toggle_pin(&mut self, path: &Path) {
        if let Some(idx) = self.pinned_files.iter().position(|p| p == path) {
            self.pinned_files.remove(idx);
        } else {
            self.pinned_files.push(path.to_path_buf());
        }
    }

    fn unpin_last_file(&mut self) -> bool {
        self.pinned_files.pop().is_some()
    }

    fn clear_current_input_line(&mut self) {
        if self.input_buffer.is_empty() {
            return;
        }

        let len = self.input_buffer.len();
        let cursor = self.cursor_position.min(len);
        let line_start = self.input_buffer[..cursor].rfind('\n').map(|idx| idx + 1).unwrap_or(0);
        let line_end = self.input_buffer[cursor..]
            .find('\n')
            .map(|idx| cursor + idx)
            .unwrap_or(len);

        let (remove_start, remove_end) = if line_start == 0 && line_end == len {
            (0, len)
        } else if line_start > 0 {
            (line_start - 1, line_end)
        } else {
            (line_start, (line_end + 1).min(len))
        };

        self.input_buffer.replace_range(remove_start..remove_end, "");
        self.cursor_position = remove_start.min(self.input_buffer.len());
    }

    fn clear_input_buffer(&mut self) {
        self.input_buffer.clear();
        self.cursor_position = 0;
    }

    pub fn is_file_finder_active(&self) -> bool {
        self.file_finder.active
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
            IncomingStreamEvent::ToolCompleted { name: _, result, is_error } => {
                if let Some(last) = self.messages.last_mut()
                    && last.role == MessageRole::Assistant
                    && let Some(idx) = self.current_tool_call
                {
                    last.complete_tool_call(idx, result, !is_error);
                }
                self.current_tool_call = None;
                self.streaming_state = StreamingState::Streaming;
            }
            IncomingStreamEvent::Thinking(content) => {
                if !content.trim().is_empty() {
                    self.append_stream(&content, None);
                }
                self.streaming_state = StreamingState::Thinking;
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
        self.pending_submission = None;
        self.pending_command = None;
        self.current_tool_call = None;
    }

    pub fn clear_chat(&mut self) {
        self.messages.clear();
        self.reset_streaming();
        self.scroll_offset = 0;
        self.last_usage = None;
        self.last_model = None;
    }

    pub fn set_messages(&mut self, messages: Vec<ChatMessage>) {
        self.messages = messages;
        self.reset_streaming();
        self.scroll_offset = 0;
    }

    pub fn queue_backend_command(&mut self, command: String) {
        self.pending_submission = Some(command);
        self.pending_user_message = None;
        self.streaming_state = StreamingState::Idle;
    }

    pub fn submit_user_message(&mut self, content: String) {
        self.pending_user_message = Some(content.clone());
        self.pending_submission = Some(self.build_submission_with_pins(&content));
        self.messages.push(ChatMessage::user(content));
        self.messages.push(ChatMessage::assistant_streaming(String::new()));
        self.streaming_state = StreamingState::Streaming;
    }

    pub fn take_pending_user_message(&mut self) -> Option<String> {
        self.pending_user_message.take()
    }

    pub fn take_pending_command(&mut self) -> Option<String> {
        self.pending_command.take()
    }

    pub fn take_pending_submission(&mut self) -> Option<String> {
        self.pending_submission.take()
    }

    pub fn has_messages(&self) -> bool {
        !self.messages.is_empty()
    }

    pub fn pinned_files(&self) -> &[PathBuf] {
        &self.pinned_files
    }

    fn build_submission_with_pins(&self, content: &str) -> String {
        if self.pinned_files.is_empty() {
            return content.to_string();
        }

        let mut out = String::from(content);
        out.push_str("\n\n[Pinned workspace files]\n");

        for path in &self.pinned_files {
            let Some(file_content) = read_file_for_prompt(&self.workspace_root, path) else {
                continue;
            };

            let language = path.extension().and_then(|ext| ext.to_str()).unwrap_or("text");

            out.push_str(&format!(
                "\nFile: {}\n```{}\n{}\n```\n",
                path.display(),
                language,
                file_content
            ));
        }

        out
    }

    pub fn load_debug_chat(&mut self) {
        self.messages.clear();
        self.pending_user_message = None;
        self.pending_submission = None;
        self.pending_command = None;
        self.file_finder = ChatFileFinder::default();
        self.pinned_files.clear();
        self.current_tool_call = None;
        self.streaming_state = StreamingState::Idle;
        self.scroll_offset = 0;

        for idx in 0..40 {
            self.messages.push(ChatMessage::user(format!(
                "Debug prompt #{idx}: explain how component layout should adapt to narrow terminals."
            )));

            let mut assistant = ChatMessage::assistant(
                "Intent\n\nDemonstrate long-scroll behavior and structured sections.\n\nActions\n\n- Build deterministic layout constraints\n- Validate wrapped content sizing\n- Keep rendering predictable under resize\n\nResult\n\nRendered stable sections with bounded heights.\n\nNext\n\nTry `/debug files` to inspect the file browser stress view."
                    .to_string(),
            );
            if idx % 3 == 0 {
                let call_idx =
                    assistant.add_tool_call("read".to_string(), format!(r#"{{"path":"src/module_{idx}.rs"}}"#));
                assistant.complete_tool_call(
                    call_idx,
                    "fn example() {\n    println!(\"debug\");\n}".to_string(),
                    true,
                );
                if let Some(tool) = assistant.tool_calls.get_mut(call_idx) {
                    tool.expanded = idx % 2 == 0;
                }
            }

            self.messages.push(assistant);
        }
    }
}

pub fn draw_chat_screen(frame: &mut Frame, app: &ChatApp) {
    let size = frame.area();

    let clear = Block::default().style(Style::default().bg(colors::BG_TERMINAL));
    frame.render_widget(clear, size);

    let main_layout = split_rects(
        size,
        Direction::Vertical,
        vec![
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(top_bordered_row_height(1)),
            Constraint::Length(chat_input_row_height(app, size.width)),
        ],
    );
    if main_layout.len() < 4 {
        return;
    }

    draw_messages(frame, main_layout[0], app);
    draw_hints(frame, main_layout[1]);
    draw_token_usage_row(frame, main_layout[2], app);
    draw_input_area(frame, main_layout[3], app);

    if app.file_finder.active {
        draw_file_finder_overlay(frame, size, app);
    }
}

fn draw_messages(frame: &mut Frame, area: Rect, app: &ChatApp) {
    let container = Block::default()
        .style(Style::default().bg(colors::BG_TERMINAL))
        .padding(Padding::new(1, 1, 2, 1));
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
    for (idx, message) in app.messages.iter().enumerate() {
        constraints.push(Constraint::Length(estimate_message_height(message, inner.width)));
        if idx + 1 < app.messages.len() {
            constraints.push(Constraint::Length(1));
        }
    }
    constraints.push(Constraint::Min(0));

    let layout = split_rects(inner, Direction::Vertical, constraints);

    let mut slot = 0usize;
    for (idx, msg) in app.messages.iter().enumerate() {
        if slot >= layout.len() {
            break;
        }

        draw_message(frame, layout[slot], msg, &app.streaming_state);
        slot += 1;

        if idx + 1 < app.messages.len() && slot < layout.len() {
            draw_message_divider(frame, layout[slot]);
            slot += 1;
        }
    }
}

fn draw_message_divider(frame: &mut Frame, area: Rect) {
    if area.width == 0 {
        return;
    }

    let divider = Paragraph::new("\u{2500}".repeat(area.width as usize))
        .style(Style::default().fg(colors::BORDER_COLOR).bg(colors::BG_TERMINAL));
    frame.render_widget(divider, area);
}

fn estimate_message_height(msg: &ChatMessage, width: u16) -> u16 {
    let content_width = width.max(1);
    let mut height = 1u16;

    match msg.role {
        MessageRole::Assistant => match &msg.sections {
            Some(sections) => {
                height += assistant_section_constraints(sections, content_width)
                    .iter()
                    .map(constraint_length)
                    .sum::<u16>()
            }
            None => {
                let content = assistant_display_content(&msg.content);
                height += wrapped_line_count(&content, content_width.saturating_sub(2)) as u16;
            }
        },
        MessageRole::Tool => {
            let content = tool_display_content(&msg.content);
            height += wrapped_line_count(&content, content_width.saturating_sub(2)) as u16;
        }
        MessageRole::User => height += wrapped_line_count(&msg.content, content_width.saturating_sub(2)) as u16,
    }

    for tool_call in &msg.tool_calls {
        height += estimate_tool_call_height(tool_call, content_width);
    }

    height
}

fn draw_message(frame: &mut Frame, area: Rect, msg: &ChatMessage, streaming_state: &StreamingState) {
    match msg.role {
        MessageRole::User => draw_user_message(frame, area, msg),
        MessageRole::Assistant => {
            let tool_constraints = msg
                .tool_calls
                .iter()
                .map(|tool_call| Constraint::Length(estimate_tool_call_height(tool_call, area.width)))
                .collect::<Vec<_>>();

            let content_offset = tool_constraints.iter().map(constraint_length).sum::<u16>();
            if !msg.tool_calls.is_empty() {
                let mut message_constraints = tool_constraints;
                message_constraints.push(Constraint::Min(0));
                let message_layout = split_rects(area, Direction::Vertical, message_constraints);

                for (idx, tool_call) in msg.tool_calls.iter().enumerate() {
                    if idx < message_layout.len() {
                        draw_tool_call_widget(frame, message_layout[idx], tool_call);
                    }
                }
            }

            if area.height > content_offset {
                let content_area = Rect::new(
                    area.x,
                    area.y + content_offset,
                    area.width,
                    area.height - content_offset,
                );

                if let Some(ref sections) = msg.sections {
                    draw_assistant_sections(frame, content_area, sections, msg.created_at);
                } else {
                    let is_streaming = *streaming_state == StreamingState::Streaming;
                    draw_assistant_raw(frame, content_area, &msg.content, is_streaming, msg.created_at);
                }
            }
        }
        MessageRole::Tool => draw_tool_output(frame, area, msg),
    }
}

fn draw_user_message(frame: &mut Frame, area: Rect, message: &ChatMessage) {
    let sections = split_rects(
        area,
        Direction::Vertical,
        vec![Constraint::Length(1), Constraint::Min(0)],
    );
    if sections.len() < 2 {
        return;
    }

    frame.render_widget(
        Paragraph::new(message_header_line(message.role, message.created_at))
            .style(Style::default().bg(colors::BG_TERMINAL)),
        sections[0],
    );

    let line = Line::from(vec![
        Span::styled("❯ ", Style::default().fg(colors::ACCENT_CYAN)),
        Span::styled(&message.content, Style::default().fg(colors::TEXT_PRIMARY)),
    ]);
    frame.render_widget(
        Paragraph::new(line)
            .style(Style::default().bg(colors::BG_TERMINAL))
            .wrap(Wrap { trim: false }),
        sections[1],
    );
}

fn draw_tool_output(frame: &mut Frame, area: Rect, message: &ChatMessage) {
    let sections = split_rects(
        area,
        Direction::Vertical,
        vec![Constraint::Length(1), Constraint::Min(0)],
    );

    if sections.len() < 2 {
        return;
    }

    frame.render_widget(
        Paragraph::new(message_header_line(message.role, message.created_at))
            .style(Style::default().bg(colors::BG_TERMINAL)),
        sections[0],
    );

    let display_content = tool_display_content(&message.content);
    let mut lines = Vec::new();

    for line in display_content.lines() {
        lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(line.to_string(), Style::default().fg(colors::TEXT_MUTED)),
        ]));
    }

    if lines.is_empty() {
        lines.push(Line::from(vec![Span::styled("  ", Style::default())]));
    }

    let text = Text::from(lines);
    frame.render_widget(
        Paragraph::new(text)
            .style(Style::default().bg(colors::BG_TERMINAL))
            .wrap(Wrap { trim: false }),
        sections[1],
    );
}

fn draw_tool_call_widget(frame: &mut Frame, area: Rect, tool_call: &ToolCallDisplay) {
    let state = tool_call.to_ui_state();
    let summary = Text::from(format_tool_arguments(&tool_call.name, &tool_call.arguments));

    if !tool_call.expanded {
        ToolCallCard.render(frame, area, &tool_call.name, &tool_call.arguments, state, summary);
        return;
    }

    let layout = split_rects(
        area,
        Direction::Vertical,
        vec![Constraint::Length(ToolCallCard::collapsed_height()), Constraint::Min(0)],
    );

    if layout.len() < 2 {
        ToolCallCard.render(frame, area, &tool_call.name, &tool_call.arguments, state, summary);
        return;
    }

    ToolCallCard.render(frame, layout[0], &tool_call.name, &tool_call.arguments, state, summary);
    draw_expanded_tool_details(frame, layout[1], tool_call);
}

fn draw_expanded_tool_details(frame: &mut Frame, area: Rect, tool_call: &ToolCallDisplay) {
    if matches!(tool_call.status, ToolCallStatus::Pending | ToolCallStatus::Running) {
        let tasks = progress_tasks_for(tool_call);
        draw_task_progress(frame, area, &tasks, "Tool progress");
        return;
    }

    if tool_call.name == "edit"
        && let Some(diff_text) = extract_edit_diff(&tool_call.arguments)
    {
        let diff_lines = parse_diff(&diff_text);
        if !diff_lines.is_empty() {
            draw_diff(frame, area, &diff_lines);
            return;
        }
    }

    if tool_call.name == "bash" {
        let command = extract_bash_command(&tool_call.arguments).unwrap_or_else(|| "bash".to_string());
        let (output, exit_code) = parse_bash_output(tool_call.output.as_deref().unwrap_or_default());
        draw_bash_output(frame, area, &command, &output, exit_code);
        return;
    }

    let content = Text::from(format_tool_output(
        &tool_call.name,
        tool_call.output.as_deref().unwrap_or_default(),
    ));
    draw_collapsible(frame, area, "Tool output", true, content);
}

fn progress_tasks_for(tool_call: &ToolCallDisplay) -> Vec<TaskItem> {
    match tool_call.status {
        ToolCallStatus::Pending => vec![
            TaskItem::new("Queued").running(),
            TaskItem::new(format!("Execute {}", tool_call.name)),
            TaskItem::new("Collect output"),
        ],
        ToolCallStatus::Running => vec![
            TaskItem::new("Queued").done(),
            TaskItem::new(format!("Execute {}", tool_call.name)).running(),
            TaskItem::new("Collect output"),
        ],
        ToolCallStatus::Success | ToolCallStatus::Error => vec![
            TaskItem::new("Queued").done(),
            TaskItem::new(format!("Execute {}", tool_call.name)).done(),
            TaskItem::new("Collect output").done(),
        ],
    }
}

fn extract_bash_command(arguments: &str) -> Option<String> {
    let value = serde_json::from_str::<Value>(arguments).ok()?;
    let object = value.as_object()?;
    object.get("command").and_then(Value::as_str).map(ToString::to_string)
}

fn extract_edit_diff(arguments: &str) -> Option<String> {
    let value = serde_json::from_str::<Value>(arguments).ok()?;
    let object = value.as_object()?;
    object.get("diff").and_then(Value::as_str).map(ToString::to_string)
}

fn parse_bash_output(raw_output: &str) -> (String, i32) {
    let Some(stripped) = raw_output.strip_prefix("Command exited with code ") else {
        return (raw_output.to_string(), 0);
    };

    let mut parts = stripped.splitn(2, '\n');
    let code_str = parts.next().unwrap_or_default().trim();
    let output = parts.next().unwrap_or_default().to_string();
    let code = code_str.parse::<i32>().unwrap_or(-1);

    (output, code)
}

fn draw_assistant_raw(frame: &mut Frame, area: Rect, content: &str, is_streaming: bool, created_at: DateTime<Utc>) {
    let sections = split_rects(
        area,
        Direction::Vertical,
        vec![Constraint::Length(1), Constraint::Min(0)],
    );
    if sections.len() < 2 {
        return;
    }

    frame.render_widget(
        Paragraph::new(message_header_line(MessageRole::Assistant, created_at))
            .style(Style::default().bg(colors::BG_TERMINAL)),
        sections[0],
    );

    let mut lines = Vec::new();
    let display_content = assistant_display_content(content);

    if is_streaming && display_content.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("◉ ", Style::default().fg(colors::ACCENT_PURPLE)),
            Span::styled("Thinking...", Style::default().fg(colors::TEXT_MUTED)),
        ]));
    } else {
        for (idx, raw_line) in display_content.lines().enumerate() {
            let prefix = if idx == 0 { "◉ " } else { "  " };
            let prefix_style = if idx == 0 {
                Style::default().fg(colors::ACCENT_PURPLE)
            } else {
                Style::default().fg(colors::TEXT_MUTED)
            };
            let trimmed = raw_line.trim_start();

            let line = if let Some(item) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")) {
                Line::from(vec![
                    Span::styled(prefix, prefix_style),
                    Span::styled("• ", Style::default().fg(colors::ACCENT_CYAN)),
                    Span::styled(item.to_string(), Style::default().fg(colors::TEXT_SECONDARY)),
                ])
            } else if trimmed.is_empty() {
                Line::from(vec![Span::styled(prefix, prefix_style)])
            } else if trimmed.ends_with(':') {
                Line::from(vec![
                    Span::styled(prefix, prefix_style),
                    Span::styled(
                        trimmed.to_string(),
                        Style::default().fg(colors::TEXT_PRIMARY).add_modifier(Modifier::BOLD),
                    ),
                ])
            } else {
                Line::from(vec![
                    Span::styled(prefix, prefix_style),
                    Span::styled(trimmed.to_string(), Style::default().fg(colors::TEXT_SECONDARY)),
                ])
            };

            lines.push(line);
        }
    }

    if lines.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "◉ ",
            Style::default().fg(colors::ACCENT_PURPLE),
        )]));
    }

    if is_streaming && let Some(last_line) = lines.last_mut() {
        last_line
            .spans
            .push(Span::styled(" █", Style::default().fg(colors::ACCENT_CYAN)));
    }

    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .style(Style::default().bg(colors::BG_TERMINAL))
            .wrap(Wrap { trim: false }),
        sections[1],
    );
}

fn message_header_line(role: MessageRole, created_at: DateTime<Utc>) -> Line<'static> {
    Line::from(vec![
        Span::styled(role.label(), Style::default().fg(colors::TEXT_MUTED)),
        Span::styled("  ", Style::default().fg(colors::TEXT_MUTED)),
        Span::styled(
            format_message_timestamp(created_at),
            Style::default().fg(colors::TEXT_MUTED),
        ),
    ])
}

fn format_message_timestamp(created_at: DateTime<Utc>) -> String {
    let local_time = created_at.with_timezone(&Local);
    let now_local = Local::now();
    if local_time.date_naive() == now_local.date_naive() {
        local_time.format("%H:%M").to_string()
    } else {
        local_time.format("%Y-%m-%d %H:%M").to_string()
    }
}

fn assistant_display_content(content: &str) -> String {
    normalize_display_content(content)
}

fn tool_display_content(content: &str) -> String {
    normalize_display_content(content)
}

fn normalize_display_content(content: &str) -> String {
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
    let normalized = normalized.replace(":- ", ":\n- ");
    let normalized = normalized.replace(":-\n", ":\n- ");
    let normalized = fix_missing_bullet_breaks(&normalized);
    collapse_blank_lines(normalized.trim())
}

fn fix_missing_bullet_breaks(content: &str) -> String {
    let chars: Vec<char> = content.chars().collect();
    if chars.is_empty() {
        return String::new();
    }

    let mut output = String::with_capacity(content.len() + 8);
    for idx in 0..chars.len() {
        let current = chars[idx];
        let next = chars.get(idx + 1).copied();
        let previous = if idx > 0 { Some(chars[idx - 1]) } else { None };

        if current == '-'
            && next == Some(' ')
            && let Some(prev) = previous
            && prev != '\n'
            && prev != '\r'
            && (prev == ':' || prev == ';' || prev == ')' || prev == ']' || prev == '"' || prev.is_alphanumeric())
        {
            output.push('\n');
        }

        output.push(current);
    }

    output
}

fn collapse_blank_lines(content: &str) -> String {
    let mut lines = Vec::new();
    let mut blank_run = 0usize;

    for line in content.lines() {
        if line.trim().is_empty() {
            blank_run += 1;
            if blank_run > 1 {
                continue;
            }
            lines.push(String::new());
            continue;
        }

        blank_run = 0;
        lines.push(line.to_string());
    }

    lines.join("\n")
}

fn format_tool_arguments(name: &str, raw_arguments: &str) -> String {
    let parsed = serde_json::from_str::<Value>(raw_arguments);
    let Ok(value) = parsed else {
        return format!("Args: {}", raw_arguments);
    };

    if name == "memory_recall"
        && let Some(obj) = value.as_object()
    {
        let query = obj.get("query").and_then(Value::as_str).unwrap_or_default();
        let count = obj.get("count").and_then(Value::as_u64).unwrap_or(5);
        let kind = obj.get("kind").and_then(Value::as_str).unwrap_or("any");
        return format!("query: {}  |  count: {}  |  kind: {}", query, count, kind);
    }

    if let Some(obj) = value.as_object() {
        let mut parts = Vec::new();
        for (idx, (key, value)) in obj.iter().enumerate() {
            if idx >= 4 {
                parts.push(format!("+{} more", obj.len().saturating_sub(idx)));
                break;
            }
            parts.push(format!("{key}: {}", format_json_compact(value)));
        }
        return format!("Args: {}", parts.join("  |  "));
    }

    format!("Args: {}", format_json_compact(&value))
}

fn format_tool_output(name: &str, raw_output: &str) -> String {
    let normalized = tool_display_content(raw_output);
    if name == "memory_recall" {
        return normalized.replacen(":\n- ", ":\n\n- ", 1);
    }
    normalized
}

fn format_json_compact(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(v) => v.to_string(),
        Value::Number(v) => v.to_string(),
        Value::String(v) => v.clone(),
        Value::Array(values) => {
            let preview = values
                .iter()
                .take(3)
                .map(format_json_compact)
                .collect::<Vec<_>>()
                .join(", ");
            if values.len() > 3 {
                format!("[{}, +{} more]", preview, values.len() - 3)
            } else {
                format!("[{}]", preview)
            }
        }
        Value::Object(map) => {
            let preview = map
                .iter()
                .take(3)
                .map(|(key, value)| format!("{key}: {}", format_json_compact(value)))
                .collect::<Vec<_>>()
                .join(", ");
            if map.len() > 3 {
                format!("{{{}, +{} more}}", preview, map.len() - 3)
            } else {
                format!("{{{}}}", preview)
            }
        }
    }
}

fn draw_assistant_sections(frame: &mut Frame, area: Rect, sections: &ResponseSections, created_at: DateTime<Utc>) {
    let outer = split_rects(
        area,
        Direction::Vertical,
        vec![Constraint::Length(1), Constraint::Min(0)],
    );
    if outer.len() < 2 {
        return;
    }

    frame.render_widget(
        Paragraph::new(message_header_line(MessageRole::Assistant, created_at))
            .style(Style::default().bg(colors::BG_TERMINAL)),
        outer[0],
    );

    let constraints = assistant_section_constraints(sections, outer[1].width);

    if constraints.is_empty() {
        return;
    }

    let layout = split_rects(outer[1], Direction::Vertical, constraints);

    let mut slot = 0;

    if let Some(ref intent) = sections.intent
        && slot < layout.len()
    {
        SectionBlock.render(
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
        SectionBlock.render(
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
        SectionBlock.render(
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
        SectionBlock.render(
            frame,
            layout[slot],
            SectionTone::Next,
            "→",
            "Next",
            Text::from(next.clone()),
        );
    }
}

fn assistant_section_constraints(sections: &ResponseSections, width: u16) -> Vec<Constraint> {
    let mut constraints = Vec::new();
    let content_width = width.saturating_sub(2);

    if let Some(intent) = &sections.intent {
        constraints.push(Constraint::Length(SectionBlock::estimate_height(
            intent,
            content_width,
            4,
        )));
    }
    if let Some(actions) = &sections.actions {
        constraints.push(Constraint::Length(SectionBlock::estimate_height(
            actions,
            content_width,
            5,
        )));
    }
    if let Some(result) = &sections.result {
        constraints.push(Constraint::Length(SectionBlock::estimate_height(
            result,
            content_width,
            6,
        )));
    }
    if let Some(next) = &sections.next {
        constraints.push(Constraint::Length(SectionBlock::estimate_height(
            next,
            content_width,
            4,
        )));
    }

    constraints
}

fn estimate_tool_call_height(tool_call: &ToolCallDisplay, width: u16) -> u16 {
    if !tool_call.expanded {
        return ToolCallCard::collapsed_height();
    }

    let body_height = if matches!(tool_call.status, ToolCallStatus::Pending | ToolCallStatus::Running) {
        5
    } else {
        match tool_call.name.as_str() {
            "edit" => extract_edit_diff(&tool_call.arguments)
                .map(|diff| parse_diff(&diff).len() as u16)
                .map(|line_count| line_count.clamp(1, 10) + 2)
                .unwrap_or(6),
            "bash" => {
                let (output, _) = parse_bash_output(tool_call.output.as_deref().unwrap_or(""));
                (wrapped_line_count(&output, width.saturating_sub(4)) as u16).clamp(1, 10) + 2
            }
            _ => {
                let output = tool_call.output.as_deref().unwrap_or("");
                (wrapped_line_count(output, width.saturating_sub(4)) as u16).clamp(1, 10) + 2
            }
        }
    };

    ToolCallCard::collapsed_height() + body_height
}

fn constraint_length(constraint: &Constraint) -> u16 {
    match *constraint {
        Constraint::Length(value) => value,
        _ => 0,
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
        HintToken::Key("Shift+Enter"),
        HintToken::Text(" newline, "),
        HintToken::Key("Ctrl+U"),
        HintToken::Text(" clear line, "),
        HintToken::Key("Ctrl+L"),
        HintToken::Text(" clear input, "),
        HintToken::Key("@"),
        HintToken::Text(" to pin files, "),
        HintToken::Key("Tab"),
        HintToken::Text(" to expand tools, "),
        HintToken::Key("Up/Down"),
        HintToken::Text(" to scroll, "),
        HintToken::Key("ctrl+d"),
        HintToken::Text(" to quit"),
    ];
    HintFooter.render(frame, area, &tokens);
}

fn draw_token_usage_row(frame: &mut Frame, area: Rect, app: &ChatApp) {
    let inner = TopBorderedInputRow.render_container(frame, area);
    let row_text = build_token_usage_text(app);
    let paragraph = Paragraph::new(row_text)
        .style(Style::default().fg(colors::TEXT_MUTED).bg(colors::BG_TERMINAL))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, inner);
}

fn build_token_usage_text(app: &ChatApp) -> String {
    let pinned_summary = if app.pinned_files().is_empty() {
        "pinned: none".to_string()
    } else {
        let joined = app
            .pinned_files()
            .iter()
            .take(2)
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let suffix = if app.pinned_files().len() > 2 {
            format!(" (+{} more)", app.pinned_files().len() - 2)
        } else {
            String::new()
        };
        format!("pinned: {joined}{suffix}")
    };

    let Some(usage) = app.last_usage else {
        return format!("Token usage appears here after the first response.  |  {pinned_summary}");
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

    parts.push(pinned_summary);
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

fn chat_input_row_height(app: &ChatApp, area_width: u16) -> u16 {
    top_bordered_row_height(chat_input_content_line_count(app, area_width))
}

fn chat_input_content_line_count(app: &ChatApp, area_width: u16) -> u16 {
    let inner_width = area_width.saturating_sub(2).max(1);
    let mut lines = wrapped_multiline_input_line_count(
        &app.input_buffer,
        inner_width
            .saturating_sub(INPUT_PROMPT_PREFIX.chars().count() as u16)
            .max(1),
        inner_width,
    ) as u16;

    if !app.pinned_files().is_empty() {
        lines += wrapped_line_count(&pinned_files_input_text(app.pinned_files()), inner_width) as u16;
    }

    lines.max(1)
}

fn wrapped_multiline_input_line_count(input: &str, first_width: u16, continuation_width: u16) -> usize {
    fn wrapped_segment_line_count(segment: &str, width: u16) -> usize {
        if width == 0 {
            return 1;
        }
        segment.chars().count().div_ceil(width as usize).max(1)
    }

    let mut segments = input.split('\n');
    let mut total = 0usize;

    if let Some(first) = segments.next() {
        total += wrapped_segment_line_count(first, first_width.max(1));
    } else {
        return 1;
    }

    for segment in segments {
        total += wrapped_segment_line_count(segment, continuation_width.max(1));
    }

    total.max(1)
}

fn pinned_files_input_text(pinned_files: &[PathBuf]) -> String {
    let mut out = String::new();
    out.push('@');
    out.push(' ');
    for (idx, path) in pinned_files.iter().enumerate() {
        if idx > 0 {
            out.push(' ');
        }
        out.push('[');
        out.push_str(&path.display().to_string());
        out.push(']');
    }
    out
}

fn draw_input_area(frame: &mut Frame, area: Rect, app: &ChatApp) {
    let show_cursor = app.streaming_state == StreamingState::Idle;
    let inner = TopBorderedInputRow.render_container(frame, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let mut lines = Vec::new();

    if !app.pinned_files().is_empty() {
        lines.push(Line::from(vec![Span::styled(
            pinned_files_input_text(app.pinned_files()),
            Style::default().fg(colors::ACCENT_GREEN),
        )]));
    }

    let cursor_char = if show_cursor { "\u{2588}" } else { " " };
    let input_segments = app.input_buffer.split('\n').collect::<Vec<_>>();

    for (idx, segment) in input_segments.iter().enumerate() {
        let prefix = if idx == 0 { INPUT_PROMPT_PREFIX } else { "  " };
        let prefix_style = if idx == 0 {
            Style::default().fg(colors::ACCENT_CYAN)
        } else {
            Style::default().fg(colors::TEXT_MUTED)
        };

        let mut spans = vec![
            Span::styled(prefix, prefix_style),
            Span::styled(*segment, Style::default().fg(colors::TEXT_PRIMARY)),
        ];

        if idx + 1 == input_segments.len() {
            spans.push(Span::styled(cursor_char, Style::default().fg(colors::ACCENT_CYAN)));
        }

        lines.push(Line::from(spans));
    }

    if lines.is_empty() {
        lines.push(Line::from(vec![
            Span::styled(INPUT_PROMPT_PREFIX, Style::default().fg(colors::ACCENT_CYAN)),
            Span::styled(cursor_char, Style::default().fg(colors::ACCENT_CYAN)),
        ]));
    }

    let paragraph = Paragraph::new(Text::from(lines))
        .style(Style::default().bg(colors::BG_TERMINAL))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, inner);
}

pub(crate) fn draw_file_finder_overlay(frame: &mut Frame, area: Rect, app: &ChatApp) {
    let rows = split_rects(
        area,
        Direction::Vertical,
        vec![Constraint::Fill(1), Constraint::Length(12), Constraint::Fill(1)],
    );
    if rows.len() < 2 {
        return;
    }

    let overlay_width = 72u16.min(area.width.saturating_sub(2)).max(24);
    let cols = split_rects(
        rows[1],
        Direction::Horizontal,
        vec![
            Constraint::Fill(1),
            Constraint::Length(overlay_width),
            Constraint::Fill(1),
        ],
    );
    if cols.len() < 2 {
        return;
    }

    let panel = cols[1];
    frame.render_widget(Clear, panel);
    let block = Block::default()
        .title(" pin file to chat ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(colors::ACCENT_CYAN))
        .style(Style::default().bg(colors::BG_TERMINAL));
    frame.render_widget(block.clone(), panel);
    let inner = block.inner(panel);
    frame.render_widget(Block::default().style(Style::default().bg(colors::BG_TERMINAL)), inner);

    let mut constraints = vec![Constraint::Length(1)];
    constraints.extend((0..app.file_finder.matches.len()).map(|_| Constraint::Length(1)));
    constraints.push(Constraint::Min(0));
    let layout = split_rects(inner, Direction::Vertical, constraints);
    if layout.is_empty() {
        return;
    }

    let input_line = Line::from(vec![
        Span::styled(
            "@",
            Style::default().fg(colors::ACCENT_CYAN).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            &app.file_finder.query,
            Style::default().fg(colors::TEXT_PRIMARY).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  (Enter pin/unpin)", Style::default().fg(colors::TEXT_MUTED)),
    ]);
    frame.render_widget(
        Paragraph::new(input_line).style(Style::default().bg(colors::BG_TERMINAL)),
        layout[0],
    );

    for (idx, path) in app.file_finder.matches.iter().enumerate() {
        if let Some(slot) = layout.get(idx + 1).copied() {
            let selected = idx == app.file_finder.selected;
            let pinned = app.pinned_files().iter().any(|p| p == path);
            let row_style = Style::default().bg(colors::BG_TERMINAL);

            let line = Line::from(vec![
                Span::styled(if selected { "> " } else { "  " }, row_style.fg(colors::ACCENT_CYAN)),
                Span::styled(
                    if pinned { "* " } else { "  " },
                    row_style.fg(if pinned { colors::ACCENT_GREEN } else { colors::TEXT_MUTED }),
                ),
                Span::styled(path.display().to_string(), row_style.fg(colors::TEXT_SECONDARY)),
            ]);
            frame.render_widget(Paragraph::new(line).style(row_style), slot);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

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
    fn test_shift_enter_inserts_newline_without_submitting() {
        let mut app = ChatApp::new();
        app.handle_input(KeyEvent::from(KeyCode::Char('a')));
        app.handle_input(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT));
        app.handle_input(KeyEvent::from(KeyCode::Char('b')));

        assert_eq!(app.input_buffer, "a\nb");
        assert!(app.messages.is_empty());
        assert_eq!(app.streaming_state, StreamingState::Idle);
    }

    #[test]
    fn test_ctrl_u_clears_current_input_line() {
        let mut app = ChatApp::new();
        app.input_buffer = "alpha\nbeta\ngamma".to_string();
        app.cursor_position = 8;

        app.handle_input(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));

        assert_eq!(app.input_buffer, "alpha\ngamma");
    }

    #[test]
    fn test_ctrl_l_clears_input_buffer() {
        let mut app = ChatApp::new();
        app.input_buffer = "line one\nline two".to_string();
        app.cursor_position = app.input_buffer.len();

        app.handle_input(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL));

        assert!(app.input_buffer.is_empty());
        assert_eq!(app.cursor_position, 0);
    }

    #[test]
    fn test_backspace_at_prompt_start_unpins_last_file() {
        let mut app = ChatApp::new();
        app.toggle_pin(Path::new("sample.rs"));
        app.toggle_pin(Path::new("README.md"));
        app.cursor_position = 0;

        app.handle_input(KeyEvent::from(KeyCode::Backspace));

        assert_eq!(app.pinned_files(), &[PathBuf::from("sample.rs")]);
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
            is_error: false,
        });

        assert_eq!(app.messages[1].tool_calls[0].status, ToolCallStatus::Success);
        assert_eq!(app.messages[1].tool_calls[0].output, Some("File contents".to_string()));
    }

    #[test]
    fn test_memory_recall_tool_auto_expands_when_completed() {
        let mut app = ChatApp::new();
        app.submit_user_message("What do you remember?".to_string());

        app.handle_stream_event(IncomingStreamEvent::ToolCalling {
            name: "memory_recall".to_string(),
            arguments: r#"{"query":"name","count":3}"#.to_string(),
        });
        app.handle_stream_event(IncomingStreamEvent::ToolCompleted {
            name: "memory_recall".to_string(),
            result: "Found 1 memories:\n- [fact] Name is Thunderus".to_string(),
            is_error: false,
        });

        assert!(app.messages[1].tool_calls[0].expanded);
    }

    #[test]
    fn test_tool_call_error_event_sets_error_status() {
        let mut app = ChatApp::new();
        app.submit_user_message("Run bad command".to_string());
        app.handle_stream_event(IncomingStreamEvent::ToolCalling {
            name: "bash".to_string(),
            arguments: r#"{"command":"false"}"#.to_string(),
        });
        app.handle_stream_event(IncomingStreamEvent::ToolCompleted {
            name: "bash".to_string(),
            result: "Command exited with code 1".to_string(),
            is_error: true,
        });

        assert_eq!(app.messages[1].tool_calls[0].status, ToolCallStatus::Error);
    }

    #[test]
    fn test_thinking_event_appends_stream_content() {
        let mut app = ChatApp::new();
        app.submit_user_message("Explain your plan".to_string());
        app.handle_stream_event(IncomingStreamEvent::Thinking("Drafting a short plan...".to_string()));

        assert_eq!(app.streaming_state, StreamingState::Thinking);
        assert!(app.messages[1].content.contains("Drafting a short plan"));
    }

    #[test]
    fn test_assistant_display_content_inserts_missing_bullet_breaks() {
        let formatted = assistant_display_content("I can help with:- one- two");
        assert_eq!(formatted, "I can help with:\n- one\n- two");
    }

    #[test]
    fn test_format_tool_arguments_memory_recall() {
        let formatted = format_tool_arguments("memory_recall", r#"{"query":"style","count":5}"#);
        assert!(formatted.contains("query: style"));
        assert!(formatted.contains("count: 5"));
    }

    #[test]
    fn test_extract_edit_diff_from_arguments() {
        let args = r#"{"path":"src/lib.rs","diff":"@@ -1,1 +1,1 @@\n-old\n+new"}"#;
        let diff = extract_edit_diff(args).expect("diff should be present");
        assert!(diff.contains("@@ -1,1 +1,1 @@"));
    }

    #[test]
    fn test_parse_bash_output_with_error_prefix() {
        let (output, code) = parse_bash_output("Command exited with code 2\nstderr text");
        assert_eq!(code, 2);
        assert_eq!(output, "stderr text");
    }

    #[test]
    fn test_tool_call_expansion() {
        let mut msg = ChatMessage::assistant_streaming(String::new());
        let idx = msg.add_tool_call("read".to_string(), r#"{"path": "/test.txt"}"#.to_string());

        assert!(!msg.tool_calls[idx].expanded);

        msg.tool_calls[idx].expanded = true;
        assert!(msg.tool_calls[idx].expanded);
    }

    #[test]
    fn test_pinned_files_are_injected_into_submission() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be valid")
            .as_nanos();
        let workspace = std::env::temp_dir().join(format!("thndrs-ui-chat-{unique}"));
        std::fs::create_dir_all(&workspace).expect("workspace should be created");
        std::fs::write(workspace.join("sample.rs"), "fn demo() {}\n").expect("sample file should be written");

        let mut app = ChatApp::new();
        app.workspace_root = workspace.clone();
        app.workspace_files = vec![PathBuf::from("sample.rs")];
        app.toggle_pin(&PathBuf::from("sample.rs"));

        app.submit_user_message("Explain this module".to_string());
        let submission = app.take_pending_submission().expect("submission should be pending");

        assert!(submission.contains("Explain this module"));
        assert!(submission.contains("[Pinned workspace files]"));
        assert!(submission.contains("File: sample.rs"));
        assert!(submission.contains("fn demo() {}"));

        std::fs::remove_dir_all(workspace).expect("workspace should be removed");
    }
}
