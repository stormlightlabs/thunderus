use std::path::PathBuf;
use thunderus_core::{ApprovalMode, ProviderConfig, SandboxMode};

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
        cwd: PathBuf, profile: String, provider: ProviderConfig, approval_mode: ApprovalMode,
        sandbox_mode: SandboxMode, allow_network: bool,
    ) -> Self {
        Self {
            cwd,
            profile,
            provider,
            approval_mode,
            sandbox_mode,
            allow_network,
            verbosity: VerbosityLevel::Quiet,
            git_branch: None,
            config_path: None,
        }
    }

    /// Get the model name
    pub fn model_name(&self) -> String {
        match &self.provider {
            ProviderConfig::Glm { model, .. } => model.clone(),
            ProviderConfig::Gemini { model, .. } => model.clone(),
            ProviderConfig::Mock { .. } => "mock".to_string(),
        }
    }

    /// Get the provider name
    pub fn provider_name(&self) -> &'static str {
        match &self.provider {
            ProviderConfig::Glm { .. } => "GLM",
            ProviderConfig::Gemini { .. } => "Gemini",
            ProviderConfig::Mock { .. } => "Mock",
        }
    }

    /// Refresh the git branch from the current working directory
    ///
    /// Uses git command to detect the current branch.
    /// If not in a git repo or git fails, sets git_branch to None.
    pub fn refresh_git_branch(&mut self) {
        match std::process::Command::new("git")
            .args(["-C", &self.cwd.to_string_lossy(), "branch", "--show-current"])
            .output()
        {
            Ok(output) => {
                self.git_branch = if output.status.success() {
                    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if branch.is_empty() { None } else { Some(branch) }
                } else {
                    None
                }
            }
            Err(_) => self.git_branch = None,
        }
    }
}
