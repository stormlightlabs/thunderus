//! `thndrs config` command definitions.

use crate::tools::shell::redact_secrets;
use crate::{config, context};
use clap::{Args, Subcommand};
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cli::Cli;

/// Config file inspection and editing commands.
#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum ConfigCommand {
    /// Print global and project config paths.
    Path,
    /// Show effective config and diagnostics.
    Show(ConfigShowCommand),
    /// Open a config file in `$EDITOR`.
    Edit(ConfigEditCommand),
}

/// `thndrs config show` options.
#[derive(Clone, Debug, Eq, PartialEq, Args)]
pub struct ConfigShowCommand {
    /// Print values in redacted format.
    #[arg(long)]
    pub redacted: bool,
}

/// `thndrs config edit` options.
#[derive(Clone, Debug, Eq, PartialEq, Args)]
pub struct ConfigEditCommand {
    /// Edit the global config file.
    #[arg(long, conflicts_with = "project")]
    pub global: bool,
    /// Edit the project config file.
    #[arg(long, conflicts_with = "global")]
    pub project: bool,
}

/// Run `thndrs config`.
pub fn run(cli: &Cli, command: &ConfigCommand) -> io::Result<()> {
    let stdout = io::stdout();
    let mut lock = stdout.lock();
    run_with_writer(cli, command, &mut lock)
}

/// Run `thndrs config` with an injected writer.
pub fn run_with_writer<W: Write>(cli: &Cli, command: &ConfigCommand, writer: &mut W) -> io::Result<()> {
    match command {
        ConfigCommand::Path => run_config_path(cli, writer),
        ConfigCommand::Show(command) => run_config_show(cli, command, writer),
        ConfigCommand::Edit(command) => run_config_edit(cli, command, writer),
    }
}

fn run_config_path<W: Write>(cli: &Cli, writer: &mut W) -> io::Result<()> {
    let workspace = context::discover_workspace_root(&cli.cwd);
    let global_path = match config::global_config_path() {
        Some(path) => config::global_config_path_display(&path),
        None => String::from("<not available>"),
    };
    let project_path = config::project_config_path(&workspace);

    writeln!(writer, "global: {global_path}")?;
    writeln!(
        writer,
        "project: {}",
        config::project_config_path_display(&project_path, &workspace),
    )?;
    Ok(())
}

fn run_config_show<W: Write>(cli: &Cli, command: &ConfigShowCommand, writer: &mut W) -> io::Result<()> {
    if !command.redacted {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "thndrs config show requires --redacted for safe output",
        ));
    }

    let workspace = context::discover_workspace_root(&cli.cwd);
    let env_vars: Vec<(String, String)> = std::env::vars().collect();
    let effective = config::load_effective(&workspace, &env_vars).map_err(io::Error::other)?;
    let effective_config = effective.config.redacted();

    writeln!(writer, "effective_config:")?;
    write_config(&effective_config, writer)?;

    writeln!(writer, "loaded_files:")?;
    if effective.layers.is_empty() {
        writeln!(writer, "  <none>")?;
    } else {
        for layer in &effective.layers {
            let path = layer.display_path.as_deref().unwrap_or("<none>");
            let hash = layer.hash.as_deref().unwrap_or("<none>");
            writeln!(writer, "  {}: {} ({hash})", layer.source.as_str(), path)?;
        }
    }

    writeln!(writer, "origins:")?;
    for (key, origin) in &effective.origins {
        writeln!(writer, "  {key}: {}: {}", origin.source.as_str(), origin.detail)?;
    }

    writeln!(writer, "diagnostics:")?;
    if effective.diagnostics.is_empty() {
        writeln!(writer, "  <none>")?;
    } else {
        for diagnostic in &effective.diagnostics {
            writeln!(writer, "  - {}", redact_secrets(diagnostic))?;
        }
    }

    Ok(())
}

fn run_config_edit<W: Write>(cli: &Cli, command: &ConfigEditCommand, writer: &mut W) -> io::Result<()> {
    let (scope, path) = select_config_path(cli, command)?;
    let mut input = io::stdin().lock();
    ensure_parent_directory(command, &path, &mut input, writer)?;

    if !path.exists() {
        fs::write(&path, "").map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("failed to create {} config file {}: {error}", scope, path.display()),
            )
        })?;
    }

    let editor = resolve_editor(std::env::var("EDITOR").ok(), scope, &path, writer)?;
    launch_editor(&editor, &path)
}

fn resolve_editor<W: Write>(
    editor: Option<String>, scope: ConfigScope, path: &Path, writer: &mut W,
) -> io::Result<String> {
    editor.filter(|editor| !editor.trim().is_empty()).ok_or_else(|| {
        let _ = writeln!(writer, "cannot open {scope} config file because EDITOR is not set");
        let _ = writeln!(writer, "set EDITOR to your preferred editor and re-run this command");
        let _ = writeln!(writer, "path: {}", path.display());
        io::Error::new(io::ErrorKind::NotFound, "$EDITOR is not set")
    })
}

fn select_config_path(cli: &Cli, command: &ConfigEditCommand) -> io::Result<(ConfigScope, PathBuf)> {
    let workspace = context::discover_workspace_root(&cli.cwd);

    match (command.global, command.project) {
        (true, false) => config::global_config_path()
            .map(|path| (ConfigScope::Global, path))
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not available")),
        (false, true) => Ok((ConfigScope::Project, config::project_config_path(&workspace))),
        (true, true) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "--global and --project cannot both be set",
        )),
        (false, false) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "choose one of --global or --project for config edit",
        )),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConfigScope {
    Global,
    Project,
}

impl std::fmt::Display for ConfigScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Global => f.write_str("global"),
            Self::Project => f.write_str("project"),
        }
    }
}

fn ensure_parent_directory<W: Write, R: BufRead>(
    command: &ConfigEditCommand, path: &Path, input: &mut R, writer: &mut W,
) -> io::Result<()> {
    let parent = match path.parent() {
        Some(parent) => parent,
        None => return Ok(()),
    };

    if parent.exists() {
        return Ok(());
    }

    let scope = if command.global { "global" } else { "project" };
    writeln!(writer, "{scope} config directory does not exist: {}", parent.display())?;
    if !confirm_parent_creation(input, writer)? {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "parent directory creation declined",
        ));
    }

    fs::create_dir_all(parent).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("failed to create config directory {}: {error}", parent.display()),
        )
    })?;

    Ok(())
}

fn confirm_parent_creation<W: Write, R: BufRead>(input: &mut R, writer: &mut W) -> io::Result<bool> {
    write!(writer, "create it before opening the editor? [y/N]: ")?;
    writer.flush()?;

    let mut response = String::new();
    input.read_line(&mut response).map_err(io::Error::other)?;
    let normalized = response.trim().to_ascii_lowercase();
    if normalized.is_empty() { Ok(false) } else { Ok(normalized == "y" || normalized == "yes") }
}

fn launch_editor(editor: &str, path: &Path) -> io::Result<()> {
    let mut parts = editor.split_whitespace();
    let program = parts
        .next()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "$EDITOR is invalid: empty command"))?;

    let status = Command::new(program)
        .args(parts)
        .arg(path)
        .status()
        .map_err(io::Error::other)?;
    if status.success() {
        return Ok(());
    }

    Err(io::Error::other(format!(
        "editor exited with status {}",
        status
            .code()
            .map(|code| code.to_string())
            .unwrap_or_else(|| String::from("aborted"))
    )))
}

fn write_config<W: Write>(config: &config::Config, writer: &mut W) -> io::Result<()> {
    writeln!(writer, "  model: {}", config.model.as_deref().unwrap_or("<unset>"))?;
    writeln!(
        writer,
        "  websearch: {}",
        config.websearch.unwrap_or(crate::cli::WebSearchMode::Auto).label()
    )?;
    writeln!(writer, "  tick_rate_ms: {}", config.tick_rate_ms.unwrap_or(0))?;
    writeln!(writer, "  mouse: {}", config.mouse.unwrap_or(false))?;
    writeln!(writer, "  verbose: {}", config.verbose.unwrap_or(false))?;
    writeln!(writer, "  theme: {:?}", config.theme.unwrap_or_default())?;
    if config.skill_dirs.is_empty() {
        writeln!(writer, "  skill_dirs: []")?;
    } else {
        writeln!(writer, "  skill_dirs:")?;
        for dir in &config.skill_dirs {
            writeln!(writer, "    {}", dir.display())?;
        }
    }

    if let Some(session_dir) = &config.session_dir {
        writeln!(writer, "  session_dir: {}", session_dir.display())?;
    } else {
        writeln!(writer, "  session_dir: <unset>")?;
    }

    if let Some(default_workspace) = &config.default_workspace {
        writeln!(writer, "  default_workspace: {}", default_workspace.display())?;
    } else {
        writeln!(writer, "  default_workspace: <unset>")?;
    }

    if config.acp_agents.is_empty() {
        writeln!(writer, "  acp_agents: []")?;
    } else {
        writeln!(writer, "  acp_agents:")?;
        for (name, agent) in &config.acp_agents {
            writeln!(writer, "    {name}:")?;
            writeln!(writer, "      command: {}", agent.command)?;
            writeln!(writer, "      args: {:?}", agent.args)?;
            if agent.env.is_empty() {
                writeln!(writer, "      env: []")?;
            } else {
                writeln!(writer, "      env:")?;
                for (key, value) in &agent.env {
                    writeln!(writer, "        {key} = {value}")?;
                }
            }
            writeln!(writer, "      enabled: {}", agent.enabled)?;
            writeln!(writer, "      timeout_secs: {}", agent.timeout_secs)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::commands::config::{ConfigCommand, ConfigShowCommand};
    use std::io::Cursor;
    fn run_config_output(cli: &Cli, command: ConfigCommand) -> io::Result<String> {
        let mut output = Cursor::new(Vec::new());
        run_with_writer(cli, &command, &mut output)?;
        String::from_utf8(output.into_inner())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "output is not valid UTF-8"))
    }

    #[test]
    fn config_path_prints_paths() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = temp.path().join("workspace");
        std::fs::create_dir_all(&workspace).expect("create workspace");

        let cli = Cli::try_parse_from([
            "thndrs",
            "--cwd",
            workspace.to_str().expect("workspace path"),
            "config",
            "path",
        ])
        .expect("parse config path");
        let output = run_config_output(&cli, ConfigCommand::Path).expect("run path");

        assert!(output.contains("global: "));
        assert!(output.contains("project: .thndrs/config.toml"));
    }

    #[test]
    fn config_show_requires_redacted() {
        let cli = Cli::try_parse_from(["thndrs", "config", "show"]).expect("parse show");
        let error = run_with_writer(
            &cli,
            &ConfigCommand::Show(ConfigShowCommand { redacted: false }),
            &mut std::io::sink(),
        )
        .expect_err("show should require --redacted");
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(error.to_string().contains("--redacted"));
    }

    #[test]
    fn config_show_masks_acp_env_values() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let workspace = tmp.path().join("workspace");
        std::fs::create_dir_all(workspace.join(".thndrs")).expect("create config dir");
        std::fs::write(
            workspace.join(".thndrs").join("config.toml"),
            r#"
            [acp_agents.local]
            command = "agent"
            env = { FOO = "plain-secret" }
            "#,
        )
        .expect("write config");

        let cli = Cli::try_parse_from([
            "thndrs",
            "--cwd",
            workspace.to_str().expect("workspace path"),
            "config",
            "show",
            "--redacted",
        ])
        .expect("parse show");
        let output =
            run_config_output(&cli, ConfigCommand::Show(ConfigShowCommand { redacted: true })).expect("run show");

        assert!(output.contains("effective_config:"));
        assert!(output.contains("[redacted]"));
        assert!(!output.contains("plain-secret"));
        assert!(output.contains("loaded_files:"));
        assert!(output.contains("origins:"));
        assert!(output.contains("diagnostics:"));
    }

    #[test]
    fn config_edit_reports_missing_editor() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join(".thndrs").join("config.toml");
        let mut output = Cursor::new(Vec::new());
        let err = resolve_editor(None, ConfigScope::Project, &path, &mut output);
        let output = String::from_utf8(output.into_inner()).expect("output utf8");

        assert!(err.is_err());
        let err = err.expect_err("missing editor error");
        assert!(matches!(
            err.kind(),
            io::ErrorKind::NotFound | io::ErrorKind::InvalidInput
        ));
        assert!(output.contains("cannot open project config file because EDITOR is not set"));
        assert!(output.contains("set EDITOR to your preferred editor and re-run this command"));
    }
}
