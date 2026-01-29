//! Unified logging and observability system.
//!
//! This module provides production-grade logging using the tracing ecosystem,
//! with support for structured events, file output, and privacy controls.
//!
//! # Environment Variables
//!
//! - `THUNDERUS_LOG`: Filter directive (like `RUST_LOG`), e.g., `thunderus=debug`
//! - `THUNDERUS_LOG_FORMAT`: Output format for stderr: `pretty`, `json`, `compact`
//! - `THUNDERUS_LOG_FILE`: Enable file logging to `~/.thunderus/logs/` (true/false)
//!
//! # Configuration
//!
//! Logging is configured via the `[logging]` section in `thunderus.toml`:
//!
//! ```toml
//! [logging]
//! level = "warn"
//! format = "pretty"
//!
//! [logging.file]
//! enabled = false
//! level = "debug"
//! max_size_mb = 50
//! max_files = 5
//!
//! [logging.privacy]
//! log_tool_args = true
//! log_tool_output = "truncate"
//! truncate_length = 500
//! ```
//!
//! # Example
//!
//! ```no_run
//! use thunderus_core::logging;
//! use thunderus_core::config::LoggingConfig;
//!
//! // Initialize logging with default settings
//! logging::init_logging(None)?;
//!
//! // Or with custom config
//! let config = LoggingConfig::default();
//! logging::init_logging(Some(config))?;
//! # Ok::<(), thunderus_core::Error>(())
//! ```

use crate::Error;
use crate::config::{FileLoggingConfig, LoggingConfig as ConfigLoggingConfig};
use std::env;
use std::io;
use std::path::PathBuf;
use std::str::FromStr;
use tracing_subscriber::{EnvFilter, Registry, fmt, layer::SubscriberExt, util::SubscriberInitExt};

/// Log output format for stderr.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogFormat {
    /// Pretty, human-readable output with colors (default for TTY)
    #[default]
    Pretty,
    /// JSON output (one line per event)
    Json,
    /// Compact, single-line output
    Compact,
}

impl LogFormat {
    /// All available log formats.
    pub const VALUES: &[LogFormat] = &[LogFormat::Pretty, LogFormat::Json, LogFormat::Compact];

    /// Parse a log format from a string.
    pub fn parse_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "pretty" => Some(LogFormat::Pretty),
            "json" => Some(LogFormat::Json),
            "compact" => Some(LogFormat::Compact),
            _ => None,
        }
    }

    /// Get the string representation of this format.
    pub fn as_str(&self) -> &'static str {
        match self {
            LogFormat::Pretty => "pretty",
            LogFormat::Json => "json",
            LogFormat::Compact => "compact",
        }
    }
}

/// How to log tool output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToolOutputLogging {
    /// Don't log tool output.
    #[default]
    None,
    /// Log truncated output (up to `truncate_length` chars).
    Truncate,
    /// Log full output (may include sensitive data).
    Full,
}

impl ToolOutputLogging {
    /// Parse from string.
    pub fn parse_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "none" => Some(ToolOutputLogging::None),
            "truncate" => Some(ToolOutputLogging::Truncate),
            "full" => Some(ToolOutputLogging::Full),
            _ => None,
        }
    }

    /// Get string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            ToolOutputLogging::None => "none",
            ToolOutputLogging::Truncate => "truncate",
            ToolOutputLogging::Full => "full",
        }
    }
}

impl FromStr for ToolOutputLogging {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ToolOutputLogging::parse_str(s).ok_or_else(|| format!("invalid tool output logging: {}", s))
    }
}

/// Logging configuration wrapper that bridges config and logging modules.
#[derive(Debug, Clone)]
pub struct LoggingConfig {
    /// Default log level for stderr output.
    pub level: String,
    /// Output format for stderr.
    pub format: LogFormat,
    /// File logging configuration (optional).
    pub file: Option<FileLoggingConfig>,
    /// Privacy controls for sensitive content.
    pub privacy: PrivacyConfig,
}

/// Privacy configuration for sensitive content in logs.
#[derive(Debug, Clone, Default)]
pub struct PrivacyConfig {
    /// Include tool arguments in trace logs.
    pub log_tool_args: bool,
    /// How to handle tool output in logs.
    pub log_tool_output: ToolOutputLogging,
    /// Maximum length for truncated content.
    pub truncate_length: usize,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "warn".to_string(),
            format: LogFormat::default(),
            file: None,
            privacy: PrivacyConfig::default(),
        }
    }
}

impl From<ConfigLoggingConfig> for LoggingConfig {
    fn from(config: ConfigLoggingConfig) -> Self {
        let format = LogFormat::parse_str(&config.format).unwrap_or_default();
        let log_tool_output = ToolOutputLogging::parse_str(&config.privacy.log_tool_output).unwrap_or_default();

        Self {
            level: config.level,
            format,
            file: if config.file.enabled { Some(config.file) } else { None },
            privacy: PrivacyConfig {
                log_tool_args: config.privacy.log_tool_args,
                log_tool_output,
                truncate_length: config.privacy.truncate_length,
            },
        }
    }
}

impl LoggingConfig {
    /// Create a new logging config with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the log level.
    pub fn with_level(mut self, level: impl Into<String>) -> Self {
        self.level = level.into();
        self
    }

    /// Set the output format.
    pub fn with_format(mut self, format: LogFormat) -> Self {
        self.format = format;
        self
    }

    /// Enable file logging.
    pub fn with_file_logging(mut self, config: FileLoggingConfig) -> Self {
        self.file = Some(config);
        self
    }

    /// Set privacy configuration.
    pub fn with_privacy(mut self, config: PrivacyConfig) -> Self {
        self.privacy = config;
        self
    }

    /// Build an EnvFilter from this config and environment variables.
    fn build_env_filter(&self) -> EnvFilter {
        let filter = env::var("THUNDERUS_LOG")
            .ok()
            .or_else(|| env::var("RUST_LOG").ok())
            .unwrap_or_else(|| self.level.clone());

        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(filter))
    }

    /// Detect if stderr is a TTY for pretty formatting.
    fn is_tty() -> bool {
        atty::is(atty::Stream::Stderr)
    }

    /// Determine the appropriate format for stderr output.
    fn detect_format(&self) -> LogFormat {
        if let Ok(fmt_str) = env::var("THUNDERUS_LOG_FORMAT")
            && let Some(fmt) = LogFormat::parse_str(&fmt_str)
        {
            return fmt;
        }

        if Self::is_tty() { LogFormat::Pretty } else { LogFormat::Compact }
    }

    /// Get the log directory path.
    fn get_log_dir() -> Result<PathBuf, Error> {
        if let Ok(custom_dir) = env::var("THUNDERUS_LOG_DIR") {
            return Ok(PathBuf::from(custom_dir));
        }

        let home = env::var("HOME")
            .or_else(|_| env::var("USERPROFILE"))
            .map_err(|_| Error::Config("Could not determine home directory".to_string()))?;

        Ok(PathBuf::from(home).join(".thunderus").join("logs"))
    }
}

/// Initialize the tracing subscriber with the given configuration.
///
/// This function sets up the global tracing subscriber with:
/// - Environment-based filter (from `THUNDERUS_LOG` or `RUST_LOG`)
/// - Formatted stderr output (pretty, json, or compact)
/// - Optional file logging with rotation
///
/// # Arguments
///
/// * `config` - Optional logging configuration. If None, uses defaults and environment variables.
///
/// # Returns
///
/// Returns `Ok(())` if the subscriber was initialized successfully, or an error if
/// initialization failed.
pub fn init_logging(config: Option<LoggingConfig>) -> Result<(), Error> {
    let config = config.unwrap_or_default();
    let env_filter = config.build_env_filter();
    let format = config.detect_format();

    let registry = Registry::default().with(env_filter);

    if let Some(_file_config) = &config.file {
        let log_dir = LoggingConfig::get_log_dir()?;
        std::fs::create_dir_all(&log_dir)
            .map_err(|e| Error::Config(format!("Failed to create log directory: {}", e)))?;

        let file_appender = tracing_appender::rolling::daily(log_dir, "thunderus.log");
        let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

        match format {
            LogFormat::Pretty => {
                registry
                    .with(fmt::layer().pretty().with_writer(io::stderr).with_ansi(true))
                    .with(fmt::layer().json().with_writer(non_blocking))
                    .init();
            }
            LogFormat::Json => {
                registry
                    .with(fmt::layer().json().with_writer(io::stderr))
                    .with(fmt::layer().json().with_writer(non_blocking))
                    .init();
            }
            LogFormat::Compact => {
                registry
                    .with(fmt::layer().compact().with_writer(io::stderr))
                    .with(fmt::layer().json().with_writer(non_blocking))
                    .init();
            }
        }
    } else {
        match format {
            LogFormat::Pretty => {
                registry
                    .with(fmt::layer().pretty().with_writer(io::stderr).with_ansi(true))
                    .init();
            }
            LogFormat::Json => {
                registry.with(fmt::layer().json().with_writer(io::stderr)).init();
            }
            LogFormat::Compact => {
                registry.with(fmt::layer().compact().with_writer(io::stderr)).init();
            }
        }
    }

    Ok(())
}

/// Redact sensitive content from a string based on privacy settings.
pub fn redact_sensitive(content: &str, privacy: &PrivacyConfig) -> String {
    if content.len() <= privacy.truncate_length {
        return content.to_string();
    }

    match privacy.log_tool_output {
        ToolOutputLogging::None => "[REDACTED]".to_string(),
        ToolOutputLogging::Truncate => {
            let mut truncated = content.chars().take(privacy.truncate_length).collect::<String>();
            truncated.push_str("...");
            truncated.push_str(&format!(" ({} total chars)", content.len()));
            truncated
        }
        ToolOutputLogging::Full => content.to_string(),
    }
}

/// Sanitize file paths for logging (remove home directory, sensitive paths).
pub fn sanitize_path(path: &std::path::Path) -> String {
    if let Ok(home) = env::var("HOME")
        && let Ok(stripped) = path.strip_prefix(&home)
    {
        return format!("~{}", stripped.display());
    }

    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_format_from_str() {
        assert_eq!(LogFormat::parse_str("pretty"), Some(LogFormat::Pretty));
        assert_eq!(LogFormat::parse_str("PRETTY"), Some(LogFormat::Pretty));
        assert_eq!(LogFormat::parse_str("json"), Some(LogFormat::Json));
        assert_eq!(LogFormat::parse_str("JSON"), Some(LogFormat::Json));
        assert_eq!(LogFormat::parse_str("compact"), Some(LogFormat::Compact));
        assert_eq!(LogFormat::parse_str("COMPACT"), Some(LogFormat::Compact));
        assert_eq!(LogFormat::parse_str("invalid"), None);
    }

    #[test]
    fn test_log_format_as_str() {
        assert_eq!(LogFormat::Pretty.as_str(), "pretty");
        assert_eq!(LogFormat::Json.as_str(), "json");
        assert_eq!(LogFormat::Compact.as_str(), "compact");
    }

    #[test]
    fn test_log_format_default() {
        assert_eq!(LogFormat::default(), LogFormat::Pretty);
    }

    #[test]
    fn test_tool_output_logging_from_str() {
        assert_eq!(ToolOutputLogging::parse_str("none"), Some(ToolOutputLogging::None));
        assert_eq!(ToolOutputLogging::parse_str("NONE"), Some(ToolOutputLogging::None));
        assert_eq!(
            ToolOutputLogging::parse_str("truncate"),
            Some(ToolOutputLogging::Truncate)
        );
        assert_eq!(ToolOutputLogging::parse_str("full"), Some(ToolOutputLogging::Full));
        assert_eq!(ToolOutputLogging::parse_str("invalid"), None);
    }

    #[test]
    fn test_tool_output_logging_as_str() {
        assert_eq!(ToolOutputLogging::None.as_str(), "none");
        assert_eq!(ToolOutputLogging::Truncate.as_str(), "truncate");
        assert_eq!(ToolOutputLogging::Full.as_str(), "full");
    }

    #[test]
    fn test_tool_output_logging_default() {
        assert_eq!(ToolOutputLogging::default(), ToolOutputLogging::None);
    }

    #[test]
    fn test_logging_config_default() {
        let config = LoggingConfig::default();
        assert_eq!(config.level, "warn");
        assert_eq!(config.format, LogFormat::Pretty);
        assert!(config.file.is_none());
    }

    #[test]
    fn test_logging_config_builder() {
        let config = LoggingConfig::new()
            .with_level("debug")
            .with_format(LogFormat::Json)
            .with_privacy(PrivacyConfig {
                log_tool_args: true,
                log_tool_output: ToolOutputLogging::Truncate,
                truncate_length: 1000,
            });

        assert_eq!(config.level, "debug");
        assert_eq!(config.format, LogFormat::Json);
        assert!(config.privacy.log_tool_args);
        assert_eq!(config.privacy.log_tool_output, ToolOutputLogging::Truncate);
        assert_eq!(config.privacy.truncate_length, 1000);
    }

    #[test]
    fn test_privacy_config_default() {
        let config = PrivacyConfig::default();
        assert!(!config.log_tool_args);
        assert_eq!(config.log_tool_output, ToolOutputLogging::None);
        assert_eq!(config.truncate_length, 0);
    }

    #[test]
    fn test_redact_sensitive_none() {
        let privacy =
            PrivacyConfig { log_tool_args: false, log_tool_output: ToolOutputLogging::None, truncate_length: 100 };

        let long_content = "a".repeat(200);
        assert_eq!(redact_sensitive(&long_content, &privacy), "[REDACTED]");
    }

    #[test]
    fn test_redact_sensitive_truncate() {
        let privacy =
            PrivacyConfig { log_tool_args: false, log_tool_output: ToolOutputLogging::Truncate, truncate_length: 10 };

        let long_content = "abcdefghijklmnopqrstuvwxyz";
        let redacted = redact_sensitive(long_content, &privacy);
        assert!(redacted.starts_with("abcdefghij"));
        assert!(redacted.contains("..."));
        assert!(redacted.contains("26 total chars"));
    }

    #[test]
    fn test_redact_sensitive_full() {
        let privacy =
            PrivacyConfig { log_tool_args: false, log_tool_output: ToolOutputLogging::Full, truncate_length: 100 };

        let long_content = "a".repeat(200);
        assert_eq!(redact_sensitive(&long_content, &privacy), long_content);
    }

    #[test]
    fn test_sanitize_path() {
        let home = env::var("HOME").unwrap_or_default();
        let test_path = PathBuf::from(home).join("test").join("file.txt");
        assert_eq!(sanitize_path(&test_path), "~test/file.txt");

        let abs_path = PathBuf::from("/var/log/test.log");
        assert_eq!(sanitize_path(&abs_path), "/var/log/test.log");
    }
}
