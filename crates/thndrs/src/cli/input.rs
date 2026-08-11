//! Cursor-aware single-line/multi-line text input model.
//!
//! Wraps a `String` with a **grapheme-cluster index** cursor so that text can
//! be inserted and deleted at an arbitrary point, not just appended. All public
//! operations keep the cursor within bounds (`0..=grapheme_count`).
//!
//! A grapheme cluster is a user-perceived character, specifically a base codepoint
//! plus any combining marks, zero-width joiners, and other sequence constituents.
//! By indexing the cursor in grapheme clusters, cursor movement, backspace,
//! delete, and transpose all operate on what the user sees as a single
//! character, even when that character is composed of multiple Rust `char`s.

pub mod history;

#[cfg(test)]
mod tests;

use std::convert::Infallible;
use std::str::FromStr;

use crossterm::event::{Event, KeyEvent, KeyEventKind, MouseEventKind};
use unicode_segmentation::UnicodeSegmentation;

/// One terminal event after the capture layer has removed release noise and
/// normalized text payloads. Application code should translate this type into
/// semantic actions instead of matching on Crossterm events directly.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TerminalInput {
    /// A key press or repeat, represented as a deterministic press.
    Key(KeyEvent),
    /// Bracketed paste, normalized to LF line endings.
    Paste(String),
    /// A normalized mouse gesture. Unsupported mouse kinds are retained as
    /// `Other` so routing remains deterministic.
    Mouse(MouseInput),
    /// The terminal's current dimensions.
    Resize { width: u16, height: u16 },
    /// The terminal gained focus.
    FocusGained,
    /// The terminal lost focus.
    FocusLost,
}

/// Mouse gestures understood by the bounded UI input layer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MouseInput {
    ScrollUp,
    ScrollDown,
    Other,
}

impl TerminalInput {
    /// Normalize one Crossterm event at the terminal boundary.
    pub fn from_event(event: Event) -> Option<Self> {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Release => None,
            Event::Key(mut key) => {
                // Key repeat is a real edit/navigation request, but downstream
                // routing should not need to branch on terminal capabilities.
                key.kind = KeyEventKind::Press;
                Some(Self::Key(key))
            }
            Event::Paste(text) => Some(Self::Paste(normalize_paste(&text))),
            Event::Mouse(mouse) => Some(Self::Mouse(match mouse.kind {
                MouseEventKind::ScrollUp => MouseInput::ScrollUp,
                MouseEventKind::ScrollDown => MouseInput::ScrollDown,
                _ => MouseInput::Other,
            })),
            Event::Resize(width, height) => Some(Self::Resize { width, height }),
            Event::FocusGained => Some(Self::FocusGained),
            Event::FocusLost => Some(Self::FocusLost),
        }
    }
}

/// Normalize CRLF and CR paste payloads without turning a paste into submits.
pub fn normalize_paste(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// A cursor-aware text buffer used for the prompt input line.
///
/// The cursor is stored as a **grapheme cluster index** so that combining marks,
/// emoji sequences, and other multi-codepoint clusters are treated as a single
/// unit. It is always in the range `0..=grapheme_count`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PromptInput {
    text: String,
    /// Grapheme-cluster index of the cursor within `text`.
    cursor: usize,
}

impl From<String> for PromptInput {
    fn from(s: String) -> Self {
        Self::from_text(&s)
    }
}

impl From<&str> for PromptInput {
    fn from(s: &str) -> Self {
        Self::from_text(s)
    }
}

impl FromStr for PromptInput {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::from_text(s))
    }
}

impl PromptInput {
    /// Create an empty input.
    pub fn new() -> Self {
        Self::default()
    }

    fn from_text(s: &str) -> Self {
        let cursor = s.graphemes(true).count();
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

    /// Current cursor position as a grapheme-cluster index.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Number of grapheme clusters in the buffer.
    pub fn len_graphemes(&self) -> usize {
        self.text.graphemes(true).count()
    }

    /// Iterator over the grapheme clusters.
    pub fn graphemes(&self) -> unicode_segmentation::Graphemes<'_> {
        self.text.graphemes(true)
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
        self.cursor = self.len_graphemes();
    }

    /// Move the cursor left by one grapheme cluster (clamped at 0).
    pub fn cursor_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    /// Move the cursor right by one grapheme cluster (clamped at end).
    pub fn cursor_right(&mut self) {
        if self.cursor < self.len_graphemes() {
            self.cursor += 1;
        }
    }

    /// Move the cursor to the start of the line (grapheme index 0).
    pub fn cursor_to_start(&mut self) {
        self.cursor = 0;
    }

    /// Move the cursor to the end of the text.
    pub fn cursor_to_end(&mut self) {
        self.cursor = self.len_graphemes();
    }

    /// Move the cursor to the previous logical line, preserving the column as
    /// closely as possible. Returns `true` when the cursor moved.
    pub fn cursor_up(&mut self) -> bool {
        let graphemes: Vec<&str> = self.graphemes().collect();
        let Some((line_start, column)) = line_start_and_column(&graphemes, self.cursor) else {
            return false;
        };
        if line_start == 0 {
            return false;
        }

        let prev_end = line_start.saturating_sub(1);
        let prev_start = graphemes[..prev_end]
            .iter()
            .rposition(|g| g.contains('\n'))
            .map_or(0, |idx| idx + 1);
        let prev_len = prev_end.saturating_sub(prev_start);
        self.cursor = prev_start + column.min(prev_len);
        true
    }

    /// Move the cursor to the next logical line, preserving the column as
    /// closely as possible. Returns `true` when the cursor moved.
    pub fn cursor_down(&mut self) -> bool {
        let graphemes: Vec<&str> = self.graphemes().collect();
        let Some((line_start, column)) = line_start_and_column(&graphemes, self.cursor) else {
            return false;
        };
        let current_end = graphemes[line_start..]
            .iter()
            .position(|g| g.contains('\n'))
            .map(|offset| line_start + offset);
        let Some(current_end) = current_end else {
            return false;
        };
        let next_start = current_end + 1;
        let next_end = graphemes[next_start..]
            .iter()
            .position(|g| g.contains('\n'))
            .map_or(graphemes.len(), |offset| next_start + offset);
        let next_len = next_end.saturating_sub(next_start);
        self.cursor = next_start + column.min(next_len);
        true
    }

    /// Move the cursor left to the start of the previous Unicode word.
    ///
    /// Uses `unicode-segmentation` word boundaries so that punctuation,
    /// numbers, and other non-whitespace runs are treated as word units.
    pub fn cursor_word_left(&mut self) {
        let graphemes: Vec<&str> = self.graphemes().collect();
        if self.cursor == 0 {
            return;
        }
        self.cursor = prev_word_boundary(&graphemes, self.cursor);
    }

    /// Move the cursor right to the start of the next Unicode word.
    ///
    /// Uses `unicode-segmentation` word boundaries so that punctuation,
    /// numbers, and other non-whitespace runs are treated as word units.
    pub fn cursor_word_right(&mut self) {
        let graphemes: Vec<&str> = self.graphemes().collect();
        let len = graphemes.len();
        if self.cursor >= len {
            return;
        }
        self.cursor = next_word_boundary(&graphemes, self.cursor);
    }

    /// Insert a character at the cursor, then advance the cursor past it.
    pub fn insert_char(&mut self, ch: char) {
        let mut encoded = [0; 4];
        self.insert_str(ch.encode_utf8(&mut encoded));
    }

    /// Insert a string at the cursor, advancing the cursor to the end of the
    /// inserted text.
    pub fn insert_str(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        let byte_idx = self.byte_offset_of(self.cursor);
        self.text.insert_str(byte_idx, s);
        self.cursor = grapheme_cursor_at_or_after_byte(&self.text, byte_idx + s.len());
    }

    /// Replace a grapheme-cluster range and place the cursor after the inserted text.
    pub fn replace_range(&mut self, start: usize, end: usize, replacement: &str) {
        let len = self.len_graphemes();
        let start = start.min(len);
        let end = end.min(len).max(start);
        let start_byte = self.byte_offset_of(start);
        let end_byte = self.byte_offset_of(end);
        self.text.replace_range(start_byte..end_byte, replacement);
        self.cursor = if replacement.is_empty() {
            start.min(self.len_graphemes())
        } else {
            grapheme_cursor_at_or_after_byte(&self.text, start_byte + replacement.len())
        };
    }

    /// Delete the grapheme cluster to the **left** of the cursor (backspace).
    ///
    /// Returns `true` if a grapheme was deleted.
    pub fn backspace(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        let graphemes: Vec<&str> = self.graphemes().collect();
        let prev = graphemes[self.cursor - 1];
        let byte_idx = self.byte_offset_of(self.cursor - 1);
        self.text.replace_range(byte_idx..byte_idx + prev.len(), "");
        self.cursor -= 1;
        true
    }

    /// Delete the grapheme cluster to the **right** of the cursor (forward delete).
    ///
    /// Returns `true` if a grapheme was deleted.
    pub fn delete_forward(&mut self) -> bool {
        let len = self.len_graphemes();
        if self.cursor >= len {
            return false;
        }
        let graphemes: Vec<&str> = self.graphemes().collect();
        let cur = graphemes[self.cursor];
        let byte_idx = self.byte_offset_of(self.cursor);
        self.text.replace_range(byte_idx..byte_idx + cur.len(), "");
        true
    }

    /// Kill from the cursor to the end of the line. Returns the killed text.
    pub fn kill_to_end_of_line(&mut self) -> String {
        let graphemes: Vec<&str> = self.graphemes().collect();
        let line_end = graphemes[self.cursor..]
            .iter()
            .position(|g| g.contains('\n'))
            .map(|offset| self.cursor + offset)
            .unwrap_or_else(|| self.len_graphemes());
        let byte_idx = self.byte_offset_of(self.cursor);
        let end_byte = self.byte_offset_of(line_end);
        let killed = self.text[byte_idx..end_byte].to_string();
        self.text.replace_range(byte_idx..end_byte, "");
        killed
    }

    /// Kill from the start of the line to the cursor. Returns the killed text.
    pub fn kill_to_start_of_line(&mut self) -> String {
        let graphemes: Vec<&str> = self.graphemes().collect();
        let line_start = graphemes[..self.cursor]
            .iter()
            .rposition(|g| g.contains('\n'))
            .map_or(0, |idx| idx + 1);
        let start_byte = self.byte_offset_of(line_start);
        let end_byte = self.byte_offset_of(self.cursor);
        let killed = self.text[start_byte..end_byte].to_string();
        self.text.replace_range(start_byte..end_byte, "");
        self.cursor = line_start;
        killed
    }

    /// Kill the word before the cursor (unix-word-rubout style). Uses Unicode
    /// word boundaries. Returns the killed text.
    pub fn kill_word_left(&mut self) -> String {
        let graphemes: Vec<&str> = self.graphemes().collect();
        if self.cursor == 0 {
            return String::new();
        }
        let target = prev_word_boundary(&graphemes, self.cursor);
        let start_byte = self.byte_offset_of(target);
        let end_byte = self.byte_offset_of(self.cursor);
        let killed = self.text[start_byte..end_byte].to_string();
        self.text.replace_range(start_byte..end_byte, "");
        self.cursor = target;
        killed
    }

    /// Kill the word after the cursor. Uses Unicode word boundaries.
    /// Returns the killed text.
    pub fn kill_word_right(&mut self) -> String {
        let graphemes: Vec<&str> = self.graphemes().collect();
        let len = graphemes.len();
        if self.cursor >= len {
            return String::new();
        }
        let target = next_word_boundary(&graphemes, self.cursor);
        let start_byte = self.byte_offset_of(self.cursor);
        let end_byte = self.byte_offset_of(target);
        let killed = self.text[start_byte..end_byte].to_string();
        self.text.replace_range(start_byte..end_byte, "");
        killed
    }

    /// Transpose the grapheme clusters at the cursor and just before it.
    /// Returns `true` if a transposition happened.
    ///
    /// At the start of the line, swaps the first two graphemes and moves the
    /// cursor past both. In the middle, swaps the grapheme before and at the
    /// cursor, then advances. At the end, swaps the last two graphemes.
    pub fn transpose_chars(&mut self) -> bool {
        let graphemes: Vec<&str> = self.graphemes().collect();
        if graphemes.len() < 2 {
            return false;
        }
        let (a, b) = if self.cursor >= graphemes.len() {
            (graphemes.len() - 2, graphemes.len() - 1)
        } else if self.cursor == 0 {
            (0, 1)
        } else {
            (self.cursor - 1, self.cursor)
        };
        let byte_a = self.byte_offset_of(a);
        let byte_b = self.byte_offset_of(b);
        let combined = format!("{}{}", graphemes[b], graphemes[a]);
        self.text.replace_range(byte_a..byte_b + graphemes[b].len(), &combined);
        self.cursor = b + 1;
        true
    }

    /// Yank (paste) text at the cursor, advancing the cursor past it.
    pub fn yank(&mut self, text: &str) {
        self.insert_str(text);
    }

    /// Convert a grapheme-cluster index to a byte offset into `text`.
    fn byte_offset_of(&self, grapheme_index: usize) -> usize {
        self.text
            .grapheme_indices(true)
            .nth(grapheme_index)
            .map(|(byte_idx, _)| byte_idx)
            .unwrap_or_else(|| self.text.len())
    }

    /// The text before the cursor.
    pub fn text_before_cursor(&self) -> &str {
        let byte_idx = self.byte_offset_of(self.cursor);
        &self.text[..byte_idx]
    }
}

/// Return the first grapheme boundary at or after a byte offset.
///
/// Inserting combining marks or joiners can merge text across the insertion
/// boundary, so advancing by the inserted text's standalone grapheme count is
/// not safe.
fn grapheme_cursor_at_or_after_byte(text: &str, byte_offset: usize) -> usize {
    text.grapheme_indices(true)
        .enumerate()
        .find_map(|(index, (start, grapheme))| (start + grapheme.len() >= byte_offset).then_some(index + 1))
        .unwrap_or_else(|| text.graphemes(true).count())
}

/// Find the line start (grapheme index) and column (grapheme offset from line
/// start) for the given cursor position.
fn line_start_and_column(graphemes: &[&str], cursor: usize) -> Option<(usize, usize)> {
    if cursor > graphemes.len() {
        return None;
    }
    let line_start = graphemes[..cursor]
        .iter()
        .rposition(|g| g.contains('\n'))
        .map_or(0, |idx| idx + 1);
    Some((line_start, cursor.saturating_sub(line_start)))
}

/// Find the start of the previous Unicode word (non-whitespace segment) at or
/// before `cursor`.
///
/// Uses `unicode-segmentation` word boundaries, skipping whitespace-only
/// segments. For "foo bar baz" at cursor 11, returns 8 (start of "baz").
fn prev_word_boundary(graphemes: &[&str], cursor: usize) -> usize {
    if cursor == 0 {
        return 0;
    }
    let combined: String = graphemes[..cursor].concat();
    let segments: Vec<(usize, &str)> = combined
        .split_word_bound_indices()
        .map(|(byte_off, word)| {
            let g_off = combined[..byte_off].graphemes(true).count();
            (g_off, word)
        })
        .collect();

    for &(start, word) in segments.iter().rev() {
        if start < cursor && !word.trim().is_empty() {
            return start;
        }
    }
    0
}

/// Find the start of the next Unicode word (non-whitespace segment) at or
/// after `cursor`.
///
/// Uses `unicode-segmentation` word boundaries, skipping whitespace-only
/// segments. For "foo bar baz" at cursor 0, returns 4 (start of "bar").
fn next_word_boundary(graphemes: &[&str], cursor: usize) -> usize {
    let len = graphemes.len();
    if cursor >= len {
        return len;
    }
    let combined: String = graphemes[cursor..].concat();
    let segments: Vec<(usize, &str)> = combined
        .split_word_bound_indices()
        .map(|(byte_off, word)| {
            let g_off = combined[..byte_off].graphemes(true).count();
            (g_off, word)
        })
        .collect();

    for &(start, word) in segments.iter() {
        if start > 0 && !word.trim().is_empty() {
            return cursor + start;
        }
    }
    len
}
