use super::Transcript;
use crate::transcript::TranscriptEntry;

impl Transcript {
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_render_entries() {
        let mut transcript = Transcript::new();
        transcript.add_user_message("Hello");
        transcript.add_model_response("Hi");

        let entries = transcript.render_entries();
        assert_eq!(entries.len(), 2);
    }
}
