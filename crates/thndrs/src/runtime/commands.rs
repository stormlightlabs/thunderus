//! CLI command dispatch and reporting.

use super::*;

pub(crate) fn run_command(cli: &Cli, command: &Command) -> io::Result<()> {
    match command {
        Command::Setup(command) => cli_commands::setup::run(cli, command),
        Command::Login(command) => cli_commands::auth::run_login(cli, command),
        Command::Logout(command) => cli_commands::auth::run_logout(cli, command),
        Command::Auth { command } => cli_commands::auth::run_auth(cli, command),
        Command::Doctor(command) => cli_commands::doctor::run(cli, command),
        Command::Config { command } => cli_commands::config::run(cli, command),
        Command::Acp { command } => run_acp_command(cli, command),
        Command::Mcp { command } => run_mcp_command(cli, command),
        Command::Skills { command } => cli_commands::skills::run(cli, command),
        Command::Run(command) => headless::run_command(cli, command),
        Command::Review(command) => review::run_command(cli, command),
        Command::Context(command) => run_context_command(cli, command),
        Command::Usage(command) => run_usage_command(cli, command),
        Command::Session { command } => run_session_command(cli, command),
        Command::Debug { command } => run_debug_command(cli, command),
    }
}

pub(crate) fn load_mcp_manager_for_workspace(workspace: &Path) -> io::Result<Arc<McpManager>> {
    let env_vars: Vec<(String, String)> = std::env::vars().collect();
    let effective = mcp::config::load_effective_mcp(workspace, &env_vars).map_err(io::Error::other)?;
    let mut manager = McpManager::from_config(&effective.config);
    manager.extend_diagnostics(effective.diagnostics);
    Ok(Arc::new(manager))
}

pub(crate) fn load_effective_mcp_for_workspace(workspace: &Path) -> io::Result<mcp::config::EffectiveMcpConfig> {
    let env_vars: Vec<(String, String)> = std::env::vars().collect();
    mcp::config::load_effective_mcp(workspace, &env_vars).map_err(io::Error::other)
}

pub(crate) fn run_mcp_command(cli: &Cli, command: &McpCommand) -> io::Result<()> {
    let stdout = io::stdout();
    let mut lock = stdout.lock();
    match command {
        McpCommand::Catalog(command) => run_mcp_catalog_command(cli, command, &mut lock),
        McpCommand::Add { name, scope, command, args, url } => {
            run_mcp_add(cli, name, *scope, command.as_deref(), args, url.as_deref(), &mut lock)
        }
        McpCommand::Remove { name, scope } => run_mcp_remove(cli, name, *scope, &mut lock),
        McpCommand::List => run_mcp_list(cli, &mut lock),
        McpCommand::Status => run_mcp_status(cli, &mut lock),
        McpCommand::Trust => run_mcp_trust(cli, &mut lock),
        McpCommand::Revoke => run_mcp_revoke(cli, &mut lock),
        McpCommand::Test { name } => run_mcp_test(cli, name, &mut lock),
        McpCommand::Tools { name } => run_mcp_tools(cli, name, &mut lock),
        McpCommand::Resources { name } => run_mcp_resources(cli, name, &mut lock),
        McpCommand::Resource { server, uri } => run_mcp_resource(cli, server, uri, &mut lock),
        McpCommand::Call { server, tool, json } => run_mcp_call(cli, server, tool, json, &mut lock),
    }
}

pub(crate) fn run_context_command(cli: &Cli, command: &ContextCommand) -> io::Result<()> {
    let (path, session_id) = resolve_context_session(cli, command.session.as_deref())?;
    let records = session::SessionReader::read_validated_records(&path, &session_id)?;
    if matches!(command.command, Some(ContextSubcommand::Telemetry)) {
        return session::export_context_telemetry(&session_id, &records);
    }
    let export = session::PersistedContextExport::from_records(&session_id, &records)?;
    let stdout = io::stdout();
    let mut writer = stdout.lock();

    match &command.command {
        Some(ContextSubcommand::Telemetry) => unreachable!("telemetry returned before stdout was locked"),
        Some(ContextSubcommand::Changes { from_request_id, to_request_id }) => {
            let selectors = match (from_request_id.as_deref(), to_request_id.as_deref()) {
                (None, None) => Vec::new(),
                (Some(from), Some(to)) => vec![from, to],
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "context changes requires both request ids or neither",
                    ));
                }
            };
            if command.json {
                if !selectors.is_empty() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "JSON changes currently supports the latest two request attempts only",
                    ));
                }
                let diff = export.diffs.last().ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "context changes requires at least two recorded request snapshots",
                    )
                })?;
                serde_json::to_writer_pretty(
                    &mut writer,
                    &serde_json::json!({
                        "schema_version": export.schema_version,
                        "policy_version": export.policy_version,
                        "session_id": export.session_id,
                        "capture_policy": export.capture_policy,
                        "redaction": export.redaction,
                        "diff": diff,
                    }),
                )
                .map_err(io::Error::other)?;
                writeln!(writer)
            } else {
                let history = session::ContextHistory::from_records(&records);
                writeln!(
                    writer,
                    "{}",
                    history.render_changes(&selectors).map_err(io::Error::other)?
                )
            }
        }
        None if command.json => writeln!(writer, "{}", export.to_json()?),
        None => {
            let history = session::ContextHistory::from_records(&records);
            writeln!(writer, "session  {session_id}")?;
            writeln!(writer, "{}", history.render_request(None).map_err(io::Error::other)?)
        }
    }
}

pub(crate) fn run_usage_command(cli: &Cli, command: &UsageCommand) -> io::Result<()> {
    let (path, session_id) = resolve_context_session(cli, command.session.as_deref())?;
    let records = session::SessionReader::read_validated_records(&path, &session_id)?;
    let export = session::PersistedContextExport::from_records(&session_id, &records)?;
    let summary = session::SessionReader::read_summary(&path);
    let requests = export.request_accounting.len();
    let provider_measured = export
        .request_accounting
        .iter()
        .filter(|record| record.accounting.provider_usage.is_some())
        .count();
    let value = serde_json::json!({
        "schema_version": export.schema_version,
        "policy_version": export.policy_version,
        "session_id": session_id,
        "usage": {
            "input_tokens": summary.input_tokens,
            "output_tokens": summary.output_tokens,
            "requests": requests,
            "provider_measured_requests": provider_measured,
        },
        "measurement_provenance": export.measurement_provenance,
    });
    let stdout = io::stdout();
    let mut writer = stdout.lock();
    if command.json {
        serde_json::to_writer_pretty(&mut writer, &value).map_err(io::Error::other)?;
        writeln!(writer)
    } else {
        writeln!(writer, "usage · {session_id}")?;
        writeln!(writer, "input   {}", summary.input_tokens)?;
        writeln!(writer, "output  {}", summary.output_tokens)?;
        writeln!(writer, "requests  {requests} · provider measured {provider_measured}")
    }
}

pub(crate) fn resolve_context_session(cli: &Cli, query: Option<&str>) -> io::Result<(PathBuf, String)> {
    let workspace = crate::context::discover_workspace_root(&cli.cwd);
    let dir = cli
        .session_dir
        .clone()
        .unwrap_or_else(|| session::sessions_dir(&workspace));
    let path = match query {
        Some(query) => session::resolve_session_file(&dir, query).map_err(io::Error::other)?,
        None => session::latest_session_file(&dir)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no sessions found"))?,
    };
    let session_id = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "session path has no valid id"))?
        .to_string();
    Ok((path, session_id))
}

pub(crate) fn run_session_command(cli: &Cli, command: &SessionCommand) -> io::Result<()> {
    let workspace = crate::context::discover_workspace_root(&cli.cwd);
    let dir = cli
        .session_dir
        .clone()
        .unwrap_or_else(|| session::sessions_dir(&workspace));
    let stdout = io::stdout();
    let stderr = io::stderr();
    let mut stdout = stdout.lock();
    let mut stderr = stderr.lock();
    let cancellation = CancelToken::new();
    let _registration = matches!(
        command,
        SessionCommand::List
            | SessionCommand::Prune { .. }
            | SessionCommand::Storage { .. }
            | SessionCommand::Purge { .. }
    )
    .then(|| cancellation::register(cancellation.clone()))
    .transpose()?;
    match command {
        SessionCommand::List => run_session_list(&dir, &workspace, &mut stdout, &mut stderr, &cancellation),
        SessionCommand::Latest => run_session_latest(&dir, &mut stdout),
        SessionCommand::Titles => run_session_titles(&dir, &mut stdout),
        SessionCommand::Show { session_id } => run_session_show(&dir, session_id, &mut stdout),
        SessionCommand::Resume { .. } => Err(io::Error::other("session resume must start an interactive session")),
        SessionCommand::Fork { session_id, turn_id } => run_session_fork(&dir, session_id, turn_id, &mut stdout),
        SessionCommand::Rename { session_id, name } => run_session_rename(&dir, session_id, name, &mut stdout),
        SessionCommand::Inspect { session_id, format, .. } => {
            run_session_inspect(&dir, session_id, *format, &mut stdout)
        }
        SessionCommand::Export { session_id, format } => run_session_export(&dir, session_id, *format, &mut stdout),
        SessionCommand::Prune { older_than, keep_count, dry_run, format } => run_session_prune(
            cli,
            &dir,
            &workspace,
            SessionPruneRequest {
                overrides: session::PruneOverrides { older_than_days: *older_than, keep_count: *keep_count },
                dry_run: *dry_run,
                format: *format,
            },
            &mut stdout,
            &mut stderr,
            &cancellation,
        ),
        SessionCommand::Storage { format } => {
            run_session_storage(cli, &dir, &workspace, *format, &mut stdout, &mut stderr, &cancellation)
        }
        SessionCommand::Archive { session_id, format } => run_session_lifecycle(
            cli,
            &dir,
            &workspace,
            session_id,
            SessionLifecycleRequest::new(session::SessionLifecycleAction::Archive, *format),
            &mut stdout,
        ),
        SessionCommand::Unarchive { session_id, format } => run_session_lifecycle(
            cli,
            &dir,
            &workspace,
            session_id,
            SessionLifecycleRequest::new(session::SessionLifecycleAction::Unarchive, *format),
            &mut stdout,
        ),
        SessionCommand::Pin { session_id, format } => run_session_lifecycle(
            cli,
            &dir,
            &workspace,
            session_id,
            SessionLifecycleRequest::new(session::SessionLifecycleAction::Pin, *format),
            &mut stdout,
        ),
        SessionCommand::Unpin { session_id, format } => run_session_lifecycle(
            cli,
            &dir,
            &workspace,
            session_id,
            SessionLifecycleRequest::new(session::SessionLifecycleAction::Unpin, *format),
            &mut stdout,
        ),
        SessionCommand::Delete { session_id, yes, allow_pinned, format } => run_session_lifecycle(
            cli,
            &dir,
            &workspace,
            session_id,
            SessionLifecycleRequest {
                action: session::SessionLifecycleAction::Delete,
                confirmed: *yes,
                allow_pinned: *allow_pinned,
                format: *format,
            },
            &mut stdout,
        ),
        SessionCommand::Restore { session_id, format } => run_session_lifecycle(
            cli,
            &dir,
            &workspace,
            session_id,
            SessionLifecycleRequest::new(session::SessionLifecycleAction::Restore, *format),
            &mut stdout,
        ),
        SessionCommand::Purge { yes, allow_pinned, format } => run_session_purge(
            &dir,
            &workspace,
            SessionPurgeRequest { confirmed: *yes, allow_pinned: *allow_pinned, format: *format },
            &mut stdout,
            &mut stderr,
            &cancellation,
        ),
    }
}

#[derive(Clone, Copy)]
pub(crate) struct SessionPruneRequest {
    pub(crate) overrides: session::PruneOverrides,
    pub(crate) dry_run: bool,
    pub(crate) format: SessionReportFormat,
}

#[derive(Clone, Copy)]
pub(crate) struct SessionPurgeRequest {
    pub(crate) confirmed: bool,
    pub(crate) allow_pinned: bool,
    pub(crate) format: SessionReportFormat,
}

pub(crate) fn run_session_prune<W: io::Write, P: io::Write>(
    cli: &Cli, dir: &Path, workspace: &Path, request: SessionPruneRequest, writer: &mut W, progress: &mut P,
    cancellation: &CancelToken,
) -> io::Result<()> {
    writeln!(progress, "Scanning sessions for pruning...")?;
    progress.flush()?;
    let inventory = session::SessionInventory::scan_cancellable(dir, workspace, cancellation)?;
    let candidates = session::select_prune_candidates(&inventory, &cli.session_retention, request.overrides, None);
    if request.dry_run {
        writeln!(
            progress,
            "Preparing prune preview for {} candidate(s)...",
            candidates.len()
        )?;
    } else {
        writeln!(progress, "Moving {} session(s) to trash...", candidates.len())?;
    }
    progress.flush()?;
    let lifecycle = session::SessionLifecycle::new(dir, workspace);
    let progress_interval = candidates.len().div_ceil(20).max(1);
    let mut progress_error = None;
    let report_result = session::apply_prune_cancellable_with_progress(
        &lifecycle,
        candidates,
        None,
        request.dry_run,
        cancellation,
        |update| {
            if progress_error.is_some()
                || (update.completed != update.total && update.completed % progress_interval != 0)
            {
                return;
            }
            progress_error = writeln!(
                progress,
                "Prune progress: {}/{} processed ({} moved, {} failed)",
                update.completed, update.total, update.moved, update.failed
            )
            .and_then(|()| progress.flush())
            .err();
        },
    );
    if let Some(error) = progress_error {
        return Err(error);
    }
    let report = report_result?;
    write_session_report(writer, request.format, &report, || {
        let selected_bytes = report.candidates.iter().map(|candidate| candidate.bytes).sum::<u64>();
        let age_limit_count = report
            .candidates
            .iter()
            .filter(|candidate| candidate.reasons.contains(&session::PruneReason::MaximumAge))
            .count();
        let keep_limit_count = report
            .candidates
            .iter()
            .filter(|candidate| candidate.reasons.contains(&session::PruneReason::LiveCount))
            .count();
        let mut lines = if report.dry_run {
            vec![format!(
                "would move {} session(s) totaling {} bytes to trash",
                report.candidates.len(),
                selected_bytes
            )]
        } else {
            vec![format!(
                "moved {} of {} selected session(s) and {} bytes to trash",
                report.deleted_session_ids.len(),
                report.candidates.len(),
                report.reclaimed_bytes
            )]
        };
        if age_limit_count > 0 {
            lines.push(format!("selected by age limit: {age_limit_count}"));
        }
        if keep_limit_count > 0 {
            lines.push(format!("selected by live-session keep limit: {keep_limit_count}"));
        }
        for failure in &report.failures {
            lines.push(format!("failed {}: {}", failure.session_id, failure.error));
        }
        lines.join(
            "
",
        )
    })
}

pub(crate) fn run_session_storage<W: io::Write, P: io::Write>(
    cli: &Cli, dir: &Path, workspace: &Path, format: SessionReportFormat, writer: &mut W, progress: &mut P,
    cancellation: &CancelToken,
) -> io::Result<()> {
    writeln!(progress, "Scanning session storage...")?;
    progress.flush()?;
    let inventory = session::SessionInventory::scan_cancellable(dir, workspace, cancellation)?;
    cancellation::check(cancellation)?;
    let reclaimable =
        session::reclaimable_bytes_from_inventory(dir, workspace, &inventory, &cli.session_retention, None);
    let value = serde_json::json!({
        "schema_version": 1,
        "live": { "count": inventory.totals.live_sessions, "bytes": inventory.sessions.iter().filter(|s| s.storage_state == session::SessionStorageState::Live).map(|s| s.owned_bytes()).sum::<u64>() },
        "archived": { "count": inventory.totals.archived_sessions, "bytes": inventory.sessions.iter().filter(|s| s.storage_state == session::SessionStorageState::Archived).map(|s| s.owned_bytes()).sum::<u64>() },
        "pinned": { "count": inventory.totals.pinned_sessions, "bytes": inventory.sessions.iter().filter(|s| s.pinned).map(|s| s.owned_bytes()).sum::<u64>() },
        "trash": { "count": inventory.totals.trash_count, "bytes": inventory.totals.trash_bytes },
        "artifacts": { "count": inventory.totals.artifact_count, "bytes": inventory.totals.artifact_bytes },
        "logs": { "bytes": inventory.totals.log_bytes },
        "reclaimable_bytes": reclaimable,
    });
    write_session_report(writer, format, &value, || {
        format!(
            "live {} ({} bytes)
archived {} ({} bytes)
pinned {}
trash {} ({} bytes)
artifacts {} ({} bytes)
logs {} bytes
reclaimable {} bytes",
            inventory.totals.live_sessions,
            value["live"]["bytes"],
            inventory.totals.archived_sessions,
            value["archived"]["bytes"],
            inventory.totals.pinned_sessions,
            inventory.totals.trash_count,
            inventory.totals.trash_bytes,
            inventory.totals.artifact_count,
            inventory.totals.artifact_bytes,
            inventory.totals.log_bytes,
            reclaimable,
        )
    })
}

#[derive(Clone, Copy)]
pub(crate) struct SessionLifecycleRequest {
    action: session::SessionLifecycleAction,
    confirmed: bool,
    allow_pinned: bool,
    format: SessionReportFormat,
}

impl SessionLifecycleRequest {
    const fn new(action: session::SessionLifecycleAction, format: SessionReportFormat) -> Self {
        Self { action, confirmed: false, allow_pinned: false, format }
    }
}

pub(crate) fn run_session_lifecycle<W: io::Write>(
    cli: &Cli, dir: &Path, workspace: &Path, session_id: &str, request: SessionLifecycleRequest, writer: &mut W,
) -> io::Result<()> {
    let lifecycle = session::SessionLifecycle::new(dir, workspace);
    if request.action == session::SessionLifecycleAction::Delete && !request.confirmed {
        let preview = lifecycle.preview_delete(session_id).map_err(io::Error::other)?;
        return write_session_report(writer, request.format, &preview, || {
            format!(
                "delete {} ({}) would move {} owned path(s) to trash; rerun with --yes",
                preview.session_id,
                preview.title,
                preview.owned_state.len(),
            )
        });
    }
    let report = match request.action {
        session::SessionLifecycleAction::Archive => lifecycle.archive(session_id),
        session::SessionLifecycleAction::Unarchive => lifecycle.unarchive(session_id),
        session::SessionLifecycleAction::Pin => lifecycle.pin(session_id),
        session::SessionLifecycleAction::Unpin => lifecycle.unpin(session_id),
        session::SessionLifecycleAction::Delete => lifecycle.delete(
            session_id,
            &session::DeleteSessionOptions { active_session_id: None, allow_pinned: request.allow_pinned },
        ),
        session::SessionLifecycleAction::Restore => lifecycle.restore(
            session_id,
            std::time::Duration::from_secs(cli.session_retention.trash_retention_seconds()),
        ),
        session::SessionLifecycleAction::PermanentDelete => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "permanent delete must use session purge",
            ));
        }
    }
    .map_err(io::Error::other)?;
    write_session_report(writer, request.format, &report, || {
        format!("{:?} session {}", request.action, report.session_id).to_lowercase()
    })
}

pub(crate) fn run_session_purge<W: io::Write, P: io::Write>(
    dir: &Path, workspace: &Path, request: SessionPurgeRequest, writer: &mut W, progress: &mut P,
    cancellation: &CancelToken,
) -> io::Result<()> {
    writeln!(progress, "Scanning sessions for purge...")?;
    progress.flush()?;
    let inventory = session::SessionInventory::scan_cancellable(dir, workspace, cancellation)?;
    let eligible = inventory
        .sessions
        .iter()
        .filter(|item| !item.locked && !item.corrupt && (request.allow_pinned || !item.pinned))
        .map(|item| (item.id.clone(), item.storage_state))
        .collect::<Vec<_>>();
    if !request.confirmed {
        let session_ids = eligible.iter().map(|(id, _)| id).collect::<Vec<_>>();
        let preview =
            serde_json::json!({ "session_ids": session_ids, "count": eligible.len(), "requires_confirmation": true });
        return write_session_report(writer, request.format, &preview, || {
            format!("purge would remove {} session(s); rerun with --yes", eligible.len())
        });
    }
    writeln!(progress, "Purging {} session(s)...", eligible.len())?;
    progress.flush()?;
    let lifecycle = session::SessionLifecycle::new(dir, workspace);
    let mut removed = Vec::new();
    let mut failures = Vec::new();
    for (id, state) in eligible {
        cancellation::check(cancellation)?;
        let result = match state {
            session::SessionStorageState::Trash => lifecycle.permanently_delete(
                &id,
                &session::PermanentDeleteOptions {
                    active_session_id: None,
                    allow_pinned: request.allow_pinned,
                    confirmed: true,
                },
            ),
            _ => lifecycle
                .delete(
                    &id,
                    &session::DeleteSessionOptions { active_session_id: None, allow_pinned: request.allow_pinned },
                )
                .and_then(|_| {
                    lifecycle.permanently_delete(
                        &id,
                        &session::PermanentDeleteOptions {
                            active_session_id: None,
                            allow_pinned: request.allow_pinned,
                            confirmed: true,
                        },
                    )
                }),
        };
        match result {
            Ok(_) => removed.push(id),
            Err(error) => failures.push(format!("{id}: {error}")),
        }
    }
    let remaining = session::SessionInventory::scan_cancellable(dir, workspace, cancellation)?;
    let mut skipped = Vec::new();
    if remaining.sessions.iter().any(|item| item.corrupt) {
        skipped.push("artifact reachability is uncertain while corrupt sessions exist".to_string());
    } else {
        for artifact in remaining
            .artifacts
            .iter()
            .filter(|item| item.referenced_by.is_empty() && !item.malformed)
        {
            cancellation::check(cancellation)?;
            for path in [&artifact.metadata_path, &artifact.body_path].into_iter().flatten() {
                cancellation::check(cancellation)?;
                if let Err(error) = std::fs::remove_file(path) {
                    if error.kind() != io::ErrorKind::NotFound {
                        failures.push(format!("{}: {error}", artifact.handle));
                    }
                }
            }
        }
    }
    let report = serde_json::json!({ "removed_session_ids": removed, "skipped": skipped, "failures": failures });
    write_session_report(writer, request.format, &report, || {
        format!(
            "purged {} session(s); {} skipped operation(s); {} failure(s)",
            removed.len(),
            skipped.len(),
            failures.len()
        )
    })
}

pub(crate) fn write_session_report<W: io::Write, T: serde::Serialize>(
    writer: &mut W, format: SessionReportFormat, value: &T, human: impl FnOnce() -> String,
) -> io::Result<()> {
    match format {
        SessionReportFormat::Human => writeln!(writer, "{}", human()),
        SessionReportFormat::Json => serde_json::to_writer_pretty(&mut *writer, value)
            .map_err(io::Error::other)
            .and_then(|()| writeln!(writer)),
    }
}

pub(crate) fn run_session_list<W: io::Write, P: io::Write>(
    dir: &Path, workspace: &Path, writer: &mut W, progress: &mut P, cancellation: &CancelToken,
) -> io::Result<()> {
    writeln!(progress, "Scanning sessions...")?;
    progress.flush()?;
    let inventory = session::SessionInventory::scan_cancellable(dir, workspace, cancellation)?;
    let sessions = inventory
        .sessions
        .iter()
        .filter(|entry| entry.storage_state == session::SessionStorageState::Live)
        .collect::<Vec<_>>();
    if sessions.is_empty() {
        writeln!(writer, "no sessions found")?;
        return Ok(());
    }
    for entry in sessions {
        let source = entry
            .parent_session_id
            .as_deref()
            .zip(entry.source_turn_id.as_deref())
            .map(|(parent, turn)| format!("{parent}@{turn}"))
            .unwrap_or_else(|| "root".to_string());
        writeln!(
            writer,
            "{}\t{}\t{}\tactivity {}\tsource {}\t{}\tin {} out {}",
            entry.id,
            entry.title,
            entry.model,
            entry.last_activity.as_deref().unwrap_or("unknown"),
            source,
            entry.state_label(),
            entry.input_tokens,
            entry.output_tokens,
        )?;
    }
    Ok(())
}

pub(crate) fn run_session_latest<W: io::Write>(dir: &Path, writer: &mut W) -> io::Result<()> {
    let Some(file) = session::latest_session_file(dir) else {
        writeln!(writer, "no sessions found")?;
        return Ok(());
    };
    let summary = session::SessionReader::read_summary(&file);
    let id = file.file_stem().and_then(|stem| stem.to_str()).unwrap_or("session");
    writeln!(writer, "id: {id}")?;
    writeln!(writer, "path: {}", file.display())?;
    writeln!(writer, "title: {}", summary.title)?;
    writeln!(writer, "model: {}", summary.model)?;
    writeln!(
        writer,
        "tokens: in {} out {}",
        summary.input_tokens, summary.output_tokens
    )?;
    Ok(())
}

pub(crate) fn run_session_titles<W: io::Write>(dir: &Path, writer: &mut W) -> io::Result<()> {
    let titles = session::list_session_titles(dir);
    if titles.is_empty() {
        writeln!(writer, "no sessions found")?;
        return Ok(());
    }

    for title in titles {
        writeln!(writer, "{title}")?;
    }
    Ok(())
}

pub(crate) fn run_session_show<W: io::Write>(dir: &Path, session_id: &str, writer: &mut W) -> io::Result<()> {
    let path = session::resolve_session_file(dir, session_id).map_err(io::Error::other)?;
    let title = session::SessionReader::read_title(&path);
    writeln!(writer, "title: {title}")?;

    let _reader = session::SessionReader;
    let transcript = session::SessionReader::read_transcript(&path);
    if transcript.is_empty() {
        writeln!(writer, "session `{session_id}` has no replayable transcript")?;
        return Ok(());
    }

    for entry in transcript {
        match entry {
            app::Entry::User { text } => writeln!(writer, "user: {text}")?,
            app::Entry::Agent { text, .. } => writeln!(writer, "assistant: {text}")?,
            app::Entry::Skill { name, path, token_estimate, .. } => {
                writeln!(writer, "skill {name}: ~{token_estimate} tokens · {path}")?;
            }
            app::Entry::Reasoning { text, .. } => writeln!(writer, "reasoning: {text}")?,
            app::Entry::Tool { name, status, output, .. } => {
                writeln!(
                    writer,
                    "tool {name} {:?}: {}",
                    status,
                    output.join(
                        "
"
                    )
                )?;
            }
            app::Entry::Status { text } => writeln!(writer, "status: {text}")?,
            app::Entry::Error { text } => writeln!(writer, "error: {text}")?,
        }
    }
    Ok(())
}

pub(crate) fn run_session_rename<W: io::Write>(
    dir: &Path, session_id: &str, name: &str, writer: &mut W,
) -> io::Result<()> {
    let path = session::resolve_session_file(dir, session_id).map_err(io::Error::other)?;
    let id = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or(session_id);
    let mut session_writer = session::SessionWriter::resume(&path, id)?;
    session_writer.append_rename(name)?;
    let title = session::SessionReader::read_title(&path);
    writeln!(writer, "renamed {id}: {title}")
}

pub(crate) fn run_session_fork<W: io::Write>(
    dir: &Path, session_id: &str, turn_id: &str, writer: &mut W,
) -> io::Result<()> {
    let path = session::resolve_session_file(dir, session_id).map_err(io::Error::other)?;
    let parent_id = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or(session_id);
    let fork_id = session::fork_session(dir, &path, parent_id, turn_id)?;
    writeln!(writer, "forked {parent_id} at {turn_id}: {fork_id}")
}

pub(crate) fn run_session_inspect<W: io::Write>(
    dir: &Path, session_id: &str, format: SessionDataFormat, writer: &mut W,
) -> io::Result<()> {
    if format != SessionDataFormat::Json {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "inspect only supports --format json",
        ));
    }
    let path = session::resolve_session_file(dir, session_id).map_err(io::Error::other)?;
    let id = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or(session_id);
    let summary = session::SessionReader::read_summary(&path);
    let semantic_records = session::SessionReader::read_validated_records(&path, id)?;
    let context = session::PersistedContextExport::from_records(id, &semantic_records)?;
    let records = session::SessionReader::read_redacted_records(&path);
    let projection = serde_json::json!({
        "schema_version": session::SCHEMA_VERSION,
        "session": {
            "id": id,
            "path": path,
            "title": summary.title,
            "model": summary.model,
            "usage": { "input_tokens": summary.input_tokens, "output_tokens": summary.output_tokens },
        },
        "context": context,
        "records": records,
    });
    serde_json::to_writer_pretty(&mut *writer, &projection).map_err(io::Error::other)?;
    writeln!(writer)?;
    Ok(())
}

pub(crate) fn run_session_export<W: io::Write>(
    dir: &Path, session_id: &str, format: SessionDataFormat, writer: &mut W,
) -> io::Result<()> {
    let path = session::resolve_session_file(dir, session_id).map_err(io::Error::other)?;
    let id = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or(session_id);
    match format {
        SessionDataFormat::Jsonl => {
            for record in session::SessionReader::read_redacted_records(&path) {
                serde_json::to_writer(&mut *writer, &record).map_err(io::Error::other)?;
                writeln!(writer)?;
            }
        }
        SessionDataFormat::Markdown => write!(writer, "{}", session::export_session(&path, id)?.to_markdown())?,
        SessionDataFormat::Html => write!(writer, "{}", session::export_session(&path, id)?.to_html()?)?,
        SessionDataFormat::Json => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "export supports --format jsonl, markdown, or html",
            ));
        }
    }
    Ok(())
}

pub(crate) fn run_debug_command(cli: &Cli, command: &DebugCommand) -> io::Result<()> {
    let workspace = crate::context::discover_workspace_root(&cli.cwd);
    let stdout = io::stdout();
    let mut lock = stdout.lock();
    match command {
        DebugCommand::Tail { lines } => run_debug_tail(&workspace, *lines, &mut lock),
        DebugCommand::SessionLog { session_id, lines } => {
            run_debug_session_log(&workspace, session_id, *lines, &mut lock)
        }
    }
}

pub(crate) fn run_debug_tail<W: io::Write>(workspace: &Path, lines: usize, writer: &mut W) -> io::Result<()> {
    let daily_dir = workspace.join(".thndrs").join("logs").join("daily");
    let Some(path) = newest_log_file(&daily_dir) else {
        return Err(io::Error::new(io::ErrorKind::NotFound, "no daily debug log found"));
    };
    write_log_tail(&path, lines, writer)
}

pub(crate) fn run_debug_session_log<W: io::Write>(
    workspace: &Path, session_id: &str, lines: usize, writer: &mut W,
) -> io::Result<()> {
    let sessions = session::sessions_dir(workspace);
    let session_path = session::resolve_session_file(&sessions, session_id).map_err(io::Error::other)?;
    let id = session_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(session_id);
    let log_path = workspace
        .join(".thndrs")
        .join("logs")
        .join("sessions")
        .join(format!("thndrs-{id}.log"));
    write_log_tail(&log_path, lines, writer)
}

pub(crate) fn write_log_tail<W: io::Write>(path: &Path, lines: usize, writer: &mut W) -> io::Result<()> {
    let tail = session::read_redacted_log_tail(path, lines.min(2_000));
    if tail.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("debug log `{}` is empty or missing", path.display()),
        ));
    }
    for line in tail {
        writeln!(writer, "{line}")?;
    }
    Ok(())
}

pub(crate) fn newest_log_file(dir: &Path) -> Option<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "log"))
        .collect();
    files.sort_by(|left, right| {
        let left_time = std::fs::metadata(left).and_then(|metadata| metadata.modified()).ok();
        let right_time = std::fs::metadata(right).and_then(|metadata| metadata.modified()).ok();
        right_time.cmp(&left_time).then_with(|| right.cmp(left))
    });
    files.into_iter().next()
}

fn run_mcp_catalog_command<W: io::Write>(
    cli: &Cli, command: &crate::cli::commands::mcp::McpCatalogCommand, writer: &mut W,
) -> io::Result<()> {
    use crate::cli::commands::mcp::McpCatalogCommand;

    match command {
        McpCatalogCommand::List => run_mcp_catalog_list(writer),
        McpCatalogCommand::Add { name, url, curation } => {
            let path = mcp::catalog::add_source(name, url, curation.as_deref()).map_err(io::Error::other)?;
            writeln!(writer, "added MCP catalog `{name}` to {}", path.display())
        }
        McpCatalogCommand::Remove { name } => {
            let path = mcp::catalog::remove_source(name).map_err(io::Error::other)?;
            writeln!(writer, "removed MCP catalog `{name}` from {}", path.display())
        }
        McpCatalogCommand::Enable { name } => {
            let path = mcp::catalog::set_source_enabled(name, true).map_err(io::Error::other)?;
            writeln!(writer, "enabled MCP catalog `{name}` in {}", path.display())
        }
        McpCatalogCommand::Disable { name } => {
            let path = mcp::catalog::set_source_enabled(name, false).map_err(io::Error::other)?;
            writeln!(writer, "disabled MCP catalog `{name}` in {}", path.display())
        }
        McpCatalogCommand::Search(args) => {
            run_mcp_catalog_search(&args.query, args.limit, args.cursor.as_deref(), args.offline, writer)
        }
        McpCatalogCommand::Show(args) => {
            run_mcp_catalog_show(&args.name, args.source.as_deref(), &args.version, args.offline, writer)
        }
        McpCatalogCommand::Configure(args) => run_mcp_catalog_configure(cli, args, writer),
    }
}

pub(crate) fn run_mcp_catalog_list<W: io::Write>(writer: &mut W) -> io::Result<()> {
    for source in mcp::catalog::sources().map_err(io::Error::other)? {
        let state = if source.enabled { "enabled" } else { "disabled" };
        let built_in = if source.built_in { "built-in" } else { "custom" };
        writeln!(
            writer,
            "{}\t{state}\t{built_in}\t{}\tcuration claim={}",
            source.name, source.url, source.curation_claim
        )?;
    }
    writeln!(
        writer,
        "Catalog sources are global only. The official catalog is a preview, uncurated discovery source."
    )
}

pub(crate) fn run_mcp_catalog_search<W: io::Write>(
    query: &str, limit: usize, cursor: Option<&str>, offline: bool, writer: &mut W,
) -> io::Result<()> {
    let search = mcp::catalog::search(query, limit, cursor, offline).map_err(io::Error::other)?;
    render_catalog_search(&search, writer)
}

pub(crate) fn run_mcp_catalog_show<W: io::Write>(
    name: &str, source: Option<&str>, version: &str, offline: bool, writer: &mut W,
) -> io::Result<()> {
    let detail = mcp::catalog::detail(name, source, version, offline).map_err(io::Error::other)?;
    let mut found = false;
    for result in &detail.results {
        for entry in &result.entries {
            found = true;
            writeln!(writer, "source: {} ({})", result.source.name, result.source.url)?;
            writeln!(writer, "retrieved: {}", catalog_retrieval(result))?;
            writeln!(writer, "catalog labels: {}", entry.curation_claim)?;
            writeln!(writer, "name: {}", entry.name)?;
            writeln!(writer, "title: {}", entry.title.as_deref().unwrap_or("not supplied"))?;
            writeln!(writer, "claimed publisher: {} (catalog claim)", entry.claimed_publisher)?;
            writeln!(writer, "version: {} (catalog-supplied)", entry.version)?;
            writeln!(writer, "status: {}", entry.status.as_deref().unwrap_or("not supplied"))?;
            writeln!(writer, "description: {}", entry.description)?;
            writeln!(
                writer,
                "available transports: {}",
                catalog_values(&entry.transports, "not supplied")
            )?;
            writeln!(
                writer,
                "platform constraints: {}",
                catalog_values(&entry.platform_constraints, "not supplied")
            )?;
            if entry.packages.is_empty() {
                writeln!(writer, "package origins: not supplied")?;
            } else {
                writeln!(writer, "package origins (catalog-supplied):")?;
                for package in &entry.packages {
                    writeln!(
                        writer,
                        "  {} {} version={} registry={} digest={} transports={} platforms={}",
                        package.registry_type,
                        package.identifier,
                        package.version.as_deref().unwrap_or("not supplied"),
                        package.registry_url.as_deref().unwrap_or("not supplied"),
                        package.sha256.as_deref().unwrap_or("not supplied"),
                        catalog_values(&package.transports, "not supplied"),
                        catalog_values(&package.platform_constraints, "not supplied"),
                    )?;
                }
            }
            writeln!(writer)?;
        }
    }
    for diagnostic in &detail.diagnostics {
        writeln!(writer, "diagnostic: {diagnostic}")?;
    }
    if !found {
        writeln!(writer, "no catalog metadata found for `{name}`")?;
    }
    writeln!(
        writer,
        "Catalog metadata is discovery only. thndrs does not verify publisher identity, curation labels, versions, or supplied digests, and this command does not start a server."
    )
}

fn render_catalog_search<W: io::Write>(search: &mcp::catalog::CatalogSearch, writer: &mut W) -> io::Result<()> {
    let mut entries = 0;
    for result in &search.results {
        writeln!(
            writer,
            "catalog: {} ({}) retrieved={}",
            result.source.name,
            result.source.url,
            catalog_retrieval(result)
        )?;
        for entry in &result.entries {
            entries += 1;
            writeln!(
                writer,
                "{}\t{}\tversion={}\tpublisher claim={}\ttransports={}\tcuration claim={}",
                entry.name,
                entry.title.as_deref().unwrap_or("-"),
                entry.version,
                entry.claimed_publisher,
                catalog_values(&entry.transports, "not supplied"),
                entry.curation_claim,
            )?;
            writeln!(writer, "  {}", entry.description)?;
        }
        if let Some(cursor) = &result.next_cursor {
            writeln!(writer, "next cursor for {}: {cursor}", result.source.name)?;
        }
    }
    for diagnostic in &search.diagnostics {
        writeln!(writer, "diagnostic: {diagnostic}")?;
    }
    if entries == 0 {
        writeln!(writer, "no catalog entries found")?;
    }
    writeln!(
        writer,
        "Catalog metadata is discovery only. thndrs does not verify publisher identity, curation labels, versions, or supplied digests, and this command does not start a server."
    )
}

pub(crate) fn run_mcp_catalog_configure<W: io::Write>(
    cli: &Cli, args: &crate::cli::commands::mcp::CatalogConfigureArgs, writer: &mut W,
) -> io::Result<()> {
    use crate::cli::commands::mcp::CatalogRecipeTransport;

    let detail = mcp::catalog::detail(&args.entry, args.source.as_deref(), &args.version, args.offline)
        .map_err(io::Error::other)?;
    let selected = detail
        .results
        .iter()
        .flat_map(|result| {
            result.entries.iter().map(move |entry| {
                (
                    result.source.clone(),
                    entry.clone(),
                    result
                        .retrieved_at
                        .clone()
                        .unwrap_or_else(crate::utils::datetime::now_iso8601),
                )
            })
        })
        .collect::<Vec<_>>();
    let (source, entry, retrieved_at) = match selected.as_slice() {
        [] => {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("no catalog metadata found for `{}`", args.entry),
            ));
        }
        [selected] => selected.clone(),
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "catalog selection is ambiguous; pass `--source` to select one catalog entry",
            ));
        }
    };
    let transport = match args.transport {
        CatalogRecipeTransport::Stdio => mcp::recipe::CatalogRecipeTransport::Stdio,
        CatalogRecipeTransport::StreamableHttp => mcp::recipe::CatalogRecipeTransport::StreamableHttp,
    };
    let recipe = mcp::recipe::resolve(&source, &entry, transport, args.package.as_deref(), retrieved_at)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let workspace = crate::context::discover_workspace_root(&cli.cwd);
    let target = mcp_config_target(args.scope);
    let path = match target {
        mcp::edit::McpConfigTarget::Global => mcp::config::global_mcp_config_path().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "could not determine the home directory for global MCP configuration",
            )
        })?,
        mcp::edit::McpConfigTarget::Project => mcp::config::project_mcp_config_path(&workspace),
    };
    writeln!(writer, "catalog: {} ({})", source.name, source.url)?;
    writeln!(writer, "claimed publisher: {} (catalog claim)", entry.claimed_publisher)?;
    writeln!(writer, "entry: {} metadata version={}", entry.name, entry.version)?;
    writeln!(
        writer,
        "origin: {} ({})",
        recipe.provenance.origin_type, recipe.provenance.origin
    )?;
    writeln!(
        writer,
        "package version: {}",
        recipe.provenance.package_version.as_deref().unwrap_or("not applicable")
    )?;
    writeln!(
        writer,
        "supplied digest: {} (catalog assertion; not verified by thndrs)",
        recipe.provenance.supplied_sha256.as_deref().unwrap_or("not supplied")
    )?;
    match recipe.server.transport {
        mcp::config::McpTransport::Stdio => {
            writeln!(
                writer,
                "command: {}",
                command_preview(&recipe.server.command, &recipe.server.args)
            )?;
            writeln!(
                writer,
                "later startup may download code: {}",
                if recipe.launcher_may_download { "yes, through the package runner" } else { "no" }
            )?;
        }
        mcp::config::McpTransport::StreamableHttp => {
            writeln!(writer, "URL: {}", recipe.server.url.as_deref().unwrap_or_default())?;
        }
    }
    writeln!(
        writer,
        "environment variable names: {}",
        if recipe.environment_names.is_empty() {
            "none".to_string()
        } else {
            recipe.environment_names.join(", ")
        }
    )?;
    writeln!(writer, "destination: {} ({})", scope_label(args.scope), path.display())?;
    writeln!(
        writer,
        "Catalog metadata is an assertion. thndrs did not contact the proposed MCP server, execute the command, or invoke a package manager."
    )?;
    if !args.yes {
        return writeln!(
            writer,
            "No files changed. Review this recipe, then rerun with `--yes` to write it."
        );
    }
    let path = mcp::edit::add_catalog_server(&workspace, target, &args.name, recipe.server, recipe.provenance)?;
    writeln!(
        writer,
        "added catalog-derived MCP server `{}` to {}",
        args.name,
        path.display()
    )?;
    if args.scope == crate::cli::commands::mcp::McpConfigScope::Project {
        writeln!(
            writer,
            "Review the project MCP configuration, inspect it with `thndrs mcp status`, then run `thndrs mcp trust` to activate it."
        )?;
    }
    Ok(())
}

fn command_preview(command: &str, args: &[String]) -> String {
    std::iter::once(command)
        .chain(args.iter().map(String::as_str))
        .map(|part| format!("{part:?}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn scope_label(scope: crate::cli::commands::mcp::McpConfigScope) -> &'static str {
    match scope {
        crate::cli::commands::mcp::McpConfigScope::Global => "global",
        crate::cli::commands::mcp::McpConfigScope::Project => "project",
    }
}

fn catalog_retrieval(result: &mcp::catalog::CatalogSearchResult) -> String {
    match (&result.retrieved_at, result.from_cache) {
        (Some(time), true) => format!("cache from {time}"),
        (Some(time), false) => time.clone(),
        (None, _) => "live response".to_string(),
    }
}

fn catalog_values(values: &[String], fallback: &str) -> String {
    if values.is_empty() { fallback.to_string() } else { values.join(", ") }
}

pub(crate) fn run_mcp_add<W: io::Write>(
    cli: &Cli, name: &str, scope: crate::cli::commands::mcp::McpConfigScope, command: Option<&str>, args: &[String],
    url: Option<&str>, writer: &mut W,
) -> io::Result<()> {
    let server = match (command, url) {
        (Some(command), None) if !command.trim().is_empty() => mcp::config::McpServerConfig {
            command: command.to_string(),
            args: args.to_vec(),
            ..mcp::config::McpServerConfig::default()
        },
        (None, Some(url)) if !url.trim().is_empty() => mcp::config::McpServerConfig {
            transport: mcp::config::McpTransport::StreamableHttp,
            url: Some(url.to_string()),
            ..mcp::config::McpServerConfig::default()
        },
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "mcp add requires exactly one of --command or --url",
            ));
        }
    };
    let workspace = crate::context::discover_workspace_root(&cli.cwd);
    let target = mcp_config_target(scope);
    let path = mcp::edit::add_server(&workspace, target, name, server)?;
    writeln!(writer, "added MCP server `{name}` to {}", path.display())?;
    if scope == crate::cli::commands::mcp::McpConfigScope::Project {
        writeln!(
            writer,
            "Review the project MCP configuration, inspect it with `thndrs mcp status`, then run `thndrs mcp trust` to activate it."
        )?;
    }
    Ok(())
}

pub(crate) fn run_mcp_remove<W: io::Write>(
    cli: &Cli, name: &str, scope: crate::cli::commands::mcp::McpConfigScope, writer: &mut W,
) -> io::Result<()> {
    let workspace = crate::context::discover_workspace_root(&cli.cwd);
    let path = mcp::edit::remove_server(&workspace, mcp_config_target(scope), name)?;
    writeln!(writer, "removed MCP server `{name}` from {}", path.display())
}

fn mcp_config_target(scope: crate::cli::commands::mcp::McpConfigScope) -> mcp::edit::McpConfigTarget {
    match scope {
        crate::cli::commands::mcp::McpConfigScope::Global => mcp::edit::McpConfigTarget::Global,
        crate::cli::commands::mcp::McpConfigScope::Project => mcp::edit::McpConfigTarget::Project,
    }
}

pub(crate) fn run_mcp_list<W: io::Write>(cli: &Cli, writer: &mut W) -> io::Result<()> {
    let workspace = crate::context::discover_workspace_root(&cli.cwd);
    let effective = load_effective_mcp_for_workspace(&workspace)?;
    if effective.config.servers.is_empty() && effective.blocked_project_servers.is_empty() {
        writeln!(writer, "no MCP servers configured")?;
        return Ok(());
    }

    for server in mcp::config::server_statuses(&effective) {
        let execution = match server.transport {
            mcp::config::McpTransport::Stdio => "execution=local-process\tpermissions=thndrs-process",
            mcp::config::McpTransport::StreamableHttp => "execution=remote-server\tpermissions=externally-owned",
        };
        let precedence = if server.overrides_global { "\twould-override=global" } else { "" };
        writeln!(
            writer,
            "{}\t{}\t{:?}\tsource={}\t{execution}{precedence}",
            server.name,
            server.state.label(),
            server.transport,
            server.source.as_str(),
        )?;
    }
    for diagnostic in effective.diagnostics {
        writeln!(writer, "diagnostic: {diagnostic}")?;
    }
    Ok(())
}

pub(crate) fn run_mcp_status<W: io::Write>(cli: &Cli, writer: &mut W) -> io::Result<()> {
    let workspace = crate::context::discover_workspace_root(&cli.cwd);
    let Some(hash) = mcp::config::project_mcp_config_hash(&workspace).map_err(io::Error::other)? else {
        writeln!(writer, "project MCP configuration: not found")?;
        return Ok(());
    };
    let trust = crate::trust::project_mcp_trust(&workspace, &hash)?;
    match trust {
        crate::trust::ProjectMcpTrust::Trusted => {
            writeln!(writer, "project MCP configuration: trusted")?;
        }
        crate::trust::ProjectMcpTrust::Untrusted => {
            writeln!(writer, "project MCP configuration: blocked by trust")?;
        }
        crate::trust::ProjectMcpTrust::Stale { trusted_hash } => {
            writeln!(writer, "project MCP configuration: blocked; configuration changed")?;
            writeln!(writer, "trusted sha256: {trusted_hash}")?;
        }
    }
    writeln!(writer, "current sha256: {hash}")?;
    writeln!(writer, "scope: workspace={} capability=mcp", workspace.display())?;
    Ok(())
}

pub(crate) fn run_mcp_trust<W: io::Write>(cli: &Cli, writer: &mut W) -> io::Result<()> {
    let workspace = crate::context::discover_workspace_root(&cli.cwd);
    let hash = mcp::config::project_mcp_config_hash(&workspace)
        .map_err(io::Error::other)?
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "project MCP configuration `.thndrs/mcp.toml` not found",
            )
        })?;
    crate::trust::trust_project_mcp(&workspace, &hash)?;
    writeln!(writer, "trusted project MCP configuration")?;
    writeln!(writer, "sha256: {hash}")?;
    writeln!(writer, "scope: workspace={} capability=mcp", workspace.display())?;
    Ok(())
}

pub(crate) fn run_mcp_revoke<W: io::Write>(cli: &Cli, writer: &mut W) -> io::Result<()> {
    let workspace = crate::context::discover_workspace_root(&cli.cwd);
    if crate::trust::revoke_project_mcp_trust(&workspace)? {
        writeln!(writer, "revoked project MCP trust for {}", workspace.display())
    } else {
        writeln!(writer, "project MCP trust was not set for {}", workspace.display())
    }
}

pub(crate) fn run_mcp_test<W: io::Write>(cli: &Cli, name: &str, writer: &mut W) -> io::Result<()> {
    let workspace = crate::context::discover_workspace_root(&cli.cwd);
    let effective = load_effective_mcp_for_workspace(&workspace)?;
    let server = configured_mcp_server(&effective, name)?;
    let client = mcp::manager::McpClient::connect(name.to_string(), &server).map_err(io::Error::other)?;
    writeln!(writer, "{name}\tready\t{} tools", client.tool_definitions().len())?;
    for diagnostic in client.diagnostics() {
        writeln!(writer, "diagnostic: {diagnostic}")?;
    }
    Ok(())
}

pub(crate) fn run_mcp_tools<W: io::Write>(cli: &Cli, name: &str, writer: &mut W) -> io::Result<()> {
    let workspace = crate::context::discover_workspace_root(&cli.cwd);
    let effective = load_effective_mcp_for_workspace(&workspace)?;
    let server = configured_mcp_server(&effective, name)?;
    let client = mcp::manager::McpClient::connect(name.to_string(), &server).map_err(io::Error::other)?;
    for tool in client.tool_definitions() {
        writeln!(writer, "{}\t{}", tool.name, tool.description)?;
    }
    Ok(())
}

pub(crate) fn run_mcp_resources<W: io::Write>(cli: &Cli, name: &str, writer: &mut W) -> io::Result<()> {
    let workspace = crate::context::discover_workspace_root(&cli.cwd);
    let effective = load_effective_mcp_for_workspace(&workspace)?;
    let server = configured_mcp_server(&effective, name)?;
    let client = mcp::manager::McpClient::connect(name.to_string(), &server).map_err(io::Error::other)?;
    if !client.server_info().resources_available {
        writeln!(writer, "MCP server `{name}` does not advertise resources")?;
        return Ok(());
    }
    for resource in client.list_resources().map_err(io::Error::other)? {
        let namespace = mcp::adapter::namespaced_resource_name(name);
        let mime_type = resource.mime_type.as_deref().unwrap_or("unknown");
        let size = resource
            .size
            .map_or_else(|| "unknown".to_string(), |size| size.to_string());
        writeln!(
            writer,
            "{namespace}\tname={}\turi={}\tmime_type={mime_type}\tsize={size}",
            resource.name, resource.uri
        )?;
    }
    Ok(())
}

pub(crate) fn run_mcp_resource<W: io::Write>(
    cli: &Cli, server_name: &str, uri: &str, writer: &mut W,
) -> io::Result<()> {
    let workspace = crate::context::discover_workspace_root(&cli.cwd);
    let effective = load_effective_mcp_for_workspace(&workspace)?;
    let server = configured_mcp_server(&effective, server_name)?;
    let client = mcp::manager::McpClient::connect(server_name.to_string(), &server).map_err(io::Error::other)?;
    let resource = client.read_resource(uri).map_err(io::Error::other)?;
    serde_json::to_writer(&mut *writer, &resource).map_err(io::Error::other)?;
    writeln!(writer)
}

pub(crate) fn run_mcp_call<W: io::Write>(
    cli: &Cli, server_name: &str, tool_name: &str, json: &str, writer: &mut W,
) -> io::Result<()> {
    let workspace = crate::context::discover_workspace_root(&cli.cwd);
    let effective = load_effective_mcp_for_workspace(&workspace)?;
    let server = configured_mcp_server(&effective, server_name)?;
    let client = mcp::manager::McpClient::connect(server_name.to_string(), &server).map_err(io::Error::other)?;
    let namespaced = mcp::adapter::namespaced_tool_name(server_name, tool_name);
    let request = tools::ToolUseRequest::new(namespaced, json.to_string(), "cli".to_string());
    let output = client.call_tool(&request);
    match output.status {
        app::ToolStatus::Failed => writeln!(
            writer,
            "failed: {}",
            output.error.unwrap_or_else(|| "MCP tool failed".to_string())
        ),
        _ => {
            for line in output.display_lines() {
                writeln!(writer, "{line}")?;
            }
            Ok(())
        }
    }
}

pub(crate) fn configured_mcp_server(
    effective: &mcp::config::EffectiveMcpConfig, name: &str,
) -> io::Result<mcp::config::McpServerConfig> {
    let server = effective.config.servers.get(name).cloned().ok_or_else(|| {
        if effective.blocked_project_servers.contains_key(name) {
            return io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("MCP server `{name}` is blocked by project trust; inspect with `thndrs mcp status`"),
            );
        }
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("MCP server `{name}` is not configured"),
        )
    })?;
    if !server.enabled {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("MCP server `{name}` is disabled"),
        ));
    }
    Ok(server)
}

pub(crate) fn run_acp_command(cli: &Cli, command: &cli_commands::acp::AcpCommand) -> io::Result<()> {
    use cli_commands::acp::AcpCommand;
    match command {
        AcpCommand::Serve => run_acp_server(cli),
        AcpCommand::List => {
            print!("{}", render_acp_list(cli));
            Ok(())
        }
        AcpCommand::Inspect { name } => {
            print!("{}", render_acp_inspect(cli, name)?);
            Ok(())
        }
        AcpCommand::Smoke { name, prompt } => {
            let stdout = io::stdout();
            let mut lock = stdout.lock();
            run_acp_smoke(cli, name, prompt, &mut lock)
        }
        AcpCommand::Logout { name } => {
            let stdout = io::stdout();
            let mut lock = stdout.lock();
            run_acp_logout(cli, name, &mut lock)
        }
        AcpCommand::ListSessions { name } => {
            let stdout = io::stdout();
            let mut lock = stdout.lock();
            run_acp_list_sessions(cli, name, &mut lock)
        }
        AcpCommand::LoadSession { name, session_id } => {
            let stdout = io::stdout();
            let mut lock = stdout.lock();
            run_acp_load_session(cli, name, session_id, &mut lock)
        }
        AcpCommand::ResumeSession { name, session_id } => {
            let stdout = io::stdout();
            let mut lock = stdout.lock();
            run_acp_resume_session(cli, name, session_id, &mut lock)
        }
        AcpCommand::CloseSession { name, session_id } => {
            let stdout = io::stdout();
            let mut lock = stdout.lock();
            run_acp_close_session(cli, name, session_id, &mut lock)
        }
        AcpCommand::Registry { file } => {
            let stdout = io::stdout();
            let mut lock = stdout.lock();
            run_acp_registry(file.as_deref(), &mut lock)
        }
        AcpCommand::Install { agent_id, name, file, yes } => {
            let stdout = io::stdout();
            let mut lock = stdout.lock();
            run_acp_install(cli, agent_id, name.clone(), file.as_deref(), *yes, &mut lock)
        }
        AcpCommand::Update { name, file, yes } => {
            let stdout = io::stdout();
            let mut lock = stdout.lock();
            run_acp_update(cli, name, file.as_deref(), *yes, &mut lock)
        }
    }
}

/// Run the local ACP agent server through the primary `thndrs` executable.
///
/// The server shares the normal configuration pipeline with the CLI/TUI while
/// keeping its protocol transport isolated: stdout remains ACP JSON-RPC and
/// diagnostics are emitted only on stderr.
pub(crate) fn run_acp_server(cli: &Cli) -> io::Result<()> {
    let server_config = server::ServerConfig::new(
        config::resolve_cli_path(&cli.cwd),
        cli.model.clone(),
        cli.session_dir.clone(),
    )
    .with_authority(cli.authority)
    .with_reasoning(cli.reasoning_effort, cli.reasoning_summary)
    .with_model_reduction(cli.context.reduction.clone());
    let _ = tracing_subscriber::fmt()
        .with_writer(io::stderr)
        .with_ansi(false)
        .try_init();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| io::Error::other(format!("failed to start ACP runtime: {error}")))?;

    runtime
        .block_on(server::run_stdio(server_config))
        .map_err(|error| io::Error::other(format!("ACP server failed: {error}")))
}

pub(crate) fn render_acp_list(cli: &Cli) -> String {
    let mut out = String::new();
    if cli.acp_agents.is_empty() {
        out.push_str(
            "no ACP agents configured
",
        );
        return out;
    }

    for (name, agent) in &cli.acp_agents {
        let status = if agent.enabled { "enabled" } else { "disabled" };
        out.push_str(&format!(
            "{name}\t{status}\t{}
",
            acp::config::redacted_command_display(agent)
        ));
    }
    out
}

pub(crate) fn render_acp_inspect(cli: &Cli, name: &str) -> io::Result<String> {
    let agent = cli
        .acp_agents
        .get(name)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("ACP agent `{name}` is not configured")))?;
    let source = cli
        .config_origins
        .get("acp_agents")
        .map(|origin| format!("{}:{}", origin.source.as_str(), origin.detail))
        .unwrap_or_else(|| "default:default".to_string());
    let env_keys = if agent.env.is_empty() {
        String::from("none")
    } else {
        agent.env.keys().cloned().collect::<Vec<_>>().join(", ")
    };
    let args = if agent.args.is_empty() { String::from("none") } else { agent.args.join(" ") };

    Ok(format!(
        "name: {name}
status: {}
command: {}
args: {args}
env_keys: {env_keys}
timeout_secs: {}
source: {source}
",
        if agent.enabled { "enabled" } else { "disabled" },
        acp::config::redacted_command_display(agent),
        agent.timeout_secs,
    ))
}

pub(crate) fn run_acp_smoke<W: io::Write>(cli: &Cli, name: &str, prompt: &str, writer: &mut W) -> io::Result<()> {
    let agent = cli
        .acp_agents
        .get(name)
        .cloned()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("ACP agent `{name}` is not configured")))?;
    if !agent.enabled {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("ACP agent `{name}` is disabled"),
        ));
    }

    let workspace_root = crate::context::discover_workspace_root(&cli.cwd);
    let mut handle = acp::runner::RunHandle::new(
        workspace_root.clone(),
        name.to_string(),
        Some(agent),
        prompt.to_string(),
    );
    if let Ok(effective_mcp) = load_effective_mcp_for_workspace(&workspace_root) {
        handle = handle
            .with_mcp_config(effective_mcp.config)
            .with_mcp_diagnostics(effective_mcp.diagnostics);
    }
    let rx = handle.spawn();
    for event in rx.iter() {
        match write_acp_event(writer, event)? {
            AcpEventWrite::Continue => {}
            AcpEventWrite::Finished => {
                writeln!(
                    writer,
                    "
finished"
                )?;
                return Ok(());
            }
            AcpEventWrite::Cancelled => {
                writeln!(
                    writer,
                    "
cancelled"
                )?;
                return Ok(());
            }
            AcpEventWrite::Failed(message) => {
                writeln!(
                    writer,
                    "
failed: {message}"
                )?;
                return Err(io::Error::other(message));
            }
        }
    }
    Err(io::Error::new(
        io::ErrorKind::UnexpectedEof,
        "ACP smoke stream ended before a terminal event",
    ))
}

pub(crate) fn run_acp_logout<W: io::Write>(cli: &Cli, name: &str, writer: &mut W) -> io::Result<()> {
    let agent = cli
        .acp_agents
        .get(name)
        .cloned()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("ACP agent `{name}` is not configured")))?;
    for line in acp::runner::logout(name, agent).map_err(io::Error::other)? {
        writeln!(writer, "{line}")?;
    }
    Ok(())
}

pub(crate) fn run_acp_list_sessions<W: io::Write>(cli: &Cli, name: &str, writer: &mut W) -> io::Result<()> {
    let agent = configured_acp_agent(cli, name)?;
    let workspace_root = crate::context::discover_workspace_root(&cli.cwd);
    let sessions = acp::runner::list_sessions(name, agent, workspace_root).map_err(io::Error::other)?;
    if sessions.is_empty() {
        writeln!(writer, "no ACP sessions found")?;
        return Ok(());
    }
    for session in sessions {
        let title = session.title.unwrap_or_else(|| "-".to_string());
        let updated_at = session.updated_at.unwrap_or_else(|| "-".to_string());
        writeln!(
            writer,
            "{}\t{}\t{}\t{}",
            session.session_id,
            session.cwd.display(),
            title,
            updated_at
        )?;
        if !session.additional_directories.is_empty() {
            let dirs = session
                .additional_directories
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            writeln!(writer, "  additional_directories: {dirs}")?;
        }
    }
    Ok(())
}

pub(crate) fn run_acp_load_session<W: io::Write>(
    cli: &Cli, name: &str, session_id: &str, writer: &mut W,
) -> io::Result<()> {
    let agent = configured_acp_agent(cli, name)?;
    let workspace_root = crate::context::discover_workspace_root(&cli.cwd);
    let events =
        acp::runner::load_session(name, agent, workspace_root, session_id.to_string()).map_err(io::Error::other)?;
    for event in events {
        match write_acp_event(writer, event)? {
            AcpEventWrite::Continue | AcpEventWrite::Finished | AcpEventWrite::Cancelled | AcpEventWrite::Failed(_) => {
            }
        }
    }
    writeln!(
        writer,
        "
loaded: {name} {session_id}"
    )?;
    Ok(())
}

pub(crate) fn run_acp_resume_session<W: io::Write>(
    cli: &Cli, name: &str, session_id: &str, writer: &mut W,
) -> io::Result<()> {
    let agent = configured_acp_agent(cli, name)?;
    let workspace_root = crate::context::discover_workspace_root(&cli.cwd);
    let metadata =
        acp::runner::resume_session(name, agent, workspace_root, session_id.to_string()).map_err(io::Error::other)?;
    writeln!(
        writer,
        "acp_session: {} {}",
        metadata.agent_name, metadata.acp_session_id
    )?;
    writeln!(writer, "resumed: {name} {session_id}")?;
    Ok(())
}

pub(crate) fn run_acp_close_session<W: io::Write>(
    cli: &Cli, name: &str, session_id: &str, writer: &mut W,
) -> io::Result<()> {
    let agent = configured_acp_agent(cli, name)?;
    for line in acp::runner::close_session(name, agent, session_id.to_string()).map_err(io::Error::other)? {
        writeln!(writer, "{line}")?;
    }
    Ok(())
}

pub(crate) fn run_acp_registry<W: io::Write>(file: Option<&Path>, writer: &mut W) -> io::Result<()> {
    let registry = match file {
        Some(path) => acp::registry::read_file(path),
        None => acp::registry::fetch_official(),
    }
    .map_err(io::Error::other)?;
    write!(writer, "{registry}")
}

pub(crate) fn run_acp_install<W: io::Write>(
    cli: &Cli, agent_id: &str, name: Option<String>, file: Option<&Path>, yes: bool, writer: &mut W,
) -> io::Result<()> {
    let workspace = crate::context::discover_workspace_root(&cli.cwd);
    let request = acp::registry::InstallRequest {
        agent_id: agent_id.to_string(),
        name,
        source: registry_source(file),
        confirmed: yes,
        timestamp: datetime::now_iso8601(),
    };
    let outcome = acp::registry::install(&workspace, &request).map_err(io::Error::other)?;
    writeln!(
        writer,
        "installed: {} {} {}",
        outcome.name, outcome.agent_id, outcome.agent_version
    )?;
    writeln!(writer, "model: {}", outcome.model)?;
    writeln!(writer, "config: {}", outcome.config_path.display())?;
    writeln!(writer, "metadata: {}", outcome.metadata_path.display())
}

pub(crate) fn run_acp_update<W: io::Write>(
    cli: &Cli, name: &str, file: Option<&Path>, yes: bool, writer: &mut W,
) -> io::Result<()> {
    let workspace = crate::context::discover_workspace_root(&cli.cwd);
    let request = acp::registry::UpdateRequest {
        name: name.to_string(),
        source: registry_source(file),
        confirmed: yes,
        timestamp: datetime::now_iso8601(),
    };
    let outcome = acp::registry::update(&workspace, &request).map_err(io::Error::other)?;
    writeln!(
        writer,
        "updated: {} {} {}",
        outcome.name, outcome.agent_id, outcome.agent_version
    )?;
    writeln!(writer, "model: {}", outcome.model)?;
    writeln!(writer, "config: {}", outcome.config_path.display())?;
    writeln!(writer, "metadata: {}", outcome.metadata_path.display())
}

pub(crate) fn registry_source(file: Option<&Path>) -> acp::registry::RegistrySource {
    match file {
        Some(path) => acp::registry::RegistrySource::File(path.to_path_buf()),
        None => acp::registry::RegistrySource::Official,
    }
}

pub(crate) fn write_acp_event<W: io::Write>(writer: &mut W, event: app::AgentEvent) -> io::Result<AcpEventWrite> {
    match event {
        app::AgentEvent::Started => writeln!(writer, "started")?,
        app::AgentEvent::Status(text) => writeln!(writer, "status: {text}")?,
        app::AgentEvent::AssistantDelta(text) => write!(writer, "{text}")?,
        app::AgentEvent::ReasoningDelta(text) => writeln!(writer, "reasoning: {text}")?,
        app::AgentEvent::Usage { input_tokens, output_tokens } => {
            writeln!(writer, "usage: input={input_tokens} output={output_tokens}")?
        }
        app::AgentEvent::CodexUsage(usage) => writeln!(
            writer,
            "account capacity: {}",
            usage.compact_status().unwrap_or_else(|| "update".to_string())
        )?,
        app::AgentEvent::RequestStarted(_) => {}
        app::AgentEvent::RequestAccounting(accounting) => {
            let input = accounting
                .provider_usage
                .as_ref()
                .and_then(|usage| usage.components.input_tokens);
            let output = accounting
                .provider_usage
                .as_ref()
                .and_then(|usage| usage.components.output_tokens);
            writeln!(
                writer,
                "request: {} bytes input={input:?} output={output:?}",
                accounting.serialized_bytes.value
            )?;
        }
        app::AgentEvent::ToolStarted { id, name, arguments } => {
            writeln!(writer, "tool_started: {name}#{id} {arguments}")?
        }
        app::AgentEvent::ToolFinished { id, status, output, .. } => writeln!(
            writer,
            "tool_finished: {id} {status:?} {}",
            output.join(
                "
"
            )
        )?,
        app::AgentEvent::StateProjectionDecision { .. } => {}
        app::AgentEvent::PermissionRequest(permission) => {
            writeln!(writer, "permission: {} ({})", permission.title, permission.tool_call_id)?;
            let _ = permission.cancel();
        }
        app::AgentEvent::PermissionResolved { tool_call_id, outcome } => {
            writeln!(writer, "permission_resolved: {tool_call_id} {outcome}")?
        }
        app::AgentEvent::AcpSession(metadata) => writeln!(
            writer,
            "acp_session: {} {}",
            metadata.agent_name, metadata.acp_session_id
        )?,
        app::AgentEvent::ModelMetadataLoaded(_) | app::AgentEvent::Retrying { .. } => {}
        app::AgentEvent::Finished => return Ok(AcpEventWrite::Finished),
        app::AgentEvent::Cancelled => return Ok(AcpEventWrite::Cancelled),
        app::AgentEvent::Failed(message) => return Ok(AcpEventWrite::Failed(message)),
    }
    Ok(AcpEventWrite::Continue)
}

pub(crate) fn configured_acp_agent(cli: &Cli, name: &str) -> io::Result<config::AcpAgentConfig> {
    let agent = cli
        .acp_agents
        .get(name)
        .cloned()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("ACP agent `{name}` is not configured")))?;
    if !agent.enabled {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("ACP agent `{name}` is disabled"),
        ));
    }
    Ok(agent)
}
