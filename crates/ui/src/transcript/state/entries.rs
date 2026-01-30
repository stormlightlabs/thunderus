use super::Transcript;
use crate::transcript::{ErrorType, TranscriptEntry};
use thunderus_core::ApprovalDecision;

impl Transcript {
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

    /// Mark current streaming response as cancelled, preserving partial output.
    pub fn mark_streaming_cancelled(&mut self, message: impl Into<String>) {
        if let Some(TranscriptEntry::ModelResponse { content, streaming }) = self.entries.back_mut()
            && *streaming
        {
            if !content.ends_with(" [cancelled]") {
                content.push_str(" [cancelled]");
            }
            *streaming = false;
            return;
        }

        self.add_cancellation_error(message);
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

    /// Add an error entry
    pub fn add_error(&mut self, message: impl Into<String>, error_type: ErrorType) {
        self.add(TranscriptEntry::error_entry(message, error_type));
    }

    /// Add a provider error with context
    pub fn add_provider_error(&mut self, message: impl Into<String>, context: Option<String>) {
        let entry =
            TranscriptEntry::error_entry(message, ErrorType::Provider).with_error_context(context.unwrap_or_default());
        self.add(entry);
    }

    /// Add a network error with retry option
    pub fn add_network_error(&mut self, message: impl Into<String>, context: Option<String>) {
        let entry =
            TranscriptEntry::error_entry(message, ErrorType::Network).with_error_context(context.unwrap_or_default());
        self.add(entry);
    }

    /// Add a cancellation error
    pub fn add_cancellation_error(&mut self, message: impl Into<String>) {
        self.add(TranscriptEntry::error_entry(message, ErrorType::Cancelled));
    }

    /// Get all entries
    pub fn entries(&self) -> &std::collections::VecDeque<TranscriptEntry> {
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
        self.focused_card_index = None;
    }

    /// Truncate entries from a specific index (for fork mode)
    ///
    /// Keeps all entries before the given index and removes everything from it onward.
    /// Used when forking from a previous point in conversation history.
    pub fn truncate_from(&mut self, index: usize) {
        if index < self.entries.len() {
            self.entries.truncate(index);
            self.scroll_offset = 0;
            self.focused_card_index = None;
        }
    }

    /// Get all user messages from transcript for history navigation
    pub fn get_user_messages(&self) -> Vec<String> {
        self.entries
            .iter()
            .filter_map(|entry| {
                if let TranscriptEntry::UserMessage { content } = entry {
                    if !content.trim().is_empty() { Some(content.clone()) } else { None }
                } else {
                    None
                }
            })
            .collect()
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
    fn test_get_user_messages() {
        let mut transcript = Transcript::new();
        transcript.add_user_message("First message");
        transcript.add_model_response("Response 1");
        transcript.add_user_message("Second message");
        transcript.add_tool_call("tool", "{}", "safe");
        transcript.add_user_message("");
        transcript.add_user_message("   ");
        transcript.add_user_message("Third message");
        transcript.add_system_message("System message");

        let user_messages = transcript.get_user_messages();
        assert_eq!(user_messages.len(), 3);
        assert_eq!(user_messages[0], "First message");
        assert_eq!(user_messages[1], "Second message");
        assert_eq!(user_messages[2], "Third message");
    }

    #[test]
    fn test_get_user_messages_empty() {
        let transcript = Transcript::new();
        let user_messages = transcript.get_user_messages();
        assert!(user_messages.is_empty());
    }

    #[test]
    fn test_get_user_messages_no_user_messages() {
        let mut transcript = Transcript::new();
        transcript.add_model_response("Response");
        transcript.add_tool_call("tool", "{}", "safe");
        transcript.add_system_message("System");

        let user_messages = transcript.get_user_messages();
        assert!(user_messages.is_empty());
    }

    #[test]
    fn test_get_user_messages_with_max_entries() {
        let mut transcript = Transcript::with_capacity(5);

        for i in 0..10 {
            transcript.add_user_message(format!("Message {}", i));
        }

        let user_messages = transcript.get_user_messages();
        assert_eq!(user_messages.len(), 5);
        assert_eq!(user_messages[0], "Message 5");
        assert_eq!(user_messages[4], "Message 9");
    }
}
