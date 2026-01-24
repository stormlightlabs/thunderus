//! Evidence state for the Inspector
//!
//! Manages the trajectory of events that lead to a memory assertion.

use thunderus_core::trajectory::TrajectoryNode;

/// State for the Inspector evidence view
#[derive(Debug, Clone, Default)]
pub struct EvidenceState {
    /// Chain of evidence (trajectory nodes)
    pub nodes: Vec<TrajectoryNode>,
    /// Selected node index in the list
    pub selected_index: usize,
    /// Vertical scroll offset for detail view
    pub detail_scroll: u16,
}

impl EvidenceState {
    /// Create a new evidence state
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the evidence nodes
    pub fn set_nodes(&mut self, nodes: Vec<TrajectoryNode>) {
        self.nodes = nodes;
        self.selected_index = 0;
        self.detail_scroll = 0;
    }

    /// Clear evidence
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.selected_index = 0;
        self.detail_scroll = 0;
    }

    /// Get the currently selected node
    pub fn selected_node(&self) -> Option<&TrajectoryNode> {
        self.nodes.get(self.selected_index)
    }

    /// Select the next node
    pub fn select_next(&mut self) {
        if !self.nodes.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.nodes.len();
            self.detail_scroll = 0;
        }
    }

    /// Select the previous node
    pub fn select_prev(&mut self) {
        if !self.nodes.is_empty() {
            self.selected_index = if self.selected_index == 0 { self.nodes.len() - 1 } else { self.selected_index - 1 };
            self.detail_scroll = 0;
        }
    }

    /// Scroll detail view down
    pub fn scroll_down(&mut self) {
        self.detail_scroll = self.detail_scroll.saturating_add(1);
    }

    /// Scroll detail view up
    pub fn scroll_up(&mut self) {
        self.detail_scroll = self.detail_scroll.saturating_sub(1);
    }
}
