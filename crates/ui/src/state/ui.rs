use crate::theme::ThemeVariant;

/// Main view modes for the TUI
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MainView {
    /// Standard transcript view
    #[default]
    Transcript,
    /// Inspector view (Trajectory/Provenance)
    Inspector,
}

/// Agent execution status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AgentStatus {
    /// Agent is idle and waiting for input
    #[default]
    Idle,
    /// Agent is currently generating a response or executing tools
    Generating,
    /// Agent execution is paused due to drift or user interruption
    Paused,
    /// Agent is in the reconcile ritual after drift detection
    Reconciling,
}

/// Diff navigation state for tracking selected patch and hunk
#[derive(Debug, Clone, Default)]
pub struct DiffNavigationState {
    /// Index of currently selected patch in the queue
    pub selected_patch_index: Option<usize>,
    /// Index of currently selected hunk within the patch
    pub selected_hunk_index: Option<usize>,
    /// Path of currently selected file within the patch
    pub selected_file_path: Option<String>,
    /// Whether to show detailed hunk view (true) or summary view (false)
    pub show_hunk_details: bool,
    /// Scroll offset within the hunk view
    pub hunk_scroll_offset: u16,
}

#[derive(Debug, Clone, Copy)]
pub enum SidebarAnimation {
    Showing { width: u16 },
    Hiding { width: u16 },
}

impl DiffNavigationState {
    /// Create a new diff navigation state
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset navigation state to initial values
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Navigate to next patch
    pub fn next_patch(&mut self, total_patches: usize) {
        match self.selected_patch_index {
            Some(idx) if idx + 1 < total_patches => {
                self.selected_patch_index = Some(idx + 1);
                self.reset_hunk_selection();
            }
            None if total_patches > 0 => {
                self.selected_patch_index = Some(0);
                self.reset_hunk_selection();
            }
            _ => {}
        }
    }

    /// Navigate to previous patch
    pub fn prev_patch(&mut self, total_patches: usize) {
        match self.selected_patch_index {
            Some(idx) if idx > 0 => {
                self.selected_patch_index = Some(idx - 1);
                self.reset_hunk_selection();
            }
            None if total_patches > 0 => {
                self.selected_patch_index = Some(total_patches - 1);
                self.reset_hunk_selection();
            }
            _ => {}
        }
    }

    /// Navigate to next hunk in current file
    pub fn next_hunk(&mut self, total_hunks: usize) {
        match self.selected_hunk_index {
            Some(idx) if idx + 1 < total_hunks => {
                self.selected_hunk_index = Some(idx + 1);
                self.hunk_scroll_offset = 0;
            }
            None if total_hunks > 0 => {
                self.selected_hunk_index = Some(0);
                self.hunk_scroll_offset = 0;
            }
            _ => {}
        }
    }

    /// Navigate to previous hunk in current file
    pub fn prev_hunk(&mut self, total_hunks: usize) {
        match self.selected_hunk_index {
            Some(idx) if idx > 0 => {
                self.selected_hunk_index = Some(idx - 1);
                self.hunk_scroll_offset = 0;
            }
            None if total_hunks > 0 => {
                self.selected_hunk_index = Some(total_hunks - 1);
                self.hunk_scroll_offset = 0;
            }
            _ => {}
        }
    }

    /// Reset hunk selection when changing patches
    fn reset_hunk_selection(&mut self) {
        self.selected_hunk_index = None;
        self.selected_file_path = None;
        self.hunk_scroll_offset = 0;
    }

    /// Toggle between summary and detailed hunk view
    pub fn toggle_details(&mut self) {
        self.show_hunk_details = !self.show_hunk_details;
    }

    /// Set the selected file path
    pub fn set_selected_file(&mut self, path: String) {
        self.selected_file_path = Some(path);
        self.selected_hunk_index = None;
        self.hunk_scroll_offset = 0;
    }

    /// Scroll within the hunk view
    pub fn scroll_hunk(&mut self, delta: i16) {
        let new_offset = self.hunk_scroll_offset as i16 + delta;
        self.hunk_scroll_offset = new_offset.max(0) as u16;
    }
}

/// UI rendering state
#[derive(Debug, Clone)]
pub struct UIState {
    /// Whether sidebar is visible
    pub sidebar_visible: bool,
    /// Current agent execution status
    pub agent_status: AgentStatus,
    /// Horizontal scroll offset for transcript
    pub scroll_horizontal: u16,
    /// Vertical scroll offset for transcript
    pub scroll_vertical: u16,
    /// Sidebar section collapse state
    pub sidebar_collapse_state: super::SidebarCollapseState,
    /// Sidebar animation state (for slide in/out)
    pub sidebar_animation: Option<SidebarAnimation>,
    /// Diff navigation state
    pub diff_navigation: DiffNavigationState,
    /// Whether this is the first session (show centered welcome view)
    pub is_first_session: bool,
    /// Animation frame counter for streaming ellipsis animation (0-3 cycle)
    pub animation_frame: u8,
    /// Active UI theme variant
    pub theme_variant: ThemeVariant,
    /// Currently active main view
    pub active_view: MainView,
}

impl UIState {
    pub fn new() -> Self {
        Self {
            sidebar_visible: true,
            agent_status: AgentStatus::Idle,
            scroll_horizontal: 0,
            scroll_vertical: 0,
            sidebar_collapse_state: super::SidebarCollapseState::default(),
            sidebar_animation: None,
            diff_navigation: DiffNavigationState::new(),
            is_first_session: true,
            animation_frame: 0,
            theme_variant: ThemeVariant::Iceberg,
            active_view: MainView::Transcript,
        }
    }

    /// Advance animation frame for streaming ellipsis (cycles 0-3)
    pub fn advance_animation_frame(&mut self) {
        self.animation_frame = (self.animation_frame + 1) % 4;
    }

    /// Get current streaming ellipsis based on animation frame
    pub fn streaming_ellipsis(&self) -> &'static str {
        match self.animation_frame {
            0 => "",
            1 => ".",
            2 => "..",
            _ => "...",
        }
    }

    /// Exit first session mode (called when first message is sent)
    pub fn exit_first_session(&mut self) {
        self.is_first_session = false;
    }

    /// Set first session mode (used when reconstructing from empty session)
    pub fn set_first_session(&mut self, value: bool) {
        self.is_first_session = value;
    }

    /// Toggle sidebar visibility
    pub fn toggle_sidebar(&mut self) {
        const SIDEBAR_WIDTH: u16 = 20;

        if self.sidebar_visible {
            self.sidebar_visible = false;
            self.sidebar_animation = Some(SidebarAnimation::Hiding { width: SIDEBAR_WIDTH });
        } else {
            self.sidebar_visible = true;
            self.sidebar_animation = Some(SidebarAnimation::Showing { width: 0 });
        }
    }

    pub fn sidebar_width_override(&self) -> Option<u16> {
        match self.active_view {
            MainView::Inspector => None,
            _ => match self.sidebar_animation {
                Some(SidebarAnimation::Showing { width }) => Some(width),
                Some(SidebarAnimation::Hiding { width }) => Some(width),
                None => {
                    if self.sidebar_visible {
                        Some(20)
                    } else {
                        None
                    }
                }
            },
        }
    }

    pub fn advance_sidebar_animation(&mut self) {
        const SIDEBAR_WIDTH: u16 = 20;
        const STEP: u16 = 4;

        let next = match self.sidebar_animation {
            Some(SidebarAnimation::Showing { width }) => {
                let new_width = (width + STEP).min(SIDEBAR_WIDTH);
                if new_width >= SIDEBAR_WIDTH {
                    self.sidebar_animation = None;
                    return;
                }
                Some(SidebarAnimation::Showing { width: new_width })
            }
            Some(SidebarAnimation::Hiding { width }) => {
                let new_width = width.saturating_sub(STEP);
                if new_width == 0 {
                    self.sidebar_animation = None;
                    return;
                }
                Some(SidebarAnimation::Hiding { width: new_width })
            }
            None => None,
        };

        self.sidebar_animation = next;
    }

    /// Set the current theme variant
    pub fn set_theme_variant(&mut self, variant: ThemeVariant) {
        self.theme_variant = variant;
    }

    /// Toggle theme variant between Iceberg and Oxocarbon
    pub fn toggle_theme_variant(&mut self) {
        self.theme_variant = match self.theme_variant {
            ThemeVariant::Iceberg => ThemeVariant::Oxocarbon,
            ThemeVariant::Oxocarbon => ThemeVariant::Iceberg,
        };
    }

    /// Start generation
    pub fn start_generation(&mut self) {
        self.agent_status = AgentStatus::Generating;
    }

    /// Stop generation
    pub fn stop_generation(&mut self) {
        self.agent_status = AgentStatus::Idle;
    }

    /// Pause generation
    pub fn pause_generation(&mut self) {
        self.agent_status = AgentStatus::Paused;
    }

    /// Start reconcile ritual
    pub fn start_reconcile(&mut self) {
        self.agent_status = AgentStatus::Reconciling;
    }

    /// Check if currently generating
    pub fn is_generating(&self) -> bool {
        self.agent_status == AgentStatus::Generating
    }

    /// Check if currently paused
    pub fn is_paused(&self) -> bool {
        self.agent_status == AgentStatus::Paused
    }

    /// Check if in reconcile ritual
    pub fn is_reconciling(&self) -> bool {
        self.agent_status == AgentStatus::Reconciling
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

    /// Toggle Inspector view
    pub fn toggle_inspector(&mut self) {
        self.active_view = match self.active_view {
            MainView::Transcript => MainView::Inspector,
            MainView::Inspector => MainView::Transcript,
        };
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

    #[test]
    fn test_diff_navigation_new() {
        let nav = DiffNavigationState::new();
        assert!(nav.selected_patch_index.is_none());
        assert!(nav.selected_hunk_index.is_none());
        assert!(nav.selected_file_path.is_none());
        assert!(!nav.show_hunk_details);
        assert_eq!(nav.hunk_scroll_offset, 0);
    }

    #[test]
    fn test_diff_navigation_reset() {
        let mut nav = DiffNavigationState::new();
        nav.selected_patch_index = Some(5);
        nav.selected_hunk_index = Some(2);
        nav.selected_file_path = Some("test.rs".to_string());
        nav.show_hunk_details = true;
        nav.hunk_scroll_offset = 10;

        nav.reset();

        assert!(nav.selected_patch_index.is_none());
        assert!(nav.selected_hunk_index.is_none());
        assert!(nav.selected_file_path.is_none());
        assert!(!nav.show_hunk_details);
        assert_eq!(nav.hunk_scroll_offset, 0);
    }

    #[test]
    fn test_diff_navigation_next_patch() {
        let mut nav = DiffNavigationState::new();
        nav.next_patch(0);
        assert!(nav.selected_patch_index.is_none());

        nav.next_patch(3);
        assert_eq!(nav.selected_patch_index, Some(0));
        assert!(nav.selected_hunk_index.is_none());

        nav.next_patch(3);
        assert_eq!(nav.selected_patch_index, Some(1));

        nav.next_patch(3);
        assert_eq!(nav.selected_patch_index, Some(2));

        nav.next_patch(3);
        assert_eq!(nav.selected_patch_index, Some(2));
    }

    #[test]
    fn test_diff_navigation_prev_patch() {
        let mut nav = DiffNavigationState::new();
        nav.prev_patch(0);
        assert!(nav.selected_patch_index.is_none());

        nav.prev_patch(3);
        assert_eq!(nav.selected_patch_index, Some(2));

        nav.prev_patch(3);
        assert_eq!(nav.selected_patch_index, Some(1));

        nav.prev_patch(3);
        assert_eq!(nav.selected_patch_index, Some(0));

        nav.prev_patch(3);
        assert_eq!(nav.selected_patch_index, Some(0));
    }

    #[test]
    fn test_diff_navigation_next_hunk() {
        let mut nav = DiffNavigationState::new();
        nav.next_hunk(0);
        assert!(nav.selected_hunk_index.is_none());

        nav.next_hunk(5);
        assert_eq!(nav.selected_hunk_index, Some(0));
        assert_eq!(nav.hunk_scroll_offset, 0);

        nav.next_hunk(5);
        assert_eq!(nav.selected_hunk_index, Some(1));

        nav.next_hunk(5);
        assert_eq!(nav.selected_hunk_index, Some(2));

        for _ in 0..10 {
            nav.next_hunk(5);
        }
        assert_eq!(nav.selected_hunk_index, Some(4));
    }

    #[test]
    fn test_diff_navigation_prev_hunk() {
        let mut nav = DiffNavigationState::new();
        nav.prev_hunk(0);
        assert!(nav.selected_hunk_index.is_none());

        nav.prev_hunk(5);
        assert_eq!(nav.selected_hunk_index, Some(4));

        nav.prev_hunk(5);
        assert_eq!(nav.selected_hunk_index, Some(3));

        nav.prev_hunk(5);
        assert_eq!(nav.selected_hunk_index, Some(2));

        nav.prev_hunk(5);
        assert_eq!(nav.selected_hunk_index, Some(1));

        nav.prev_hunk(5);
        assert_eq!(nav.selected_hunk_index, Some(0));

        nav.prev_hunk(5);
        assert_eq!(nav.selected_hunk_index, Some(0));
    }

    #[test]
    fn test_diff_navigation_toggle_details() {
        let mut nav = DiffNavigationState::new();
        assert!(!nav.show_hunk_details);

        nav.toggle_details();
        assert!(nav.show_hunk_details);

        nav.toggle_details();
        assert!(!nav.show_hunk_details);
    }

    #[test]
    fn test_diff_navigation_set_selected_file() {
        let mut nav = DiffNavigationState::new();

        nav.set_selected_file("src/main.rs".to_string());
        assert_eq!(nav.selected_file_path, Some("src/main.rs".to_string()));
        assert!(nav.selected_hunk_index.is_none());
        assert_eq!(nav.hunk_scroll_offset, 0);
    }

    #[test]
    fn test_diff_navigation_scroll_hunk() {
        let mut nav = DiffNavigationState::new();

        nav.scroll_hunk(10);
        assert_eq!(nav.hunk_scroll_offset, 10);

        nav.scroll_hunk(-5);
        assert_eq!(nav.hunk_scroll_offset, 5);

        nav.scroll_hunk(-10);
        assert_eq!(nav.hunk_scroll_offset, 0);

        nav.scroll_hunk(100);
        assert_eq!(nav.hunk_scroll_offset, 100);
    }

    #[test]
    fn test_diff_navigation_patch_change_resets_hunk() {
        let mut nav = DiffNavigationState::new();
        nav.selected_hunk_index = Some(3);
        nav.selected_file_path = Some("test.rs".to_string());
        nav.hunk_scroll_offset = 15;
        nav.next_patch(5);

        assert_eq!(nav.selected_patch_index, Some(0));
        assert!(nav.selected_hunk_index.is_none());
        assert!(nav.selected_file_path.is_none());
        assert_eq!(nav.hunk_scroll_offset, 0);
    }
}
