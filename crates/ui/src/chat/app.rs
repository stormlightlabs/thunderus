use super::render;
use super::{ChatFileFinder, ChatMessage, IncomingStreamEvent, MessageRole, StreamingState, TokenUsage};
use crate::colors;
use crate::components::{top_bordered_row_height, wrapped_line_count};
use crate::files::{fuzzy_match_paths, read_file_for_prompt, workspace_files};
use crate::layout::split as split_rects;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use std::path::{Path, PathBuf};

const MAX_INPUT_HISTORY: usize = 200;

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
    input_history: Vec<String>,
    history_cursor: Option<usize>,
    history_draft: Option<String>,
    workspace_root: PathBuf,
    workspace_files: Vec<PathBuf>,
    pub(super) file_finder: ChatFileFinder,
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
            input_history: Vec::new(),
            history_cursor: None,
            history_draft: None,
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
                self.reset_history_navigation();
                self.input_buffer.insert(self.cursor_position, c);
                self.cursor_position += 1;
            }
            KeyCode::Backspace => {
                self.reset_history_navigation();
                if self.cursor_position > 0 {
                    self.cursor_position -= 1;
                    self.input_buffer.remove(self.cursor_position);
                } else {
                    self.unpin_last_file();
                }
            }
            KeyCode::Delete => {
                self.reset_history_navigation();
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
                self.reset_history_navigation();
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
                    self.reset_history_navigation();
                }
            }
            KeyCode::Up => {
                if !self.try_navigate_history_up() && self.scroll_offset > 0 {
                    self.scroll_offset -= 1;
                }
            }
            KeyCode::Down => {
                if !self.try_navigate_history_down() {
                    self.scroll_offset += 1;
                }
            }
            KeyCode::PageUp => {
                self.scroll_offset = self.scroll_offset.saturating_sub(5);
            }
            KeyCode::PageDown => {
                self.scroll_offset += 5;
            }
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

    fn input_is_multiline(&self) -> bool {
        self.input_buffer.contains('\n')
    }

    fn try_navigate_history_up(&mut self) -> bool {
        if self.input_is_multiline() || self.input_history.is_empty() {
            return false;
        }

        let next_index = match self.history_cursor {
            Some(0) => 0,
            Some(idx) => idx.saturating_sub(1),
            None => {
                self.history_draft = Some(self.input_buffer.clone());
                self.input_history.len().saturating_sub(1)
            }
        };

        self.set_input_from_history(next_index);
        true
    }

    fn try_navigate_history_down(&mut self) -> bool {
        if self.input_is_multiline() || self.input_history.is_empty() {
            return false;
        }

        let Some(current) = self.history_cursor else {
            return false;
        };

        if current + 1 < self.input_history.len() {
            self.set_input_from_history(current + 1);
            return true;
        }

        self.history_cursor = None;
        self.input_buffer = self.history_draft.take().unwrap_or_default();
        self.cursor_position = self.input_buffer.len();
        true
    }

    fn set_input_from_history(&mut self, index: usize) {
        if let Some(entry) = self.input_history.get(index) {
            self.input_buffer = entry.clone();
            self.cursor_position = self.input_buffer.len();
            self.history_cursor = Some(index);
        }
    }

    fn reset_history_navigation(&mut self) {
        self.history_cursor = None;
        self.history_draft = None;
    }

    fn record_input_history(&mut self, content: &str) {
        if content.is_empty() {
            return;
        }

        self.input_history.push(content.to_string());
        if self.input_history.len() > MAX_INPUT_HISTORY {
            let overflow = self.input_history.len() - MAX_INPUT_HISTORY;
            self.input_history.drain(0..overflow);
        }
        self.reset_history_navigation();
    }

    fn rebuild_input_history_from_messages(&mut self) {
        self.input_history = self
            .messages
            .iter()
            .filter(|message| message.role == MessageRole::User)
            .map(|message| message.content.clone())
            .collect();
        if self.input_history.len() > MAX_INPUT_HISTORY {
            let overflow = self.input_history.len() - MAX_INPUT_HISTORY;
            self.input_history.drain(0..overflow);
        }
        self.reset_history_navigation();
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

    pub(super) fn toggle_pin(&mut self, path: &Path) {
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
        self.reset_history_navigation();

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
        self.reset_history_navigation();
        self.input_buffer.clear();
        self.cursor_position = 0;
    }

    pub fn is_file_finder_active(&self) -> bool {
        self.file_finder.active
    }

    pub fn append_stream(&mut self, content: &str, reasoning: Option<&str>) {
        if let Some(last) = self.messages.last_mut()
            && last.role == MessageRole::Assistant
        {
            if let Some(reasoning_delta) = reasoning {
                last.append_reasoning_delta(reasoning_delta);
            }

            if !content.is_empty() {
                last.content.push_str(content);
            }
        }
    }

    pub fn handle_stream_event(&mut self, event: IncomingStreamEvent) {
        match event {
            IncomingStreamEvent::Delta { content, reasoning_content } => {
                let has_reasoning = reasoning_content
                    .as_deref()
                    .is_some_and(|reasoning| !reasoning.is_empty());
                if let Some(delta) = content {
                    self.append_stream(&delta, reasoning_content.as_deref());
                } else if has_reasoning {
                    self.append_stream("", reasoning_content.as_deref());
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
                    self.append_stream("", Some(&content));
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
        self.input_history.clear();
        self.reset_history_navigation();
    }

    pub fn set_messages(&mut self, messages: Vec<ChatMessage>) {
        self.messages = messages;
        self.rebuild_input_history_from_messages();
        self.reset_streaming();
        self.scroll_offset = 0;
    }

    pub fn queue_backend_command(&mut self, command: String) {
        self.pending_submission = Some(command);
        self.pending_user_message = None;
        self.streaming_state = StreamingState::Idle;
    }

    pub fn submit_user_message(&mut self, content: String) {
        self.record_input_history(&content);
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
        self.rebuild_input_history_from_messages();
    }

    pub fn draw_file_finder_overlay(&self, frame: &mut Frame, area: Rect) {
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
        constraints.extend((0..self.file_finder.matches.len()).map(|_| Constraint::Length(1)));
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
                &self.file_finder.query,
                Style::default().fg(colors::TEXT_PRIMARY).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  (Enter pin/unpin)", Style::default().fg(colors::TEXT_MUTED)),
        ]);
        frame.render_widget(
            Paragraph::new(input_line).style(Style::default().bg(colors::BG_TERMINAL)),
            layout[0],
        );

        for (idx, path) in self.file_finder.matches.iter().enumerate() {
            if let Some(slot) = layout.get(idx + 1).copied() {
                let selected = idx == self.file_finder.selected;
                let pinned = self.pinned_files().iter().any(|p| p == path);
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

    pub fn draw_chat_screen(&self, frame: &mut Frame) {
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
                Constraint::Length(self.chat_input_row_height(size.width)),
            ],
        );
        if main_layout.len() < 4 {
            return;
        }

        render::draw_messages(frame, main_layout[0], self);
        render::draw_hints(frame, main_layout[1]);
        render::draw_token_usage_row(frame, main_layout[2], self);
        render::draw_input_area(frame, main_layout[3], self);

        if self.file_finder.active {
            self.draw_file_finder_overlay(frame, size);
        }
    }

    pub fn chat_input_row_height(&self, area_width: u16) -> u16 {
        top_bordered_row_height(self.chat_input_content_line_count(area_width))
    }

    fn chat_input_content_line_count(&self, area_width: u16) -> u16 {
        let inner_width = area_width.saturating_sub(2).max(1);
        let mut lines = render::wrapped_multiline_input_line_count(
            &self.input_buffer,
            inner_width
                .saturating_sub(render::INPUT_PROMPT_PREFIX.chars().count() as u16)
                .max(1),
            inner_width,
        ) as u16;

        if !self.pinned_files().is_empty() {
            lines += wrapped_line_count(&render::pinned_files_input_text(self.pinned_files()), inner_width) as u16;
        }

        lines.max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ToolCallStatus;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::path::{Path, PathBuf};
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
    fn test_up_down_navigate_input_history_and_restore_draft() {
        let mut app = ChatApp::new();
        app.submit_user_message("first".to_string());
        app.submit_user_message("second".to_string());
        app.streaming_state = StreamingState::Idle;
        app.input_buffer = "draft".to_string();
        app.cursor_position = app.input_buffer.len();

        app.handle_input(KeyEvent::from(KeyCode::Up));
        assert_eq!(app.input_buffer, "second");

        app.handle_input(KeyEvent::from(KeyCode::Up));
        assert_eq!(app.input_buffer, "first");

        app.handle_input(KeyEvent::from(KeyCode::Down));
        assert_eq!(app.input_buffer, "second");

        app.handle_input(KeyEvent::from(KeyCode::Down));
        assert_eq!(app.input_buffer, "draft");
        assert_eq!(app.cursor_position, 5);
    }

    #[test]
    fn test_up_down_do_not_trigger_history_for_multiline_input() {
        let mut app = ChatApp::new();
        app.submit_user_message("first".to_string());
        app.submit_user_message("second".to_string());
        app.streaming_state = StreamingState::Idle;

        app.input_buffer = "line one\nline two".to_string();
        app.cursor_position = app.input_buffer.len();

        app.handle_input(KeyEvent::from(KeyCode::Up));
        assert_eq!(app.input_buffer, "line one\nline two");
        assert_eq!(app.history_cursor, None);

        app.handle_input(KeyEvent::from(KeyCode::Down));
        assert_eq!(app.input_buffer, "line one\nline two");
        assert_eq!(app.history_cursor, None);
    }

    #[test]
    fn test_set_messages_rebuilds_input_history_from_user_messages() {
        let mut app = ChatApp::new();
        app.set_messages(vec![
            ChatMessage::assistant("ready".to_string()),
            ChatMessage::user("persisted first".to_string()),
            ChatMessage::assistant("ack".to_string()),
            ChatMessage::user("persisted second".to_string()),
        ]);

        app.handle_input(KeyEvent::from(KeyCode::Up));
        assert_eq!(app.input_buffer, "persisted second");
        app.handle_input(KeyEvent::from(KeyCode::Up));
        assert_eq!(app.input_buffer, "persisted first");
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
    fn test_append_stream_tracks_reasoning_delta() {
        let mut app = ChatApp::new();
        app.messages.push(ChatMessage::user("Hello".to_string()));
        app.messages.push(ChatMessage::assistant_streaming(String::new()));

        app.append_stream("", Some("Thinking step 1. "));
        app.append_stream("", Some("Thinking step 2."));

        assert_eq!(
            app.messages[1].reasoning_content.as_deref(),
            Some("Thinking step 1. Thinking step 2.")
        );
        assert!(app.messages[1].content.is_empty());
    }

    #[test]
    fn test_delta_event_with_reasoning_only_is_rendered() {
        let mut app = ChatApp::new();
        app.submit_user_message("Why?".to_string());

        app.handle_stream_event(IncomingStreamEvent::Delta {
            content: None,
            reasoning_content: Some("Because this is the rationale.".to_string()),
        });

        assert_eq!(
            app.messages[1].reasoning_content.as_deref(),
            Some("Because this is the rationale.")
        );
        assert!(app.messages[1].content.is_empty());
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
    fn test_thinking_event_appends_reasoning_content() {
        let mut app = ChatApp::new();
        app.submit_user_message("Explain your plan".to_string());
        app.handle_stream_event(IncomingStreamEvent::Thinking("Drafting a short plan...".to_string()));

        assert_eq!(app.streaming_state, StreamingState::Thinking);
        assert!(
            app.messages[1]
                .reasoning_content
                .as_deref()
                .is_some_and(|reasoning| reasoning.contains("Drafting a short plan"))
        );
        assert!(app.messages[1].content.is_empty());
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
