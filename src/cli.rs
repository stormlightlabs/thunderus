//! Command-line interface for `thndrs`.
//!
//! The entrypoint parses raw flags with [`clap`] and normalizes them into a
//! flat [`Cli`] runtime config, plus the [`WebSearchMode`] value enum.

use std::path::{Path, PathBuf};

use clap::{Parser, ValueEnum};
use serde::Deserialize;

use crate::{config, context};

/// Clap entrypoint that launches the TUI when run with no subcommand.
#[derive(Parser, Debug)]
#[command(version, about = "agentic pair programmer")]
struct CliArgs {
    /// Working directory used for context loading and display.
    #[arg(long)]
    cwd: Option<PathBuf>,

    /// Model to use for completions.
    #[arg(long)]
    model: Option<String>,

    /// Web search provider policy.
    #[arg(long, value_enum)]
    websearch: Option<WebSearchMode>,

    /// Event poll interval in milliseconds.
    #[arg(long)]
    tick_rate_ms: Option<u64>,

    /// Run without the alternate screen buffer (for debugging and terminal-capture tests).
    #[arg(long, default_value_t = false)]
    no_alt_screen: bool,

    /// Disable terminal mouse capture so native selection and scrollback work.
    #[arg(long, default_value_t = false)]
    no_mouse: bool,

    /// Enable terminal mouse capture for wheel scrolling inside the TUI.
    #[arg(long, default_value_t = false)]
    mouse: bool,

    /// Show diagnostic transcript rows such as provider events and log paths.
    #[arg(long, default_value_t = false)]
    verbose: bool,

    /// UI color theme.
    #[arg(long, value_enum)]
    theme: Option<Theme>,

    /// Print the assembled prompt bundle/lowered messages with secrets redacted
    /// and exit without calling the provider.
    #[arg(long, default_value_t = false)]
    print_prompt: bool,
}

/// Normalized runtime configuration after defaults, TOML, and flags are merged.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Cli {
    /// Working directory used for context loading and display.
    pub cwd: PathBuf,
    /// Model to use for completions.
    pub model: String,
    /// Web search provider policy.
    pub websearch: WebSearchMode,
    /// Event poll interval in milliseconds.
    pub tick_rate_ms: u64,
    /// Run without the alternate screen buffer (for debugging and terminal-capture tests).
    pub no_alt_screen: bool,
    /// Disable terminal mouse capture so native selection and scrollback work.
    pub no_mouse: bool,
    /// Enable terminal mouse capture for wheel scrolling inside the TUI.
    pub mouse: bool,
    /// Show diagnostic transcript rows such as provider events and log paths.
    pub verbose: bool,
    /// UI color theme.
    pub theme: Theme,
    /// Print the assembled prompt bundle/lowered messages with secrets redacted.
    pub print_prompt: bool,
}

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
#[derive(ValueEnum, Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum WebSearchMode {
    /// Let thndrs choose provider-native search only for prompts that need it.
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
    /// - `Native` → `"native"` (Kimi-backed server-side search)
    /// - `Exa` → `"exa"` (Exa-backed server-side search)
    /// - `None` → `"none"` (disable server-side search; pass a local `web_search`
    ///   tool through unchanged)
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

impl Default for Cli {
    /// Used by tests and as the implicit baseline before flag overrides.
    fn default() -> Self {
        Cli {
            cwd: PathBuf::from("."),
            model: String::from("umans-coder"),
            websearch: WebSearchMode::Auto,
            tick_rate_ms: 100,
            no_alt_screen: false,
            no_mouse: false,
            mouse: false,
            verbose: false,
            theme: Theme::default(),
            print_prompt: false,
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

    /// Test-friendly parser that applies defaults but skips config file loading.
    pub fn try_parse_from<I, T>(itr: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        Ok(Self::from_parts(
            CliArgs::try_parse_from(itr)?,
            config::Config::default(),
        ))
    }

    fn try_parse_configured_from<I, T>(itr: I) -> Result<Result<Self, config::ConfigError>, clap::Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        let args = CliArgs::try_parse_from(itr)?;
        let workspace_root = context::discover_workspace_root(args.cwd.as_deref().unwrap_or_else(|| Path::new(".")));
        Ok(config::load(&workspace_root).map(|config| Self::from_parts(args, config)))
    }

    fn from_parts(args: CliArgs, config: config::Config) -> Self {
        let defaults = Self::default();
        Cli {
            cwd: args.cwd.unwrap_or(defaults.cwd),
            model: args.model.or(config.model).unwrap_or(defaults.model),
            websearch: args.websearch.or(config.websearch).unwrap_or(defaults.websearch),
            tick_rate_ms: args
                .tick_rate_ms
                .or(config.tick_rate_ms)
                .unwrap_or(defaults.tick_rate_ms),
            no_alt_screen: args.no_alt_screen || config.no_alt_screen.unwrap_or(defaults.no_alt_screen),
            no_mouse: args.no_mouse || config.no_mouse.unwrap_or(defaults.no_mouse),
            mouse: args.mouse || config.mouse.unwrap_or(defaults.mouse),
            verbose: args.verbose || config.verbose.unwrap_or(defaults.verbose),
            theme: args.theme.or(config.theme).unwrap_or(defaults.theme),
            print_prompt: args.print_prompt || config.print_prompt.unwrap_or(defaults.print_prompt),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cli_defaults_match_spec() {
        let cli = Cli::try_parse_from(["thndrs"]).expect("default parse");
        assert_eq!(cli.cwd, PathBuf::from("."));
        assert_eq!(cli.model, "umans-coder");
        assert_eq!(cli.websearch, WebSearchMode::Auto);
        assert_eq!(cli.tick_rate_ms, 100);
        assert!(!cli.no_alt_screen);
        assert!(!cli.no_mouse);
        assert!(!cli.mouse);
        assert!(!cli.verbose);
        assert_eq!(cli.theme, Theme::EldritchMinimal);
    }

    #[test]
    fn cli_args_override_config_values() {
        let args = CliArgs::try_parse_from([
            "thndrs",
            "--model",
            "cli-model",
            "--verbose",
            "--theme",
            "catppuccin-mocha",
        ])
        .unwrap();
        let config = config::Config {
            model: Some("config-model".to_string()),
            websearch: Some(WebSearchMode::Native),
            tick_rate_ms: Some(250),
            verbose: Some(false),
            theme: Some(Theme::EldritchMinimal),
            ..config::Config::default()
        };

        let cli = Cli::from_parts(args, config);

        assert_eq!(cli.model, "cli-model");
        assert_eq!(cli.websearch, WebSearchMode::Native);
        assert_eq!(cli.tick_rate_ms, 250);
        assert!(cli.verbose);
        assert_eq!(cli.theme, Theme::CatppuccinMocha);
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
