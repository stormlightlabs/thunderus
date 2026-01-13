use std::path::PathBuf;
use thunderus_core::{ApprovalMode, ProviderConfig, SandboxMode, TokensUsed};

/// State for the input composer
#[derive(Debug, Clone, Default)]
pub struct InputState {
    /// Current input buffer
    pub buffer: String,
    /// Cursor position
    pub cursor: usize,
    /// Message history for navigation
    pub message_history: Vec<String>,
    /// Current position in history (None = new message)
    pub history_index: Option<usize>,
    /// Temporary buffer for new message while navigating history
    pub temp_buffer: Option<String>,
}

impl InputState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_char(&mut self, c: char) {
        self.buffer.insert(self.cursor, c);
        self.cursor += 1;
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 && !self.buffer.is_empty() {
            self.cursor -= 1;
            self.buffer.remove(self.cursor);
        }
    }

    pub fn delete(&mut self) {
        if self.cursor < self.buffer.len() {
            self.buffer.remove(self.cursor);
        }
    }

    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn move_right(&mut self) {
        if self.cursor < self.buffer.len() {
            self.cursor += 1;
        }
    }

    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.buffer.len();
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
        self.cursor = 0;
    }

    pub fn take(&mut self) -> String {
        let buffer = std::mem::take(&mut self.buffer);
        self.cursor = 0;
        buffer
    }

    /// Add a message to history (typically called after sending a message)
    pub fn add_to_history(&mut self, message: String) {
        if let Some(last) = self.message_history.last()
            && last == &message
        {
            return;
        }
        self.message_history.push(message);
        self.reset_history_navigation();
    }

    /// Navigate up in history (older messages)
    pub fn navigate_up(&mut self) {
        if self.message_history.is_empty() {
            return;
        }

        if self.history_index.is_none() && !self.buffer.is_empty() {
            self.temp_buffer = Some(self.buffer.clone());
        }

        let new_index = match self.history_index {
            None => self.message_history.len().saturating_sub(1),
            Some(idx) => idx.saturating_sub(1),
        };

        if let Some(message) = self.message_history.get(new_index) {
            self.buffer = message.clone();
            self.cursor = self.buffer.len();
            self.history_index = Some(new_index);
        }
    }

    /// Navigate down in history (newer messages)
    pub fn navigate_down(&mut self) {
        if self.message_history.is_empty() {
            return;
        }

        match self.history_index {
            None => (),
            Some(idx) => {
                if idx + 1 >= self.message_history.len() {
                    self.buffer = self.temp_buffer.take().unwrap_or_default();
                    self.cursor = self.buffer.len();
                    self.history_index = None;
                } else {
                    let new_index = idx + 1;
                    if let Some(message) = self.message_history.get(new_index) {
                        self.buffer = message.clone();
                        self.cursor = self.buffer.len();
                        self.history_index = Some(new_index);
                    }
                }
            }
        }
    }

    /// Reset history navigation state (called when user starts typing new message)
    pub fn reset_history_navigation(&mut self) {
        self.history_index = None;
        self.temp_buffer = None;
    }

    /// Check if currently navigating history
    pub fn is_navigating_history(&self) -> bool {
        self.history_index.is_some()
    }

    /// Get current history position indicator for UI display
    pub fn history_position(&self) -> Option<String> {
        self.history_index.map(|idx| {
            let total = self.message_history.len();
            format!("{}/{}", idx + 1, total)
        })
    }
}

/// Session statistics for the UI
#[derive(Debug, Clone, Default)]
pub struct SessionStats {
    /// Total input tokens used
    pub input_tokens: u32,
    /// Total output tokens used
    pub output_tokens: u32,
    /// Number of approval gates triggered
    pub approval_gates: u32,
    /// Number of tools executed
    pub tools_executed: u32,
}

impl SessionStats {
    pub fn total_tokens(&self) -> u32 {
        self.input_tokens + self.output_tokens
    }

    pub fn add_tokens(&mut self, tokens: &TokensUsed) {
        self.input_tokens += tokens.input;
        self.output_tokens += tokens.output;
    }

    pub fn increment_approval_gate(&mut self) {
        self.approval_gates += 1;
    }

    pub fn increment_tools_executed(&mut self) {
        self.tools_executed += 1;
    }
}

/// Approval state for pending approvals
#[derive(Debug, Clone)]
pub struct ApprovalState {
    /// Pending approval action
    pub action: String,
    /// Risk level
    pub risk: String,
    /// Description
    pub description: Option<String>,
    /// User's decision
    pub decision: Option<bool>, // Some(true) = approved, Some(false) = rejected, None = pending
}

/// Verbosity levels for TUI display
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VerbosityLevel {
    /// Minimal output, essential information only
    #[default]
    Quiet,
    /// Balanced output (default)
    Default,
    /// Detailed output with all information
    Verbose,
}

impl VerbosityLevel {
    pub const VALUES: &[VerbosityLevel] = &[VerbosityLevel::Quiet, VerbosityLevel::Default, VerbosityLevel::Verbose];

    pub fn as_str(&self) -> &'static str {
        match self {
            VerbosityLevel::Quiet => "quiet",
            VerbosityLevel::Default => "default",
            VerbosityLevel::Verbose => "verbose",
        }
    }

    pub fn toggle(&mut self) {
        *self = match self {
            VerbosityLevel::Quiet => VerbosityLevel::Default,
            VerbosityLevel::Default => VerbosityLevel::Verbose,
            VerbosityLevel::Verbose => VerbosityLevel::Quiet,
        }
    }
}

/// Session event for sidebar display
#[derive(Debug, Clone)]
pub struct SessionEvent {
    /// Event type
    pub event_type: String,
    /// Event message
    pub message: String,
    /// Timestamp (simplified as string for now)
    pub timestamp: String,
}

/// Modified file information
#[derive(Debug, Clone)]
pub struct ModifiedFile {
    /// File path
    pub path: String,
    /// Modification type
    pub mod_type: String, // "edited", "created", "deleted"
}

/// Git diff entry for sidebar display
#[derive(Debug, Clone)]
pub struct GitDiff {
    /// File path
    pub path: String,
    /// Number of lines added
    pub added: usize,
    /// Number of lines deleted
    pub deleted: usize,
}

impl ApprovalState {
    pub fn pending(action: String, risk: String) -> Self {
        Self { action, risk, description: None, decision: None }
    }

    pub fn is_pending(&self) -> bool {
        self.decision.is_none()
    }

    pub fn approve(&mut self) {
        self.decision = Some(true);
    }

    pub fn reject(&mut self) {
        self.decision = Some(false);
    }
}

/// Main application state
#[derive(Debug, Clone)]
pub struct AppState {
    /// Current working directory
    pub cwd: PathBuf,
    /// Profile name
    pub profile: String,
    /// Provider configuration
    pub provider: ProviderConfig,
    /// Approval mode
    pub approval_mode: ApprovalMode,
    /// Sandbox mode
    pub sandbox_mode: SandboxMode,
    /// Verbosity level
    pub verbosity: VerbosityLevel,
    /// Git branch (if in a git repo)
    pub git_branch: Option<String>,
    /// Session statistics
    pub stats: SessionStats,
    /// Input composer state
    pub input: InputState,
    /// Pending approval (if any)
    pub pending_approval: Option<ApprovalState>,
    /// Whether sidebar is visible
    pub sidebar_visible: bool,
    /// Whether the user is currently generating
    pub generating: bool,
    /// Session events for sidebar
    pub session_events: Vec<SessionEvent>,
    /// Modified files list
    pub modified_files: Vec<ModifiedFile>,
    /// Git diff queue
    pub git_diff_queue: Vec<GitDiff>,
    /// Horizontal scroll offset for transcript
    pub scroll_horizontal: u16,
    /// Vertical scroll offset for transcript
    pub scroll_vertical: u16,
}

impl AppState {
    pub fn new(
        cwd: PathBuf, profile: String, provider: ProviderConfig, approval_mode: ApprovalMode, sandbox_mode: SandboxMode,
    ) -> Self {
        Self {
            cwd,
            profile,
            provider,
            approval_mode,
            sandbox_mode,
            verbosity: VerbosityLevel::default(),
            git_branch: None,
            stats: SessionStats::default(),
            input: InputState::new(),
            pending_approval: None,
            sidebar_visible: true,
            generating: false,
            session_events: Vec::new(),
            modified_files: Vec::new(),
            git_diff_queue: Vec::new(),
            scroll_horizontal: 0,
            scroll_vertical: 0,
        }
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

    /// Get the model name as a string
    pub fn model_name(&self) -> String {
        match &self.provider {
            ProviderConfig::Glm { model, .. } => model.clone(),
            ProviderConfig::Gemini { model, .. } => model.clone(),
        }
    }

    /// Get the provider name
    pub fn provider_name(&self) -> &'static str {
        match &self.provider {
            ProviderConfig::Glm { .. } => "GLM",
            ProviderConfig::Gemini { .. } => "Gemini",
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
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            cwd: PathBuf::from("."),
            profile: "default".to_string(),
            provider: ProviderConfig::Glm {
                api_key: "".to_string(),
                model: "glm-4.7".to_string(),
                base_url: "https://open.bigmodel.cn/api/paas/v4".to_string(),
            },
            approval_mode: ApprovalMode::Auto,
            sandbox_mode: SandboxMode::default(),
            verbosity: VerbosityLevel::default(),
            git_branch: None,
            stats: SessionStats::default(),
            input: InputState::new(),
            pending_approval: None,
            sidebar_visible: true,
            generating: false,
            session_events: Vec::new(),
            modified_files: Vec::new(),
            git_diff_queue: Vec::new(),
            scroll_horizontal: 0,
            scroll_vertical: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_state() {
        let mut input = InputState::new();

        input.insert_char('H');
        assert_eq!(input.buffer, "H");
        assert_eq!(input.cursor, 1);

        input.insert_char('i');
        assert_eq!(input.buffer, "Hi");
        assert_eq!(input.cursor, 2);

        input.backspace();
        assert_eq!(input.buffer, "H");
        assert_eq!(input.cursor, 1);

        input.move_home();
        assert_eq!(input.cursor, 0);

        input.move_end();
        assert_eq!(input.cursor, 1);

        let taken = input.take();
        assert_eq!(taken, "H");
        assert_eq!(input.buffer, "");
        assert_eq!(input.cursor, 0);
    }

    #[test]
    fn test_input_state_navigation() {
        let mut input = InputState::new();

        input.insert_char('A');
        input.insert_char('B');
        input.insert_char('C');

        assert_eq!(input.buffer, "ABC");
        assert_eq!(input.cursor, 3);

        input.move_left();
        assert_eq!(input.cursor, 2);

        input.move_left();
        assert_eq!(input.cursor, 1);

        input.insert_char('X');
        assert_eq!(input.buffer, "AXBC");
        assert_eq!(input.cursor, 2);

        input.delete();
        assert_eq!(input.buffer, "AXC");
        assert_eq!(input.cursor, 2);
    }

    #[test]
    fn test_session_stats() {
        let mut stats = SessionStats::default();

        let tokens = TokensUsed::new(10, 20);
        stats.add_tokens(&tokens);

        assert_eq!(stats.input_tokens, 10);
        assert_eq!(stats.output_tokens, 20);
        assert_eq!(stats.total_tokens(), 30);

        stats.increment_approval_gate();
        assert_eq!(stats.approval_gates, 1);

        stats.increment_tools_executed();
        assert_eq!(stats.tools_executed, 1);
    }

    #[test]
    fn test_approval_state() {
        let mut approval = ApprovalState::pending("patch.feature".to_string(), "risky".to_string());

        assert!(approval.is_pending());
        assert!(approval.decision.is_none());

        approval.approve();
        assert!(!approval.is_pending());
        assert_eq!(approval.decision, Some(true));

        let mut approval2 = ApprovalState::pending("delete.file".to_string(), "dangerous".to_string());
        approval2.reject();
        assert_eq!(approval2.decision, Some(false));
    }

    #[test]
    fn test_app_state() {
        let state = AppState::default();

        assert_eq!(state.cwd, PathBuf::from("."));
        assert_eq!(state.profile, "default");
        assert_eq!(state.provider_name(), "GLM");
        assert_eq!(state.model_name(), "glm-4.7".to_string());
        assert!(state.sidebar_visible);
        assert!(!state.generating);
    }

    #[test]
    fn test_app_state_toggle_sidebar() {
        let mut state = AppState::default();

        assert!(state.sidebar_visible);
        state.toggle_sidebar();
        assert!(!state.sidebar_visible);
        state.toggle_sidebar();
        assert!(state.sidebar_visible);
    }

    #[test]
    fn test_app_state_generation() {
        let mut state = AppState::default();

        assert!(!state.is_generating());

        state.start_generation();
        assert!(state.is_generating());

        state.stop_generation();
        assert!(!state.is_generating());
    }

    #[test]
    fn test_app_state_with_custom_provider() {
        let provider = ProviderConfig::Gemini {
            api_key: "test-key".to_string(),
            model: "gemini-2.5-flash".to_string(),
            base_url: "https://api.example.com".to_string(),
        };

        let state = AppState::new(
            PathBuf::from("/workspace"),
            "custom".to_string(),
            provider,
            ApprovalMode::FullAccess,
            SandboxMode::Policy,
        );

        assert_eq!(state.cwd, PathBuf::from("/workspace"));
        assert_eq!(state.profile, "custom");
        assert_eq!(state.provider_name(), "Gemini");
        assert_eq!(state.model_name(), "gemini-2.5-flash".to_string());
        assert_eq!(state.approval_mode, ApprovalMode::FullAccess);
    }

    #[test]
    fn test_scroll_horizontal() {
        let mut state = AppState::default();

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
        let mut state = AppState::default();

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
        let mut state = AppState::default();

        state.scroll_horizontal(50);
        state.scroll_vertical(100);

        assert_eq!(state.scroll_horizontal, 50);
        assert_eq!(state.scroll_vertical, 100);

        state.reset_scroll();

        assert_eq!(state.scroll_horizontal, 0);
        assert_eq!(state.scroll_vertical, 0);
    }

    #[test]
    fn test_input_state_history_navigation() {
        let mut input = InputState::new();

        input.navigate_up();
        input.navigate_down();
        assert_eq!(input.buffer, "");
        assert!(input.history_index.is_none());

        input.add_to_history("first message".to_string());
        input.add_to_history("second message".to_string());
        input.add_to_history("third message".to_string());

        input.buffer = "current new message".to_string();
        input.cursor = input.buffer.len();

        input.navigate_up();
        assert_eq!(input.buffer, "third message");
        assert_eq!(input.history_index, Some(2));
        assert_eq!(input.temp_buffer, Some("current new message".to_string()));

        input.navigate_up();
        assert_eq!(input.buffer, "second message");
        assert_eq!(input.history_index, Some(1));

        input.navigate_up();
        assert_eq!(input.buffer, "first message");
        assert_eq!(input.history_index, Some(0));

        input.navigate_up();
        assert_eq!(input.buffer, "first message");
        assert_eq!(input.history_index, Some(0));

        input.navigate_down();
        assert_eq!(input.buffer, "second message");
        assert_eq!(input.history_index, Some(1));

        input.navigate_down();
        assert_eq!(input.buffer, "third message");
        assert_eq!(input.history_index, Some(2));

        input.navigate_down();
        assert_eq!(input.buffer, "current new message");
        assert_eq!(input.history_index, None);
        assert_eq!(input.temp_buffer, None);

        input.navigate_down();
        assert_eq!(input.buffer, "current new message");
        assert_eq!(input.history_index, None);
    }

    #[test]
    fn test_input_state_history_without_temp_buffer() {
        let mut input = InputState::new();

        input.add_to_history("single message".to_string());
        input.navigate_up();

        assert_eq!(input.buffer, "single message");
        assert_eq!(input.history_index, Some(0));
        assert!(input.temp_buffer.is_none());

        input.navigate_down();
        assert_eq!(input.buffer, "");
        assert_eq!(input.history_index, None);
    }

    #[test]
    fn test_input_state_add_to_history_prevents_duplicates() {
        let mut input = InputState::new();

        input.add_to_history("test message".to_string());
        input.add_to_history("test message".to_string()); // Duplicate
        input.add_to_history("different message".to_string());

        assert_eq!(input.message_history.len(), 2);
        assert_eq!(input.message_history[0], "test message");
        assert_eq!(input.message_history[1], "different message");
    }

    #[test]
    fn test_input_state_reset_history_navigation() {
        let mut input = InputState::new();

        input.add_to_history("message".to_string());

        input.buffer = "current message".to_string();
        input.navigate_up();
        assert!(input.is_navigating_history());
        assert!(input.temp_buffer.is_some());

        input.reset_history_navigation();
        assert!(!input.is_navigating_history());
        assert!(input.temp_buffer.is_none());
    }

    #[test]
    fn test_input_state_history_position() {
        let mut input = InputState::new();

        assert!(input.history_position().is_none());

        input.add_to_history("first".to_string());
        input.add_to_history("second".to_string());
        input.add_to_history("third".to_string());

        input.navigate_up();
        assert_eq!(input.history_position(), Some("3/3".to_string()));

        input.navigate_up();
        assert_eq!(input.history_position(), Some("2/3".to_string()));

        input.navigate_up();
        assert_eq!(input.history_position(), Some("1/3".to_string()));

        input.navigate_down();
        assert_eq!(input.history_position(), Some("2/3".to_string()));

        input.navigate_down();
        assert_eq!(input.history_position(), Some("3/3".to_string()));

        input.navigate_down(); // Back to new message
        assert!(input.history_position().is_none());
    }

    #[test]
    fn test_input_state_edit_history_message() {
        let mut input = InputState::new();

        input.add_to_history("original message".to_string());
        input.navigate_up();

        input.buffer = "modified message".to_string();
        input.cursor = input.buffer.len();

        let sent = input.take();
        assert_eq!(sent, "modified message");

        input.add_to_history(sent);
        assert_eq!(input.message_history.last(), Some(&"modified message".to_string()));
    }
}
