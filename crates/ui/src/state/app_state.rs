use crate::{ThemeVariant, fuzzy_finder::FuzzyFinder};

use std::path::PathBuf;
use thunderus_core::{ApprovalMode, ProviderConfig, SandboxMode};

use super::{
    ApprovalState, ApprovalUIState, ComposerMode, ComposerState, ConfigEditorState, ConfigState,
    DiffNavigationState, EvidenceState, ExitState, GitDiff, HeaderState, InputState, MemoryHitsState,
    ModelSelectorState, ModifiedFile, SessionEvent, SessionStats, SessionTrackingState, SidebarCollapseState,
    UIState, VerbosityLevel, WelcomeState,
};

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
    /// Memory hits panel state
    pub memory_hits: MemoryHitsState,
    /// Evidence state for Inspector
    pub evidence: EvidenceState,
    /// Config editor state (when open)
    pub config_editor: Option<ConfigEditorState>,
    /// Test mode for deterministic TUI testing
    pub test_mode: bool,
}

impl AppState {
    pub fn new(
        cwd: PathBuf, profile: String, provider: ProviderConfig, approval_mode: ApprovalMode,
        sandbox_mode: SandboxMode, allow_network: bool,
    ) -> Self {
        let model_name = match &provider {
            ProviderConfig::Glm { model, .. } => model.clone(),
            ProviderConfig::Gemini { model, .. } => model.clone(),
            ProviderConfig::Mock { .. } => "mock".to_string(),
        };

        Self {
            config: ConfigState::new(cwd, profile, provider, approval_mode, sandbox_mode, allow_network),
            session: SessionTrackingState::new(),
            ui: UIState::new(),
            input: InputState::new(),
            composer: ComposerState::new(),
            approval_ui: ApprovalUIState::default(),
            exit_state: ExitState::new(),
            welcome: WelcomeState::new(),
            session_header: HeaderState::new(),
            model_selector: ModelSelectorState::new(model_name),
            memory_hits: MemoryHitsState::new(),
            evidence: EvidenceState::new(),
            config_editor: None,
            test_mode: false,
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

    pub fn theme_variant(&self) -> ThemeVariant {
        self.ui.theme_variant
    }

    pub fn set_theme_variant(&mut self, variant: ThemeVariant) {
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

    pub fn memory_patches(&self) -> &[thunderus_core::MemoryPatch] {
        &self.session.memory_patches
    }

    pub fn memory_patches_mut(&mut self) -> &mut Vec<thunderus_core::MemoryPatch> {
        &mut self.session.memory_patches
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

    pub fn is_paused(&self) -> bool {
        self.ui.is_paused()
    }

    pub fn pause_generation(&mut self) {
        self.ui.pause_generation();
    }

    pub fn start_reconcile(&mut self) {
        self.ui.start_reconcile();
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

    /// Enable test mode for deterministic TUI testing
    pub fn set_test_mode(&mut self, enabled: bool) {
        self.test_mode = enabled;
    }

    /// Check if test mode is enabled
    pub fn is_test_mode(&self) -> bool {
        self.test_mode
    }

    /// Open the config editor with current settings
    pub fn open_config_editor(&mut self) {
        self.config_editor = Some(ConfigEditorState::new(
            self.config.profile.clone(),
            self.config.approval_mode,
            self.config.sandbox_mode,
            self.config.allow_network,
            self.config.model_name(),
            self.config.config_path.clone(),
        ));
    }

    /// Close the config editor
    pub fn close_config_editor(&mut self) {
        self.config_editor = None;
    }

    /// Check if the config editor is open
    pub fn is_config_editor_open(&self) -> bool {
        self.config_editor.is_some()
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
                    thinking: Default::default(),
                    options: Default::default(),
                },
                ApprovalMode::Auto,
                SandboxMode::default(),
                false,
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
            memory_hits: MemoryHitsState::default(),
            evidence: EvidenceState::default(),
            config_editor: None,
            test_mode: false,
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
            thinking: Default::default(),
            options: Default::default(),
        };

        let state = AppState::new(
            PathBuf::from("/workspace"),
            "custom".to_string(),
            provider,
            ApprovalMode::FullAccess,
            SandboxMode::Policy,
            false,
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
                thinking: Default::default(),
                options: Default::default(),
            },
            ApprovalMode::Auto,
            SandboxMode::Policy,
            false,
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
                thinking: Default::default(),
                options: Default::default(),
            },
            ApprovalMode::Auto,
            SandboxMode::Policy,
            false,
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
                thinking: Default::default(),
                options: Default::default(),
            },
            ApprovalMode::Auto,
            SandboxMode::Policy,
            false,
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
