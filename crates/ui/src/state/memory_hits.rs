//! Memory hits panel state
//!
//! Manages the state for displaying memory search results in the TUI.

use std::collections::HashSet;
use thunderus_store::SearchHit;

/// State for the memory hits panel
#[derive(Debug, Clone, Default)]
pub struct MemoryHitsState {
    /// Current search results
    pub hits: Vec<SearchHit>,
    /// Selected hit index
    pub selected_index: usize,
    /// Pinned document IDs (added to context set)
    pub pinned_ids: HashSet<String>,
    /// Panel visibility state
    pub visible: bool,
    /// Current search query
    pub query: String,
    /// Search execution time (ms)
    pub search_time_ms: u64,
}

impl MemoryHitsState {
    /// Create a new memory hits state
    pub fn new() -> Self {
        Self::default()
    }

    /// Update the hits from a search result
    pub fn set_hits(&mut self, hits: Vec<SearchHit>, query: String, search_time_ms: u64) {
        let is_empty = hits.is_empty();
        self.hits = hits;
        self.query = query;
        self.search_time_ms = search_time_ms;
        self.selected_index = 0;
        self.visible = !is_empty;
    }

    /// Clear all hits and hide the panel
    pub fn clear(&mut self) {
        self.hits.clear();
        self.query.clear();
        self.selected_index = 0;
        self.visible = false;
    }

    /// Check if the panel is visible
    pub fn is_visible(&self) -> bool {
        self.visible && !self.hits.is_empty()
    }

    /// Get the currently selected hit
    pub fn selected_hit(&self) -> Option<&SearchHit> {
        self.hits.get(self.selected_index)
    }

    /// Select the next hit
    pub fn select_next(&mut self) {
        if !self.hits.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.hits.len();
        }
    }

    /// Select the previous hit
    pub fn select_prev(&mut self) {
        if !self.hits.is_empty() {
            self.selected_index = if self.selected_index == 0 { self.hits.len() - 1 } else { self.selected_index - 1 };
        }
    }

    /// Check if a document is pinned
    pub fn is_pinned(&self, id: &str) -> bool {
        self.pinned_ids.contains(id)
    }

    /// Pin a document
    pub fn pin(&mut self, id: String) {
        self.pinned_ids.insert(id);
    }

    /// Unpin a document
    pub fn unpin(&mut self, id: &str) {
        self.pinned_ids.remove(id);
    }

    /// Toggle pin state for a document
    pub fn toggle_pin(&mut self, id: &str) {
        if self.is_pinned(id) {
            self.unpin(id);
        } else {
            self.pin(id.to_string());
        }
    }

    /// Get the number of pinned documents
    pub fn pinned_count(&self) -> usize {
        self.pinned_ids.len()
    }

    /// Get all pinned document IDs
    pub fn pinned_ids(&self) -> &HashSet<String> {
        &self.pinned_ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use thunderus_core::memory::MemoryKind;

    fn create_test_hit(id: &str, title: &str) -> SearchHit {
        SearchHit {
            id: id.to_string(),
            kind: MemoryKind::Fact,
            title: title.to_string(),
            path: format!("semantic/FACTS/{}.md", id),
            anchor: None,
            snippet: format!("Test snippet for {}", title),
            score: -5.0,
            event_ids: vec![],
        }
    }

    #[test]
    fn test_memory_hits_state_new() {
        let state = MemoryHitsState::new();
        assert!(!state.is_visible());
        assert_eq!(state.selected_index, 0);
        assert!(state.hits.is_empty());
    }

    #[test]
    fn test_set_hits() {
        let mut state = MemoryHitsState::new();
        let hits = vec![create_test_hit("test-1", "Test 1"), create_test_hit("test-2", "Test 2")];
        state.set_hits(hits, "test query".to_string(), 15);
        assert!(state.is_visible());
        assert_eq!(state.hits.len(), 2);
        assert_eq!(state.query, "test query");
        assert_eq!(state.search_time_ms, 15);
        assert_eq!(state.selected_index, 0);
    }

    #[test]
    fn test_clear() {
        let mut state = MemoryHitsState::new();
        let hits = vec![create_test_hit("test-1", "Test 1")];
        state.set_hits(hits, "query".to_string(), 10);
        state.pin("test-1".to_string());

        state.clear();
        assert!(!state.is_visible());
        assert!(state.hits.is_empty());
        assert!(state.query.is_empty());
        assert_eq!(state.selected_index, 0);
    }

    #[test]
    fn test_selected_hit() {
        let mut state = MemoryHitsState::new();
        assert!(state.selected_hit().is_none());

        let hits = vec![create_test_hit("test-1", "Test 1")];
        state.set_hits(hits, "query".to_string(), 10);
        assert!(state.selected_hit().is_some());
        assert_eq!(state.selected_hit().unwrap().id, "test-1");
    }

    #[test]
    fn test_select_navigation() {
        let mut state = MemoryHitsState::new();
        let hits = vec![
            create_test_hit("test-1", "Test 1"),
            create_test_hit("test-2", "Test 2"),
            create_test_hit("test-3", "Test 3"),
        ];
        state.set_hits(hits, "query".to_string(), 10);
        assert_eq!(state.selected_index, 0);

        state.select_next();
        assert_eq!(state.selected_index, 1);

        state.select_next();
        assert_eq!(state.selected_index, 2);

        state.select_next();
        assert_eq!(state.selected_index, 0);

        state.select_prev();
        assert_eq!(state.selected_index, 2);
    }

    #[test]
    fn test_pin_unpin() {
        let mut state = MemoryHitsState::new();
        assert!(!state.is_pinned("test-1"));

        state.pin("test-1".to_string());
        assert!(state.is_pinned("test-1"));
        assert_eq!(state.pinned_count(), 1);

        state.unpin("test-1");
        assert!(!state.is_pinned("test-1"));
        assert_eq!(state.pinned_count(), 0);
    }

    #[test]
    fn test_toggle_pin() {
        let mut state = MemoryHitsState::new();
        assert!(!state.is_pinned("test-1"));

        state.toggle_pin("test-1");
        assert!(state.is_pinned("test-1"));

        state.toggle_pin("test-1");
        assert!(!state.is_pinned("test-1"));
    }

    #[test]
    fn test_pinned_ids() {
        let mut state = MemoryHitsState::new();
        state.pin("test-1".to_string());
        state.pin("test-2".to_string());

        assert_eq!(state.pinned_count(), 2);
        assert!(state.pinned_ids().contains("test-1"));
        assert!(state.pinned_ids().contains("test-2"));
    }

    #[test]
    fn test_empty_hits_not_visible() {
        let mut state = MemoryHitsState::new();
        state.set_hits(vec![], "query".to_string(), 10);
        assert!(!state.is_visible());
    }
}
