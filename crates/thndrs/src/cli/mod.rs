//! Command-line interface for `thndrs`.
//!
//! The entrypoint parses raw flags with [`clap`] directly into the flat [`Cli`]
//! runtime config.

pub mod app;
pub mod commands;
pub mod git;
pub mod input;
pub mod renderer;

use std::collections::BTreeMap;
use std::path::PathBuf;

use clap::parser::ValueSource;
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};
use serde::Deserialize;

use crate::config;

/// Built-in UI color theme.
#[derive(ValueEnum, Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[repr(u8)]
#[serde(rename_all = "kebab-case")]
pub enum Theme {
    /// Default high-contrast dark theme.
    #[default]
    EldritchMinimal,
    /// Muted blue-gray dark theme.
    IcebergDark,
    /// Catppuccin Mocha dark theme.
    CatppuccinMocha,
}

/// Web search policy for a turn.
#[derive(ValueEnum, Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum WebSearchMode {
    /// Let thndrs choose provider-native search only for prompts that need it.
    #[default]
    Auto,
    /// Umans server-side native web search.
    Native,
    /// Exa-backed server-side search for manual experiments.
    Exa,
    /// Pass a local `web_search` tool through unchanged.
    None,
}

/// Requested reasoning control for a provider model.
///
/// The exact choices are model-specific. [`Auto`](Self::Auto) delegates to the
/// provider default, while [`On`](Self::On) and [`None`](Self::None) are used by
/// providers such as Umans that expose a reasoning toggle rather than a depth.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningEffort {
    /// Use the provider's default reasoning behavior without an override.
    #[default]
    Auto,
    /// Explicitly enable provider-native reasoning where it is toggleable.
    On,
    /// Disable explicit reasoning for latency-sensitive work.
    None,
    /// Minimal reasoning for supporting Responses models.
    Minimal,
    /// Quick, well-scoped work.
    Low,
    /// Balanced reasoning quality and latency.
    Medium,
    /// Difficult multi-step work.
    High,
    /// The deepest supported effort for this provider route.
    Xhigh,
    /// Maximum effort for the hardest quality-first work.
    Max,
}

impl ReasoningEffort {
    /// Every normalized reasoning control label.
    pub const ALL: [Self; 9] = [
        Self::Auto,
        Self::On,
        Self::None,
        Self::Minimal,
        Self::Low,
        Self::Medium,
        Self::High,
        Self::Xhigh,
        Self::Max,
    ];

    /// Stable configuration label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::On => "on",
            Self::None => "none",
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Xhigh => "xhigh",
            Self::Max => "max",
        }
    }

    /// User-facing label for picker controls.
    pub fn display_label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::On => "On",
            Self::None => "None",
            Self::Minimal => "Minimal",
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
            Self::Xhigh => "Extra High",
            Self::Max => "Max",
        }
    }

    /// Short picker description for this reasoning level.
    pub fn description(self) -> &'static str {
        match self {
            Self::Auto => "Use the provider default",
            Self::On => "Enable provider-native reasoning",
            Self::None => "Lowest latency; no explicit reasoning",
            Self::Minimal => "Minimal reasoning for simple work",
            Self::Low => "Quick, well-scoped work",
            Self::Medium => "Balanced quality and latency",
            Self::High => "Difficult multi-step work",
            Self::Xhigh => "Deep reasoning for complex work",
            Self::Max => "Hardest quality-first work",
        }
    }

    /// Parse a configuration or ACP option value.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "on" | "enabled" => Some(Self::On),
            "none" => Some(Self::None),
            "minimal" => Some(Self::Minimal),
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            "xhigh" | "x-high" | "extra-high" => Some(Self::Xhigh),
            "max" => Some(Self::Max),
            _ => None,
        }
    }
}

/// Whether providers render a reasoning summary in the transcript.
///
/// TODO: Forward this through every supported provider/model family that advertises
/// compatible reasoning-summary controls.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningSummary {
    /// Keep reasoning opaque while preserving it for provider continuity.
    #[default]
    Off,
    /// Request the provider's best available reasoning summary.
    Auto,
}

impl ReasoningSummary {
    /// Stable configuration label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Auto => "auto",
        }
    }

    /// Parse a configuration or ACP option value.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "off" => Some(Self::Off),
            "auto" => Some(Self::Auto),
            _ => None,
        }
    }
}

/// Top-level non-interactive commands.
#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum Command {
    /// Run first-run setup checks and guided credential setup.
    Setup(commands::setup::SetupCommand),
    /// Store a provider API key in the credential store.
    Login(commands::auth::LoginCommand),
    /// Remove a provider API key from the credential store.
    Logout(commands::auth::LogoutCommand),
    /// Inspect provider authentication state.
    Auth {
        #[command(subcommand)]
        command: commands::auth::AuthCommand,
    },
    /// Print safe setup diagnostics.
    Doctor(commands::doctor::DoctorCommand),
    /// Inspect and edit configuration files.
    Config {
        #[command(subcommand)]
        command: commands::config::ConfigCommand,
    },
    /// Inspect configured ACP agents without opening the TUI.
    Acp {
        #[command(subcommand)]
        command: commands::acp::AcpCommand,
    },
    /// Inspect and call configured MCP servers without opening the TUI.
    Mcp {
        #[command(subcommand)]
        command: commands::mcp::McpCommand,
    },
    /// Inspect local append-only session history.
    #[command(alias = "sessions")]
    Session {
        #[command(subcommand)]
        command: commands::session::SessionCommand,
    },
    /// Read bounded, redacted local diagnostic logs.
    Debug {
        #[command(subcommand)]
        command: commands::debug::DebugCommand,
    },
}

impl WebSearchMode {
    /// Display/config label for this mode.
    pub fn label(self) -> &'static str {
        match self {
            WebSearchMode::Auto => "auto",
            WebSearchMode::Native => "native",
            WebSearchMode::Exa => "exa",
            WebSearchMode::None => "none",
        }
    }

    /// Map a concrete [`WebSearchMode`] to the header value expected by
    /// `X-Umans-Websearch-Provider`.
    ///
    /// - `Native` -> `"native"` (Kimi-backed server-side search)
    /// - `Exa` -> `"exa"` (Exa-backed server-side search)
    /// - `None` -> `"none"` (disable server-side search; pass a local
    ///   `web_search` tool through unchanged)
    pub fn header_value(self) -> &'static str {
        match self {
            WebSearchMode::Auto => "none",
            WebSearchMode::Native => "native",
            WebSearchMode::Exa => "exa",
            WebSearchMode::None => "none",
        }
    }

    /// Resolve `auto` into a concrete provider mode for a single user prompt.
    pub fn resolve_for_prompt(self, prompt: &str) -> Self {
        if self != WebSearchMode::Auto {
            return self;
        }

        if prompt_needs_web_search(prompt) { WebSearchMode::Native } else { WebSearchMode::None }
    }
}

/// Runtime configuration after defaults, TOML, env, and flags are merged.
#[derive(Parser, Clone, Debug)]
#[command(version, about = "agentic pair programmer")]
pub struct Cli {
    /// Working directory used for context loading and display.
    #[arg(long, default_value = ".")]
    pub cwd: PathBuf,
    /// Model to use for completions.
    ///
    /// If omitted, first-run setup asks for a provider and model before the
    /// coding workspace becomes usable.
    #[arg(long, default_value = "")]
    pub model: String,
    /// Web search provider policy.
    #[arg(long, value_enum, default_value = "auto")]
    pub websearch: WebSearchMode,
    /// Configured reasoning effort for supporting provider models.
    ///
    /// TODO: Route this through every provider/model family that supports
    /// reasoning-effort controls.
    #[arg(skip)]
    pub reasoning_effort: ReasoningEffort,
    /// Configured reasoning-summary policy for supporting provider models.
    ///
    /// TODO: Route this through every provider/model family that supports
    /// reasoning-summary controls.
    #[arg(skip)]
    pub reasoning_summary: ReasoningSummary,
    /// Event poll interval in milliseconds.
    #[arg(long, default_value_t = 100)]
    pub tick_rate_ms: u64,
    /// Compatibility no-op; the TUI always renders inline.
    #[arg(long, default_value_t = true)]
    pub no_alt_screen: bool,
    /// Disable terminal mouse capture so native selection and scrollback work.
    #[arg(long, default_value_t = false, conflicts_with = "mouse")]
    pub no_mouse: bool,
    /// Enable terminal mouse capture for overlay mouse events.
    ///
    /// Capture is toggled on only while an overlay needs mouse events, so native
    /// terminal text selection works at all other times.
    #[arg(long, default_value_t = false, conflicts_with = "no_mouse")]
    pub mouse: bool,
    /// Show diagnostic transcript rows such as provider events and log paths.
    #[arg(long, default_value_t = false)]
    pub verbose: bool,
    /// UI color theme.
    #[arg(long, value_enum, default_value = "eldritch-minimal")]
    pub theme: Theme,
    /// Print the assembled prompt bundle/lowered messages with secrets redacted.
    #[arg(long, default_value_t = false)]
    pub print_prompt: bool,
    /// Additional skill directories to scan.
    #[arg(long = "skill-dir")]
    pub skill_dirs: Vec<PathBuf>,
    /// Directory for append-only session JSONL files.
    #[arg(long = "session-dir")]
    pub session_dir: Option<PathBuf>,
    /// Config diagnostics from effective config loading.
    #[arg(skip)]
    pub config_diagnostics: Vec<String>,
    /// Effective config layers for session metadata.
    #[arg(skip)]
    pub config_layers: Vec<config::LoadedConfigLayer>,
    /// Effective config key origins for session metadata.
    #[arg(skip)]
    pub config_origins: BTreeMap<String, config::ConfigOrigin>,
    /// Effective external ACP agent configs.
    #[arg(skip)]
    pub acp_agents: config::AcpAgentsConfig,
    /// Optional non-interactive command.
    #[command(subcommand)]
    pub command: Option<Command>,
}

impl Default for Cli {
    /// Used by tests and as the implicit baseline before flag overrides.
    fn default() -> Self {
        Cli {
            cwd: PathBuf::from("."),
            model: String::new(),
            websearch: WebSearchMode::Auto,
            reasoning_effort: ReasoningEffort::default(),
            reasoning_summary: ReasoningSummary::default(),
            tick_rate_ms: 100,
            no_alt_screen: true,
            no_mouse: false,
            mouse: false,
            verbose: false,
            theme: Theme::default(),
            print_prompt: false,
            skill_dirs: Vec::new(),
            session_dir: None,
            config_diagnostics: Vec::new(),
            config_layers: Vec::new(),
            config_origins: BTreeMap::new(),
            acp_agents: BTreeMap::new(),
            command: None,
        }
    }
}

impl Cli {
    /// Parse command-line arguments, load TOML config, and merge them.
    pub fn parse_configured() -> Result<Self, config::ConfigError> {
        match Self::try_parse_configured_from(std::env::args_os()) {
            Ok(cli) => cli,
            Err(err) => err.exit(),
        }
    }

    /// Test-friendly parser that applies CLI defaults but skips config loading.
    pub fn try_parse_from<I, T>(itr: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        let (cli, _) = Self::try_parse_matches_from(itr)?;
        Ok(cli)
    }

    fn try_parse_configured_from<I, T>(itr: I) -> Result<Result<Self, config::ConfigError>, clap::Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        let env_vars: Vec<(String, String)> = std::env::vars().filter(|(key, _)| key.starts_with("THNDRS_")).collect();
        Self::try_parse_configured_from_env(itr, &env_vars)
    }

    fn try_parse_configured_from_env<I, T>(
        itr: I, env_vars: &[(String, String)],
    ) -> Result<Result<Self, config::ConfigError>, clap::Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        let (cli, matches) = Self::try_parse_matches_from(itr)?;
        let configured_default_workspace;
        let initial_workspace = if is_command_line(&matches, "cwd") {
            cli.cwd.as_path()
        } else {
            configured_default_workspace = match config::default_workspace_before_project_config(env_vars) {
                Ok(path) => path,
                Err(err) => return Ok(Err(err)),
            };
            configured_default_workspace.as_path()
        };
        Ok(
            config::load_effective(initial_workspace, env_vars)
                .map(|effective| cli.with_effective(effective, &matches)),
        )
    }

    fn try_parse_matches_from<I, T>(itr: I) -> Result<(Self, clap::ArgMatches), clap::Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        let matches = Self::command().try_get_matches_from(itr)?;
        let cli = Self::from_arg_matches(&matches)?;
        Ok((cli, matches))
    }

    fn with_effective(mut self, effective: config::EffectiveConfig, matches: &clap::ArgMatches) -> Self {
        let defaults = Self::default();
        let mut config = effective.config;
        let mut origins = effective.origins;

        if is_command_line(matches, "model") {
            config.model = Some(self.model.clone());
            insert_cli_origin(&mut origins, "model", "--model");
        }
        if is_command_line(matches, "websearch") {
            config.websearch = Some(self.websearch);
            insert_cli_origin(&mut origins, "websearch", "--websearch");
        }
        if is_command_line(matches, "tick_rate_ms") {
            config.tick_rate_ms = Some(self.tick_rate_ms);
            insert_cli_origin(&mut origins, "tick_rate_ms", "--tick-rate-ms");
        }
        if is_command_line(matches, "theme") {
            config.theme = Some(self.theme);
            insert_cli_origin(&mut origins, "theme", "--theme");
        }
        if is_command_line(matches, "mouse") {
            config.mouse = Some(true);
            insert_cli_origin(&mut origins, "mouse", "--mouse");
        } else if is_command_line(matches, "no_mouse") {
            config.mouse = Some(false);
            insert_cli_origin(&mut origins, "mouse", "--no-mouse");
        }
        if is_command_line(matches, "verbose") {
            config.verbose = Some(true);
            insert_cli_origin(&mut origins, "verbose", "--verbose");
        }
        if is_command_line(matches, "skill_dirs") && !self.skill_dirs.is_empty() {
            config
                .skill_dirs
                .extend(self.skill_dirs.iter().map(|dir| config::resolve_cli_path(dir)));
            config::deduplicate_paths(&mut config.skill_dirs);
            insert_cli_origin(&mut origins, "skill_dirs", "--skill-dir");
        }
        if is_command_line(matches, "session_dir")
            && let Some(ref session_dir) = self.session_dir
        {
            config.session_dir = Some(config::resolve_cli_path(session_dir));
            insert_cli_origin(&mut origins, "session_dir", "--session-dir");
        }

        self.cwd = if is_command_line(matches, "cwd") {
            self.cwd
        } else {
            config.default_workspace.unwrap_or(defaults.cwd)
        };
        self.model = config.model.unwrap_or(defaults.model);
        self.websearch = config.websearch.unwrap_or(defaults.websearch);
        self.reasoning_effort = config.reasoning_effort.unwrap_or(defaults.reasoning_effort);
        self.reasoning_summary = config.reasoning_summary.unwrap_or(defaults.reasoning_summary);
        self.tick_rate_ms = config.tick_rate_ms.unwrap_or(defaults.tick_rate_ms);
        self.mouse = config.mouse.unwrap_or(defaults.mouse);
        self.verbose = config.verbose.unwrap_or(defaults.verbose);
        self.theme = config.theme.unwrap_or(defaults.theme);
        self.skill_dirs = config.skill_dirs;
        self.session_dir = config.session_dir;
        self.config_diagnostics = effective.diagnostics;
        self.config_layers = effective.layers;
        self.config_origins = origins;
        self.acp_agents = config.acp_agents;
        self
    }
}

fn prompt_needs_web_search(prompt: &str) -> bool {
    let text = prompt.to_ascii_lowercase();
    const NEEDLES: &[&str] = &[
        "latest",
        "current",
        "today",
        "recent",
        "news",
        "release",
        "changelog",
        "docs",
        "documentation",
        "look up",
        "lookup",
        "search web",
        "web search",
        "browse",
        "internet",
        "online",
        "pricing",
        "benchmark",
    ];
    NEEDLES.iter().any(|needle| text.contains(needle))
}

fn is_command_line(matches: &clap::ArgMatches, id: &str) -> bool {
    matches.value_source(id) == Some(ValueSource::CommandLine)
}

fn insert_cli_origin(origins: &mut BTreeMap<String, config::ConfigOrigin>, key: &str, flag: &str) {
    origins.insert(
        key.to_string(),
        config::ConfigOrigin { source: config::ConfigSource::CliFlag, detail: flag.to_string() },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn cli_defaults_match_spec() {
        let cli = Cli::try_parse_from(["thndrs"]).expect("default parse");
        assert_eq!(cli.cwd, PathBuf::from("."));
        assert!(cli.model.is_empty());
        assert_eq!(cli.websearch, WebSearchMode::Auto);
        assert_eq!(cli.tick_rate_ms, 100);
        assert!(cli.no_alt_screen);
        assert!(!cli.no_mouse);
        assert!(!cli.mouse);
        assert!(!cli.verbose);
        assert_eq!(cli.theme, Theme::EldritchMinimal);
        assert!(cli.skill_dirs.is_empty());
    }

    #[test]
    fn cli_flags_override_config_values() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let workspace = tmp.path();
        fs::create_dir_all(workspace.join(".thndrs")).expect("create .thndrs dir");
        fs::write(
            workspace.join(".thndrs").join("config.toml"),
            "model = \"config-model\"\nwebsearch = \"native\"\ntick_rate_ms = 250\nverbose = false\ntheme = \"eldritch-minimal\"\n",
        )
        .expect("write config");

        let (cli, matches) = Cli::try_parse_matches_from([
            "thndrs",
            "--model",
            "cli-model",
            "--verbose",
            "--theme",
            "catppuccin-mocha",
        ])
        .unwrap();
        let effective = config::load_effective(workspace, &[]).expect("load effective");
        let cli = cli.with_effective(effective, &matches);

        assert_eq!(cli.model, "cli-model");
        assert_eq!(cli.websearch, WebSearchMode::Native);
        assert_eq!(cli.tick_rate_ms, 250);
        assert!(cli.verbose);
        assert_eq!(cli.theme, Theme::CatppuccinMocha);
        assert_eq!(
            cli.config_origins.get("model"),
            Some(&config::ConfigOrigin { source: config::ConfigSource::CliFlag, detail: "--model".to_string() })
        );
    }

    #[test]
    fn cli_mouse_flag_overrides_env() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let (cli, matches) = Cli::try_parse_matches_from(["thndrs", "--no-mouse"]).unwrap();
        let effective =
            config::load_effective(tmp.path(), &[("THNDRS_MOUSE".to_string(), "true".to_string())]).unwrap();
        let cli = cli.with_effective(effective, &matches);

        assert!(!cli.mouse);
        assert_eq!(
            cli.config_origins.get("mouse"),
            Some(&config::ConfigOrigin { source: config::ConfigSource::CliFlag, detail: "--no-mouse".to_string() })
        );
    }

    #[test]
    fn cli_model_flag_overrides_env() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let (cli, matches) = Cli::try_parse_matches_from(["thndrs", "--model", "cli-model"]).unwrap();
        let effective =
            config::load_effective(tmp.path(), &[("THNDRS_MODEL".to_string(), "env-model".to_string())]).unwrap();
        let cli = cli.with_effective(effective, &matches);

        assert_eq!(cli.model, "cli-model");
        assert_eq!(
            cli.config_origins.get("model"),
            Some(&config::ConfigOrigin { source: config::ConfigSource::CliFlag, detail: "--model".to_string() })
        );
    }

    #[test]
    fn default_workspace_discovers_project_config_when_cwd_omitted() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let workspace = tmp.path().join("workspace");
        fs::create_dir_all(workspace.join(".thndrs")).expect("create project config dir");
        fs::write(
            workspace.join(".thndrs").join("config.toml"),
            "model = \"project-model\"\n",
        )
        .expect("write project config");

        let env_vars = [("THNDRS_DEFAULT_WORKSPACE".to_string(), workspace.display().to_string())];
        let cli = Cli::try_parse_configured_from_env(["thndrs"], &env_vars)
            .expect("parse args")
            .expect("load config");

        assert_eq!(cli.cwd, workspace);
        assert_eq!(cli.model, "project-model");
        assert_eq!(
            cli.config_origins.get("model"),
            Some(&config::ConfigOrigin {
                source: config::ConfigSource::ProjectFile,
                detail: ".thndrs/config.toml".to_string()
            })
        );
    }

    #[test]
    fn configured_cli_carries_acp_agents() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let workspace = tmp.path().join("workspace");
        fs::create_dir_all(workspace.join(".thndrs")).expect("create project config dir");
        fs::write(
            workspace.join(".thndrs").join("config.toml"),
            r#"
            model = "acp:local"

            [acp_agents.local]
            command = "agent"
            args = ["--stdio"]
            "#,
        )
        .expect("write project config");

        let env_vars = [("THNDRS_DEFAULT_WORKSPACE".to_string(), workspace.display().to_string())];
        let cli = Cli::try_parse_configured_from_env(["thndrs"], &env_vars)
            .expect("parse args")
            .expect("load config");

        assert_eq!(cli.model, "acp:local");
        assert_eq!(cli.acp_agents["local"].command, "agent");
        assert_eq!(cli.acp_agents["local"].args, vec!["--stdio"]);
    }

    #[test]
    // TODO: this should be a table drive test
    fn websearch_explicit_values_parse() {
        let auto = Cli::try_parse_from(["thndrs", "--websearch", "auto"]).unwrap();
        assert_eq!(auto.websearch, WebSearchMode::Auto);

        let native = Cli::try_parse_from(["thndrs", "--websearch", "native"]).unwrap();
        assert_eq!(native.websearch, WebSearchMode::Native);

        let exa = Cli::try_parse_from(["thndrs", "--websearch", "exa"]).unwrap();
        assert_eq!(exa.websearch, WebSearchMode::Exa);

        let none = Cli::try_parse_from(["thndrs", "--websearch", "none"]).unwrap();
        assert_eq!(none.websearch, WebSearchMode::None);
    }

    #[test]
    fn invalid_websearch_is_rejected() {
        let result = Cli::try_parse_from(["thndrs", "--websearch", "totally-bogus"]);
        assert!(result.is_err(), "invalid websearch mode should be rejected");
    }

    #[test]
    fn model_and_cwd_overrides_parse() {
        let cli = Cli::try_parse_from([
            "thndrs",
            "--cwd",
            "/tmp/repo",
            "--model",
            "umans-glm-5.2",
            "--tick-rate-ms",
            "250",
            "--no-alt-screen",
        ])
        .expect("explicit flags parse");
        assert_eq!(cli.cwd, PathBuf::from("/tmp/repo"));
        assert_eq!(cli.model, "umans-glm-5.2");
        assert_eq!(cli.tick_rate_ms, 250);
        assert!(cli.no_alt_screen);
    }

    #[test]
    fn print_prompt_flag_parses() {
        let cli = Cli::try_parse_from(["thndrs", "--print-prompt"]).expect("parse");
        assert!(cli.print_prompt);
    }

    #[test]
    fn acp_list_command_parses() {
        let cli = Cli::try_parse_from(["thndrs", "acp", "list"]).expect("parse");
        assert_eq!(
            cli.command,
            Some(Command::Acp { command: commands::acp::AcpCommand::List })
        );
    }

    #[test]
    fn acp_serve_command_parses() {
        let cli = Cli::try_parse_from(["thndrs", "--cwd", "/workspace", "acp", "serve"]).expect("parse");

        assert_eq!(cli.cwd, PathBuf::from("/workspace"));
        assert_eq!(
            cli.command,
            Some(Command::Acp { command: commands::acp::AcpCommand::Serve })
        );
    }

    #[test]
    fn first_run_setup_command_parses() {
        let cli = Cli::try_parse_from(["thndrs", "setup", "--provider", "umans", "--project"]).expect("parse");
        assert_eq!(
            cli.command,
            Some(Command::Setup(commands::setup::SetupCommand {
                provider: Some(commands::setup::SetupProviderArg::Umans),
                global: false,
                project: true,
            }))
        );
    }

    #[test]
    fn auth_commands_parse() {
        let login = Cli::try_parse_from(["thndrs", "login", "opencode-go"]).expect("parse");
        assert_eq!(
            login.command,
            Some(Command::Login(commands::auth::LoginCommand {
                provider: commands::setup::SetupProviderArg::OpencodeGo,
            }))
        );

        let zen_login = Cli::try_parse_from(["thndrs", "login", "opencode-zen"]).expect("parse");
        assert_eq!(
            zen_login.command,
            Some(Command::Login(commands::auth::LoginCommand {
                provider: commands::setup::SetupProviderArg::OpencodeZen,
            }))
        );

        let codex_login = Cli::try_parse_from(["thndrs", "login", "chatgpt-codex"]).expect("parse");
        assert_eq!(
            codex_login.command,
            Some(Command::Login(commands::auth::LoginCommand {
                provider: commands::setup::SetupProviderArg::ChatgptCodex,
            }))
        );

        let logout = Cli::try_parse_from(["thndrs", "logout", "umans"]).expect("parse");
        assert_eq!(
            logout.command,
            Some(Command::Logout(commands::auth::LogoutCommand {
                provider: commands::setup::SetupProviderArg::Umans,
            }))
        );

        let status = Cli::try_parse_from(["thndrs", "auth", "status"]).expect("parse");
        assert_eq!(
            status.command,
            Some(Command::Auth { command: commands::auth::AuthCommand::Status })
        );
    }

    #[test]
    fn doctor_command_parses() {
        let cli = Cli::try_parse_from(["thndrs", "doctor", "--json"]).expect("parse");
        assert_eq!(
            cli.command,
            Some(Command::Doctor(commands::doctor::DoctorCommand { json: true }))
        );
    }

    #[test]
    fn config_commands_parse() {
        let path = Cli::try_parse_from(["thndrs", "config", "path"]).expect("parse");
        assert_eq!(
            path.command,
            Some(Command::Config { command: commands::config::ConfigCommand::Path })
        );

        let show = Cli::try_parse_from(["thndrs", "config", "show", "--redacted"]).expect("parse");
        assert_eq!(
            show.command,
            Some(Command::Config {
                command: commands::config::ConfigCommand::Show(commands::config::ConfigShowCommand { redacted: true })
            })
        );

        let edit = Cli::try_parse_from(["thndrs", "config", "edit", "--global"]).expect("parse");
        assert_eq!(
            edit.command,
            Some(Command::Config {
                command: commands::config::ConfigCommand::Edit(commands::config::ConfigEditCommand {
                    global: true,
                    project: false,
                })
            })
        );

        let edit = Cli::try_parse_from(["thndrs", "config", "edit", "--project"]).expect("parse");
        assert_eq!(
            edit.command,
            Some(Command::Config {
                command: commands::config::ConfigCommand::Edit(commands::config::ConfigEditCommand {
                    global: false,
                    project: true,
                })
            })
        );
    }

    #[test]
    fn acp_inspect_command_parses() {
        let cli = Cli::try_parse_from(["thndrs", "acp", "inspect", "local"]).expect("parse");
        assert_eq!(
            cli.command,
            Some(Command::Acp { command: commands::acp::AcpCommand::Inspect { name: "local".to_string() } })
        );
    }

    #[test]
    fn acp_smoke_command_parses_prompt() {
        let cli = Cli::try_parse_from(["thndrs", "acp", "smoke", "local", "--prompt", "hello"]).expect("parse");
        assert_eq!(
            cli.command,
            Some(Command::Acp {
                command: commands::acp::AcpCommand::Smoke { name: "local".to_string(), prompt: "hello".to_string() }
            })
        );
    }

    #[test]
    fn acp_logout_command_parses() {
        let cli = Cli::try_parse_from(["thndrs", "acp", "logout", "local"]).expect("parse");
        assert_eq!(
            cli.command,
            Some(Command::Acp { command: commands::acp::AcpCommand::Logout { name: "local".to_string() } })
        );
    }

    #[test]
    fn acp_session_commands_parse() {
        let list = Cli::try_parse_from(["thndrs", "acp", "list-sessions", "local"]).expect("parse");
        assert_eq!(
            list.command,
            Some(Command::Acp { command: commands::acp::AcpCommand::ListSessions { name: "local".to_string() } })
        );

        let load = Cli::try_parse_from(["thndrs", "acp", "load-session", "local", "external-1"]).expect("parse");
        assert_eq!(
            load.command,
            Some(Command::Acp {
                command: commands::acp::AcpCommand::LoadSession {
                    name: "local".to_string(),
                    session_id: "external-1".to_string()
                }
            })
        );

        let resume = Cli::try_parse_from(["thndrs", "acp", "resume-session", "local", "external-1"]).expect("parse");
        assert_eq!(
            resume.command,
            Some(Command::Acp {
                command: commands::acp::AcpCommand::ResumeSession {
                    name: "local".to_string(),
                    session_id: "external-1".to_string()
                }
            })
        );

        let close = Cli::try_parse_from(["thndrs", "acp", "close-session", "local", "external-1"]).expect("parse");
        assert_eq!(
            close.command,
            Some(Command::Acp {
                command: commands::acp::AcpCommand::CloseSession {
                    name: "local".to_string(),
                    session_id: "external-1".to_string()
                }
            })
        );
    }

    #[test]
    fn acp_registry_command_parses() {
        let cli = Cli::try_parse_from(["thndrs", "acp", "registry", "--file", "registry.json"]).expect("parse");
        assert_eq!(
            cli.command,
            Some(Command::Acp {
                command: commands::acp::AcpCommand::Registry { file: Some(PathBuf::from("registry.json")) }
            })
        );
    }

    #[test]
    fn acp_install_and_update_commands_parse() {
        let install = Cli::try_parse_from([
            "thndrs",
            "acp",
            "install",
            "codex-acp",
            "--name",
            "codex",
            "--file",
            "registry.json",
            "--yes",
        ])
        .expect("parse install");
        assert_eq!(
            install.command,
            Some(Command::Acp {
                command: commands::acp::AcpCommand::Install {
                    agent_id: "codex-acp".to_string(),
                    name: Some("codex".to_string()),
                    file: Some(PathBuf::from("registry.json")),
                    yes: true,
                }
            })
        );

        let update = Cli::try_parse_from(["thndrs", "acp", "update", "codex", "--file", "registry.json", "--yes"])
            .expect("parse update");
        assert_eq!(
            update.command,
            Some(Command::Acp {
                command: commands::acp::AcpCommand::Update {
                    name: "codex".to_string(),
                    file: Some(PathBuf::from("registry.json")),
                    yes: true,
                }
            })
        );
    }

    #[test]
    fn no_mouse_flag_parses() {
        let cli = Cli::try_parse_from(["thndrs", "--no-mouse"]).expect("parse");
        assert!(cli.no_mouse);
    }

    #[test]
    fn mouse_flag_parses() {
        let cli = Cli::try_parse_from(["thndrs", "--mouse"]).expect("parse");
        assert!(cli.mouse);
    }

    #[test]
    fn mouse_and_no_mouse_conflict() {
        let err = Cli::try_parse_from(["thndrs", "--mouse", "--no-mouse"]).expect_err("conflict rejected");
        assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
    }

    #[test]
    fn verbose_flag_parses() {
        let cli = Cli::try_parse_from(["thndrs", "--verbose"]).expect("parse");
        assert!(cli.verbose);
    }

    #[test]
    fn theme_flag_parses() {
        let cli = Cli::try_parse_from(["thndrs", "--theme", "iceberg-dark"]).expect("parse");
        assert_eq!(cli.theme, Theme::IcebergDark);

        let cli = Cli::try_parse_from(["thndrs", "--theme", "catppuccin-mocha"]).expect("parse");
        assert_eq!(cli.theme, Theme::CatppuccinMocha);
    }

    #[test]
    fn print_prompt_defaults_false() {
        let cli = Cli::try_parse_from(["thndrs"]).expect("parse");
        assert!(!cli.print_prompt);
    }

    #[test]
    fn websearch_header_value_native() {
        assert_eq!(WebSearchMode::Native.header_value(), "native");
    }

    #[test]
    fn websearch_label_auto() {
        assert_eq!(WebSearchMode::Auto.label(), "auto");
    }

    #[test]
    fn websearch_header_value_exa() {
        assert_eq!(WebSearchMode::Exa.header_value(), "exa");
    }

    #[test]
    fn websearch_header_value_none() {
        assert_eq!(WebSearchMode::None.header_value(), "none");
    }

    #[test]
    fn auto_websearch_resolves_from_prompt() {
        assert_eq!(
            WebSearchMode::Auto.resolve_for_prompt("Can you clear completed sections from TODO.md?"),
            WebSearchMode::None
        );
        assert_eq!(
            WebSearchMode::Auto.resolve_for_prompt("Look up the latest Umans docs"),
            WebSearchMode::Native
        );
    }

    #[test]
    fn mcp_commands_parse() {
        let list = Cli::try_parse_from(["thndrs", "mcp", "list"]).expect("parse");
        assert_eq!(
            list.command,
            Some(Command::Mcp { command: commands::mcp::McpCommand::List })
        );

        let test = Cli::try_parse_from(["thndrs", "mcp", "test", "docs"]).expect("parse");
        assert_eq!(
            test.command,
            Some(Command::Mcp { command: commands::mcp::McpCommand::Test { name: "docs".to_string() } })
        );

        let tools = Cli::try_parse_from(["thndrs", "mcp", "tools", "docs"]).expect("parse");
        assert_eq!(
            tools.command,
            Some(Command::Mcp { command: commands::mcp::McpCommand::Tools { name: "docs".to_string() } })
        );

        let call = Cli::try_parse_from(["thndrs", "mcp", "call", "docs", "echo", "--json", r#"{"text":"ok"}"#])
            .expect("parse");
        assert_eq!(
            call.command,
            Some(Command::Mcp {
                command: commands::mcp::McpCommand::Call {
                    server: "docs".to_string(),
                    tool: "echo".to_string(),
                    json: r#"{"text":"ok"}"#.to_string()
                }
            })
        );
    }

    #[test]
    fn session_commands_parse() {
        let list = Cli::try_parse_from(["thndrs", "sessions", "list"]).expect("parse");
        assert_eq!(
            list.command,
            Some(Command::Session { command: commands::session::SessionCommand::List })
        );

        let latest = Cli::try_parse_from(["thndrs", "session", "latest"]).expect("parse");
        assert_eq!(
            latest.command,
            Some(Command::Session { command: commands::session::SessionCommand::Latest })
        );

        let titles = Cli::try_parse_from(["thndrs", "session", "titles"]).expect("parse");
        assert_eq!(
            titles.command,
            Some(Command::Session { command: commands::session::SessionCommand::Titles })
        );

        let show = Cli::try_parse_from(["thndrs", "session", "show", "session-1"]).expect("parse");
        assert_eq!(
            show.command,
            Some(Command::Session {
                command: commands::session::SessionCommand::Show { session_id: "session-1".to_string() }
            })
        );

        let resume = Cli::try_parse_from(["thndrs", "sessions", "resume", "session-1"]).expect("parse");
        assert_eq!(
            resume.command,
            Some(Command::Session {
                command: commands::session::SessionCommand::Resume { session_id: "session-1".to_string() }
            })
        );

        let inspect =
            Cli::try_parse_from(["thndrs", "sessions", "inspect", "session-1", "--format", "json"]).expect("parse");
        assert_eq!(
            inspect.command,
            Some(Command::Session {
                command: commands::session::SessionCommand::Inspect {
                    session_id: "session-1".to_string(),
                    format: commands::session::SessionDataFormat::Json,
                }
            })
        );

        let export =
            Cli::try_parse_from(["thndrs", "sessions", "export", "session-1", "--format", "jsonl"]).expect("parse");
        assert_eq!(
            export.command,
            Some(Command::Session {
                command: commands::session::SessionCommand::Export {
                    session_id: "session-1".to_string(),
                    format: commands::session::SessionDataFormat::Jsonl,
                }
            })
        );

        let debug =
            Cli::try_parse_from(["thndrs", "debug", "session-log", "session-1", "--lines", "10"]).expect("parse");
        assert_eq!(
            debug.command,
            Some(Command::Debug {
                command: commands::debug::DebugCommand::SessionLog { session_id: "session-1".to_string(), lines: 10 }
            })
        );
    }
}
