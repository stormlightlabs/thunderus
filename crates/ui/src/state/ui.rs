/// UI rendering state
#[derive(Debug, Clone)]
pub struct UIState {
    /// Whether sidebar is visible
    pub sidebar_visible: bool,
    /// Whether the user is currently generating
    pub generating: bool,
    /// Horizontal scroll offset for transcript
    pub scroll_horizontal: u16,
    /// Vertical scroll offset for transcript
    pub scroll_vertical: u16,
    /// Sidebar section collapse state
    pub sidebar_collapse_state: super::SidebarCollapseState,
}

impl UIState {
    pub fn new() -> Self {
        Self {
            sidebar_visible: true,
            generating: false,
            scroll_horizontal: 0,
            scroll_vertical: 0,
            sidebar_collapse_state: super::SidebarCollapseState::default(),
        }
    }

    /// Toggle sidebar visibility
    pub fn toggle_sidebar(&mut self) {
        self.sidebar_visible = !self.sidebar_visible;
    }

    /// Start generation
    pub fn start_generation(&mut self) {
        self.generating = true;
    }

    /// Stop generation
    pub fn stop_generation(&mut self) {
        self.generating = false;
    }

    /// Check if currently generating
    pub fn is_generating(&self) -> bool {
        self.generating
    }

    /// Scroll transcript horizontally
    pub fn scroll_horizontal(&mut self, delta: i16) {
        let new_offset = self.scroll_horizontal as i16 + delta;
        self.scroll_horizontal = new_offset.max(0) as u16;
    }

    /// Scroll transcript vertically
    pub fn scroll_vertical(&mut self, delta: i16) {
        let new_offset = self.scroll_vertical as i16 + delta;
        self.scroll_vertical = new_offset.max(0) as u16;
    }

    /// Reset scroll to top-left
    pub fn reset_scroll(&mut self) {
        self.scroll_horizontal = 0;
        self.scroll_vertical = 0;
    }
}

impl Default for UIState {
    fn default() -> Self {
        Self::new()
    }
}

/// Approval UI state
#[derive(Debug, Clone, Default)]
pub struct ApprovalUIState {
    /// Pending approval (if any)
    pub pending_approval: Option<super::ApprovalState>,
    /// Pending teaching hint (if any)
    pub pending_hint: Option<String>,
}

impl ApprovalUIState {
    /// Show a teaching hint to the user
    ///
    /// This sets a pending hint that will be displayed in a popup until the user
    /// presses any key to dismiss it.
    pub fn show_hint(&mut self, hint: impl Into<String>) {
        self.pending_hint = Some(hint.into());
    }

    /// Dismiss the current teaching hint if one is shown
    pub fn dismiss_hint(&mut self) {
        self.pending_hint = None;
    }

    /// Check if a teaching hint is currently shown
    pub fn has_pending_hint(&self) -> bool {
        self.pending_hint.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scroll_horizontal() {
        let mut state = UIState::default();

        assert_eq!(state.scroll_horizontal, 0);

        state.scroll_horizontal(10);
        assert_eq!(state.scroll_horizontal, 10);

        state.scroll_horizontal(-5);
        assert_eq!(state.scroll_horizontal, 5);

        state.scroll_horizontal(-10);
        assert_eq!(state.scroll_horizontal, 0);
    }

    #[test]
    fn test_scroll_vertical() {
        let mut state = UIState::default();

        assert_eq!(state.scroll_vertical, 0);

        state.scroll_vertical(20);
        assert_eq!(state.scroll_vertical, 20);

        state.scroll_vertical(-10);
        assert_eq!(state.scroll_vertical, 10);

        state.scroll_vertical(-15);
        assert_eq!(state.scroll_vertical, 0);
    }

    #[test]
    fn test_reset_scroll() {
        let mut state = UIState::default();

        state.scroll_horizontal(50);
        state.scroll_vertical(100);

        assert_eq!(state.scroll_horizontal, 50);
        assert_eq!(state.scroll_vertical, 100);

        state.reset_scroll();

        assert_eq!(state.scroll_horizontal, 0);
        assert_eq!(state.scroll_vertical, 0);
    }
}
