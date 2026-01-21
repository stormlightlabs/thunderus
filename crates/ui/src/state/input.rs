/// State for the input composer
#[derive(Debug, Clone, Default)]
pub struct InputState {
    /// Current input buffer
    pub buffer: String,
    /// Cursor position
    pub cursor: usize,
    /// Message history for navigation
    pub message_history: Vec<String>,
    /// Current position in history (None = new message)
    pub history_index: Option<usize>,
    /// Temporary buffer for new message while navigating history
    pub temp_buffer: Option<String>,
}

impl InputState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_char(&mut self, c: char) {
        self.buffer.insert(self.cursor, c);
        self.cursor += 1;
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 && !self.buffer.is_empty() {
            self.cursor -= 1;
            self.buffer.remove(self.cursor);
        }
    }

    pub fn delete(&mut self) {
        if self.cursor < self.buffer.len() {
            self.buffer.remove(self.cursor);
        }
    }

    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn move_right(&mut self) {
        if self.cursor < self.buffer.len() {
            self.cursor += 1;
        }
    }

    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.buffer.len();
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
        self.cursor = 0;
    }

    pub fn take(&mut self) -> String {
        let buffer = std::mem::take(&mut self.buffer);
        self.cursor = 0;
        buffer
    }

    /// Add a message to history (typically called after sending a message)
    pub fn add_to_history(&mut self, message: String) {
        if let Some(last) = self.message_history.last()
            && last == &message
        {
            return;
        }
        self.message_history.push(message);
        self.reset_history_navigation();
    }

    /// Navigate up in history (older messages)
    pub fn navigate_up(&mut self) {
        if self.message_history.is_empty() {
            return;
        }

        if self.history_index.is_none() && !self.buffer.is_empty() {
            self.temp_buffer = Some(self.buffer.clone());
        }

        let new_index = match self.history_index {
            None => self.message_history.len().saturating_sub(1),
            Some(idx) => idx.saturating_sub(1),
        };

        if let Some(message) = self.message_history.get(new_index) {
            self.buffer = message.clone();
            self.cursor = self.buffer.len();
            self.history_index = Some(new_index);
        }
    }

    /// Navigate down in history (newer messages)
    pub fn navigate_down(&mut self) {
        if self.message_history.is_empty() {
            return;
        }

        match self.history_index {
            None => (),
            Some(idx) => {
                if idx + 1 >= self.message_history.len() {
                    self.buffer = self.temp_buffer.take().unwrap_or_default();
                    self.cursor = self.buffer.len();
                    self.history_index = None;
                } else {
                    let new_index = idx + 1;
                    if let Some(message) = self.message_history.get(new_index) {
                        self.buffer = message.clone();
                        self.cursor = self.buffer.len();
                        self.history_index = Some(new_index);
                    }
                }
            }
        }
    }

    /// Reset history navigation state (called when user starts typing new message)
    pub fn reset_history_navigation(&mut self) {
        self.history_index = None;
        self.temp_buffer = None;
    }

    /// Check if currently navigating history
    pub fn is_navigating_history(&self) -> bool {
        self.history_index.is_some()
    }

    /// Get current history position indicator for UI display
    pub fn history_position(&self) -> Option<String> {
        self.history_index.map(|idx| {
            let total = self.message_history.len();
            format!("{}/{}", idx + 1, total)
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_state() {
        let mut input = InputState::new();

        input.insert_char('H');
        assert_eq!(input.buffer, "H");
        assert_eq!(input.cursor, 1);

        input.insert_char('i');
        assert_eq!(input.buffer, "Hi");
        assert_eq!(input.cursor, 2);

        input.backspace();
        assert_eq!(input.buffer, "H");
        assert_eq!(input.cursor, 1);

        input.move_home();
        assert_eq!(input.cursor, 0);

        input.move_end();
        assert_eq!(input.cursor, 1);

        let taken = input.take();
        assert_eq!(taken, "H");
        assert_eq!(input.buffer, "");
        assert_eq!(input.cursor, 0);
    }

    #[test]
    fn test_input_state_navigation() {
        let mut input = InputState::new();

        input.insert_char('A');
        input.insert_char('B');
        input.insert_char('C');

        assert_eq!(input.buffer, "ABC");
        assert_eq!(input.cursor, 3);

        input.move_left();
        assert_eq!(input.cursor, 2);

        input.move_left();
        assert_eq!(input.cursor, 1);

        input.insert_char('X');
        assert_eq!(input.buffer, "AXBC");
        assert_eq!(input.cursor, 2);

        input.delete();
        assert_eq!(input.buffer, "AXC");
        assert_eq!(input.cursor, 2);
    }

    #[test]
    fn test_input_state_history_navigation() {
        let mut input = InputState::new();

        input.navigate_up();
        input.navigate_down();
        assert_eq!(input.buffer, "");
        assert!(input.history_index.is_none());

        input.add_to_history("first message".to_string());
        input.add_to_history("second message".to_string());
        input.add_to_history("third message".to_string());

        input.buffer = "current new message".to_string();
        input.cursor = input.buffer.len();

        input.navigate_up();
        assert_eq!(input.buffer, "third message");
        assert_eq!(input.history_index, Some(2));
        assert_eq!(input.temp_buffer, Some("current new message".to_string()));

        input.navigate_up();
        assert_eq!(input.buffer, "second message");
        assert_eq!(input.history_index, Some(1));

        input.navigate_up();
        assert_eq!(input.buffer, "first message");
        assert_eq!(input.history_index, Some(0));

        input.navigate_up();
        assert_eq!(input.buffer, "first message");
        assert_eq!(input.history_index, Some(0));

        input.navigate_down();
        assert_eq!(input.buffer, "second message");
        assert_eq!(input.history_index, Some(1));

        input.navigate_down();
        assert_eq!(input.buffer, "third message");
        assert_eq!(input.history_index, Some(2));

        input.navigate_down();
        assert_eq!(input.buffer, "current new message");
        assert_eq!(input.history_index, None);
        assert_eq!(input.temp_buffer, None);

        input.navigate_down();
        assert_eq!(input.buffer, "current new message");
        assert_eq!(input.history_index, None);
    }

    #[test]
    fn test_input_state_history_without_temp_buffer() {
        let mut input = InputState::new();

        input.add_to_history("single message".to_string());
        input.navigate_up();

        assert_eq!(input.buffer, "single message");
        assert_eq!(input.history_index, Some(0));
        assert!(input.temp_buffer.is_none());

        input.navigate_down();
        assert_eq!(input.buffer, "");
        assert_eq!(input.history_index, None);
    }

    #[test]
    fn test_input_state_add_to_history_prevents_duplicates() {
        let mut input = InputState::new();

        input.add_to_history("test message".to_string());
        input.add_to_history("test message".to_string());
        input.add_to_history("different message".to_string());

        assert_eq!(input.message_history.len(), 2);
        assert_eq!(input.message_history[0], "test message");
        assert_eq!(input.message_history[1], "different message");
    }

    #[test]
    fn test_input_state_reset_history_navigation() {
        let mut input = InputState::new();

        input.add_to_history("message".to_string());

        input.buffer = "current message".to_string();
        input.navigate_up();
        assert!(input.is_navigating_history());
        assert!(input.temp_buffer.is_some());

        input.reset_history_navigation();
        assert!(!input.is_navigating_history());
        assert!(input.temp_buffer.is_none());
    }

    #[test]
    fn test_input_state_history_position() {
        let mut input = InputState::new();

        assert!(input.history_position().is_none());

        input.add_to_history("first".to_string());
        input.add_to_history("second".to_string());
        input.add_to_history("third".to_string());

        input.navigate_up();
        assert_eq!(input.history_position(), Some("3/3".to_string()));

        input.navigate_up();
        assert_eq!(input.history_position(), Some("2/3".to_string()));

        input.navigate_up();
        assert_eq!(input.history_position(), Some("1/3".to_string()));

        input.navigate_down();
        assert_eq!(input.history_position(), Some("2/3".to_string()));

        input.navigate_down();
        assert_eq!(input.history_position(), Some("3/3".to_string()));

        input.navigate_down();
        assert!(input.history_position().is_none());
    }

    #[test]
    fn test_input_state_edit_history_message() {
        let mut input = InputState::new();

        input.add_to_history("original message".to_string());
        input.navigate_up();

        input.buffer = "modified message".to_string();
        input.cursor = input.buffer.len();

        let sent = input.take();
        assert_eq!(sent, "modified message");

        input.add_to_history(sent);
        assert_eq!(input.message_history.last(), Some(&"modified message".to_string()));
    }
}
