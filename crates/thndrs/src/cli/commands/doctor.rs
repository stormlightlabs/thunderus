//! `thndrs doctor` command definitions.

use std::collections::BTreeMap;
use std::io::{self, IsTerminal, Write};
use std::path::Path;

use crate::context;
use serde_json::to_string_pretty;

use crate::acp::config::provider_label;
use crate::cli::Cli;
use crate::mcp;
use crate::session;
use crate::thndrs_core::auth;
use crate::thndrs_core::diagnostics::{
    DoctorAcpStatus, DoctorConfigFile, DoctorCredential, DoctorMcpStatus, DoctorReport, DoctorSession,
    DoctorTerminalSummary, DoctorToolAvailability,
};
use crate::tools::shell::redact_secrets;
use clap::Args;

const DOCTOR_DOCS_URL: &str = "https://thndrs.stormlightlabs.org/docs/";

/// Options for `thndrs doctor`.
#[derive(Clone, Debug, Eq, PartialEq, Args)]
pub struct DoctorCommand {
    /// Emit machine-readable JSON diagnostics.
    #[arg(long)]
    pub json: bool,
}

/// Run setup diagnostics.
pub fn run(cli: &Cli, command: &DoctorCommand) -> io::Result<()> {
    let stdout = io::stdout();
    let mut lock = stdout.lock();
    run_with_writer(cli, command, &mut lock)
}

/// Run `thndrs doctor` with an injected writer.
pub fn run_with_writer<W: Write>(cli: &Cli, command: &DoctorCommand, writer: &mut W) -> io::Result<()> {
    let report = gather_doctor_report(cli);
    if command.json {
        writeln!(writer, "{}", to_string_pretty(&report).map_err(io::Error::other)?)?;
    } else {
        render_human_report(&report, writer)?;
    }

    if !report.blocking_issues.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            report
                .setup_hint
                .clone()
                .unwrap_or_else(|| String::from("setup required for selected provider")),
        ));
    }

    if report.mcp.failed > 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "MCP configuration is invalid; check diagnostics",
        ));
    }

    Ok(())
}

fn gather_doctor_report(cli: &Cli) -> DoctorReport {
    let workspace = context::discover_workspace_root(&cli.cwd);
    let model = cli.model.clone();
    let provider = (!model.trim().is_empty()).then(|| provider_label(&model).to_string());
    let config_diagnostics = cli.config_diagnostics.iter().map(|line| redact_secrets(line)).collect();

    let config_files = cli
        .config_layers
        .iter()
        .map(|layer| DoctorConfigFile {
            source: layer.source.as_str().to_string(),
            path: layer.display_path.clone().unwrap_or_else(|| String::from("<none>")),
            sha256: layer.hash.clone(),
        })
        .collect();

    let credentials = collect_credential_statuses(&workspace);
    let search_programs = crate::tools::search::backend::SearchPrograms::discover();
    let tools = DoctorToolAvailability {
        rg: search_programs.rg().is_some(),
        fd: search_programs.fd().is_some(),
        file_discovery: search_programs.file_discovery_label().to_string(),
        content_search: search_programs.content_search_label().to_string(),
        degraded: search_programs.is_degraded(),
    };
    let session_path = cli
        .session_dir
        .clone()
        .unwrap_or_else(|| session::sessions_dir(&workspace));
    let (writable, reason) = check_session_dir_writable(&session_path);
    let mcp = collect_mcp_status(&workspace);
    let acp = collect_acp_status(&cli.acp_agents);
    let terminal = collect_terminal_summary();

    let mut blocking_issues = Vec::new();
    let mut setup_hint = None;

    match provider.as_deref() {
        None => {
            blocking_issues.push(String::from("no model provider selected"));
            setup_hint = Some(String::from("run `thndrs setup` to choose a provider and model"));
        }
        Some(provider) => {
            if let Some(env_var) = required_credential_env_for_provider(provider)
                && auth::credential_source(env_var, &workspace).is_none()
            {
                let hint = format!("run `thndrs setup --provider {provider}` or `thndrs login {provider}`");
                blocking_issues.push(format!("missing credential for model provider {provider}: {env_var}"));
                setup_hint = Some(hint);
            }
        }
    }

    DoctorReport {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        workspace: workspace.display().to_string(),
        model,
        provider,
        config_files,
        config_diagnostics,
        credentials,
        tools,
        session: DoctorSession { path: session_path.display().to_string(), writable, reason },
        mcp,
        acp,
        terminal,
        blocking_issues,
        docs_url: DOCTOR_DOCS_URL.to_string(),
        setup_hint,
    }
}

fn collect_credential_statuses(workspace: &Path) -> Vec<DoctorCredential> {
    [
        ("opencode-go", auth::OPENCODE_GO_KEY_ENV),
        ("opencode-zen", auth::OPENCODE_ZEN_KEY_ENV),
    ]
    .into_iter()
    .map(|(provider, env_var)| DoctorCredential {
        provider: provider.to_string(),
        source: auth::credential_source(env_var, workspace)
            .map(|source| source.label().to_string())
            .unwrap_or_else(|| String::from("missing")),
    })
    .collect()
}

fn required_credential_env_for_provider(provider: &str) -> Option<&'static str> {
    match provider {
        "opencode-go" => Some(auth::OPENCODE_GO_KEY_ENV),
        "opencode-zen" => Some(auth::OPENCODE_ZEN_KEY_ENV),
        _ => None,
    }
}

fn collect_mcp_status(workspace: &Path) -> DoctorMcpStatus {
    let env_vars: Vec<(String, String)> = std::env::vars().collect();
    match mcp::config::load_effective_mcp(workspace, &env_vars) {
        Ok(effective) => {
            let skipped = count_mcp_skipped(&effective.diagnostics);
            let configured = effective.config.servers.len() + skipped;
            let ready = effective
                .config
                .servers
                .values()
                .filter(|server| server.enabled)
                .count();
            DoctorMcpStatus {
                configured,
                ready,
                skipped,
                failed: 0,
                diagnostics: effective
                    .diagnostics
                    .into_iter()
                    .map(|diagnostic| redact_secrets(&diagnostic))
                    .collect(),
            }
        }
        Err(err) => {
            let diagnostic = redact_secrets(&format!("failed to load MCP config: {err}"));
            DoctorMcpStatus { configured: 0, ready: 0, skipped: 0, failed: 1, diagnostics: vec![diagnostic] }
        }
    }
}

fn collect_acp_status(acp_agents: &BTreeMap<String, crate::config::AcpAgentConfig>) -> DoctorAcpStatus {
    let configured = acp_agents.len();
    let enabled = acp_agents.values().filter(|agent| agent.enabled).count();
    DoctorAcpStatus { configured, enabled, disabled: configured.saturating_sub(enabled) }
}

fn collect_terminal_summary() -> DoctorTerminalSummary {
    let (width, height) = crossterm::terminal::size().unwrap_or((80, 24));
    let term_env = std::env::var("TERM").ok().filter(|value| !value.trim().is_empty());
    let no_color = std::env::var_os("NO_COLOR").is_some();

    DoctorTerminalSummary { tty: io::stdout().is_terminal(), width, height, term_env, no_color }
}

fn check_session_dir_writable(path: &Path) -> (bool, Option<String>) {
    if path.exists() {
        if !path.is_dir() {
            return (false, Some(format!("{} is not a directory", path.display())));
        }
        return match check_directory_file_writable(path) {
            Ok(()) => (true, None),
            Err(error) => (false, Some(format!("session directory is not writable: {error}"))),
        };
    }

    let parent = match path.parent() {
        Some(parent) => parent,
        None => return (false, Some(format!("{path:?} is not a valid path"))),
    };

    if !parent.exists() {
        return (
            false,
            Some(format!("parent directory does not exist: {}", parent.display())),
        );
    }

    match check_directory_file_writable(parent) {
        Ok(()) => {
            let warning = String::from("session directory does not exist and will be created on first write");
            (true, Some(warning))
        }
        Err(error) => (
            false,
            Some(format!("session directory parent is not writable: {error}")),
        ),
    }
}

fn check_directory_file_writable(dir: &Path) -> io::Result<()> {
    let marker = dir.join(format!(".thndrs-doctor-write-check-{}", std::process::id()));
    std::fs::File::create_new(&marker)?;
    std::fs::remove_file(&marker).map_err(io::Error::other)
}

fn count_mcp_skipped(diagnostics: &[String]) -> usize {
    diagnostics
        .iter()
        .filter(|message| {
            message.starts_with("mcp server `") && message.contains("` skipped: unresolved environment variable ")
        })
        .count()
}

fn render_human_report<W: Write>(report: &DoctorReport, writer: &mut W) -> io::Result<()> {
    writeln!(writer, "thndrs doctor")?;
    writeln!(writer, "app_version: {}", report.app_version)?;
    writeln!(writer, "workspace: {}", report.workspace)?;
    writeln!(writer, "model: {}", report.model)?;
    writeln!(writer, "provider: {}", report.provider.as_deref().unwrap_or("<none>"))?;

    writeln!(writer, "config_files:")?;
    if report.config_files.is_empty() {
        writeln!(writer, "  <none>")?;
    } else {
        for file in &report.config_files {
            if let Some(hash) = &file.sha256 {
                writeln!(writer, "  {}: {} ({hash})", file.source, file.path)?;
            } else {
                writeln!(writer, "  {}: {}", file.source, file.path)?;
            }
        }
    }

    writeln!(writer, "config_diagnostics:")?;
    if report.config_diagnostics.is_empty() {
        writeln!(writer, "  <none>")?;
    } else {
        for diagnostic in &report.config_diagnostics {
            writeln!(writer, "  - {diagnostic}")?;
        }
    }

    writeln!(writer, "credentials:")?;
    for credential in &report.credentials {
        writeln!(writer, "  {}: {}", credential.provider, credential.source)?;
    }

    writeln!(writer, "tools:")?;
    writeln!(writer, "  rg: {}", bool_to_status(report.tools.rg))?;
    writeln!(writer, "  fd: {}", bool_to_status(report.tools.fd))?;
    writeln!(writer, "  file_discovery: {}", report.tools.file_discovery)?;
    writeln!(writer, "  content_search: {}", report.tools.content_search)?;
    writeln!(
        writer,
        "  degraded: {}",
        if report.tools.degraded { "yes" } else { "no" }
    )?;

    writeln!(
        writer,
        "session_dir: {} [{}]",
        report.session.path,
        if report.session.writable { "writable" } else { "read-only" }
    )?;
    if let Some(reason) = &report.session.reason {
        writeln!(writer, "session_dir_detail: {reason}")?;
    }

    writeln!(
        writer,
        "mcp: configured={} ready={} skipped={} failed={}",
        report.mcp.configured, report.mcp.ready, report.mcp.skipped, report.mcp.failed
    )?;
    for diagnostic in &report.mcp.diagnostics {
        writeln!(writer, "  - mcp: {diagnostic}")?;
    }

    writeln!(
        writer,
        "acp: configured={} enabled={} disabled={}",
        report.acp.configured, report.acp.enabled, report.acp.disabled
    )?;

    writeln!(
        writer,
        "terminal: tty={} size={}x{} no_color={}",
        bool_to_status(report.terminal.tty),
        report.terminal.width,
        report.terminal.height,
        report.terminal.no_color
    )?;
    if let Some(term_env) = &report.terminal.term_env {
        writeln!(writer, "terminal.term: {term_env}")?;
    }

    writeln!(writer, "docs: {}", report.docs_url)?;
    if !report.blocking_issues.is_empty() {
        for issue in &report.blocking_issues {
            writeln!(writer, "issue: {issue}")?;
        }
    }
    if let Some(setup_hint) = &report.setup_hint {
        writeln!(writer, "setup: {setup_hint}")?;
    }

    Ok(())
}

fn bool_to_status(value: bool) -> &'static str {
    if value { "available" } else { "missing" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Cli;
    use crate::cli::Command;
    use crate::config;
    use std::ffi::OsString;
    use std::io::Cursor;

    fn with_env_vars<R>(vars: &[(&str, Option<&str>)], action: impl FnOnce() -> R) -> R {
        let _guard = crate::test_env::lock();

        let previous: Vec<(String, Option<OsString>)> = vars
            .iter()
            .map(|&(key, _)| (key.to_string(), std::env::var_os(key)))
            .collect();

        for (key, _) in &previous {
            unsafe {
                std::env::remove_var(key);
            }
        }
        for (key, value) in vars {
            match value {
                Some(value) => unsafe {
                    std::env::set_var(key, value);
                },
                None => unsafe {
                    std::env::remove_var(key);
                },
            }
        }

        let result = action();

        for (key, value) in previous {
            match value {
                Some(value) => unsafe {
                    std::env::set_var(key, value);
                },
                None => unsafe {
                    std::env::remove_var(key);
                },
            }
        }

        result
    }

    fn run_with_output(args: &[&str]) -> (String, Option<io::ErrorKind>) {
        let (cli, matches) = Cli::try_parse_matches_from(args).expect("parse doctor");
        let env_vars = std::env::vars().collect::<Vec<_>>();
        let workspace = cli.cwd.clone();
        let cli = cli.with_effective(
            config::load_effective(&workspace, &env_vars).expect("load config"),
            &matches,
        );
        let mut output = Cursor::new(Vec::new());
        let command = match &cli.command {
            Some(Command::Doctor(command)) => command,
            _ => panic!("expected doctor command"),
        };

        let error = run_with_writer(&cli, command, &mut output).err().map(|err| err.kind());
        (
            String::from_utf8(output.into_inner()).expect("output is not UTF-8"),
            error,
        )
    }

    fn workspace_with_doctor_config() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().expect("temp workspace");
        let workspace = tmp.path();
        std::fs::create_dir_all(workspace.join(".thndrs")).expect("create .thndrs");
        std::fs::create_dir_all(workspace.join(".thndrs/sessions")).expect("create sessions");

        let config_toml = r#"
            [acp_agents.local]
            command = "agent"
            args = ["--stdio"]
            enabled = true
        "#;
        let mcp_toml = r#"
            [servers.test]
            command = "mcp-server"
            args = ["--stdio"]
            enabled = true
        "#;

        std::fs::write(workspace.join(".thndrs/config.toml"), config_toml).expect("write config");
        std::fs::write(workspace.join(".thndrs/mcp.toml"), mcp_toml).expect("write mcp config");

        tmp
    }

    #[test]
    fn doctor_reports_json_and_does_not_leak_secrets() {
        let workspace = workspace_with_doctor_config();
        let workspace_path = workspace.path();
        let workspace_path = workspace_path.to_str().expect("workspace path");

        let (output, error_kind) = with_env_vars(
            &[
                ("HOME", Some(workspace_path)),
                ("UMANS_API_KEY", Some("sk-very-secret-umans")),
                ("OPENCODE_GO_KEY", Some("sk-go-secret")),
                ("OPENCODE_ZEN_KEY", Some("sk-zen-secret")),
            ],
            || {
                run_with_output(&[
                    "thndrs",
                    "--cwd",
                    workspace_path,
                    "--model",
                    "opencode/big-pickle",
                    "doctor",
                    "--json",
                ])
            },
        );

        assert_eq!(error_kind, None);
        assert!(!output.contains("sk-very-secret-umans"));
        let report: DoctorReport = serde_json::from_str(&output).expect("valid JSON");
        assert_eq!(report.model, "opencode/big-pickle");
        assert_eq!(report.provider.as_deref(), Some("opencode-zen"));
        assert_eq!(report.credentials[0].provider, "opencode-go");
        assert!(matches!(
            report.credentials[0].source.as_str(),
            "environment" | "project credentials" | "global credentials"
        ));
        assert!(!report.config_files.is_empty());
    }

    #[test]
    fn doctor_human_output_reports_incomplete_setup_without_assuming_umans() {
        let workspace = workspace_with_doctor_config();
        let workspace_path = workspace.path().to_str().expect("workspace path");

        let (output, error_kind) = with_env_vars(
            &[
                ("HOME", Some(workspace_path)),
                ("NO_COLOR", None),
                ("TERM", Some("xterm")),
            ],
            || run_with_output(&["thndrs", "--cwd", workspace_path, "doctor"]),
        );

        assert!(output.contains("thndrs doctor"));
        assert!(output.contains("model: \nprovider: <none>"));
        assert!(output.contains("issue: no model provider selected"));
        assert!(output.contains("setup: run `thndrs setup` to choose a provider and model"));
        assert!(output.contains("file_discovery:"));
        assert!(output.contains("content_search:"));
        assert!(output.contains("degraded:"));
        assert!(!output.contains("missing credential for model provider umans"));
        assert!(!output.contains("UMANS_API_KEY"));
        assert_eq!(error_kind, Some(io::ErrorKind::PermissionDenied));
    }

    #[test]
    fn doctor_json_reports_no_provider_for_incomplete_setup() {
        let workspace = workspace_with_doctor_config();
        let workspace_path = workspace.path().to_str().expect("workspace path");

        let (output, error_kind) = with_env_vars(&[("HOME", Some(workspace_path))], || {
            run_with_output(&["thndrs", "--cwd", workspace_path, "doctor", "--json"])
        });

        let report: DoctorReport = serde_json::from_str(&output).expect("doctor json");
        assert_eq!(report.model, "");
        assert_eq!(report.provider, None);
        assert!(!report.tools.file_discovery.is_empty());
        assert!(!report.tools.content_search.is_empty());
        assert_eq!(report.tools.degraded, !report.tools.fd || !report.tools.rg);
        assert_eq!(report.blocking_issues, vec!["no model provider selected"]);
        assert_eq!(
            report.setup_hint.as_deref(),
            Some("run `thndrs setup` to choose a provider and model")
        );
        assert!(!output.contains("UMANS_API_KEY"));
        assert_eq!(error_kind, Some(io::ErrorKind::PermissionDenied));
    }

    #[test]
    fn doctor_keeps_provider_specific_diagnostics_after_model_selection() {
        let workspace = workspace_with_doctor_config();
        let workspace_path = workspace.path().to_str().expect("workspace path");

        let (output, error_kind) = with_env_vars(&[("HOME", Some(workspace_path)), ("UMANS_API_KEY", None)], || {
            run_with_output(&[
                "thndrs",
                "--cwd",
                workspace_path,
                "--model",
                "opencode/big-pickle",
                "doctor",
                "--json",
            ])
        });

        let report: DoctorReport = serde_json::from_str(&output).expect("doctor json");
        assert_eq!(report.provider.as_deref(), Some("opencode-zen"));
        assert_eq!(
            report.blocking_issues,
            vec!["missing credential for model provider opencode-zen: OPENCODE_ZEN_KEY"]
        );
        assert_eq!(
            report.setup_hint.as_deref(),
            Some("run `thndrs setup --provider opencode-zen` or `thndrs login opencode-zen`")
        );
        assert_eq!(error_kind, Some(io::ErrorKind::PermissionDenied));
    }

    #[test]
    fn doctor_json_does_not_include_secret_credentials() {
        let workspace = workspace_with_doctor_config();
        let workspace_path = workspace.path().to_str().expect("workspace path");

        let (output, error_kind) = with_env_vars(
            &[
                ("HOME", Some(workspace_path)),
                ("UMANS_API_KEY", Some("sk-secret-umans")),
                ("OPENCODE_GO_KEY", Some("op-secret")),
                ("OPENCODE_ZEN_KEY", Some("zen-secret")),
            ],
            || {
                run_with_output(&[
                    "thndrs",
                    "--cwd",
                    workspace_path,
                    "--model",
                    "opencode/big-pickle",
                    "doctor",
                    "--json",
                ])
            },
        );

        assert_eq!(error_kind, None);
        assert!(!output.contains("sk-secret-umans"));
        assert!(!output.contains("op-secret"));
        assert!(!output.contains("zen-secret"));
        let report: DoctorReport = serde_json::from_str(&output).expect("doctor json");
        assert!(report.credentials.iter().all(|cred| cred.source != "environment"
            || cred.provider == "opencode-go"
            || cred.provider == "opencode-zen"));
        assert!(
            !report
                .config_diagnostics
                .iter()
                .any(|item| item.contains("sk-secret-umans")
                    || item.contains("op-secret")
                    || item.contains("zen-secret"))
        );
    }

    #[test]
    fn doctor_reports_path_and_counts_for_acp_and_mcp() {
        let workspace = workspace_with_doctor_config();
        let workspace_path = workspace.path().to_str().expect("workspace path");

        let (output, error_kind) = with_env_vars(
            &[
                ("HOME", Some(workspace_path)),
                ("UMANS_API_KEY", Some("sk-umans")),
                ("OPENCODE_GO_KEY", Some("op-secret")),
                ("OPENCODE_ZEN_KEY", Some("zen-secret")),
            ],
            || {
                run_with_output(&[
                    "thndrs",
                    "--cwd",
                    workspace_path,
                    "--model",
                    "acp:local",
                    "doctor",
                    "--json",
                ])
            },
        );
        assert_eq!(error_kind, None);
        let report: DoctorReport = serde_json::from_str(&output).expect("parse report");
        assert_eq!(report.provider.as_deref(), Some("acp"));
        assert!(
            report.session.path.ends_with(".thndrs/sessions"),
            "unexpected session path: {}",
            report.session.path
        );
        assert_eq!(report.acp.configured, 1);
        assert!(report.acp.enabled >= 1);
        assert!(report.mcp.configured >= 1);
    }

    #[test]
    fn doctor_reports_mcp_invalid_config_as_cli_error() {
        let workspace = tempfile::tempdir().expect("temp workspace");
        let workspace_path = workspace.path().to_str().expect("workspace path");
        std::fs::create_dir_all(workspace.path().join(".thndrs")).expect("create .thndrs");
        std::fs::create_dir_all(workspace.path().join(".thndrs").join("sessions")).expect("create sessions");
        let invalid_mcp = r#"
            [servers.bad]
            transport = "stdio"
            command = ""
            enabled = true
        "#;
        std::fs::write(workspace.path().join(".thndrs/mcp.toml"), invalid_mcp).expect("write invalid mcp");

        let (_, error_kind) = with_env_vars(
            &[
                ("HOME", Some(workspace_path)),
                ("UMANS_API_KEY", Some("sk-umans")),
                ("OPENCODE_ZEN_KEY", Some("sk-zen")),
            ],
            || {
                run_with_output(&[
                    "thndrs",
                    "--cwd",
                    workspace_path,
                    "--model",
                    "opencode/big-pickle",
                    "doctor",
                    "--json",
                ])
            },
        );

        assert_eq!(error_kind, Some(io::ErrorKind::InvalidInput));
    }

    #[test]
    fn check_mcp_skipped_count_parses_known_messages() {
        assert_eq!(
            count_mcp_skipped(&[String::from(
                "mcp server `docs` skipped: unresolved environment variable DOCS_TOKEN"
            )]),
            1
        );
        assert_eq!(count_mcp_skipped(&[String::from("failed to load MCP config: bad")]), 0);
    }
}
