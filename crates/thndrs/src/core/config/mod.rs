//! TOML configuration loading and effective config resolution.
//!
//! Config files are optional. Supported paths are exactly:
//! - Global: `~/.thndrs/config.toml`
//! - Project: `.thndrs/config.toml`
//!
//! Malformed files and unknown keys are errors so users do not run with
//! silently ignored settings.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};
use thndrs_agent::context::ContextConfig;

use crate::cli::{ReasoningEffort, ReasoningSummary, Theme, WebSearchMode};
use crate::utils;

static CONFIG_KEYS: [&str; 13] = [
    "model",
    "websearch",
    "reasoning_effort",
    "reasoning_summary",
    "tick_rate_ms",
    "theme",
    "mouse",
    "verbose",
    "skill_dirs",
    "session_dir",
    "default_workspace",
    "acp_agents",
    "context",
];

/// Built-in completion model used when config and CLI flags do not override it.
pub const DEFAULT_MODEL: &str = "opencode/big-pickle";

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read config {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse config {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("invalid environment variable {name}: {message}")]
    InvalidEnv { name: String, message: String },
    #[error("unknown environment variable {name}")]
    UnknownEnv { name: String },
    #[error("secret-shaped key `{key}` is not allowed in config; use provider env vars instead")]
    SecretInConfig { key: String },
    #[error("invalid config {key}: {message}")]
    InvalidConfig { key: String, message: String },
    #[error("conflicting CLI flags: --mouse and --no-mouse cannot both be set")]
    ConflictingMouseFlags,
}

/// Configuration for one external ACP agent.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct AcpAgentConfig {
    /// Executable command launched over stdio.
    pub command: String,
    /// Command-line arguments passed after [`AcpAgentConfig::command`].
    pub args: Vec<String>,
    /// Environment variables passed to the ACP child process.
    pub env: BTreeMap<String, String>,
    /// Whether this agent is selectable.
    pub enabled: bool,
    /// Timeout for lifecycle requests in seconds.
    pub timeout_secs: u64,
}

impl Default for AcpAgentConfig {
    fn default() -> Self {
        Self { command: String::new(), args: Vec::new(), env: BTreeMap::new(), enabled: true, timeout_secs: 60 }
    }
}

impl AcpAgentConfig {
    /// Return a copy with environment values redacted for diagnostics/metadata.
    pub fn redacted(&self) -> Self {
        let env = self
            .env
            .keys()
            .map(|key| (key.clone(), "[redacted]".to_string()))
            .collect();
        Self { env, ..self.clone() }
    }
}

/// Named ACP agent configurations.
pub type AcpAgentsConfig = BTreeMap<String, AcpAgentConfig>;

/// User-editable configuration loaded from TOML.
///
/// Only ordinary runtime keys are present. CLI-only flags (`print_prompt`,
/// `cwd`, `no_alt_screen`, `no_mouse`) are not TOML keys. Secret-shaped keys
/// are rejected before deserialization reaches this struct.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub model: Option<String>,
    pub websearch: Option<WebSearchMode>,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub reasoning_summary: Option<ReasoningSummary>,
    pub tick_rate_ms: Option<u64>,
    pub mouse: Option<bool>,
    pub verbose: Option<bool>,
    pub theme: Option<Theme>,
    pub skill_dirs: Vec<PathBuf>,
    pub session_dir: Option<PathBuf>,
    pub default_workspace: Option<PathBuf>,
    pub acp_agents: AcpAgentsConfig,
    pub context: ContextConfig,
}

impl Config {
    /// Merge `other` over `self`, keeping existing values when `other` omits a field.
    pub fn merge(mut self, other: Config) -> Self {
        self.model = other.model.or(self.model);
        self.websearch = other.websearch.or(self.websearch);
        self.reasoning_effort = other.reasoning_effort.or(self.reasoning_effort);
        self.reasoning_summary = other.reasoning_summary.or(self.reasoning_summary);
        self.tick_rate_ms = other.tick_rate_ms.or(self.tick_rate_ms);
        self.mouse = other.mouse.or(self.mouse);
        self.verbose = other.verbose.or(self.verbose);
        self.theme = other.theme.or(self.theme);
        self.session_dir = other.session_dir.or(self.session_dir);
        self.default_workspace = other.default_workspace.or(self.default_workspace);
        self.skill_dirs.extend(other.skill_dirs);
        self.acp_agents.extend(other.acp_agents);
        if other.context != ContextConfig::default() {
            self.context = other.context;
        }
        self
    }

    /// Return a copy with ACP environment values redacted.
    pub fn redacted(&self) -> Self {
        let mut redacted = self.clone();
        redacted.acp_agents = redacted
            .acp_agents
            .iter()
            .map(|(name, agent)| (name.clone(), agent.redacted()))
            .collect();
        redacted
    }
}

/// The fully resolved configuration after merging all layers.
///
/// Records where each loaded value came from so prompt inspection, sessions,
/// and export can explain provenance without leaking secrets.
#[derive(Clone, Debug)]
pub struct EffectiveConfig {
    /// Final resolved runtime config values.
    pub config: Config,
    /// Loaded config file layers in precedence order (global first, then project).
    pub layers: Vec<LoadedConfigLayer>,
    /// Per-key origin tracking.
    pub origins: BTreeMap<String, ConfigOrigin>,
    /// Non-fatal diagnostics produced during loading.
    pub diagnostics: Vec<String>,
}

/// A single loaded config file layer.
#[derive(Clone, Debug)]
pub struct LoadedConfigLayer {
    pub source: ConfigSource,
    pub config: Config,
    pub path: Option<PathBuf>,
    /// Redacted path label safe for diagnostics and persisted metadata.
    pub display_path: Option<String>,
    /// Lowercase hex SHA-256 of file bytes.
    pub hash: Option<String>,
}

/// Where a config value originated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigSource {
    Default,
    GlobalFile,
    ProjectFile,
    Environment,
    CliFlag,
}

impl ConfigSource {
    pub fn as_str(self) -> &'static str {
        match self {
            ConfigSource::Default => "default",
            ConfigSource::GlobalFile => "global",
            ConfigSource::ProjectFile => "project",
            ConfigSource::Environment => "env",
            ConfigSource::CliFlag => "cli",
        }
    }
}

/// Provenance label for a single config key.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigOrigin {
    pub source: ConfigSource,
    pub detail: String,
}

impl Default for ConfigOrigin {
    fn default() -> Self {
        Self { source: ConfigSource::Default, detail: "default".to_string() }
    }
}

/// The single supported global config path: `~/.thndrs/config.toml`.
pub fn global_config_path() -> Option<PathBuf> {
    utils::home_dir().map(|home| home.join(".thndrs").join("config.toml"))
}

/// The single supported project config path: `<workspace>/.thndrs/config.toml`.
pub fn project_config_path(workspace: &Path) -> PathBuf {
    workspace.join(".thndrs").join("config.toml")
}

/// Write the selected model into a TOML config file.
///
/// Preserves existing config content and only replaces or inserts the top-level
/// `model` key. Nested table keys named `model` are left untouched.
pub fn write_model_config(path: &Path, model: &str) -> std::io::Result<()> {
    let existing = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(err),
    };
    let next = upsert_top_level_toml_string(&existing, "model", model);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, next)
}

/// Write the selected model into a TOML config file only when no top-level
/// `model` key exists. Returns whether a key was written.
pub fn write_model_config_if_missing(path: &Path, model: &str) -> std::io::Result<bool> {
    let existing = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(err),
    };
    if has_top_level_toml_key(&existing, "model") {
        return Ok(false);
    }
    let next = upsert_top_level_toml_string(&existing, "model", model);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, next)?;
    Ok(true)
}

/// Return whether a TOML config file contains a top-level `model` key.
pub fn model_config_has_model(path: &Path) -> std::io::Result<bool> {
    let existing = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(err),
    };
    Ok(has_top_level_toml_key(&existing, "model"))
}

/// Write the selected model into the project config and return the path used.
pub fn write_project_model(workspace: &Path, model: &str) -> std::io::Result<PathBuf> {
    let path = project_config_path(workspace);
    write_model_config(&path, model)?;
    Ok(path)
}

/// Write the selected reasoning effort into a TOML config file.
///
/// Preserves existing config content and only replaces or inserts the top-level
/// `reasoning_effort` key. Nested table keys with the same name are left untouched.
pub fn write_reasoning_effort_config(path: &Path, effort: ReasoningEffort) -> std::io::Result<()> {
    let existing = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(err),
    };
    let next = upsert_top_level_toml_string(&existing, "reasoning_effort", effort.label());
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, next)
}

/// Write the selected reasoning effort into the project config and return the path used.
pub fn write_project_reasoning_effort(workspace: &Path, effort: ReasoningEffort) -> std::io::Result<PathBuf> {
    let path = project_config_path(workspace);
    write_reasoning_effort_config(&path, effort)?;
    Ok(path)
}

fn upsert_top_level_toml_string(content: &str, key: &str, value: &str) -> String {
    let assignment = format!("{key} = {}\n", toml_basic_string(value));
    let mut output = String::new();
    let mut wrote = false;

    for line in content.lines() {
        let trimmed = line.trim_start();
        if !wrote && is_toml_key_assignment(trimmed, key) {
            output.push_str(&assignment);
            wrote = true;
            continue;
        }
        if !wrote && trimmed.starts_with('[') {
            output.push_str(&assignment);
            wrote = true;
        }
        output.push_str(line);
        output.push('\n');
    }

    if !wrote {
        if !output.is_empty() && !output.ends_with('\n') {
            output.push('\n');
        }
        output.push_str(&assignment);
    }

    output
}

fn has_top_level_toml_key(content: &str, key: &str) -> bool {
    for line in content.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('[') {
            return false;
        }
        if is_toml_key_assignment(trimmed, key) {
            return true;
        }
    }
    false
}

fn is_toml_key_assignment(line: &str, key: &str) -> bool {
    let Some(rest) = line.strip_prefix(key) else {
        return false;
    };
    rest.trim_start().starts_with('=')
}

fn toml_basic_string(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0C}' => out.push_str("\\f"),
            ch if ch.is_control() => out.push_str(&format!("\\u{:04X}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

/// Keys that look like secrets and must not appear in TOML config.
fn is_secret_shaped_key(key: &str) -> bool {
    let lower = key.to_lowercase();
    lower.ends_with("_api_key")
        || lower.ends_with("_token")
        || lower.ends_with("_secret")
        || lower.ends_with("_password")
        || lower.ends_with("secret")
        || lower.ends_with("password")
}

/// Check raw TOML text for secret-shaped keys before deserialization.
fn check_for_secret_keys(content: &str) -> Result<(), ConfigError> {
    let parsed: toml::Value = match toml::from_str(content) {
        Ok(v) => v,
        Err(_) => return Ok(()), // parse errors are caught later by load_file
    };
    check_value_for_secret_keys(&parsed, None)
}

fn check_value_for_secret_keys(value: &toml::Value, parent: Option<&str>) -> Result<(), ConfigError> {
    match value {
        toml::Value::Table(table) => {
            for (key, value) in table {
                let dotted = match parent {
                    Some(parent) => format!("{parent}.{key}"),
                    None => key.clone(),
                };
                if is_secret_shaped_key(key) {
                    return Err(ConfigError::SecretInConfig { key: dotted });
                }
                check_value_for_secret_keys(value, Some(&dotted))?;
            }
            Ok(())
        }
        toml::Value::Array(values) => {
            for value in values {
                check_value_for_secret_keys(value, parent)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn load_file(path: &Path) -> Result<(Config, String), ConfigError> {
    let content = fs::read_to_string(path).map_err(|source| ConfigError::Read { path: path.to_path_buf(), source })?;
    check_for_secret_keys(&content)?;
    let config: Config =
        toml::from_str(&content).map_err(|source| ConfigError::Parse { path: path.to_path_buf(), source })?;
    validate_config(&config)?;
    let hash = sha256_hex(content.as_bytes());
    Ok((config, hash))
}

fn validate_config(config: &Config) -> Result<(), ConfigError> {
    for (name, agent) in &config.acp_agents {
        validate_acp_agent_name(name)?;
        if agent.command.trim().is_empty() {
            return Err(ConfigError::InvalidConfig {
                key: format!("acp_agents.{name}.command"),
                message: "command is required".to_string(),
            });
        }
    }
    Ok(())
}

/// Validate an ACP agent name accepted by `acp:<name>` model ids.
pub fn validate_acp_agent_name(name: &str) -> Result<(), ConfigError> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(ConfigError::InvalidConfig {
            key: format!("acp_agents.{name}"),
            message: "name must match [A-Za-z0-9_-]+".to_string(),
        });
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let result = hasher.finalize();
    hex_encode(&result)
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

pub fn load_env(
    env_vars: &[(String, String)], origins: &mut BTreeMap<String, ConfigOrigin>, _diagnostics: &mut Vec<String>,
) -> Result<Config, ConfigError> {
    let mut config = Config::default();

    for (key, value) in env_vars {
        if !key.starts_with("THNDRS_") {
            continue;
        }
        let config_key = &key["THNDRS_".len()..];
        let lower = config_key.to_lowercase();

        match lower.as_str() {
            "model" => {
                config.model = Some(value.clone());
                origins.insert("model".to_string(), env_origin(key));
            }
            "websearch" => {
                config.websearch = Some(parse_websearch_env(key, value)?);
                origins.insert("websearch".to_string(), env_origin(key));
            }
            "reasoning_effort" => {
                config.reasoning_effort = Some(parse_reasoning_effort_env(key, value)?);
                origins.insert("reasoning_effort".to_string(), env_origin(key));
            }
            "reasoning_summary" => {
                config.reasoning_summary = Some(parse_reasoning_summary_env(key, value)?);
                origins.insert("reasoning_summary".to_string(), env_origin(key));
            }
            "tick_rate_ms" => {
                config.tick_rate_ms = Some(parse_u64_env(key, value)?);
                origins.insert("tick_rate_ms".to_string(), env_origin(key));
            }
            "theme" => {
                config.theme = Some(parse_theme_env(key, value)?);
                origins.insert("theme".to_string(), env_origin(key));
            }
            "mouse" => {
                config.mouse = Some(parse_bool_env(key, value)?);
                origins.insert("mouse".to_string(), env_origin(key));
            }
            "verbose" => {
                config.verbose = Some(parse_bool_env(key, value)?);
                origins.insert("verbose".to_string(), env_origin(key));
            }
            "skill_dirs" => {
                config.skill_dirs = parse_path_list_env(value);
                origins.insert("skill_dirs".to_string(), env_origin(key));
            }
            "session_dir" => {
                config.session_dir = Some(PathBuf::from(value));
                origins.insert("session_dir".to_string(), env_origin(key));
            }
            "default_workspace" => {
                config.default_workspace = Some(PathBuf::from(value));
                origins.insert("default_workspace".to_string(), env_origin(key));
            }
            _ => {
                return Err(ConfigError::UnknownEnv { name: key.clone() });
            }
        }
    }

    Ok(config)
}

fn env_origin(env_var: &str) -> ConfigOrigin {
    ConfigOrigin { source: ConfigSource::Environment, detail: env_var.to_string() }
}

fn parse_bool_env(name: &str, value: &str) -> Result<bool, ConfigError> {
    match value.to_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(ConfigError::InvalidEnv {
            name: name.to_string(),
            message: format!("expected one of 1, 0, true, false, yes, no, on, off (got '{value}')"),
        }),
    }
}

fn parse_u64_env(name: &str, value: &str) -> Result<u64, ConfigError> {
    value.parse().map_err(|_| ConfigError::InvalidEnv {
        name: name.to_string(),
        message: format!("expected a positive integer (got '{value}')"),
    })
}

fn parse_websearch_env(name: &str, value: &str) -> Result<WebSearchMode, ConfigError> {
    match value.to_lowercase().as_str() {
        "auto" => Ok(WebSearchMode::Auto),
        "native" => Ok(WebSearchMode::Native),
        "exa" => Ok(WebSearchMode::Exa),
        "none" => Ok(WebSearchMode::None),
        _ => Err(ConfigError::InvalidEnv {
            name: name.to_string(),
            message: format!("must be one of auto, native, exa, none (got '{value}')"),
        }),
    }
}

fn parse_reasoning_effort_env(name: &str, value: &str) -> Result<ReasoningEffort, ConfigError> {
    ReasoningEffort::parse(value).ok_or_else(|| ConfigError::InvalidEnv {
        name: name.to_string(),
        message: format!("must be one of auto, on, none, minimal, low, medium, high, xhigh, max (got '{value}')"),
    })
}

fn parse_reasoning_summary_env(name: &str, value: &str) -> Result<ReasoningSummary, ConfigError> {
    ReasoningSummary::parse(value).ok_or_else(|| ConfigError::InvalidEnv {
        name: name.to_string(),
        message: format!("must be one of off, auto (got '{value}')"),
    })
}

fn parse_theme_env(name: &str, value: &str) -> Result<Theme, ConfigError> {
    match value.to_lowercase().as_str() {
        "eldritch-minimal" | "eldritch_minimal" | "eldritchminimal" => Ok(Theme::EldritchMinimal),
        "iceberg-dark" | "iceberg_dark" | "icebergdark" => Ok(Theme::IcebergDark),
        "catppuccin-mocha" | "catppuccin_mocha" | "catppuccinmocha" => Ok(Theme::CatppuccinMocha),
        _ => Err(ConfigError::InvalidEnv { name: name.to_string(), message: format!("unknown theme '{value}'") }),
    }
}

fn parse_path_list_env(value: &str) -> Vec<PathBuf> {
    let separator = if cfg!(windows) { ';' } else { ':' };
    value
        .split(separator)
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .collect()
}

/// Load and merge config layers, producing an [`EffectiveConfig`].
///
/// Precedence here is environment > project config > global config > defaults.
/// Command-line flags are applied by [`crate::cli::Cli`] after parsing.
pub fn load_effective(workspace: &Path, env_vars: &[(String, String)]) -> Result<EffectiveConfig, ConfigError> {
    let mut layers = Vec::new();
    let mut origins: BTreeMap<String, ConfigOrigin> = BTreeMap::new();
    let mut diagnostics = Vec::new();
    let cwd = process_cwd();
    let mut merged = default_config(workspace, &cwd);

    if let Some(ref global_path) = global_config_path()
        && global_path.is_file()
    {
        let (mut global_config, hash) = load_file(global_path)?;
        let display_path = global_path_display(global_path);
        let base = global_path.parent().unwrap_or(workspace);
        resolve_config_paths(&mut global_config, base);
        layers.push(LoadedConfigLayer {
            source: ConfigSource::GlobalFile,
            config: global_config.redacted(),
            path: Some(global_path.clone()),
            display_path: Some(display_path.clone()),
            hash: Some(hash),
        });
        record_origins(&global_config, ConfigSource::GlobalFile, &display_path, &mut origins);
        merged = merged.merge(global_config);
    }

    let project_path = project_config_path(workspace);
    if project_path.is_file() {
        let (mut project_config, hash) = load_file(&project_path)?;
        let display_path = project_path_display(&project_path, workspace);
        let base = project_path.parent().unwrap_or(workspace);
        resolve_config_paths(&mut project_config, base);
        layers.push(LoadedConfigLayer {
            source: ConfigSource::ProjectFile,
            config: project_config.redacted(),
            path: Some(project_path.clone()),
            display_path: Some(display_path.clone()),
            hash: Some(hash),
        });
        record_origins(&project_config, ConfigSource::ProjectFile, &display_path, &mut origins);
        merged = merged.merge(project_config);
    }

    let mut env_config = load_env(env_vars, &mut origins, &mut diagnostics)?;
    if has_any_value(&env_config) {
        resolve_config_paths(&mut env_config, &cwd);
        merged = merged.merge(env_config);
    }

    deduplicate_paths(&mut merged.skill_dirs);

    for key in CONFIG_KEYS {
        origins.entry(key.to_string()).or_default();
    }

    Ok(EffectiveConfig { config: merged, layers, origins, diagnostics })
}

/// Resolve the workspace used to find project config when `--cwd` is omitted.
///
/// Project config cannot be loaded until the workspace is known, so this uses
/// only defaults, global config, and environment variables. The full effective
/// config is loaded afterward from the returned workspace.
pub fn default_workspace_before_project_config(env_vars: &[(String, String)]) -> Result<PathBuf, ConfigError> {
    let cwd = process_cwd();
    let mut merged = default_config(Path::new("."), &cwd);

    if let Some(ref global_path) = global_config_path()
        && global_path.is_file()
    {
        let (mut global_config, _) = load_file(global_path)?;
        let base = global_path.parent().unwrap_or_else(|| Path::new("."));
        resolve_config_paths(&mut global_config, base);
        merged = merged.merge(global_config);
    }

    let mut origins = BTreeMap::new();
    let mut diagnostics = Vec::new();
    let mut env_config = load_env(env_vars, &mut origins, &mut diagnostics)?;
    if has_any_value(&env_config) {
        resolve_config_paths(&mut env_config, &cwd);
        merged = merged.merge(env_config);
    }

    Ok(merged.default_workspace.unwrap_or(cwd))
}

fn record_origins(config: &Config, source: ConfigSource, detail: &str, origins: &mut BTreeMap<String, ConfigOrigin>) {
    if config.model.is_some() {
        origins.insert("model".to_string(), ConfigOrigin { source, detail: detail.to_string() });
    }
    if config.websearch.is_some() {
        origins.insert(
            "websearch".to_string(),
            ConfigOrigin { source, detail: detail.to_string() },
        );
    }
    if config.reasoning_effort.is_some() {
        origins.insert(
            "reasoning_effort".to_string(),
            ConfigOrigin { source, detail: detail.to_string() },
        );
    }
    if config.reasoning_summary.is_some() {
        origins.insert(
            "reasoning_summary".to_string(),
            ConfigOrigin { source, detail: detail.to_string() },
        );
    }
    if config.tick_rate_ms.is_some() {
        origins.insert(
            "tick_rate_ms".to_string(),
            ConfigOrigin { source, detail: detail.to_string() },
        );
    }
    if config.theme.is_some() {
        origins.insert("theme".to_string(), ConfigOrigin { source, detail: detail.to_string() });
    }
    if config.mouse.is_some() {
        origins.insert("mouse".to_string(), ConfigOrigin { source, detail: detail.to_string() });
    }
    if config.verbose.is_some() {
        origins.insert(
            "verbose".to_string(),
            ConfigOrigin { source, detail: detail.to_string() },
        );
    }
    if !config.skill_dirs.is_empty() {
        origins.insert(
            "skill_dirs".to_string(),
            ConfigOrigin { source, detail: detail.to_string() },
        );
    }
    if config.session_dir.is_some() {
        origins.insert(
            "session_dir".to_string(),
            ConfigOrigin { source, detail: detail.to_string() },
        );
    }
    if config.default_workspace.is_some() {
        origins.insert(
            "default_workspace".to_string(),
            ConfigOrigin { source, detail: detail.to_string() },
        );
    }
    if !config.acp_agents.is_empty() {
        origins.insert(
            "acp_agents".to_string(),
            ConfigOrigin { source, detail: detail.to_string() },
        );
        for name in config.acp_agents.keys() {
            origins.insert(
                format!("acp_agents.{name}"),
                ConfigOrigin { source, detail: detail.to_string() },
            );
        }
    }
}

fn has_any_value(config: &Config) -> bool {
    config.model.is_some()
        || config.websearch.is_some()
        || config.reasoning_effort.is_some()
        || config.reasoning_summary.is_some()
        || config.tick_rate_ms.is_some()
        || config.theme.is_some()
        || config.mouse.is_some()
        || config.verbose.is_some()
        || !config.skill_dirs.is_empty()
        || config.session_dir.is_some()
        || config.default_workspace.is_some()
        || !config.acp_agents.is_empty()
}

fn default_config(workspace: &Path, cwd: &Path) -> Config {
    Config {
        model: Some(DEFAULT_MODEL.to_string()),
        websearch: Some(WebSearchMode::Auto),
        reasoning_effort: Some(ReasoningEffort::default()),
        reasoning_summary: Some(ReasoningSummary::default()),
        tick_rate_ms: Some(100),
        mouse: Some(false),
        verbose: Some(false),
        theme: Some(Theme::default()),
        skill_dirs: Vec::new(),
        session_dir: Some(resolve_path(&workspace.join(".thndrs").join("sessions"), cwd)),
        default_workspace: Some(cwd.to_path_buf()),
        acp_agents: BTreeMap::new(),
        context: ContextConfig::default(),
    }
}

fn global_path_display(path: &Path) -> String {
    if let Some(home) = utils::home_dir()
        && let Ok(rel) = path.strip_prefix(&home)
    {
        return format!("~/{}", rel.display());
    }
    path.display().to_string()
}

/// Render global config path using `~` when it is under the current home directory.
pub fn global_config_path_display(path: &Path) -> String {
    global_path_display(path)
}

fn project_path_display(path: &Path, workspace: &Path) -> String {
    if let Ok(rel) = path.strip_prefix(workspace) {
        return rel.display().to_string();
    }
    path.display().to_string()
}

/// Render project config paths relative to workspace when possible.
pub fn project_config_path_display(path: &Path, workspace: &Path) -> String {
    project_path_display(path, workspace)
}

/// Resolve relative path config values against the config file that declared them.
///
/// `skill_dirs`, `session_dir`, and `default_workspace` are resolved relative
/// to their declaring file's parent directory. Environment values resolve
/// against the process cwd. CLI values resolve against the process cwd.
#[cfg(test)]
pub fn resolve_paths(config: &mut Config, layers: &[LoadedConfigLayer], workspace: &Path) {
    let mut resolved_skill_dirs = Vec::new();
    for layer in layers {
        let base = layer.path.as_ref().and_then(|p| p.parent()).unwrap_or(workspace);
        for dir in &layer.config.skill_dirs {
            let resolved = resolve_path(dir, base);
            if !resolved_skill_dirs.contains(&resolved) {
                resolved_skill_dirs.push(resolved);
            }
        }
    }

    config.skill_dirs = resolved_skill_dirs;
    resolve_config_paths(config, workspace);
}

fn resolve_config_paths(config: &mut Config, base: &Path) {
    for dir in &mut config.skill_dirs {
        *dir = resolve_path(dir, base);
    }
    if let Some(session_dir) = &mut config.session_dir {
        *session_dir = resolve_path(session_dir, base);
    }
    if let Some(default_workspace) = &mut config.default_workspace {
        *default_workspace = resolve_path(default_workspace, base);
    }
}

fn resolve_path(path: &Path, base: &Path) -> PathBuf {
    if path.is_absolute() { normalize_path(path) } else { normalize_path(base.join(path)) }
}

pub fn resolve_cli_path(path: &Path) -> PathBuf {
    resolve_path(path, &process_cwd())
}

fn process_cwd() -> PathBuf {
    std::env::current_dir()
        .map(normalize_path)
        .unwrap_or_else(|_| PathBuf::from("."))
}

fn normalize_path(path: impl AsRef<Path>) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.as_ref().components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::Normal(part) => normalized.push(part),
        }
    }
    normalized
}

pub fn deduplicate_paths(paths: &mut Vec<PathBuf>) {
    let mut deduped = Vec::new();
    for path in paths.drain(..) {
        if !deduped.contains(&path) {
            deduped.push(path);
        }
    }
    *paths = deduped;
}

#[cfg(test)]
mod tests;
