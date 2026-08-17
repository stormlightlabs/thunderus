use super::*;
use crate::cli::{DEFAULT_TICK_RATE_MS, ReasoningEffort, ReasoningSummary, Theme};
use std::path::PathBuf;

fn with_home<T>(home: &Path, f: impl FnOnce() -> T) -> T {
    let _guard = crate::test_env::lock();
    let old_home = std::env::var_os("HOME");

    unsafe {
        std::env::set_var("HOME", home);
    }

    let result = f();

    unsafe {
        if let Some(old_home) = old_home {
            std::env::set_var("HOME", old_home);
        } else {
            std::env::remove_var("HOME");
        }
    }

    result
}

#[test]
fn config_merge_overrides_only_present_values() {
    let base = Config { model: Some("base".to_string()), verbose: Some(false), ..Config::default() };
    let over = Config::default();

    assert_eq!(
        base.merge(over),
        Config { model: Some("base".to_string()), verbose: Some(false), ..Config::default() }
    );
}

#[test]
fn config_merge_preserves_an_omitted_status_line() {
    let status_line = StatusLineConfig {
        left: vec![
            StatusSegment::RunState,
            StatusSegment::Authority,
            StatusSegment::Workspace,
        ],
        right: vec![StatusSegment::Route, StatusSegment::QueueCount],
    };
    let base = Config { status_line: Some(status_line.clone()), ..Config::default() };

    assert_eq!(base.merge(Config::default()).status_line, Some(status_line));
}

#[test]
fn session_retention_config_parses_and_validates_age_order() {
    let parsed: Config = toml::from_str(
        "[session_retention]\nenabled = false\nmax_age_days = 45\nmax_live_count = 80\nmin_age_days = 2\ntrash_retention_days = 10\n",
    )
    .expect("retention config");
    let policy = parsed.session_retention.expect("policy");
    assert!(!policy.enabled);
    assert_eq!(policy.max_age_days, 45);
    assert_eq!(policy.max_live_count, 80);
    assert_eq!(policy.min_age_days, 2);
    assert_eq!(policy.trash_retention_days, 10);

    let invalid = Config {
        session_retention: Some(crate::session::SessionRetentionPolicy {
            max_age_days: 1,
            min_age_days: 2,
            ..Default::default()
        }),
        ..Default::default()
    };
    assert!(matches!(
        validate_config(&invalid),
        Err(ConfigError::InvalidConfig { key, .. }) if key == "session_retention.min_age_days"
    ));
}

#[test]
fn parses_and_validates_typed_status_line_fields() {
    let config: Config = toml::from_str(
        r#"
        [status_line]
        left = ["run-state", "active-tool", "authority", "workspace"]
        right = ["route", "queue-count"]
        "#,
    )
    .expect("status line parses");

    validate_config(&config).expect("status line validates");
    let status = config.status_line.expect("status line exists");
    assert_eq!(status.left[0], StatusSegment::RunState);
    assert_eq!(status.right, [StatusSegment::Route, StatusSegment::QueueCount]);
}

#[test]
fn rejects_status_lines_without_the_active_model() {
    let config: Config = toml::from_str(
        r#"
        [status_line]
        left = ["run-state", "authority"]
        right = ["queue-count"]
        "#,
    )
    .expect("status line parses");

    let error = validate_config(&config).expect_err("active model must remain visible");
    assert!(matches!(error, ConfigError::InvalidConfig { key, message }
            if key == "status_line" && message.contains("`route` is required")));
}

#[test]
fn rejects_unknown_status_line_fields() {
    let error = toml::from_str::<Config>(
        r#"
        [status_line]
        left = ["run-state", "authority", "shell-command"]
        right = []
        "#,
    )
    .expect_err("unknown status fields must fail");

    assert!(error.to_string().contains("shell-command"));
}

#[test]
fn retired_websearch_config_keys_are_rejected() {
    let error = toml::from_str::<Config>("websearch = \"none\"").expect_err("retired key");
    assert!(error.to_string().contains("websearch"));
}

#[test]
fn parses_known_config_fields() {
    let config: Config = toml::from_str(
        r#"
        model = "umans-glm-5.2"
        reasoning_effort = "xhigh"
        reasoning_summary = "auto"
        tick_rate_ms = 250
        theme = "catppuccin-mocha"
        skill_dirs = ["vendor/agent-skills"]
        session_dir = "/tmp/sessions"
        default_workspace = "/home/user/projects"

        [acp_agents.claude]
        command = "claude"
        args = ["--acp"]
        env = { FOO = "bar" }
        enabled = false
        timeout_secs = 15
        "#,
    )
    .expect("config parses");

    assert_eq!(config.model.as_deref(), Some("umans-glm-5.2"));
    assert_eq!(config.reasoning_effort, Some(ReasoningEffort::Xhigh));
    assert_eq!(config.reasoning_summary, Some(ReasoningSummary::Auto));
    assert_eq!(config.tick_rate_ms, Some(250));
    assert_eq!(config.theme, Some(Theme::CatppuccinMocha));
    assert_eq!(config.skill_dirs, vec![PathBuf::from("vendor/agent-skills")]);
    assert_eq!(config.session_dir, Some(PathBuf::from("/tmp/sessions")));
    assert_eq!(config.default_workspace, Some(PathBuf::from("/home/user/projects")));
    assert_eq!(config.acp_agents["claude"].command, "claude");
    assert_eq!(config.acp_agents["claude"].args, vec!["--acp"]);
    assert_eq!(config.acp_agents["claude"].env["FOO"], "bar");
    assert!(!config.acp_agents["claude"].enabled);
    assert_eq!(config.acp_agents["claude"].timeout_secs, 15);
}

#[test]
fn rejects_unknown_config_fields() {
    let err = toml::from_str::<Config>("unknown = true").expect_err("unknown rejected");
    assert!(err.to_string().contains("unknown field"));
}

#[test]
fn rejects_unknown_acp_agent_fields() {
    let err = toml::from_str::<Config>(
        r#"
        [acp_agents.local]
        command = "agent"
        transport = "tcp"
        "#,
    )
    .expect_err("unknown ACP field rejected");
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
fn rejects_mouse_as_config_key() {
    let err = toml::from_str::<Config>("mouse = true").expect_err("mouse rejected");
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
fn rejects_secret_shaped_acp_env_key() {
    let err = check_for_secret_keys(
        r#"
        [acp_agents.local]
        command = "agent"
        env.OPENAI_API_KEY = "secret"
        "#,
    )
    .expect_err("ACP env secret key rejected");
    assert!(matches!(err, ConfigError::SecretInConfig { key } if key == "acp_agents.local.env.OPENAI_API_KEY"));
}

#[test]
fn validates_acp_agent_names() {
    assert!(validate_acp_agent_name("claude_1-local").is_ok());
    let err = validate_acp_agent_name("bad/name").expect_err("invalid name rejected");
    assert!(
        matches!(err, ConfigError::InvalidConfig { key, message } if key == "acp_agents.bad/name" && message.contains("[A-Za-z0-9_-]+"))
    );
}

#[test]
fn load_file_requires_acp_agent_command() {
    let tmp = tempfile::tempdir().unwrap();
    let config_path = tmp.path().join("config.toml");
    fs::write(&config_path, "[acp_agents.local]\n").unwrap();

    let err = load_file(&config_path).expect_err("missing command rejected");

    assert!(
        matches!(err, ConfigError::InvalidConfig { key, message } if key == "acp_agents.local.command" && message == "command is required")
    );
}

#[test]
fn acp_agent_defaults_are_applied() {
    let config: Config = toml::from_str(
        r#"
        [acp_agents.local]
        command = "agent"
        "#,
    )
    .expect("config parses");

    let agent = &config.acp_agents["local"];
    assert!(agent.args.is_empty());
    assert!(agent.env.is_empty());
    assert!(agent.enabled);
    assert_eq!(agent.timeout_secs, 60);
}

#[test]
fn allows_normal_keys_without_secret_check() {
    check_for_secret_keys("model = \"umans-coder\"").expect("normal keys pass");
}

#[test]
fn write_project_model_creates_project_config() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let path = write_project_model(tmp.path(), "chatgpt-codex/gpt-5.5").expect("write project model");

    assert_eq!(path, tmp.path().join(".thndrs").join("config.toml"));
    assert_eq!(
        fs::read_to_string(path).expect("read project config"),
        "model = \"chatgpt-codex/gpt-5.5\"\n"
    );
}

#[test]
fn write_project_reasoning_effort_preserves_model_and_agent_config() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join(".thndrs").join("config.toml");
    fs::create_dir_all(path.parent().expect("config parent")).expect("create config parent");
    fs::write(
        &path,
        r#"model = "chatgpt-codex/gpt-5.6-terra"

[acp_agents.local]
command = "agent"
"#,
    )
    .expect("seed config");

    let written = write_project_reasoning_effort(tmp.path(), ReasoningEffort::Max).expect("write effort");

    assert_eq!(written, path);
    assert_eq!(
        fs::read_to_string(path).expect("read config"),
        r#"model = "chatgpt-codex/gpt-5.6-terra"
reasoning_effort = "max"

[acp_agents.local]
command = "agent"
"#
    );
}

#[test]
fn write_model_config_replaces_model_and_preserves_agent_config() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("config.toml");
    fs::write(
        &path,
        r#"model = "opencode/big-pickle"

[acp_agents.local]
command = "agent"
"#,
    )
    .expect("seed config");

    write_model_config(&path, "chatgpt-codex/gpt-5.5").expect("write model config");

    assert_eq!(
        fs::read_to_string(path).expect("read config"),
        r#"model = "chatgpt-codex/gpt-5.5"

[acp_agents.local]
command = "agent"
"#
    );
}

#[test]
fn write_model_config_inserts_before_first_table() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("config.toml");
    fs::write(
        &path,
        r#"# Project config

[acp_agents.local]
command = "agent"
"#,
    )
    .expect("seed config");

    write_model_config(&path, "chatgpt-codex/gpt-5.5").expect("write model config");

    assert_eq!(
        fs::read_to_string(path).expect("read config"),
        r#"model = "chatgpt-codex/gpt-5.5"
# Project config

[acp_agents.local]
command = "agent"
"#
    );
}

#[test]
fn write_model_config_reports_existing_read_errors() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let err = write_model_config(tmp.path(), "chatgpt-codex/gpt-5.5").expect_err("directory read should fail");

    assert_ne!(err.kind(), std::io::ErrorKind::NotFound);
}

#[test]
fn write_model_config_rejects_invalid_existing_configuration() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("config.toml");
    let original = "[acp_agents.local]\nmodel = \"invalid\"\n";
    fs::write(&path, original).expect("seed config");

    let error = write_model_config(&path, "chatgpt-codex/gpt-5.5").expect_err("invalid config rejected");

    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert_eq!(fs::read_to_string(path).expect("read config"), original);
}

#[test]
fn write_model_config_if_missing_does_not_replace_existing_top_level_model() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("config.toml");
    fs::write(&path, "model = \"opencode/big-pickle\"\n").expect("seed config");

    let wrote = write_model_config_if_missing(&path, "chatgpt-codex/gpt-5.5").expect("write missing model");

    assert!(!wrote);
    assert_eq!(
        fs::read_to_string(path).expect("read config"),
        "model = \"opencode/big-pickle\"\n"
    );
}

#[test]
fn write_model_config_if_missing_preserves_existing_agent_config() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("config.toml");
    fs::write(
        &path,
        r#"[acp_agents.local]
command = "agent"
"#,
    )
    .expect("seed config");

    let wrote = write_model_config_if_missing(&path, "chatgpt-codex/gpt-5.5").expect("write missing model");

    assert!(wrote);
    assert_eq!(
        fs::read_to_string(path).expect("read config"),
        r#"model = "chatgpt-codex/gpt-5.5"
[acp_agents.local]
command = "agent"
"#
    );
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
fn env_loads_reasoning_controls_and_rejects_invalid_values() {
    let mut origins = BTreeMap::new();
    let mut diagnostics = Vec::new();
    let config = load_env(
        &[
            ("THNDRS_REASONING_EFFORT".to_string(), "high".to_string()),
            ("THNDRS_REASONING_SUMMARY".to_string(), "auto".to_string()),
        ],
        &mut origins,
        &mut diagnostics,
    )
    .expect("load controls");

    assert_eq!(config.reasoning_effort, Some(ReasoningEffort::High));
    assert_eq!(config.reasoning_summary, Some(ReasoningSummary::Auto));
    assert!(
        load_env(
            &[("THNDRS_REASONING_EFFORT".to_string(), "unbounded".to_string())],
            &mut BTreeMap::new(),
            &mut Vec::new(),
        )
        .is_err()
    );
    assert_eq!(
        load_env(
            &[("THNDRS_REASONING_EFFORT".to_string(), "max".to_string())],
            &mut BTreeMap::new(),
            &mut Vec::new(),
        )
        .expect("max is valid")
        .reasoning_effort,
        Some(ReasoningEffort::Max)
    );
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
    let err = load_env(&[("THNDRS_VERBOSE".to_string(), "maybe".to_string())], &mut o, &mut d).unwrap_err();
    assert!(matches!(err, ConfigError::InvalidEnv { name, .. } if name == "THNDRS_VERBOSE"));
}

#[test]
fn env_rejects_invalid_tick_rate_ms() {
    let mut o = BTreeMap::new();
    let mut d = Vec::new();
    let err = load_env(
        &[("THNDRS_TICK_RATE_MS".to_string(), "fast".to_string())],
        &mut o,
        &mut d,
    )
    .unwrap_err();
    assert!(
        matches!(err, ConfigError::InvalidEnv { name, message } if name == "THNDRS_TICK_RATE_MS" && message.contains("positive integer"))
    );
}

#[test]
fn env_rejects_invalid_theme() {
    let mut o = BTreeMap::new();
    let mut d = Vec::new();
    let err = load_env(&[("THNDRS_THEME".to_string(), "sepia".to_string())], &mut o, &mut d).unwrap_err();
    assert!(
        matches!(err, ConfigError::InvalidEnv { name, message } if name == "THNDRS_THEME" && message.contains("unknown theme"))
    );
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
fn env_rejects_cli_only_keys() {
    for key in [
        "THNDRS_PRINT_PROMPT",
        "THNDRS_CWD",
        "THNDRS_NO_ALT_SCREEN",
        "THNDRS_NO_MOUSE",
    ] {
        let mut o = BTreeMap::new();
        let mut d = Vec::new();
        let err = load_env(&[(key.to_string(), "true".to_string())], &mut o, &mut d).unwrap_err();
        assert!(
            matches!(err, ConfigError::UnknownEnv { name } if name == key),
            "{key} should be rejected as a CLI-only key"
        );
    }
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
fn provider_secret_env_vars_do_not_enter_config_or_origins() {
    let mut origins = BTreeMap::new();
    let mut diagnostics = Vec::new();
    let config = load_env(
        &[
            ("UMANS_API_KEY".to_string(), "sk-umans-secret".to_string()),
            ("OPENCODE_GO_KEY".to_string(), "sk-opencode-secret".to_string()),
        ],
        &mut origins,
        &mut diagnostics,
    )
    .unwrap();

    assert_eq!(config, Config::default());
    assert!(origins.is_empty());
    assert!(diagnostics.is_empty());
}

#[test]
fn effective_config_defaults_when_no_files() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let workspace = tmp.path().join("workspace");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&workspace).unwrap();
    let effective = with_home(&home, || load_effective(&workspace, &[]).unwrap());

    assert!(effective.layers.is_empty(), "no config files should produce no layers");
    assert_eq!(effective.config.model, None);
    assert_eq!(effective.config.tick_rate_ms, Some(DEFAULT_TICK_RATE_MS));
    assert_eq!(effective.config.verbose, Some(false));
    assert_eq!(effective.config.theme, Some(Theme::EldritchMinimal));
    assert_eq!(
        effective.config.session_dir,
        Some(workspace.join(".thndrs").join("sessions"))
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
    let effective = load_effective(Path::new("."), &[]).unwrap();

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

    let effective = with_home(&home, || load_effective(tmp.path(), &[]).unwrap());

    assert_eq!(effective.config.model.as_deref(), Some("global-model"));
    assert_eq!(effective.layers.len(), 1);
    assert_eq!(effective.layers[0].source, ConfigSource::GlobalFile);
    assert_eq!(
        effective.layers[0].display_path.as_deref(),
        Some("~/.thndrs/config.toml")
    );
    assert!(effective.layers[0].hash.is_some(), "global file should have a hash");
}

#[test]
fn project_runtime_configuration_is_active() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let workspace = tmp.path().join("workspace");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(workspace.join(".thndrs")).unwrap();
    fs::write(
        workspace.join(".thndrs/config.toml"),
        "model = \"acp:project\"\n[acp_agents.project]\ncommand = \"project-agent\"\n",
    )
    .unwrap();

    let effective = with_home(&home, || load_effective(&workspace, &[]).unwrap());

    assert_eq!(effective.config.model.as_deref(), Some("acp:project"));
    assert_eq!(effective.config.acp_agents["project"].command, "project-agent");
    assert!(effective.diagnostics.is_empty());
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

    let effective = with_home(&home, || load_effective(&workspace, &[]).unwrap());

    assert_eq!(effective.config.model.as_deref(), Some("project"));
    assert_eq!(effective.layers.len(), 2);
    assert_eq!(effective.layers[1].display_path.as_deref(), Some(".thndrs/config.toml"));
}

#[test]
fn effective_config_project_overrides_global_acp_agent_by_name() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(home.join(".thndrs")).unwrap();
    fs::write(
        home.join(".thndrs").join("config.toml"),
        r#"
        [acp_agents.shared]
        command = "global-agent"
        args = ["--global"]

        [acp_agents.global_only]
        command = "global-only"
        "#,
    )
    .unwrap();

    let workspace = tmp.path().join("workspace");
    fs::create_dir_all(workspace.join(".thndrs")).unwrap();
    fs::write(
        workspace.join(".thndrs").join("config.toml"),
        r#"
        [acp_agents.shared]
        command = "project-agent"

        [acp_agents.project_only]
        command = "project-only"
        "#,
    )
    .unwrap();

    let effective = with_home(&home, || load_effective(&workspace, &[]).unwrap());

    assert_eq!(effective.config.acp_agents["shared"].command, "project-agent");
    assert!(effective.config.acp_agents["shared"].args.is_empty());
    assert_eq!(effective.config.acp_agents["global_only"].command, "global-only");
    assert_eq!(effective.config.acp_agents["project_only"].command, "project-only");
    assert_eq!(
        effective.origins.get("acp_agents.shared"),
        Some(&ConfigOrigin { source: ConfigSource::ProjectFile, detail: ".thndrs/config.toml".to_string() })
    );
}

#[test]
fn effective_config_preserves_disabled_acp_agent() {
    let tmp = tempfile::tempdir().unwrap();
    let workspace = tmp.path().join("workspace");
    fs::create_dir_all(workspace.join(".thndrs")).unwrap();
    fs::write(
        workspace.join(".thndrs").join("config.toml"),
        r#"
        [acp_agents.disabled]
        command = "agent"
        enabled = false
        "#,
    )
    .unwrap();

    let home = tmp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let effective = with_home(&home, || load_effective(&workspace, &[]).unwrap());

    assert!(!effective.config.acp_agents["disabled"].enabled);
}

#[test]
fn loaded_config_layers_redact_acp_env_values() {
    let tmp = tempfile::tempdir().unwrap();
    let workspace = tmp.path().join("workspace");
    fs::create_dir_all(workspace.join(".thndrs")).unwrap();
    fs::write(
        workspace.join(".thndrs").join("config.toml"),
        r#"
        [acp_agents.local]
        command = "agent"
        env = { FOO = "plain-value" }
        "#,
    )
    .unwrap();

    let home = tmp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let effective = with_home(&home, || load_effective(&workspace, &[]).unwrap());
    let redacted_layer = effective
        .layers
        .iter()
        .find(|layer| layer.config.acp_agents.contains_key("local"))
        .expect("project ACP config layer");

    assert_eq!(effective.config.acp_agents["local"].env["FOO"], "plain-value");
    assert_eq!(redacted_layer.config.acp_agents["local"].env["FOO"], "[redacted]");
}

#[test]
fn effective_config_env_overrides_config_files() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(home.join(".thndrs")).unwrap();
    fs::write(home.join(".thndrs").join("config.toml"), "model = \"global\"\n").unwrap();

    let workspace = tmp.path().join("workspace");
    fs::create_dir_all(workspace.join(".thndrs")).unwrap();
    fs::write(workspace.join(".thndrs").join("config.toml"), "model = \"project\"\n").unwrap();

    let effective = with_home(&home, || {
        load_effective(&workspace, &[("THNDRS_MODEL".to_string(), "env-model".to_string())]).unwrap()
    });

    assert_eq!(effective.config.model.as_deref(), Some("env-model"));
    assert_eq!(
        effective.origins.get("model"),
        Some(&ConfigOrigin { source: ConfigSource::Environment, detail: "THNDRS_MODEL".to_string() })
    );
}

#[test]
fn old_and_typo_project_config_paths_are_ignored() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let workspace = tmp.path().join("workspace");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(workspace.join(".thndrs")).unwrap();
    fs::create_dir_all(workspace.join(".thdrs")).unwrap();
    fs::write(workspace.join(".thndrs.toml"), "model = \"old-root\"\n").unwrap();
    fs::write(
        workspace.join(".thndrs").join(".thndrs.toml"),
        "model = \"old-hidden\"\n",
    )
    .unwrap();
    fs::write(workspace.join(".thndrs").join("thndrs.toml"), "model = \"old-name\"\n").unwrap();
    fs::write(
        workspace.join(".thdrs").join("config.toml"),
        "model = \"typo-config\"\n",
    )
    .unwrap();
    fs::write(
        workspace.join(".thdrs").join(".thndrs.toml"),
        "model = \"typo-hidden\"\n",
    )
    .unwrap();
    fs::write(workspace.join(".thdrs").join("thndrs.toml"), "model = \"typo-name\"\n").unwrap();

    let effective = with_home(&home, || load_effective(&workspace, &[]).unwrap());

    assert!(effective.layers.is_empty());
    assert_eq!(effective.config.model, None);
}

#[test]
fn old_global_config_paths_are_ignored() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(home.join(".thndrs")).unwrap();
    fs::write(home.join(".thndrs.toml"), "model = \"old-root\"\n").unwrap();
    fs::write(home.join(".thndrs").join(".thndrs.toml"), "model = \"old-hidden\"\n").unwrap();
    fs::write(home.join(".thndrs").join("thndrs.toml"), "model = \"old-name\"\n").unwrap();

    let effective = with_home(&home, || load_effective(tmp.path(), &[]).unwrap());

    assert!(effective.layers.is_empty());
    assert_eq!(effective.config.model, None);
}

#[test]
fn load_effective_resolves_paths_without_dropping_env_skill_dirs() {
    let tmp = tempfile::tempdir().unwrap();
    let workspace = tmp.path().join("workspace");
    fs::create_dir_all(workspace.join(".thndrs")).unwrap();
    fs::write(
        workspace.join(".thndrs").join("config.toml"),
        "skill_dirs = [\"project-skills\"]\nsession_dir = \"project-sessions\"\ndefault_workspace = \"project-workspace\"\n",
    )
    .unwrap();

    let cwd = std::env::current_dir().unwrap();
    let env_vars = [
        ("THNDRS_SKILL_DIRS".to_string(), "env-skills".to_string()),
        ("THNDRS_SESSION_DIR".to_string(), "env-sessions".to_string()),
    ];

    let home = tmp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let effective = with_home(&home, || load_effective(&workspace, &env_vars).unwrap());

    assert_eq!(
        effective.config.skill_dirs,
        vec![workspace.join(".thndrs").join("project-skills"), cwd.join("env-skills"),]
    );
    assert_eq!(effective.config.session_dir, Some(cwd.join("env-sessions")));
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
        path: Some(config_path),
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
        path: Some(config_path),
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
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(&home).unwrap();

    let path = with_home(&home, || global_config_path().unwrap());

    assert_eq!(path, home.join(".thndrs").join("config.toml"));
}

#[test]
fn project_config_path_is_under_workspace() {
    let path = project_config_path(Path::new("/repo"));
    assert_eq!(path, PathBuf::from("/repo/.thndrs/config.toml"));
}

#[test]
fn config_path_display_prefers_workspace_home_then_absolute() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let workspace = tmp.path().join("workspace");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&workspace).unwrap();

    let global = with_home(&home, || global_path_display(&home.join(".thndrs").join("config.toml")));
    assert_eq!(global, "~/.thndrs/config.toml");

    assert_eq!(
        project_path_display(&workspace.join(".thndrs").join("config.toml"), &workspace),
        ".thndrs/config.toml"
    );

    let outside = tmp.path().join("outside").join(".thndrs").join("config.toml");
    assert_eq!(
        project_path_display(&outside, &workspace),
        outside.display().to_string()
    );
}

#[test]
fn origins_only_include_config_keys() {
    let tmp = tempfile::tempdir().unwrap();
    let effective = load_effective(tmp.path(), &[]).unwrap();

    assert_eq!(effective.origins.len(), CONFIG_KEYS.len());
    for key in effective.origins.keys() {
        assert!(
            CONFIG_KEYS.contains(&key.as_str()),
            "repository search diagnostics must not be exposed as config keys: {key}"
        );
    }
}

#[test]
fn effective_config_snapshot() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let workspace = tmp.path().join("workspace");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(workspace.join(".thndrs")).unwrap();
    fs::write(
        workspace.join(".thndrs").join("config.toml"),
        "model = \"project-model\"\nsession_dir = \"sessions\"\n",
    )
    .unwrap();

    let effective = with_home(&home, || {
        load_effective(&workspace, &[("THNDRS_VERBOSE".to_string(), "on".to_string())]).unwrap()
    });
    let snapshot = format!(
        "model={:?}\nverbose={:?}\nsession_dir_suffix={}\nlayers={:?}\norigins={:?}",
        effective.config.model,
        effective.config.verbose,
        effective
            .config
            .session_dir
            .as_ref()
            .and_then(|path| path.strip_prefix(&workspace).ok())
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "<outside>".to_string()),
        effective
            .layers
            .iter()
            .map(|layer| (layer.source.as_str(), layer.display_path.as_deref().unwrap_or("")))
            .collect::<Vec<_>>(),
        effective
            .origins
            .iter()
            .map(|(key, origin)| (key.as_str(), origin.source.as_str(), origin.detail.as_str()))
            .collect::<Vec<_>>()
    );

    insta::assert_snapshot!(snapshot, @r###"
model=Some("project-model")
verbose=Some(true)
session_dir_suffix=.thndrs/sessions
layers=[("project", ".thndrs/config.toml")]
origins=[("acp_agents", "default", "default"), ("context", "default", "default"), ("default_workspace", "default", "default"), ("model", "project", ".thndrs/config.toml"), ("reasoning_effort", "default", "default"), ("reasoning_summary", "default", "default"), ("session_dir", "project", ".thndrs/config.toml"), ("session_retention", "default", "default"), ("skill_dirs", "default", "default"), ("status_line", "default", "default"), ("theme", "default", "default"), ("tick_rate_ms", "default", "default"), ("verbose", "env", "THNDRS_VERBOSE")]
"###);
}
