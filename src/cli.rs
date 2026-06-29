//! Command-line interface for `thndrs`.
//!
//! The entrypoint is a flat, single [`Cli`] struct parsed with [`clap`]
//! derive ([`Parser`]), plus the [`WebSearchMode`] value enum.

use std::path::PathBuf;

use clap::{Parser, ValueEnum};

/// Clap entrypoint that launches the TUI when run with no subcommand.
#[derive(Parser, Debug)]
#[command(version, about = "Minimal Rust + Ratatui coding harness")]
pub struct Cli {
    /// Working directory used for context loading and display.
    #[arg(long, default_value = ".")]
    pub cwd: PathBuf,

    /// Model to use for completions.
    #[arg(long, default_value = "umans-coder")]
    pub model: String,

    /// Web search provider to forward to the model.
    #[arg(long, value_enum, default_value_t = WebSearchMode::Native)]
    pub websearch: WebSearchMode,

    /// Event poll interval in milliseconds.
    #[arg(long, default_value_t = 100)]
    pub tick_rate_ms: u64,

    /// Run without the alternate screen buffer (for debugging and terminal-capture tests).
    #[arg(long, default_value_t = false)]
    pub no_alt_screen: bool,

    /// Print the assembled prompt bundle/lowered messages with secrets redacted
    /// and exit without calling the provider.
    #[arg(long, default_value_t = false)]
    pub print_prompt: bool,
}

/// Maps directly to `X-Umans-Websearch-Provider`.
#[derive(ValueEnum, Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebSearchMode {
    /// Umans server-side native web search.
    Native,
    /// Exa-backed server-side search for manual experiments.
    Exa,
    /// Pass a local `web_search` tool through unchanged.
    None,
}

impl Default for Cli {
    /// Used by tests and as the implicit baseline before flag overrides.
    fn default() -> Self {
        Cli {
            cwd: PathBuf::from("."),
            model: String::from("umans-coder"),
            websearch: WebSearchMode::Native,
            tick_rate_ms: 100,
            no_alt_screen: false,
            print_prompt: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn cli_defaults_match_spec() {
        let cli = Cli::try_parse_from(["thndrs"]).expect("default parse");
        assert_eq!(cli.cwd, PathBuf::from("."));
        assert_eq!(cli.model, "umans-coder");
        assert_eq!(cli.websearch, WebSearchMode::Native);
        assert_eq!(cli.tick_rate_ms, 100);
        assert!(!cli.no_alt_screen);
    }

    #[test]
    fn websearch_explicit_values_parse() {
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
    fn print_prompt_defaults_false() {
        let cli = Cli::try_parse_from(["thndrs"]).expect("parse");
        assert!(!cli.print_prompt);
    }
}
