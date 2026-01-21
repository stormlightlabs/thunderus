use crate::fuzzy_finder::FuzzyFinder;

use std::path::PathBuf;
use thunderus_core::{ApprovalMode, ProviderConfig, SandboxMode};

mod approval;
mod composer;
mod header;
mod input;
mod model_selector;
mod session;
mod sidebar;
mod ui;
mod welcome;

pub use approval::ApprovalState;
pub use composer::{ComposerMode, ComposerState};
pub use header::HeaderState;
pub use input::InputState;
pub use model_selector::ModelSelectorState;
pub use session::{SessionStats, SessionTrackingState};
pub use sidebar::{SidebarCollapseState, SidebarSection};
pub use ui::{ApprovalUIState, DiffNavigationState, UIState};
pub use welcome::{RecentSessionInfo, WELCOME_TIPS, WelcomeState};

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
    /// Modification type (edited, created, deleted)
    pub mod_type: String,
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

/// Configuration and settings for the application
#[derive(Debug, Clone)]
pub struct ConfigState {
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
    /// Network access allowed
    pub allow_network: bool,
    /// Verbosity level
    pub verbosity: VerbosityLevel,
    /// Git branch (if in a git repo)
    pub git_branch: Option<String>,
    /// Path to config.toml (if provided by CLI)
    pub config_path: Option<PathBuf>,
}

impl ConfigState {
    pub fn new(
        cwd: PathBuf, profile: String, provider: ProviderConfig, approval_mode: ApprovalMode, sandbox_mode: SandboxMode,
    ) -> Self {
        Self {
            cwd,
            profile,
            provider,
            approval_mode,
            sandbox_mode,
            allow_network: false,
            verbosity: VerbosityLevel::default(),
            git_branch: None,
            config_path: None,
        }
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

    /// Refresh the git branch from the current working directory
    ///
    /// Uses git command to detect the current branch.
    /// If not in a git repo or git fails, sets git_branch to None.
    pub fn refresh_git_branch(&mut self) {
        if let Ok(output) = std::process::Command::new("git")
            .args(["-C", &self.cwd.to_string_lossy(), "branch", "--show-current"])
            .output()
        {
            if output.status.success() {
                let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
                self.git_branch = if branch.is_empty() { None } else { Some(branch) };
            } else {
                self.git_branch = None;
            }
        } else {
            self.git_branch = None;
        }
    }
}

/// Exit detection state
#[derive(Debug, Clone, Default)]
pub struct ExitState {
    /// Consecutive CTRL+C press count for exit detection
    ctrl_c_press_count: u8,
    /// Last CTRL+C press timestamp (for reset)
    last_ctrl_c_time: Option<std::time::Instant>,
}

impl ExitState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a CTRL+C press and return whether it should trigger exit
    pub fn record_ctrl_c_press(&mut self) -> bool {
        let now = std::time::Instant::now();
        const RESET_DURATION_MS: u64 = 2000;

        if let Some(last_time) = self.last_ctrl_c_time
            && now.duration_since(last_time).as_millis() > RESET_DURATION_MS as u128
        {
            self.ctrl_c_press_count = 0;
        }

        self.ctrl_c_press_count += 1;
        self.last_ctrl_c_time = Some(now);

        self.ctrl_c_press_count >= 2
    }

    /// Reset CTRL+C press count
    pub fn reset_ctrl_c_count(&mut self) {
        self.ctrl_c_press_count = 0;
        self.last_ctrl_c_time = None;
    }
}

/// Main application state
#[derive(Debug, Clone)]
pub struct AppState {
    /// Configuration and settings
    pub config: ConfigState,
    /// Session tracking data
    pub session: SessionTrackingState,
    /// UI rendering state
    pub ui: UIState,
    /// Input composer state
    pub input: InputState,
    /// Composer state (fuzzy finder, modes)
    pub composer: ComposerState,
    /// Approval UI state
    pub approval_ui: ApprovalUIState,
    /// Exit detection state
    pub exit_state: ExitState,
    /// Welcome screen state
    pub welcome: WelcomeState,
    /// Session header state (minimal header with task title and stats)
    pub session_header: HeaderState,
    /// Model selector state (for footer model/agent selection)
    pub model_selector: ModelSelectorState,
}

impl AppState {
    pub fn new(
        cwd: PathBuf, profile: String, provider: ProviderConfig, approval_mode: ApprovalMode, sandbox_mode: SandboxMode,
    ) -> Self {
        let model_name = match &provider {
            ProviderConfig::Glm { model, .. } => model.clone(),
            ProviderConfig::Gemini { model, .. } => model.clone(),
        };

        Self {
            config: ConfigState::new(cwd, profile, provider, approval_mode, sandbox_mode),
            session: SessionTrackingState::new(),
            ui: UIState::new(),
            input: InputState::new(),
            composer: ComposerState::new(),
            approval_ui: ApprovalUIState::default(),
            exit_state: ExitState::new(),
            welcome: WelcomeState::new(),
            session_header: HeaderState::new(),
            model_selector: ModelSelectorState::new(model_name),
        }
    }

    pub fn cwd(&self) -> &PathBuf {
        &self.config.cwd
    }

    pub fn profile(&self) -> &str {
        &self.config.profile
    }

    pub fn provider(&self) -> &ProviderConfig {
        &self.config.provider
    }

    pub fn approval_mode(&self) -> &ApprovalMode {
        &self.config.approval_mode
    }

    pub fn sandbox_mode(&self) -> &SandboxMode {
        &self.config.sandbox_mode
    }

    pub fn allow_network(&self) -> bool {
        self.config.allow_network
    }

    pub fn verbosity(&self) -> VerbosityLevel {
        self.config.verbosity
    }

    pub fn git_branch(&self) -> Option<&String> {
        self.config.git_branch.as_ref()
    }

    pub fn theme_variant(&self) -> crate::theme::ThemeVariant {
        self.ui.theme_variant
    }

    pub fn set_theme_variant(&mut self, variant: crate::theme::ThemeVariant) {
        self.ui.set_theme_variant(variant);
    }

    pub fn toggle_theme_variant(&mut self) {
        self.ui.toggle_theme_variant();
    }

    pub fn model_name(&self) -> String {
        self.config.model_name()
    }

    pub fn provider_name(&self) -> &'static str {
        self.config.provider_name()
    }

    pub fn refresh_git_branch(&mut self) {
        self.config.refresh_git_branch();
    }

    pub fn stats(&self) -> &SessionStats {
        &self.session.stats
    }

    pub fn stats_mut(&mut self) -> &mut SessionStats {
        &mut self.session.stats
    }

    pub fn session_events(&self) -> &[SessionEvent] {
        &self.session.session_events
    }

    pub fn session_events_mut(&mut self) -> &mut Vec<SessionEvent> {
        &mut self.session.session_events
    }

    pub fn modified_files(&self) -> &[ModifiedFile] {
        &self.session.modified_files
    }

    pub fn modified_files_mut(&mut self) -> &mut Vec<ModifiedFile> {
        &mut self.session.modified_files
    }

    pub fn git_diff_queue(&self) -> &[GitDiff] {
        &self.session.git_diff_queue
    }

    pub fn git_diff_queue_mut(&mut self) -> &mut Vec<GitDiff> {
        &mut self.session.git_diff_queue
    }

    pub fn patches(&self) -> &[thunderus_core::Patch] {
        &self.session.patches
    }

    pub fn patches_mut(&mut self) -> &mut Vec<thunderus_core::Patch> {
        &mut self.session.patches
    }

    pub fn last_message(&self) -> Option<&String> {
        self.session.last_message.as_ref()
    }

    pub fn set_last_message(&mut self, message: Option<String>) {
        self.session.last_message = message;
    }

    pub fn sidebar_visible(&self) -> bool {
        self.ui.sidebar_visible
    }

    pub fn toggle_sidebar(&mut self) {
        self.ui.toggle_sidebar();
    }

    pub fn start_generation(&mut self) {
        self.ui.start_generation();
    }

    pub fn stop_generation(&mut self) {
        self.ui.stop_generation();
    }

    pub fn is_generating(&self) -> bool {
        self.ui.is_generating()
    }

    pub fn advance_animation_frame(&mut self) {
        self.ui.advance_animation_frame();
    }

    pub fn streaming_ellipsis(&self) -> &'static str {
        self.ui.streaming_ellipsis()
    }

    pub fn scroll_horizontal(&mut self, delta: i16) {
        self.ui.scroll_horizontal(delta);
    }

    pub fn scroll_vertical(&mut self, delta: i16) {
        self.ui.scroll_vertical(delta);
    }

    pub fn reset_scroll(&mut self) {
        self.ui.reset_scroll();
    }

    pub fn is_first_session(&self) -> bool {
        self.ui.is_first_session
    }

    pub fn exit_first_session(&mut self) {
        self.ui.exit_first_session();
    }

    pub fn set_first_session(&mut self, value: bool) {
        self.ui.set_first_session(value);
    }

    pub fn sidebar_collapse_state(&self) -> &SidebarCollapseState {
        &self.ui.sidebar_collapse_state
    }

    pub fn sidebar_collapse_state_mut(&mut self) -> &mut SidebarCollapseState {
        &mut self.ui.sidebar_collapse_state
    }

    pub fn enter_fuzzy_finder(&mut self, original_input: String, original_cursor: usize) {
        self.composer
            .enter_fuzzy_finder(self.config.cwd.clone(), original_input, original_cursor);
    }

    pub fn exit_fuzzy_finder(&mut self) {
        self.composer.exit_fuzzy_finder();
    }

    pub fn is_fuzzy_finder_active(&self) -> bool {
        self.composer.is_fuzzy_finder_active()
    }

    pub fn fuzzy_finder_mut(&mut self) -> Option<&mut FuzzyFinder> {
        self.composer.fuzzy_finder_mut()
    }

    pub fn fuzzy_finder(&self) -> Option<&FuzzyFinder> {
        self.composer.fuzzy_finder()
    }

    pub fn composer_mode(&self) -> &ComposerMode {
        &self.composer.composer_mode
    }

    pub fn pending_approval(&self) -> Option<&ApprovalState> {
        self.approval_ui.pending_approval.as_ref()
    }

    pub fn pending_approval_mut(&mut self) -> &mut Option<ApprovalState> {
        &mut self.approval_ui.pending_approval
    }

    pub fn show_hint(&mut self, hint: impl Into<String>) {
        self.approval_ui.show_hint(hint);
    }

    pub fn dismiss_hint(&mut self) {
        self.approval_ui.dismiss_hint();
    }

    pub fn has_pending_hint(&self) -> bool {
        self.approval_ui.has_pending_hint()
    }

    pub fn record_ctrl_c_press(&mut self) -> bool {
        self.exit_state.record_ctrl_c_press()
    }

    pub fn reset_ctrl_c_count(&mut self) {
        self.exit_state.reset_ctrl_c_count();
    }

    pub fn diff_navigation(&self) -> &DiffNavigationState {
        &self.ui.diff_navigation
    }

    pub fn diff_navigation_mut(&mut self) -> &mut DiffNavigationState {
        &mut self.ui.diff_navigation
    }

    /// Navigate to next patch in the diff queue
    pub fn next_patch(&mut self, total_patches: usize) {
        self.ui.diff_navigation.next_patch(total_patches);
    }

    /// Navigate to previous patch in the diff queue
    pub fn prev_patch(&mut self, total_patches: usize) {
        self.ui.diff_navigation.prev_patch(total_patches);
    }

    /// Navigate to next hunk in current patch/file
    pub fn next_hunk(&mut self, total_hunks: usize) {
        self.ui.diff_navigation.next_hunk(total_hunks);
    }

    /// Navigate to previous hunk in current patch/file
    pub fn prev_hunk(&mut self, total_hunks: usize) {
        self.ui.diff_navigation.prev_hunk(total_hunks);
    }

    /// Toggle between summary and detailed hunk view
    pub fn toggle_hunk_details(&mut self) {
        self.ui.diff_navigation.toggle_details();
    }

    /// Set the selected file path for diff navigation
    pub fn set_selected_file(&mut self, path: String) {
        self.ui.diff_navigation.set_selected_file(path);
    }

    /// Get the selected patch index
    pub fn selected_patch_index(&self) -> Option<usize> {
        self.ui.diff_navigation.selected_patch_index
    }

    /// Get the selected hunk index
    pub fn selected_hunk_index(&self) -> Option<usize> {
        self.ui.diff_navigation.selected_hunk_index
    }

    /// Get the selected file path
    pub fn selected_file_path(&self) -> Option<&String> {
        self.ui.diff_navigation.selected_file_path.as_ref()
    }

    /// Reset diff navigation state
    pub fn reset_diff_navigation(&mut self) {
        self.ui.diff_navigation.reset();
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            config: ConfigState::new(
                PathBuf::from("."),
                "default".to_string(),
                ProviderConfig::Glm {
                    api_key: "".to_string(),
                    model: "glm-4.7".to_string(),
                    base_url: "https://open.bigmodel.cn/api/paas/v4".to_string(),
                },
                ApprovalMode::Auto,
                SandboxMode::default(),
            ),
            session: SessionTrackingState::default(),
            ui: UIState::default(),
            input: InputState::new(),
            composer: ComposerState::default(),
            approval_ui: ApprovalUIState::default(),
            exit_state: ExitState::default(),
            welcome: WelcomeState::default(),
            session_header: HeaderState::default(),
            model_selector: ModelSelectorState::new("glm-4.7".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_state() {
        let state = AppState::default();

        assert_eq!(state.config.cwd, PathBuf::from("."));
        assert_eq!(state.config.profile, "default");
        assert_eq!(state.provider_name(), "GLM");
        assert_eq!(state.model_name(), "glm-4.7".to_string());
        assert!(state.sidebar_visible());
        assert!(!state.is_generating());
    }

    #[test]
    fn test_app_state_toggle_sidebar() {
        let mut state = AppState::default();

        assert!(state.sidebar_visible());
        state.toggle_sidebar();
        for _ in 0..10 {
            state.ui.advance_sidebar_animation();
        }
        assert!(!state.sidebar_visible());
        state.toggle_sidebar();
        for _ in 0..10 {
            state.ui.advance_sidebar_animation();
        }
        assert!(state.sidebar_visible());
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

        assert_eq!(state.config.cwd, PathBuf::from("/workspace"));
        assert_eq!(state.config.profile, "custom");
        assert_eq!(state.provider_name(), "Gemini");
        assert_eq!(state.model_name(), "gemini-2.5-flash".to_string());
        assert_eq!(*state.approval_mode(), ApprovalMode::FullAccess);
    }

    #[test]
    fn test_refresh_git_branch_no_repo() {
        let temp = std::env::temp_dir();
        let mut state = AppState::new(
            temp.clone(),
            "test".to_string(),
            ProviderConfig::Glm {
                api_key: "test".to_string(),
                model: "glm-4.7".to_string(),
                base_url: "https://api.example.com".to_string(),
            },
            ApprovalMode::Auto,
            SandboxMode::Policy,
        );

        state.refresh_git_branch();
        assert!(state.config.git_branch.is_none());
    }

    #[test]
    fn test_refresh_git_branch_with_repo() {
        let temp = tempfile::TempDir::new().unwrap();
        let working_dir = temp.path();

        std::process::Command::new("git")
            .args(["init"])
            .current_dir(working_dir)
            .output()
            .unwrap();

        std::process::Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(working_dir)
            .output()
            .unwrap();

        std::process::Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(working_dir)
            .output()
            .unwrap();

        std::process::Command::new("git")
            .args(["checkout", "-b", "test-refresh-branch"])
            .current_dir(working_dir)
            .output()
            .unwrap();

        let mut state = AppState::new(
            working_dir.to_path_buf(),
            "test".to_string(),
            ProviderConfig::Glm {
                api_key: "test".to_string(),
                model: "glm-4.7".to_string(),
                base_url: "https://api.example.com".to_string(),
            },
            ApprovalMode::Auto,
            SandboxMode::Policy,
        );

        state.refresh_git_branch();
        assert_eq!(state.config.git_branch, Some("test-refresh-branch".to_string()));
    }

    #[test]
    fn test_refresh_git_branch_updates_existing() {
        let temp = tempfile::TempDir::new().unwrap();
        let working_dir = temp.path();

        std::process::Command::new("git")
            .args(["init"])
            .current_dir(working_dir)
            .output()
            .unwrap();

        std::process::Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(working_dir)
            .output()
            .unwrap();

        std::process::Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(working_dir)
            .output()
            .unwrap();

        std::process::Command::new("git")
            .args(["checkout", "-b", "initial-branch"])
            .current_dir(working_dir)
            .output()
            .unwrap();

        let mut state = AppState::new(
            working_dir.to_path_buf(),
            "test".to_string(),
            ProviderConfig::Glm {
                api_key: "test".to_string(),
                model: "glm-4.7".to_string(),
                base_url: "https://api.example.com".to_string(),
            },
            ApprovalMode::Auto,
            SandboxMode::Policy,
        );

        state.refresh_git_branch();
        assert_eq!(state.config.git_branch, Some("initial-branch".to_string()));

        std::process::Command::new("git")
            .args(["checkout", "-b", "new-branch"])
            .current_dir(working_dir)
            .output()
            .unwrap();

        state.refresh_git_branch();
        assert_eq!(state.config.git_branch, Some("new-branch".to_string()));
    }
}
