use super::*;
use crate::cli::{Theme, WebSearchMode};
use std::path::PathBuf;

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
        skill_dirs = ["vendor/agent-skills"]
        session_dir = "/tmp/sessions"
        default_workspace = "/home/user/projects"
        "#,
    )
    .expect("config parses");

    assert_eq!(config.model.as_deref(), Some("umans-glm-5.2"));
    assert_eq!(config.websearch, Some(WebSearchMode::Native));
    assert_eq!(config.tick_rate_ms, Some(250));
    assert_eq!(config.mouse, Some(true));
    assert_eq!(config.theme, Some(Theme::CatppuccinMocha));
    assert_eq!(config.skill_dirs, vec![PathBuf::from("vendor/agent-skills")]);
    assert_eq!(config.session_dir, Some(PathBuf::from("/tmp/sessions")));
    assert_eq!(config.default_workspace, Some(PathBuf::from("/home/user/projects")));
}

#[test]
fn rejects_unknown_config_fields() {
    let err = toml::from_str::<Config>("unknown = true").expect_err("unknown rejected");
    assert!(err.to_string().contains("unknown field"));
}

#[test]
fn rejects_lsp_enabled_as_unknown_key() {
    let err = toml::from_str::<Config>("lsp_enabled = true").expect_err("lsp_enabled rejected");
    assert!(err.to_string().contains("unknown field"));
}

#[test]
fn rejects_print_prompt_as_config_key() {
    let err = toml::from_str::<Config>("print_prompt = true").expect_err("print_prompt rejected");
    assert!(err.to_string().contains("unknown field"));
}

#[test]
fn rejects_cwd_as_config_key() {
    let err = toml::from_str::<Config>("cwd = \"/tmp\"").expect_err("cwd rejected");
    assert!(err.to_string().contains("unknown field"));
}

#[test]
fn rejects_no_mouse_as_config_key() {
    let err = toml::from_str::<Config>("no_mouse = true").expect_err("no_mouse rejected");
    assert!(err.to_string().contains("unknown field"));
}

#[test]
fn rejects_no_alt_screen_as_config_key() {
    let err = toml::from_str::<Config>("no_alt_screen = true").expect_err("no_alt_screen rejected");
    assert!(err.to_string().contains("unknown field"));
}

#[test]
fn rejects_secret_shaped_api_key() {
    let err = check_for_secret_keys("umans_api_key = \"abc\"").expect_err("secret key rejected");
    assert!(matches!(err, ConfigError::SecretInConfig { key } if key == "umans_api_key"));
}

#[test]
fn rejects_secret_shaped_token() {
    let err = check_for_secret_keys("auth_token = \"abc\"").expect_err("secret key rejected");
    assert!(matches!(err, ConfigError::SecretInConfig { key } if key == "auth_token"));
}

#[test]
fn rejects_secret_shaped_password() {
    let err = check_for_secret_keys("password = \"abc\"").expect_err("secret key rejected");
    assert!(matches!(err, ConfigError::SecretInConfig { key } if key == "password"));
}

#[test]
fn rejects_nested_secret_shaped_key() {
    let err = check_for_secret_keys("[provider]\napi_token = \"abc\"").expect_err("nested secret key rejected");
    assert!(matches!(err, ConfigError::SecretInConfig { key } if key == "provider.api_token"));
}

#[test]
fn rejects_dotted_secret_shaped_key() {
    let err = check_for_secret_keys("provider.api_token = \"abc\"").expect_err("dotted secret key rejected");
    assert!(matches!(err, ConfigError::SecretInConfig { key } if key == "provider.api_token"));
}

#[test]
fn allows_normal_keys_without_secret_check() {
    check_for_secret_keys("model = \"umans-coder\"").expect("normal keys pass");
}

#[test]
fn env_loads_model() {
    let mut origins = BTreeMap::new();
    let mut diagnostics = Vec::new();
    let config = load_env(
        &[("THNDRS_MODEL".to_string(), "umans-glm-5.2".to_string())],
        &mut origins,
        &mut diagnostics,
    )
    .unwrap();

    assert_eq!(config.model.as_deref(), Some("umans-glm-5.2"));
    assert_eq!(
        origins.get("model"),
        Some(&ConfigOrigin { source: ConfigSource::Environment, detail: "THNDRS_MODEL".to_string() })
    );
}

#[test]
fn env_loads_boolean_mouse() {
    for (val, expected) in [
        ("1", true),
        ("true", true),
        ("yes", true),
        ("on", true),
        ("0", false),
        ("false", false),
        ("no", false),
        ("off", false),
    ] {
        let mut o = BTreeMap::new();
        let mut d = Vec::new();
        let config = load_env(&[("THNDRS_MOUSE".to_string(), val.to_string())], &mut o, &mut d).unwrap();
        assert_eq!(
            config.mouse,
            Some(expected),
            "THNDRS_MOUSE={val} should parse as {expected}"
        );
    }
}

#[test]
fn env_boolean_case_insensitive() {
    let mut o = BTreeMap::new();
    let mut d = Vec::new();
    let config = load_env(&[("THNDRS_VERBOSE".to_string(), "YES".to_string())], &mut o, &mut d).unwrap();
    assert_eq!(config.verbose, Some(true));

    let mut o = BTreeMap::new();
    let mut d = Vec::new();
    let config = load_env(&[("THNDRS_VERBOSE".to_string(), "Off".to_string())], &mut o, &mut d).unwrap();
    assert_eq!(config.verbose, Some(false));
}

#[test]
fn env_rejects_invalid_boolean() {
    let mut o = BTreeMap::new();
    let mut d = Vec::new();
    let err = load_env(&[("THNDRS_MOUSE".to_string(), "maybe".to_string())], &mut o, &mut d).unwrap_err();
    assert!(matches!(err, ConfigError::InvalidEnv { name, .. } if name == "THNDRS_MOUSE"));
}

#[test]
fn env_rejects_unknown_var() {
    let mut o = BTreeMap::new();
    let mut d = Vec::new();
    let err = load_env(
        &[("THNDRS_LSP_ENABLED".to_string(), "true".to_string())],
        &mut o,
        &mut d,
    )
    .unwrap_err();
    assert!(matches!(err, ConfigError::UnknownEnv { name } if name == "THNDRS_LSP_ENABLED"));
}

#[test]
fn env_loads_websearch() {
    let mut o = BTreeMap::new();
    let mut d = Vec::new();
    let config = load_env(&[("THNDRS_WEBSEARCH".to_string(), "exa".to_string())], &mut o, &mut d).unwrap();
    assert_eq!(config.websearch, Some(WebSearchMode::Exa));
}

#[test]
fn env_rejects_invalid_websearch() {
    let mut o = BTreeMap::new();
    let mut d = Vec::new();
    let err = load_env(
        &[("THNDRS_WEBSEARCH".to_string(), "google".to_string())],
        &mut o,
        &mut d,
    )
    .unwrap_err();
    assert!(matches!(err, ConfigError::InvalidEnv { name, .. } if name == "THNDRS_WEBSEARCH"));
}

#[test]
fn env_loads_skill_dirs_path_list() {
    let mut o = BTreeMap::new();
    let mut d = Vec::new();
    let separator = if cfg!(windows) { ";" } else { ":" };
    let val = format!("/a/b{separator}/c/d");
    let config = load_env(&[("THNDRS_SKILL_DIRS".to_string(), val)], &mut o, &mut d).unwrap();
    assert_eq!(config.skill_dirs, vec![PathBuf::from("/a/b"), PathBuf::from("/c/d")]);
}

#[test]
fn env_loads_session_dir() {
    let mut o = BTreeMap::new();
    let mut d = Vec::new();
    let config = load_env(
        &[("THNDRS_SESSION_DIR".to_string(), "/tmp/sessions".to_string())],
        &mut o,
        &mut d,
    )
    .unwrap();
    assert_eq!(config.session_dir, Some(PathBuf::from("/tmp/sessions")));
}

#[test]
fn env_loads_default_workspace() {
    let mut o = BTreeMap::new();
    let mut d = Vec::new();
    let config = load_env(
        &[(
            "THNDRS_DEFAULT_WORKSPACE".to_string(),
            "/home/user/projects".to_string(),
        )],
        &mut o,
        &mut d,
    )
    .unwrap();
    assert_eq!(config.default_workspace, Some(PathBuf::from("/home/user/projects")));
}

#[test]
fn env_ignores_non_thndrs_vars() {
    let mut o = BTreeMap::new();
    let mut d = Vec::new();
    let config = load_env(&[("UMANS_API_KEY".to_string(), "secret".to_string())], &mut o, &mut d).unwrap();
    assert_eq!(config, Config::default());
}

#[test]
fn effective_config_defaults_when_no_files() {
    let tmp = tempfile::tempdir().unwrap();
    let cli_flags = CliFlagValues::default();
    let effective = load_effective(tmp.path(), &cli_flags, &[]).unwrap();

    assert!(effective.layers.is_empty(), "no config files should produce no layers");
    assert_eq!(effective.config.model.as_deref(), Some("umans-coder"));
    assert_eq!(effective.config.websearch, Some(WebSearchMode::Auto));
    assert_eq!(effective.config.tick_rate_ms, Some(100));
    assert_eq!(effective.config.mouse, Some(false));
    assert_eq!(effective.config.verbose, Some(false));
    assert_eq!(effective.config.theme, Some(Theme::EldritchMinimal));
    assert_eq!(
        effective.config.session_dir,
        Some(tmp.path().join(".thndrs").join("sessions"))
    );
    assert!(
        effective
            .config
            .default_workspace
            .as_ref()
            .is_some_and(|path| path.is_absolute()),
        "default_workspace should be resolved"
    );
    assert!(
        effective.origins.values().all(|o| o.source == ConfigSource::Default),
        "all origins should be default"
    );
}

#[test]
fn effective_config_default_session_dir_is_absolute_for_relative_workspace() {
    let cli_flags = CliFlagValues::default();
    let effective = load_effective(Path::new("."), &cli_flags, &[]).unwrap();

    assert!(
        effective
            .config
            .session_dir
            .as_ref()
            .is_some_and(|path| path.is_absolute()),
        "default session_dir should be resolved"
    );
}

#[test]
fn effective_config_loads_global_file() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(home.join(".thndrs")).unwrap();
    fs::write(home.join(".thndrs").join("config.toml"), "model = \"global-model\"\n").unwrap();

    let old_home = std::env::var_os("HOME");
    unsafe {
        std::env::set_var("HOME", &home);
    }

    let cli_flags = CliFlagValues::default();
    let effective = load_effective(tmp.path(), &cli_flags, &[]).unwrap();

    assert_eq!(effective.config.model.as_deref(), Some("global-model"));
    assert_eq!(effective.layers.len(), 1);
    assert_eq!(effective.layers[0].source, ConfigSource::GlobalFile);
    assert_eq!(
        effective.layers[0].display_path.as_deref(),
        Some("~/.thndrs/config.toml")
    );
    assert!(effective.layers[0].hash.is_some(), "global file should have a hash");

    unsafe {
        if let Some(old_home) = old_home {
            std::env::set_var("HOME", old_home);
        } else {
            std::env::remove_var("HOME");
        }
    }
}

#[test]
fn effective_config_project_overrides_global() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(home.join(".thndrs")).unwrap();
    fs::write(home.join(".thndrs").join("config.toml"), "model = \"global\"\n").unwrap();

    let workspace = tmp.path().join("workspace");
    fs::create_dir_all(workspace.join(".thndrs")).unwrap();
    fs::write(workspace.join(".thndrs").join("config.toml"), "model = \"project\"\n").unwrap();

    let old_home = std::env::var_os("HOME");

    unsafe {
        std::env::set_var("HOME", &home);
    }

    let cli_flags = CliFlagValues::default();
    let effective = load_effective(&workspace, &cli_flags, &[]).unwrap();

    assert_eq!(effective.config.model.as_deref(), Some("project"));
    assert_eq!(effective.layers.len(), 2);
    assert_eq!(effective.layers[1].display_path.as_deref(), Some(".thndrs/config.toml"));

    unsafe {
        if let Some(old_home) = old_home {
            std::env::set_var("HOME", old_home);
        } else {
            std::env::remove_var("HOME");
        }
    }
}

#[test]
fn effective_config_env_overrides_project() {
    let tmp = tempfile::tempdir().unwrap();
    let workspace = tmp.path();
    fs::create_dir_all(workspace.join(".thndrs")).unwrap();
    fs::write(workspace.join(".thndrs").join("config.toml"), "model = \"project\"\n").unwrap();

    let cli_flags = CliFlagValues::default();
    let effective = load_effective(
        workspace,
        &cli_flags,
        &[("THNDRS_MODEL".to_string(), "env-model".to_string())],
    )
    .unwrap();

    assert_eq!(effective.config.model.as_deref(), Some("env-model"));
    assert_eq!(
        effective.origins.get("model"),
        Some(&ConfigOrigin { source: ConfigSource::Environment, detail: "THNDRS_MODEL".to_string() })
    );
}

#[test]
fn effective_config_cli_overrides_env() {
    let tmp = tempfile::tempdir().unwrap();
    let cli_flags = CliFlagValues { model: Some("cli-model".to_string()), ..CliFlagValues::default() };
    let effective = load_effective(
        tmp.path(),
        &cli_flags,
        &[("THNDRS_MODEL".to_string(), "env-model".to_string())],
    )
    .unwrap();

    assert_eq!(effective.config.model.as_deref(), Some("cli-model"));
    assert_eq!(
        effective.origins.get("model"),
        Some(&ConfigOrigin { source: ConfigSource::CliFlag, detail: "--model".to_string() })
    );
}

#[test]
fn effective_config_cli_mouse_overrides_env() {
    let tmp = tempfile::tempdir().unwrap();
    let cli_flags = CliFlagValues { mouse: Some(false), ..CliFlagValues::default() };
    let effective = load_effective(
        tmp.path(),
        &cli_flags,
        &[("THNDRS_MOUSE".to_string(), "true".to_string())],
    )
    .unwrap();

    assert_eq!(effective.config.mouse, Some(false));
    assert_eq!(
        effective.origins.get("mouse"),
        Some(&ConfigOrigin { source: ConfigSource::CliFlag, detail: "--mouse/--no-mouse".to_string() })
    );
}

#[test]
fn load_effective_resolves_paths_without_dropping_env_or_cli_skill_dirs() {
    let tmp = tempfile::tempdir().unwrap();
    let workspace = tmp.path().join("workspace");
    fs::create_dir_all(workspace.join(".thndrs")).unwrap();
    fs::write(
        workspace.join(".thndrs").join("config.toml"),
        "skill_dirs = [\"project-skills\"]\nsession_dir = \"project-sessions\"\ndefault_workspace = \"project-workspace\"\n",
    )
    .unwrap();

    let cwd = std::env::current_dir().unwrap();
    let cli_flags = CliFlagValues {
        skill_dirs: vec![PathBuf::from("cli-skills")],
        session_dir: Some(PathBuf::from("cli-sessions")),
        ..CliFlagValues::default()
    };
    let env_vars = [
        ("THNDRS_SKILL_DIRS".to_string(), "env-skills".to_string()),
        ("THNDRS_SESSION_DIR".to_string(), "env-sessions".to_string()),
    ];

    let effective = load_effective(&workspace, &cli_flags, &env_vars).unwrap();

    assert_eq!(
        effective.config.skill_dirs,
        vec![
            workspace.join(".thndrs").join("project-skills"),
            cwd.join("env-skills"),
            cwd.join("cli-skills"),
        ]
    );
    assert_eq!(effective.config.session_dir, Some(cwd.join("cli-sessions")));
    assert_eq!(
        effective.config.default_workspace,
        Some(workspace.join(".thndrs").join("project-workspace"))
    );
}

#[test]
fn resolve_paths_makes_skill_dirs_absolute() {
    let tmp = tempfile::tempdir().unwrap();
    let config_dir = tmp.path().join(".thndrs");
    fs::create_dir_all(&config_dir).unwrap();
    let config_path = config_dir.join("config.toml");
    fs::write(&config_path, "skill_dirs = [\"vendor/skills\", \"/abs/path\"]\n").unwrap();

    let (config, hash) = load_file(&config_path).unwrap();
    let layers = vec![LoadedConfigLayer {
        source: ConfigSource::ProjectFile,
        config: config.clone(),
        path: Some(config_path.clone()),
        display_path: Some(".thndrs/config.toml".to_string()),
        hash: Some(hash),
    }];

    let mut merged = config;
    resolve_paths(&mut merged, &layers, tmp.path());

    assert!(
        merged.skill_dirs[0].is_absolute(),
        "relative skill_dir should be resolved to absolute"
    );
    assert!(
        merged.skill_dirs[0].ends_with("vendor/skills"),
        "resolved path should preserve the relative suffix"
    );
    assert_eq!(
        merged.skill_dirs[1],
        PathBuf::from("/abs/path"),
        "absolute path should be unchanged"
    );
}

#[test]
fn resolve_paths_deduplicates_skill_dirs() {
    let tmp = tempfile::tempdir().unwrap();
    let config_dir = tmp.path().join(".thndrs");
    fs::create_dir_all(&config_dir).unwrap();

    let config_path = config_dir.join("config.toml");
    fs::write(&config_path, "skill_dirs = [\"skills\", \"skills\"]\n").unwrap();

    let (config, hash) = load_file(&config_path).unwrap();
    let layers = vec![LoadedConfigLayer {
        source: ConfigSource::ProjectFile,
        config: config.clone(),
        path: Some(config_path.clone()),
        display_path: Some(".thndrs/config.toml".to_string()),
        hash: Some(hash),
    }];

    let mut merged = config;
    resolve_paths(&mut merged, &layers, tmp.path());

    assert_eq!(
        merged.skill_dirs.len(),
        1,
        "duplicate skill dirs should be deduplicated"
    );
}

#[test]
fn global_config_path_is_thndrs_config_toml() {
    let home = std::env::var_os("HOME");
    if let Some(home) = home {
        unsafe {
            std::env::set_var("HOME", &home);
        }
        let path = global_config_path().unwrap();
        let p = match path.to_str() {
            Some(s) => s,
            None => "",
        }
        .to_string();
        assert!(
            path.ends_with(".thndrs/config.toml"),
            "global path should be ~/.thndrs/config.toml: {p}"
        );
    }
}

#[test]
fn project_config_path_is_under_workspace() {
    let path = project_config_path(Path::new("/repo"));
    assert_eq!(path, PathBuf::from("/repo/.thndrs/config.toml"));
}
