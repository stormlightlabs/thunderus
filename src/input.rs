//! Cursor-aware single-line/multi-line text input model.
//!
//! Wraps a `String` with a char-index cursor position so that text can be
//! inserted and deleted at an arbitrary point, not just appended. All public
//! operations keep the cursor within bounds (`0..=len_chars`).

/// A cursor-aware text buffer used for the prompt input line.
///
/// The cursor is stored as a **char index** (not byte index) so that
/// multi-byte characters are handled correctly. It is always in the range
/// `0..=text.chars().count()`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PromptInput {
    text: String,
    /// Char index of the cursor within `text`.
    cursor: usize,
}

impl PromptInput {
    /// Create an empty input.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create from an existing string, placing the cursor at the end.
    pub fn from_str(s: &str) -> Self {
        let cursor = s.chars().count();
        Self { text: s.to_string(), cursor }
    }

    /// The full text content.
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// The full text content, owned.
    pub fn text(&self) -> String {
        self.text.clone()
    }

    /// Current cursor position as a char index.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Number of characters in the buffer.
    pub fn len_chars(&self) -> usize {
        self.chars().count()
    }

    /// Iterator over the characters.
    pub fn chars(&self) -> std::str::Chars<'_> {
        self.text.chars()
    }

    /// Whether the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Clear all text and reset the cursor.
    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
    }

    /// Set the text to `s`, placing the cursor at the end.
    pub fn set_text(&mut self, s: &str) {
        self.text = s.to_string();
        self.cursor = self.len_chars();
    }

    /// Move the cursor left by one character (clamped at 0).
    pub fn cursor_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    /// Move the cursor right by one character (clamped at end).
    pub fn cursor_right(&mut self) {
        if self.cursor < self.len_chars() {
            self.cursor += 1;
        }
    }

    /// Move the cursor to the start of the line (char index 0).
    pub fn cursor_to_start(&mut self) {
        self.cursor = 0;
    }

    /// Move the cursor to the end of the text.
    pub fn cursor_to_end(&mut self) {
        self.cursor = self.len_chars();
    }

    /// Move the cursor to the previous logical line, preserving the column as
    /// closely as possible. Returns `true` when the cursor moved.
    pub fn cursor_up(&mut self) -> bool {
        let chars: Vec<char> = self.chars().collect();
        let Some((line_start, column)) = line_start_and_column(&chars, self.cursor) else {
            return false;
        };
        if line_start == 0 {
            return false;
        }

        let prev_end = line_start.saturating_sub(1);
        let prev_start = chars[..prev_end]
            .iter()
            .rposition(|ch| *ch == '\n')
            .map_or(0, |idx| idx + 1);
        let prev_len = prev_end.saturating_sub(prev_start);
        self.cursor = prev_start + column.min(prev_len);
        true
    }

    /// Move the cursor to the next logical line, preserving the column as
    /// closely as possible. Returns `true` when the cursor moved.
    pub fn cursor_down(&mut self) -> bool {
        let chars: Vec<char> = self.chars().collect();
        let Some((line_start, column)) = line_start_and_column(&chars, self.cursor) else {
            return false;
        };
        let current_end = chars[line_start..]
            .iter()
            .position(|ch| *ch == '\n')
            .map(|offset| line_start + offset);
        let Some(current_end) = current_end else {
            return false;
        };
        let next_start = current_end + 1;
        let next_end = chars[next_start..]
            .iter()
            .position(|ch| *ch == '\n')
            .map_or(chars.len(), |offset| next_start + offset);
        let next_len = next_end.saturating_sub(next_start);
        self.cursor = next_start + column.min(next_len);
        true
    }

    /// Move the cursor left to the start of the previous word.
    ///
    /// Skips whitespace going backwards, then skips non-whitespace until the
    /// preceding whitespace boundary.
    pub fn cursor_word_left(&mut self) {
        let chars: Vec<char> = self.chars().collect();
        if self.cursor == 0 {
            return;
        }
        let mut i = self.cursor;

        while i > 0 && chars[i - 1].is_whitespace() {
            i -= 1;
        }

        while i > 0 && !chars[i - 1].is_whitespace() {
            i -= 1;
        }
        self.cursor = i;
    }

    /// Move the cursor right to the start of the next word.
    ///
    /// Skips the current word's non-whitespace characters, then skips
    /// whitespace until the next word starts.
    pub fn cursor_word_right(&mut self) {
        let chars: Vec<char> = self.chars().collect();
        let len = chars.len();
        if self.cursor >= len {
            return;
        }
        let mut i = self.cursor;

        while i < len && !chars[i].is_whitespace() {
            i += 1;
        }

        while i < len && chars[i].is_whitespace() {
            i += 1;
        }
        self.cursor = i;
    }

    /// Insert a character at the cursor, then advance the cursor past it.
    pub fn insert_char(&mut self, ch: char) {
        let byte_idx = self.byte_offset_of(self.cursor);
        self.text.insert(byte_idx, ch);
        self.cursor += 1;
    }

    /// Insert a string at the cursor, advancing the cursor to the end of the
    /// inserted text.
    pub fn insert_str(&mut self, s: &str) {
        let count = s.chars().count();
        if count == 0 {
            return;
        }
        let byte_idx = self.byte_offset_of(self.cursor);
        self.text.insert_str(byte_idx, s);
        self.cursor += count;
    }

    /// Replace a character-index range and place the cursor after the inserted text.
    pub fn replace_range(&mut self, start: usize, end: usize, replacement: &str) {
        let len = self.len_chars();
        let start = start.min(len);
        let end = end.min(len).max(start);
        let start_byte = self.byte_offset_of(start);
        let end_byte = self.byte_offset_of(end);
        self.text.replace_range(start_byte..end_byte, replacement);
        self.cursor = start + replacement.chars().count();
    }

    /// Delete the character to the **left** of the cursor (backspace).
    ///
    /// Returns `true` if a character was deleted.
    pub fn backspace(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        let chars: Vec<char> = self.chars().collect();
        let prev = chars[self.cursor - 1];
        let byte_idx = self.byte_offset_of(self.cursor - 1);
        self.text.replace_range(byte_idx..byte_idx + prev.len_utf8(), "");
        self.cursor -= 1;
        true
    }

    /// Delete the character to the **right** of the cursor (forward delete).
    ///
    /// Returns `true` if a character was deleted.
    pub fn delete_forward(&mut self) -> bool {
        let len = self.len_chars();
        if self.cursor >= len {
            return false;
        }
        let chars: Vec<char> = self.chars().collect();
        let cur = chars[self.cursor];
        let byte_idx = self.byte_offset_of(self.cursor);
        self.text.replace_range(byte_idx..byte_idx + cur.len_utf8(), "");
        true
    }

    /// Convert a char index to a byte offset into `text`.
    fn byte_offset_of(&self, char_index: usize) -> usize {
        self.text
            .char_indices()
            .nth(char_index)
            .map(|(byte_idx, _)| byte_idx)
            .unwrap_or_else(|| self.text.len())
    }

    /// The text before the cursor.
    pub fn text_before_cursor(&self) -> &str {
        let byte_idx = self.byte_offset_of(self.cursor);
        &self.text[..byte_idx]
    }

    /// The text from the cursor to the end.
    pub fn text_after_cursor(&self) -> &str {
        let byte_idx = self.byte_offset_of(self.cursor);
        &self.text[byte_idx..]
    }
}

fn line_start_and_column(chars: &[char], cursor: usize) -> Option<(usize, usize)> {
    if cursor > chars.len() {
        return None;
    }
    let line_start = chars[..cursor]
        .iter()
        .rposition(|ch| *ch == '\n')
        .map_or(0, |idx| idx + 1);
    Some((line_start, cursor.saturating_sub(line_start)))
}

impl From<String> for PromptInput {
    fn from(s: String) -> Self {
        Self::from_str(&s)
    }
}

impl From<&str> for PromptInput {
    fn from(s: &str) -> Self {
        Self::from_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_is_empty() {
        let p = PromptInput::new();
        assert!(p.is_empty());
        assert_eq!(p.cursor(), 0);
        assert_eq!(p.as_str(), "");
    }

    #[test]
    fn from_str_places_cursor_at_end() {
        let p = PromptInput::from_str("hello");
        assert_eq!(p.as_str(), "hello");
        assert_eq!(p.cursor(), 5);
    }

    #[test]
    fn insert_char_advances_cursor() {
        let mut p = PromptInput::from_str("helo");
        p.cursor_left();
        assert_eq!(p.cursor(), 3);

        p.insert_char('l');
        assert_eq!(p.as_str(), "hello");
        assert_eq!(p.cursor(), 4);
    }

    #[test]
    fn insert_char_at_start() {
        let mut p = PromptInput::from_str("world");
        p.cursor_to_start();
        p.insert_char('!');
        assert_eq!(p.as_str(), "!world");
        assert_eq!(p.cursor(), 1);
    }

    #[test]
    fn insert_char_at_end() {
        let mut p = PromptInput::from_str("hi");
        p.insert_char('!');
        assert_eq!(p.as_str(), "hi!");
        assert_eq!(p.cursor(), 3);
    }

    #[test]
    fn backspace_deletes_left() {
        let mut p = PromptInput::from_str("hello");
        p.cursor_left();
        assert!(p.backspace());
        assert_eq!(p.as_str(), "helo");
        assert_eq!(p.cursor(), 3);
    }

    #[test]
    fn backspace_at_start_is_noop() {
        let mut p = PromptInput::from_str("hello");
        p.cursor_to_start();
        assert!(!p.backspace());
        assert_eq!(p.as_str(), "hello");
        assert_eq!(p.cursor(), 0);
    }

    #[test]
    fn delete_forward_deletes_right() {
        let mut p = PromptInput::from_str("hello");
        p.cursor_to_start();
        p.cursor_right();
        assert!(p.delete_forward());
        assert_eq!(p.as_str(), "hllo");
        assert_eq!(p.cursor(), 1);
    }

    #[test]
    fn delete_forward_at_end_is_noop() {
        let mut p = PromptInput::from_str("hello");
        assert!(!p.delete_forward());
        assert_eq!(p.as_str(), "hello");
    }

    #[test]
    fn cursor_left_clamped() {
        let mut p = PromptInput::from_str("ab");
        p.cursor_left();
        p.cursor_left();
        p.cursor_left();
        assert_eq!(p.cursor(), 0);
    }

    #[test]
    fn cursor_right_clamped() {
        let mut p = PromptInput::from_str("ab");
        p.cursor_right();
        assert_eq!(p.cursor(), 2);
    }

    #[test]
    fn cursor_to_start_and_end() {
        let mut p = PromptInput::from_str("hello");
        p.cursor_to_start();
        assert_eq!(p.cursor(), 0);
        p.cursor_to_end();
        assert_eq!(p.cursor(), 5);
    }

    #[test]
    fn cursor_up_moves_between_logical_lines() {
        let mut p = PromptInput::from_str("x\n x\n x");

        assert!(p.cursor_up());
        assert_eq!(p.cursor(), 4);

        assert!(p.cursor_up());
        assert_eq!(p.cursor(), 1);

        assert!(!p.cursor_up());
        assert_eq!(p.cursor(), 1);
    }

    #[test]
    fn cursor_down_moves_between_logical_lines() {
        let mut p = PromptInput::from_str("x\n x\n x");
        p.cursor_to_start();
        p.cursor_right();

        assert!(p.cursor_down());
        assert_eq!(p.cursor(), 3);

        assert!(p.cursor_down());
        assert_eq!(p.cursor(), 6);

        assert!(!p.cursor_down());
        assert_eq!(p.cursor(), 6);
    }

    #[test]
    fn cursor_up_and_down_clamp_to_shorter_lines() {
        let mut p = PromptInput::from_str("long\nx\nwide");
        p.cursor_to_start();
        p.cursor_right();
        p.cursor_right();
        p.cursor_right();

        assert!(p.cursor_down());
        assert_eq!(p.cursor(), 6);

        assert!(p.cursor_down());
        assert_eq!(p.cursor(), 8);
    }

    #[test]
    fn word_left_skips_whitespace_then_word() {
        let mut p = PromptInput::from_str("foo bar baz");
        p.cursor_word_left();
        assert_eq!(p.cursor(), 8);

        p.cursor_word_left();
        assert_eq!(p.cursor(), 4);

        p.cursor_word_left();
        assert_eq!(p.cursor(), 0);
    }

    #[test]
    fn word_left_from_within_word() {
        let mut p = PromptInput::from_str("foo bar");
        p.cursor_word_left();
        assert_eq!(p.cursor(), 4);

        p.cursor_left();
        assert_eq!(p.cursor(), 3);

        p.cursor_word_left();
        assert_eq!(p.cursor(), 0);
    }

    #[test]
    fn word_right_skips_word_then_whitespace() {
        let mut p = PromptInput::from_str("foo bar baz");
        p.cursor_to_start();
        p.cursor_word_right();
        assert_eq!(p.cursor(), 4);

        p.cursor_word_right();
        assert_eq!(p.cursor(), 8);

        p.cursor_word_right();
        assert_eq!(p.cursor(), 11);
    }

    #[test]
    fn word_left_multiple_spaces() {
        let mut p = PromptInput::from_str("a   b");
        p.cursor_word_left();
        assert_eq!(p.cursor(), 4);
        p.cursor_word_left();
        assert_eq!(p.cursor(), 0);
    }

    #[test]
    fn insert_str_at_cursor() {
        let mut p = PromptInput::from_str("hello world");
        p.cursor_to_start();
        p.cursor_word_right();
        p.cursor_left();
        p.insert_str(" big");
        assert_eq!(p.as_str(), "hello big world");
        assert_eq!(p.cursor(), 9);
    }

    #[test]
    fn insert_str_empty_is_noop() {
        let mut p = PromptInput::from_str("hi");
        p.insert_str("");
        assert_eq!(p.as_str(), "hi");
        assert_eq!(p.cursor(), 2);
    }

    #[test]
    fn clear_resets_cursor() {
        let mut p = PromptInput::from_str("hello");
        p.clear();
        assert!(p.is_empty());
        assert_eq!(p.cursor(), 0);
    }

    #[test]
    fn set_text_places_cursor_at_end() {
        let mut p = PromptInput::from_str("old");
        p.set_text("new value");
        assert_eq!(p.as_str(), "new value");
        assert_eq!(p.cursor(), 9);
    }

    #[test]
    fn multibyte_char_handling() {
        let mut p = PromptInput::from_str("héllo");
        assert_eq!(p.len_chars(), 5);
        assert_eq!(p.cursor(), 5);

        p.cursor_to_start();
        p.cursor_right();
        p.insert_char('x');

        assert_eq!(p.as_str(), "hxéllo");
        assert_eq!(p.cursor(), 2);
    }

    #[test]
    fn backspace_multibyte() {
        let mut p = PromptInput::from_str("héllo");
        p.cursor_left();
        p.backspace();
        assert_eq!(p.as_str(), "hélo");
        assert_eq!(p.cursor(), 3);
    }

    #[test]
    fn text_before_and_after_cursor() {
        let mut p = PromptInput::from_str("hello world");
        p.cursor_to_start();
        p.cursor_word_right();
        assert_eq!(p.text_before_cursor(), "hello ");
        assert_eq!(p.text_after_cursor(), "world");
    }

    #[test]
    fn insert_newline() {
        let mut p = PromptInput::from_str("line1");
        p.insert_char('\n');
        p.insert_str("line2");
        assert_eq!(p.as_str(), "line1\nline2");
        assert_eq!(p.cursor(), 11);
    }
}
