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

    /// Web search provider policy.
    #[arg(long, value_enum, default_value_t = WebSearchMode::Auto)]
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

/// Web search policy for a turn.
#[derive(ValueEnum, Clone, Copy, Debug, Eq, PartialEq)]
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
        assert_eq!(cli.websearch, WebSearchMode::Auto);
        assert_eq!(cli.tick_rate_ms, 100);
        assert!(!cli.no_alt_screen);
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
