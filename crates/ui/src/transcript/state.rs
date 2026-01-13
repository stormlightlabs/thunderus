use crate::transcript::entry::{ApprovalDecision, TranscriptEntry};
use std::collections::VecDeque;

/// Transcript manages a conversation history with entries
///
/// Supports:
/// - Adding entries (user messages, model responses, tool calls, etc.)
/// - Streaming text updates for model responses
/// - Setting approval decisions on pending prompts
/// - Scrolling through history
#[derive(Debug, Clone, PartialEq)]
pub struct Transcript {
    entries: VecDeque<TranscriptEntry>,
    max_entries: usize,
    scroll_offset: usize,
}

impl Transcript {
    /// Create a new transcript with default max entries
    pub fn new() -> Self {
        Self { entries: VecDeque::with_capacity(100), max_entries: 1000, scroll_offset: 0 }
    }

    /// Create a new transcript with custom max entries
    pub fn with_capacity(max_entries: usize) -> Self {
        Self { entries: VecDeque::with_capacity(max_entries.min(100)), max_entries, scroll_offset: 0 }
    }

    /// Add an entry to the transcript
    pub fn add(&mut self, entry: TranscriptEntry) {
        if self.entries.len() >= self.max_entries {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
        self.scroll_to_bottom();
    }

    /// Add a user message
    pub fn add_user_message(&mut self, content: impl Into<String>) {
        self.add(TranscriptEntry::user_message(content));
    }

    /// Add a model response
    pub fn add_model_response(&mut self, content: impl Into<String>) {
        self.add(TranscriptEntry::model_response(content));
    }

    /// Add a streaming model response (appends to last streaming response)
    pub fn add_streaming_token(&mut self, token: &str) {
        if let Some(TranscriptEntry::ModelResponse { content, streaming }) = self.entries.back_mut() {
            content.push_str(token);
            *streaming = true;
        } else {
            self.add(TranscriptEntry::streaming_response(token));
        }
    }

    /// Mark current streaming response as complete
    pub fn finish_streaming(&mut self) {
        if let Some(last) = self.entries.back_mut()
            && let TranscriptEntry::ModelResponse { streaming, .. } = last
        {
            *streaming = false;
        }
    }

    /// Add a tool call
    pub fn add_tool_call(&mut self, tool: impl Into<String>, arguments: impl Into<String>, risk: impl Into<String>) {
        self.add(TranscriptEntry::tool_call(tool, arguments, risk));
    }

    /// Add a tool result
    pub fn add_tool_result(&mut self, tool: impl Into<String>, result: impl Into<String>, success: bool) {
        self.add(TranscriptEntry::tool_result(tool, result, success));
    }

    /// Add an approval prompt
    pub fn add_approval_prompt(&mut self, action: impl Into<String>, risk: impl Into<String>) {
        self.add(TranscriptEntry::approval_prompt(action, risk));
    }

    /// Set decision on pending approval prompt
    pub fn set_approval_decision(&mut self, decision: ApprovalDecision) -> bool {
        for entry in self.entries.iter_mut().rev() {
            if let TranscriptEntry::ApprovalPrompt { decision: dec, .. } = entry
                && dec.is_none()
            {
                *dec = Some(decision);
                return true;
            }
        }
        false
    }

    /// Add a system message
    pub fn add_system_message(&mut self, content: impl Into<String>) {
        self.add(TranscriptEntry::system_message(content));
    }

    /// Get all entries
    pub fn entries(&self) -> &VecDeque<TranscriptEntry> {
        &self.entries
    }

    /// Get number of entries
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if transcript is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get last entry
    pub fn last(&self) -> Option<&TranscriptEntry> {
        self.entries.back()
    }

    /// Get last entry mutably
    pub fn last_mut(&mut self) -> Option<&mut TranscriptEntry> {
        self.entries.back_mut()
    }

    /// Check if there's a pending approval prompt
    pub fn has_pending_approval(&self) -> bool {
        self.entries.iter().any(|e| e.is_pending())
    }

    /// Clear all entries
    pub fn clear(&mut self) {
        self.entries.clear();
        self.scroll_offset = 0;
    }

    /// Scroll to bottom (most recent)
    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
    }

    /// Scroll up
    pub fn scroll_up(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_add(lines);
        self.scroll_offset = self.scroll_offset.min(self.entries.len().saturating_sub(1));
    }

    /// Scroll down
    pub fn scroll_down(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
    }

    /// Get scroll offset
    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    /// Check if scrolled to bottom
    pub fn is_at_bottom(&self) -> bool {
        self.scroll_offset == 0
    }

    /// Get visible entries based on scroll offset
    pub fn visible_entries(&self, max_visible: usize) -> Vec<&TranscriptEntry> {
        if self.entries.is_empty() {
            return vec![];
        }

        let start = self.scroll_offset;
        let end = (start + max_visible).min(self.entries.len());
        let entries_slice: Vec<&TranscriptEntry> = self.entries.iter().collect();
        entries_slice[start..end].to_vec()
    }

    /// Get entries for rendering (handles VecDeque internals)
    pub fn render_entries(&self) -> Vec<&TranscriptEntry> {
        self.entries.iter().collect()
    }
}

impl Default for Transcript {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transcript_new() {
        let transcript = Transcript::new();
        assert!(transcript.is_empty());
        assert_eq!(transcript.len(), 0);
    }

    #[test]
    fn test_transcript_default() {
        let transcript = Transcript::default();
        assert!(transcript.is_empty());
    }

    #[test]
    fn test_add_user_message() {
        let mut transcript = Transcript::new();
        transcript.add_user_message("Hello");
        assert_eq!(transcript.len(), 1);
        assert!(transcript.last().is_some());
    }

    #[test]
    fn test_add_model_response() {
        let mut transcript = Transcript::new();
        transcript.add_model_response("Hi there");
        assert_eq!(transcript.len(), 1);
    }

    #[test]
    fn test_add_tool_call() {
        let mut transcript = Transcript::new();
        transcript.add_tool_call("fs.read", "{ path: '/tmp' }", "safe");
        assert_eq!(transcript.len(), 1);
        assert!(transcript.last().unwrap().is_tool_entry());
    }

    #[test]
    fn test_add_tool_result() {
        let mut transcript = Transcript::new();
        transcript.add_tool_result("fs.read", "content", true);
        assert_eq!(transcript.len(), 1);
        assert!(transcript.last().unwrap().is_tool_entry());
    }

    #[test]
    fn test_add_approval_prompt() {
        let mut transcript = Transcript::new();
        transcript.add_approval_prompt("patch.feature", "risky");
        assert_eq!(transcript.len(), 1);
        assert!(transcript.has_pending_approval());
    }

    #[test]
    fn test_set_approval_decision() {
        let mut transcript = Transcript::new();
        transcript.add_approval_prompt("patch.feature", "risky");

        assert!(transcript.has_pending_approval());
        let success = transcript.set_approval_decision(ApprovalDecision::Approved);
        assert!(success);
        assert!(!transcript.has_pending_approval());
    }

    #[test]
    fn test_streaming_tokens() {
        let mut transcript = Transcript::new();
        transcript.add_streaming_token("Hello");
        transcript.add_streaming_token(" ");
        transcript.add_streaming_token("World");
        assert_eq!(transcript.len(), 1);

        if let TranscriptEntry::ModelResponse { content, streaming, .. } = transcript.last().unwrap() {
            assert_eq!(content, "Hello World");
            assert!(*streaming);
        } else {
            panic!("Expected ModelResponse");
        }
    }

    #[test]
    fn test_streaming_after_other_entry() {
        let mut transcript = Transcript::new();
        transcript.add_user_message("Previous message");
        transcript.add_streaming_token("New");
        assert_eq!(transcript.len(), 2);
    }

    #[test]
    fn test_finish_streaming() {
        let mut transcript = Transcript::new();
        transcript.add_streaming_token("Hello");
        transcript.finish_streaming();

        if let TranscriptEntry::ModelResponse { streaming, .. } = transcript.last().unwrap() {
            assert!(!streaming);
        } else {
            panic!("Expected ModelResponse");
        }
    }

    #[test]
    fn test_clear() {
        let mut transcript = Transcript::new();
        transcript.add_user_message("Hello");
        transcript.add_model_response("Hi");

        assert_eq!(transcript.len(), 2);

        transcript.clear();
        assert!(transcript.is_empty());
        assert_eq!(transcript.len(), 0);
    }

    #[test]
    fn test_scroll_to_bottom() {
        let mut transcript = Transcript::new();
        for i in 0..10 {
            transcript.add_user_message(format!("Message {}", i));
        }
        transcript.scroll_up(5);
        transcript.scroll_to_bottom();

        assert!(transcript.is_at_bottom());
        assert_eq!(transcript.scroll_offset(), 0);
    }

    #[test]
    fn test_max_entries() {
        let mut transcript = Transcript::with_capacity(5);
        for i in 0..10 {
            transcript.add_user_message(format!("Message {}", i));
        }

        assert_eq!(transcript.len(), 5);
        if let TranscriptEntry::UserMessage { content } = transcript.entries().front().unwrap() {
            assert_eq!(content, "Message 5");
        }
    }

    #[test]
    fn test_entry_order() {
        let mut transcript = Transcript::new();
        transcript.add_user_message("First");
        transcript.add_model_response("Response");
        transcript.add_tool_call("tool", "{}", "safe");

        let entries = transcript.entries();
        assert_eq!(entries.len(), 3);
        assert!(matches!(entries[0], TranscriptEntry::UserMessage { .. }));
        assert!(matches!(entries[1], TranscriptEntry::ModelResponse { .. }));
        assert!(matches!(entries[2], TranscriptEntry::ToolCall { .. }));
    }

    #[test]
    fn test_render_entries() {
        let mut transcript = Transcript::new();
        transcript.add_user_message("Hello");
        transcript.add_model_response("Hi");

        let entries = transcript.render_entries();
        assert_eq!(entries.len(), 2);
    }
}
