//! TOML configuration loading.
//!
//! Config files are optional.
//!
//! Malformed files are errors so users do not run with silently ignored settings.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::cli::{Theme, WebSearchMode};

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
}

/// User-editable configuration loaded from TOML.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub model: Option<String>,
    pub websearch: Option<WebSearchMode>,
    pub tick_rate_ms: Option<u64>,
    pub no_alt_screen: Option<bool>,
    pub no_mouse: Option<bool>,
    pub mouse: Option<bool>,
    pub verbose: Option<bool>,
    pub print_prompt: Option<bool>,
    pub theme: Option<Theme>,
}

impl Config {
    /// Merge `other` over `self`, keeping existing values when `other` omits a field.
    pub fn merge(mut self, other: Config) -> Self {
        self.model = other.model.or(self.model);
        self.websearch = other.websearch.or(self.websearch);
        self.tick_rate_ms = other.tick_rate_ms.or(self.tick_rate_ms);
        self.no_alt_screen = other.no_alt_screen.or(self.no_alt_screen);
        self.no_mouse = other.no_mouse.or(self.no_mouse);
        self.mouse = other.mouse.or(self.mouse);
        self.verbose = other.verbose.or(self.verbose);
        self.print_prompt = other.print_prompt.or(self.print_prompt);
        self.theme = other.theme.or(self.theme);
        self
    }
}

/// Load global config followed by workspace config. Later files override earlier files.
pub fn load(workspace_root: &Path) -> Result<Config, ConfigError> {
    let mut config = Config::default();
    for path in global_candidates()
        .into_iter()
        .chain(project_candidates(workspace_root))
    {
        if path.is_file() {
            config = config.merge(load_file(&path)?);
        }
    }
    Ok(config)
}

fn load_file(path: &Path) -> Result<Config, ConfigError> {
    let content = fs::read_to_string(path).map_err(|source| ConfigError::Read { path: path.to_path_buf(), source })?;
    toml::from_str(&content).map_err(|source| ConfigError::Parse { path: path.to_path_buf(), source })
}

fn global_candidates() -> Vec<PathBuf> {
    match home_dir() {
        Some(home) => {
            let dir = home.join(".thndrs");
            vec![
                home.join(".thndrs.toml"),
                dir.join("config.toml"),
                dir.join(".thndrs.toml"),
                dir.join("thndrs.toml"),
            ]
        }
        None => Vec::new(),
    }
}

fn project_candidates(root: &Path) -> Vec<PathBuf> {
    let dir = root.join(".thndrs");
    let typo_dir = root.join(".thdrs");
    vec![
        root.join(".thndrs.toml"),
        dir.join("config.toml"),
        dir.join(".thndrs.toml"),
        dir.join("thndrs.toml"),
        typo_dir.join("config.toml"),
        typo_dir.join(".thndrs.toml"),
        typo_dir.join("thndrs.toml"),
    ]
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_merge_overrides_only_present_values() {
        let base = Config {
            model: Some("base".to_string()),
            websearch: Some(WebSearchMode::Auto),
            verbose: Some(false),
            ..Config::default()
        };
        let over = Config { websearch: Some(WebSearchMode::Native), mouse: Some(true), ..Config::default() };

        assert_eq!(
            base.merge(over),
            Config {
                model: Some("base".to_string()),
                websearch: Some(WebSearchMode::Native),
                verbose: Some(false),
                mouse: Some(true),
                ..Config::default()
            }
        );
    }

    #[test]
    fn parses_known_config_fields() {
        let config: Config = toml::from_str(
            r#"
            model = "umans-glm-5.2"
            websearch = "native"
            tick_rate_ms = 250
            mouse = true
            theme = "catppuccin-mocha"
            "#,
        )
        .expect("config parses");

        assert_eq!(config.model.as_deref(), Some("umans-glm-5.2"));
        assert_eq!(config.websearch, Some(WebSearchMode::Native));
        assert_eq!(config.tick_rate_ms, Some(250));
        assert_eq!(config.mouse, Some(true));
        assert_eq!(config.theme, Some(Theme::CatppuccinMocha));
    }

    #[test]
    fn rejects_unknown_config_fields() {
        let err = toml::from_str::<Config>("unknown = true").expect_err("unknown rejected");
        assert!(err.to_string().contains("unknown field"));
    }

    #[test]
    fn project_candidates_include_supported_spellings() {
        let root = Path::new("/repo");
        let candidates = project_candidates(root);

        assert!(candidates.contains(&PathBuf::from("/repo/.thndrs.toml")));
        assert!(candidates.contains(&PathBuf::from("/repo/.thndrs/config.toml")));
        assert!(candidates.contains(&PathBuf::from("/repo/.thndrs/.thndrs.toml")));
        assert!(candidates.contains(&PathBuf::from("/repo/.thndrs/thndrs.toml")));
        assert!(candidates.contains(&PathBuf::from("/repo/.thdrs/config.toml")));
    }
}
