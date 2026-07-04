//! Command-line interface for `thndrs`.
//!
//! The entrypoint parses raw flags with [`clap`] directly into the flat [`Cli`]
//! runtime config.

use std::collections::BTreeMap;
use std::path::PathBuf;

use clap::parser::ValueSource;
use clap::{CommandFactory, FromArgMatches, Parser, ValueEnum};
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
    #[arg(long, default_value = "umans-coder")]
    pub model: String,
    /// Web search provider policy.
    #[arg(long, value_enum, default_value = "auto")]
    pub websearch: WebSearchMode,
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
}

impl Default for Cli {
    /// Used by tests and as the implicit baseline before flag overrides.
    fn default() -> Self {
        Cli {
            cwd: PathBuf::from("."),
            model: String::from("umans-coder"),
            websearch: WebSearchMode::Auto,
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
        self.tick_rate_ms = config.tick_rate_ms.unwrap_or(defaults.tick_rate_ms);
        self.mouse = config.mouse.unwrap_or(defaults.mouse);
        self.verbose = config.verbose.unwrap_or(defaults.verbose);
        self.theme = config.theme.unwrap_or(defaults.theme);
        self.skill_dirs = config.skill_dirs;
        self.session_dir = config.session_dir;
        self.config_diagnostics = effective.diagnostics;
        self.config_layers = effective.layers;
        self.config_origins = origins;
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
        assert_eq!(cli.model, "umans-coder");
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
}
