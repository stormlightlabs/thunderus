mod entries;
mod focus;
mod scroll;

use crate::transcript::entry::TranscriptEntry;

use std::collections::VecDeque;

/// Transcript manages a conversation history with entries
///
/// Supports:
/// - Adding entries (user messages, model responses, tool calls, etc.)
/// - Streaming text updates for model responses
/// - Setting approval decisions on pending prompts
/// - Scrolling through history
/// - Focusing and navigating between action cards
#[derive(Debug, Clone, PartialEq)]
pub struct Transcript {
    entries: VecDeque<TranscriptEntry>,
    max_entries: usize,
    scroll_offset: usize,
    focused_card_index: Option<usize>,
}

impl Transcript {
    /// Create a new transcript with default max entries
    pub fn new() -> Self {
        Self { entries: VecDeque::with_capacity(100), max_entries: 1000, scroll_offset: 0, focused_card_index: None }
    }

    /// Create a new transcript with custom max entries
    pub fn with_capacity(max_entries: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(max_entries.min(100)),
            max_entries,
            scroll_offset: 0,
            focused_card_index: None,
        }
    }
}

impl Default for Transcript {
    fn default() -> Self {
        Self::new()
    }
}
